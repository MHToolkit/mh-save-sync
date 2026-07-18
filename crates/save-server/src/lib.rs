use axum::body::{Body, to_bytes};
use axum::extract::{Extension, Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::TryStreamExt;
use object_store::aws::{AmazonS3Builder, Checksum};
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt};
use save_crypto::{
    CRYPTO_SUITE_V1, DeviceCertificate, canonical_http_request, verify_device_certificate,
};
use save_domain::SnapshotId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    backend: Backend,
}

#[derive(Debug, Clone)]
struct AuthContext {
    account_handle: String,
    device_cert_id: String,
}

const AUTH_ACCOUNT: &str = "x-mh-account";
const AUTH_DEVICE: &str = "x-mh-device-cert";
const AUTH_CHALLENGE: &str = "x-mh-challenge-id";
const AUTH_NONCE: &str = "x-mh-nonce";
const AUTH_TIMESTAMP: &str = "x-mh-timestamp";
const AUTH_SIGNATURE: &str = "x-mh-signature";

#[derive(Clone)]
enum Backend {
    Memory(Arc<Mutex<InMemoryState>>),
    Persistent(Arc<PersistentState>),
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            backend: Backend::Memory(Arc::new(Mutex::new(InMemoryState::default()))),
        }
    }
}

#[derive(Default)]
struct InMemoryState {
    chunks: BTreeMap<String, Vec<u8>>,
    manifests: BTreeMap<String, Vec<u8>>,
    uploads: BTreeMap<Uuid, UploadSession>,
    heads: BTreeMap<String, SnapshotId>,
    snapshots: BTreeMap<String, SnapshotRow>,
    snapshot_accounts: BTreeMap<String, String>,
    snapshot_chunks: BTreeMap<String, Vec<String>>,
    accounts: BTreeMap<String, Vec<u8>>,
    devices: BTreeMap<String, MemoryDevice>,
    challenges: BTreeMap<Uuid, MemoryChallenge>,
}

#[derive(Clone)]
struct MemoryDevice {
    account_handle: String,
    public_key: Vec<u8>,
    revoked: bool,
}
#[derive(Clone)]
struct MemoryChallenge {
    account_handle: String,
    device_cert_id: String,
    nonce: Vec<u8>,
    expires: u64,
    used: bool,
}

struct PersistentState {
    pool: PgPool,
    object_store: Arc<dyn ObjectStore>,
}

const READINESS_PAGE_SIZE: i64 = 256;
const TRACKED_ORPHAN_COUNT_SQL: &str = "SELECT count(*) FROM objects o WHERE \
     o.created_at < now() - make_interval(secs => $1::double precision) \
     AND NOT EXISTS (SELECT 1 FROM snapshot_objects so WHERE so.account_handle=o.account_handle AND so.object_id=o.object_id) \
     AND NOT EXISTS (SELECT 1 FROM snapshots s WHERE s.account_handle=o.account_handle AND s.encrypted_manifest_object=o.object_id) \
     AND NOT EXISTS (SELECT 1 FROM upload_sessions us WHERE us.account_handle=o.account_handle AND us.expires_at>now() AND (us.manifest_id=o.object_id OR us.required_chunks ? o.object_id))";
const TRACKED_ORPHAN_MARK_SQL: &str = "INSERT INTO orphan_gc_marks(account_handle,object_id,storage_key) \
     SELECT o.account_handle,o.object_id,o.storage_key FROM objects o WHERE \
     o.created_at < now() - make_interval(secs => $1::double precision) \
     AND NOT EXISTS (SELECT 1 FROM snapshot_objects so WHERE so.account_handle=o.account_handle AND so.object_id=o.object_id) \
     AND NOT EXISTS (SELECT 1 FROM snapshots s WHERE s.account_handle=o.account_handle AND s.encrypted_manifest_object=o.object_id) \
     AND NOT EXISTS (SELECT 1 FROM upload_sessions us WHERE us.account_handle=o.account_handle AND us.expires_at>now() AND (us.manifest_id=o.object_id OR us.required_chunks ? o.object_id)) \
     ON CONFLICT (account_handle,storage_key) DO NOTHING";
const READINESS_OBJECT_PAGE_SQL: &str = "SELECT DISTINCT o.storage_key \
     FROM snapshots s \
     JOIN snapshot_objects so \
       ON so.account_handle=s.account_handle AND so.snapshot_id=s.id \
     JOIN objects o \
       ON o.account_handle=so.account_handle AND o.object_id=so.object_id \
     WHERE ($1::text IS NULL OR o.storage_key > $1) \
     ORDER BY o.storage_key \
     LIMIT $2";

fn readiness_next_cursor(storage_keys: &[String]) -> Option<String> {
    storage_keys.last().cloned()
}

#[derive(Debug, Clone)]
struct UploadSession {
    account_handle: Option<String>,
    device_cert_id: Option<String>,
    logical_save_id: String,
    base_head: Option<SnapshotId>,
    parents: Vec<SnapshotId>,
    required_chunks: BTreeSet<String>,
    uploaded_chunks: BTreeSet<String>,
    manifest_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRow {
    pub snapshot_id: SnapshotId,
    pub logical_save_id: String,
    pub parents: Vec<SnapshotId>,
    pub manifest_id: String,
    pub conflict: bool,
    /// A conflict remains immutable history after the user explicitly chooses a side.
    #[serde(default)]
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictResolutionKind {
    KeepCloudHead,
    ReplaceWithLocal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveConflictRequest {
    pub chosen_snapshot_id: SnapshotId,
    pub resolution: ConflictResolutionKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveConflictResponse {
    pub conflict_snapshot_id: SnapshotId,
    pub chosen_snapshot_id: SnapshotId,
    pub resolution: ConflictResolutionKind,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotObjectResponse {
    pub object_id: String,
    pub sha256: String,
    pub bytes_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDownloadResponse {
    pub snapshot_id: SnapshotId,
    pub encrypted_manifest: SnapshotObjectResponse,
    pub chunks: Vec<SnapshotObjectResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub backend: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OrphanGcReport {
    pub eligible: u64,
    pub deleted: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBootstrapRequest {
    pub account_handle: String,
    pub root_public_key_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRequest {
    pub account_handle: String,
    pub device_cert_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub challenge_id: Uuid,
    pub nonce_b64: String,
    pub expires_unix_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRegisterRequest {
    pub account_handle: String,
    pub cert_id: String,
    pub device_public_key_b64: String,
    pub certificate_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeginSnapshotRequest {
    #[serde(default)]
    pub account_handle: Option<String>,
    #[serde(default)]
    pub device_cert_id: Option<String>,
    pub logical_save_id: String,
    pub base_head: Option<SnapshotId>,
    pub parents: Vec<SnapshotId>,
    pub encrypted_manifest_id: String,
    pub chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeginSnapshotResponse {
    pub upload_id: Uuid,
    pub missing_chunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutChunkRequest {
    pub chunk_id: String,
    pub sha256: String,
    pub bytes_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutManifestRequest {
    pub manifest_id: String,
    pub sha256: String,
    pub bytes_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSnapshotRequest {
    pub snapshot_id: SnapshotId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommitOutcomeKind {
    FastForward,
    Conflict,
    FirstSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitSnapshotResponse {
    pub outcome: CommitOutcomeKind,
    pub head: SnapshotId,
    pub conflict_snapshot: Option<SnapshotId>,
}

#[derive(Debug, thiserror::Error)]
enum BackendError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("backend unavailable: {0}")]
    Unavailable(String),
}

type ApiError = (StatusCode, String);

impl BackendError {
    fn api(self) -> ApiError {
        match self {
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::Invalid(message) => (StatusCode::BAD_REQUEST, message),
            Self::Unavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message),
        }
    }
}

impl AppState {
    pub async fn persistent_from_env() -> anyhow::Result<Self> {
        let database_url = database_url_from_env()?;
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await?;
        sqlx::migrate!("../../deploy/compose/migrations")
            .run(&pool)
            .await?;

        let endpoint = required_env("S3_ENDPOINT")?;
        let bucket = std::env::var("S3_BUCKET").unwrap_or_else(|_| "mh-save-sync".into());
        let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".into());
        let access_key = env_or_file("S3_ACCESS_KEY", "S3_ACCESS_KEY_FILE")?;
        let secret_key = env_or_file("S3_SECRET_KEY", "S3_SECRET_KEY_FILE")?;
        let allow_http = std::env::var("S3_ALLOW_HTTP").map_or_else(
            |_| endpoint.starts_with("http://"),
            |v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"),
        );
        let object_store = AmazonS3Builder::new()
            .with_endpoint(endpoint)
            .with_region(region)
            .with_bucket_name(bucket.clone())
            .with_access_key_id(access_key)
            .with_secret_access_key(secret_key)
            .with_allow_http(allow_http)
            .with_virtual_hosted_style_request(false)
            .with_checksum_algorithm(Checksum::SHA256)
            .build()
            .map_err(|_| anyhow::anyhow!("object-store unavailable"))?;
        let object_store: Arc<dyn ObjectStore> = Arc::new(object_store);
        object_store
            .put(
                &ObjectPath::from("__mh_save_sync_readiness__"),
                Vec::new().into(),
            )
            .await
            .map_err(|_| anyhow::anyhow!("object-store unavailable"))?;
        Ok(Self {
            backend: Backend::Persistent(Arc::new(PersistentState { pool, object_store })),
        })
    }

    fn backend_name(&self) -> &'static str {
        match self.backend {
            Backend::Memory(_) => "memory",
            Backend::Persistent(_) => "postgres-s3",
        }
    }

    async fn readiness(&self) -> Result<(), BackendError> {
        match &self.backend {
            Backend::Memory(inner) => {
                let guard = inner.lock().unwrap();
                for (snapshot_key, row) in &guard.snapshots {
                    let account = guard
                        .snapshot_accounts
                        .get(snapshot_key)
                        .map(String::as_str)
                        .unwrap_or_default();
                    if !guard
                        .manifests
                        .contains_key(&scoped_key(account, &row.manifest_id))
                    {
                        return Err(BackendError::Unavailable("missing-manifest".into()));
                    }
                }
                Ok(())
            }
            Backend::Persistent(p) => {
                let mut readiness_tx = p
                    .pool
                    .begin()
                    .await
                    .map_err(|_| BackendError::Unavailable("database".into()))?;
                sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
                    .execute(&mut *readiness_tx)
                    .await
                    .map_err(|_| BackendError::Unavailable("database".into()))?;
                p.object_store
                    .head(&ObjectPath::from("__mh_save_sync_readiness__"))
                    .await
                    .map_err(|_| BackendError::Unavailable("object-store".into()))?;

                let mut after_storage_key: Option<String> = None;
                loop {
                    let rows = sqlx::query(READINESS_OBJECT_PAGE_SQL)
                        .bind(after_storage_key.as_deref())
                        .bind(READINESS_PAGE_SIZE)
                        .fetch_all(&mut *readiness_tx)
                        .await
                        .map_err(|_| BackendError::Unavailable("database".into()))?;
                    if rows.is_empty() {
                        break;
                    }
                    let storage_keys = rows
                        .iter()
                        .map(|row| row.get::<String, _>("storage_key"))
                        .collect::<Vec<_>>();
                    for key in &storage_keys {
                        p.object_store
                            .head(&ObjectPath::from(key.as_str()))
                            .await
                            .map_err(|error| match error {
                                object_store::Error::NotFound { .. } => {
                                    BackendError::Unavailable("missing-object".into())
                                }
                                _ => BackendError::Unavailable("object-store".into()),
                            })?;
                    }
                    after_storage_key = readiness_next_cursor(&storage_keys);
                }
                readiness_tx
                    .commit()
                    .await
                    .map_err(|_| BackendError::Unavailable("database".into()))?;
                Ok(())
            }
        }
    }

    pub async fn gc_orphans(
        &self,
        grace: Duration,
        dry_run: bool,
    ) -> anyhow::Result<OrphanGcReport> {
        let Backend::Persistent(persistent) = &self.backend else {
            anyhow::bail!("orphan GC requires the PostgreSQL + S3 backend");
        };
        collect_orphan_objects(persistent, grace, dry_run)
            .await
            .map_err(anyhow::Error::from)
    }
}

async fn collect_orphan_objects(
    persistent: &PersistentState,
    grace: Duration,
    dry_run: bool,
) -> Result<OrphanGcReport, BackendError> {
    let grace_seconds = grace.as_secs_f64();
    if !grace_seconds.is_finite() || grace_seconds < 1.0 {
        return Err(BackendError::Invalid(
            "orphan GC grace must be at least one second".into(),
        ));
    }

    if dry_run {
        let tracked = preview_tracked_orphans(persistent, grace_seconds).await?;
        let untracked = preview_untracked_orphans(persistent, grace).await?;
        return Ok(OrphanGcReport {
            eligible: tracked + untracked,
            deleted: 0,
            dry_run: true,
        });
    }

    mark_tracked_orphans(persistent, grace_seconds).await?;
    mark_untracked_orphans(persistent, grace).await?;

    let mut eligible = 0_u64;
    let mut deleted = 0_u64;
    while let Some(mark) = claim_orphan_mark(persistent).await? {
        eligible += 1;
        if sweep_claimed_orphan(persistent, &mark).await? {
            deleted += 1;
        }
    }
    Ok(OrphanGcReport {
        eligible,
        deleted,
        dry_run: false,
    })
}

async fn preview_tracked_orphans(
    persistent: &PersistentState,
    grace_seconds: f64,
) -> Result<u64, BackendError> {
    let count: i64 = sqlx::query_scalar(TRACKED_ORPHAN_COUNT_SQL)
        .bind(grace_seconds)
        .fetch_one(&persistent.pool)
        .await
        .map_err(db_unavailable)?;
    Ok(count as u64)
}

async fn mark_tracked_orphans(
    persistent: &PersistentState,
    grace_seconds: f64,
) -> Result<(), BackendError> {
    sqlx::query(TRACKED_ORPHAN_MARK_SQL)
        .bind(grace_seconds)
        .execute(&persistent.pool)
        .await
        .map_err(db_unavailable)?;
    Ok(())
}

fn parse_account_object_key(storage_key: &str) -> Option<(Vec<u8>, String)> {
    let parts = storage_key.split('/').collect::<Vec<_>>();
    if parts.len() != 4
        || parts[0] != "accounts"
        || !matches!(parts[2], "chunks" | "manifests" | "exports")
        || parts[3].is_empty()
    {
        return None;
    }
    let account = hex::decode(parts[1]).ok()?;
    (account.len() == 20).then(|| (account, parts[3].to_owned()))
}

fn object_is_older_than(meta: &object_store::ObjectMeta, grace: Duration) -> bool {
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    let modified = meta.last_modified.timestamp();
    modified >= 0 && now.as_secs().saturating_sub(modified as u64) >= grace.as_secs()
}

async fn preview_untracked_orphans(
    persistent: &PersistentState,
    grace: Duration,
) -> Result<u64, BackendError> {
    let mut objects = persistent
        .object_store
        .list(Some(&ObjectPath::from("accounts")));
    let mut count = 0_u64;
    while let Some(meta) = objects.try_next().await.map_err(object_store_unavailable)? {
        if !object_is_older_than(&meta, grace) {
            continue;
        }
        let storage_key = meta.location.as_ref();
        let Some((account, object_id)) = parse_account_object_key(storage_key) else {
            continue;
        };
        if !storage_metadata_or_roots_exist(&persistent.pool, &account, &object_id, storage_key)
            .await?
        {
            count += 1;
        }
    }
    Ok(count)
}

async fn mark_untracked_orphans(
    persistent: &PersistentState,
    grace: Duration,
) -> Result<(), BackendError> {
    let mut objects = persistent
        .object_store
        .list(Some(&ObjectPath::from("accounts")));
    while let Some(meta) = objects.try_next().await.map_err(object_store_unavailable)? {
        if !object_is_older_than(&meta, grace) {
            continue;
        }
        let storage_key = meta.location.as_ref();
        let Some((account, object_id)) = parse_account_object_key(storage_key) else {
            continue;
        };
        let mut tx = persistent.pool.begin().await.map_err(db_unavailable)?;
        lock_gc_object_tx(&mut tx, &account, &object_id).await?;
        if !storage_metadata_or_roots_exist_tx(&mut tx, &account, &object_id, storage_key).await? {
            sqlx::query(
                "INSERT INTO orphan_gc_marks(account_handle,object_id,storage_key) \
                 VALUES ($1,$2,$3) ON CONFLICT (account_handle,storage_key) DO NOTHING",
            )
            .bind(&account)
            .bind(&object_id)
            .bind(storage_key)
            .execute(&mut *tx)
            .await
            .map_err(db_unavailable)?;
        }
        tx.commit().await.map_err(db_unavailable)?;
    }
    Ok(())
}

#[derive(Debug)]
struct ClaimedOrphan {
    account: Vec<u8>,
    object_id: String,
    storage_key: String,
    lease_token: Uuid,
}

async fn claim_orphan_mark(
    persistent: &PersistentState,
) -> Result<Option<ClaimedOrphan>, BackendError> {
    let mut tx = persistent.pool.begin().await.map_err(db_unavailable)?;
    let row = sqlx::query(
        "SELECT account_handle,object_id,storage_key FROM orphan_gc_marks \
         WHERE lease_until IS NULL OR lease_until<now() \
         ORDER BY marked_at LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_unavailable)?;
    let Some(row) = row else {
        tx.rollback().await.map_err(db_unavailable)?;
        return Ok(None);
    };
    let account: Vec<u8> = row.get("account_handle");
    let object_id: String = row.get("object_id");
    let storage_key: String = row.get("storage_key");
    let lease_token = Uuid::new_v4();
    sqlx::query(
        "UPDATE orphan_gc_marks SET lease_token=$1,lease_until=now()+interval '5 minutes' \
         WHERE account_handle=$2 AND storage_key=$3",
    )
    .bind(lease_token)
    .bind(&account)
    .bind(&storage_key)
    .execute(&mut *tx)
    .await
    .map_err(db_unavailable)?;
    tx.commit().await.map_err(db_unavailable)?;
    Ok(Some(ClaimedOrphan {
        account,
        object_id,
        storage_key,
        lease_token,
    }))
}

async fn sweep_claimed_orphan(
    persistent: &PersistentState,
    mark: &ClaimedOrphan,
) -> Result<bool, BackendError> {
    let mut tx = persistent.pool.begin().await.map_err(db_unavailable)?;
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM orphan_gc_marks \
         WHERE account_handle=$1 AND storage_key=$2 AND lease_token=$3 AND lease_until>now())",
    )
    .bind(&mark.account)
    .bind(&mark.storage_key)
    .bind(mark.lease_token)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_unavailable)?;
    if !owned {
        tx.rollback().await.map_err(db_unavailable)?;
        return Ok(false);
    }
    lock_gc_object_tx(&mut tx, &mark.account, &mark.object_id).await?;
    if object_roots_exist_tx(&mut tx, &mark.account, &mark.object_id).await? {
        delete_orphan_mark_tx(&mut tx, mark).await?;
        tx.commit().await.map_err(db_unavailable)?;
        return Ok(false);
    }

    match persistent
        .object_store
        .delete(&ObjectPath::from(mark.storage_key.clone()))
        .await
    {
        Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
        Err(error) => return Err(object_store_unavailable(error)),
    }
    sqlx::query("DELETE FROM objects WHERE account_handle=$1 AND object_id=$2 AND storage_key=$3")
        .bind(&mark.account)
        .bind(&mark.object_id)
        .bind(&mark.storage_key)
        .execute(&mut *tx)
        .await
        .map_err(db_unavailable)?;
    delete_orphan_mark_tx(&mut tx, mark).await?;
    tx.commit().await.map_err(db_unavailable)?;
    Ok(true)
}

async fn delete_orphan_mark_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    mark: &ClaimedOrphan,
) -> Result<(), BackendError> {
    sqlx::query(
        "DELETE FROM orphan_gc_marks WHERE account_handle=$1 AND storage_key=$2 AND lease_token=$3",
    )
    .bind(&mark.account)
    .bind(&mark.storage_key)
    .bind(mark.lease_token)
    .execute(&mut **tx)
    .await
    .map_err(db_unavailable)?;
    Ok(())
}

async fn lock_gc_object_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account: &[u8],
    object_id: &str,
) -> Result<(), BackendError> {
    let lock_name = format!("{}:{object_id}", hex::encode(account));
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(lock_name)
        .execute(&mut **tx)
        .await
        .map_err(db_unavailable)?;
    Ok(())
}

async fn object_roots_exist_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account: &[u8],
    object_id: &str,
) -> Result<bool, BackendError> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM snapshot_objects WHERE account_handle=$1 AND object_id=$2) \
             OR EXISTS(SELECT 1 FROM snapshots WHERE account_handle=$1 AND encrypted_manifest_object=$2) \
             OR EXISTS(SELECT 1 FROM upload_sessions \
                       WHERE account_handle=$1 AND expires_at>now() \
                         AND (manifest_id=$2 OR required_chunks ? $2))",
    )
    .bind(account)
    .bind(object_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(db_unavailable)
}

async fn storage_metadata_or_roots_exist(
    pool: &PgPool,
    account: &[u8],
    object_id: &str,
    storage_key: &str,
) -> Result<bool, BackendError> {
    let mut tx = pool.begin().await.map_err(db_unavailable)?;
    let exists =
        storage_metadata_or_roots_exist_tx(&mut tx, account, object_id, storage_key).await?;
    tx.rollback().await.map_err(db_unavailable)?;
    Ok(exists)
}

async fn storage_metadata_or_roots_exist_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account: &[u8],
    object_id: &str,
    storage_key: &str,
) -> Result<bool, BackendError> {
    if object_roots_exist_tx(tx, account, object_id).await? {
        return Ok(true);
    }
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM objects WHERE storage_key=$1)")
        .bind(storage_key)
        .fetch_one(&mut **tx)
        .await
        .map_err(db_unavailable)
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/version", get(version))
        .route("/v1/accounts/bootstrap", post(account_bootstrap))
        .route("/v1/accounts/challenge", post(account_challenge))
        .route("/v1/devices/register", post(device_register))
        .route("/v1/devices/{cert_id}/revoke", post(device_revoke))
        .route("/v1/snapshots/begin", post(begin_snapshot))
        .route("/v1/snapshots/{upload_id}/chunks", post(put_chunk))
        .route("/v1/snapshots/{upload_id}/manifest", post(put_manifest))
        .route("/v1/snapshots/{upload_id}/commit", post(commit_snapshot))
        .route(
            "/v1/snapshots/{snapshot_id}/encrypted-bundle",
            get(get_encrypted_bundle),
        )
        .route("/v1/heads/{logical_save_id}", get(get_head))
        .route("/v1/history/{logical_save_id}", get(get_history))
        .route("/v1/conflicts/{logical_save_id}", get(get_conflicts))
        .route(
            "/v1/conflicts/{logical_save_id}/{snapshot_id}/resolve",
            post(resolve_conflict),
        )
        .layer(from_fn_with_state(
            state.clone(),
            authenticate_write_request,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn requires_resource_auth(method: &axum::http::Method, path: &str) -> bool {
    (method == axum::http::Method::POST
        && (path == "/v1/snapshots/begin"
            || (path.starts_with("/v1/snapshots/")
                && (path.ends_with("/chunks")
                    || path.ends_with("/manifest")
                    || path.ends_with("/commit")))
            || (path.starts_with("/v1/devices/") && path.ends_with("/revoke"))
            || (path.starts_with("/v1/conflicts/") && path.ends_with("/resolve"))))
        || (method == axum::http::Method::GET
            && (path.starts_with("/v1/heads/")
                || path.starts_with("/v1/history/")
                || path.starts_with("/v1/conflicts/")
                || (path.starts_with("/v1/snapshots/") && path.ends_with("/encrypted-bundle"))))
}

async fn authenticate_write_request(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(request.uri().path())
        .to_owned();
    if !requires_resource_auth(&method, request.uri().path()) {
        return next.run(request).await;
    }
    let headers = request.headers();
    let field = |name: &'static str| -> Result<String, ApiError> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
            .ok_or_else(|| (StatusCode::UNAUTHORIZED, format!("missing {name}")))
    };
    let parsed = (|| {
        let account = field(AUTH_ACCOUNT)?;
        let device = field(AUTH_DEVICE)?;
        validate_hex(&account, 20)?;
        validate_hex(&device, 16)?;
        let challenge = field(AUTH_CHALLENGE)?;
        let challenge_id = Uuid::parse_str(&challenge)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid challenge id".into()))?;
        let nonce_b64 = field(AUTH_NONCE)?;
        let nonce = decode_b64(&nonce_b64)?;
        let timestamp = field(AUTH_TIMESTAMP)?
            .parse::<u64>()
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid timestamp".into()))?;
        let signature = decode_b64_len(&field(AUTH_SIGNATURE)?, 64)?;
        Ok::<_, ApiError>((
            account,
            device,
            challenge_id,
            nonce_b64,
            nonce,
            timestamp,
            signature,
        ))
    })();
    let (account, device, challenge_id, nonce_b64, nonce, timestamp, signature) = match parsed {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let (parts, incoming_body) = request.into_parts();
    let body = match to_bytes(incoming_body, 128 * 1024 * 1024).await {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid request body").into_response(),
    };
    let message = canonical_http_request(
        method.as_str(),
        &path,
        &sha256_hex(&body),
        &challenge_id.to_string(),
        &nonce_b64,
        timestamp,
    );
    if let Err(error) = verify_and_consume_challenge(
        &state,
        ChallengeProof {
            account_hex: &account,
            device_hex: &device,
            challenge_id,
            nonce: &nonce,
            timestamp,
            signature_bytes: &signature,
            message: &message,
        },
    )
    .await
    {
        return error.into_response();
    }
    request = Request::from_parts(parts, Body::from(body));
    request.extensions_mut().insert(AuthContext {
        account_handle: account,
        device_cert_id: device,
    });
    next.run(request).await
}

struct ChallengeProof<'a> {
    account_hex: &'a str,
    device_hex: &'a str,
    challenge_id: Uuid,
    nonce: &'a [u8],
    timestamp: u64,
    signature_bytes: &'a [u8],
    message: &'a [u8],
}

async fn verify_and_consume_challenge(
    state: &AppState,
    proof: ChallengeProof<'_>,
) -> Result<(), ApiError> {
    let ChallengeProof {
        account_hex,
        device_hex,
        challenge_id,
        nonce,
        timestamp,
        signature_bytes,
        message,
    } = proof;
    let now = unix_seconds();
    if timestamp.abs_diff(now) > 300 {
        return Err((StatusCode::UNAUTHORIZED, "request timestamp expired".into()));
    }
    let account = hex::decode(account_hex).map_err(invalid_hex)?;
    let device = hex::decode(device_hex).map_err(invalid_hex)?;
    let signature = Signature::from_slice(signature_bytes)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid signature".into()))?;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let challenge = guard
                .challenges
                .get(&challenge_id)
                .filter(|c| {
                    c.account_handle == account_hex
                        && c.device_cert_id == device_hex
                        && c.nonce == nonce
                        && !c.used
                        && c.expires > now
                })
                .cloned()
                .ok_or_else(|| {
                    (
                        StatusCode::UNAUTHORIZED,
                        "challenge invalid, expired, or already used".into(),
                    )
                })?;
            let device_row = guard
                .devices
                .get(device_hex)
                .filter(|d| d.account_handle == account_hex && !d.revoked)
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "active device not found".into()))?;
            let key = VerifyingKey::from_bytes(
                device_row
                    .public_key
                    .as_slice()
                    .try_into()
                    .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid device key".into()))?,
            )
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid device key".into()))?;
            key.verify(message, &signature).map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    "signature verification failed".into(),
                )
            })?;
            guard
                .challenges
                .get_mut(&challenge_id)
                .expect("challenge locked")
                .used = true;
            let _ = challenge;
            Ok(())
        }
        Backend::Persistent(p) => {
            let mut tx = p
                .pool
                .begin()
                .await
                .map_err(db_unavailable)
                .map_err(BackendError::api)?;
            let row = sqlx::query("SELECT d.device_public_key FROM auth_challenges c JOIN devices d ON d.cert_id=c.device_cert_id AND d.account_handle=c.account_handle WHERE c.id=$1 AND c.account_handle=$2 AND c.device_cert_id=$3 AND c.nonce=$4 AND c.used_at IS NULL AND c.expires_at>now() AND d.revoked_at IS NULL FOR UPDATE OF c")
                .bind(challenge_id).bind(&account).bind(&device).bind(nonce)
                .fetch_optional(&mut *tx).await.map_err(db_unavailable).map_err(BackendError::api)?
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "challenge invalid, expired, or already used".into()))?;
            let public: Vec<u8> = row.get("device_public_key");
            let key = VerifyingKey::from_bytes(
                public
                    .as_slice()
                    .try_into()
                    .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid device key".into()))?,
            )
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid device key".into()))?;
            key.verify(message, &signature).map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    "signature verification failed".into(),
                )
            })?;
            let affected = sqlx::query(
                "UPDATE auth_challenges SET used_at=now() WHERE id=$1 AND used_at IS NULL",
            )
            .bind(challenge_id)
            .execute(&mut *tx)
            .await
            .map_err(db_unavailable)
            .map_err(BackendError::api)?
            .rows_affected();
            if affected != 1 {
                return Err((StatusCode::UNAUTHORIZED, "challenge already used".into()));
            }
            tx.commit()
                .await
                .map_err(db_unavailable)
                .map_err(BackendError::api)?;
            Ok(())
        }
    }
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        backend: state.backend_name().into(),
    })
}

async fn ready(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<HealthResponse>)> {
    if let Err(error) = state.readiness().await {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: error.to_string(),
                version: env!("CARGO_PKG_VERSION").into(),
                backend: state.backend_name().into(),
            }),
        ));
    }
    Ok(Json(HealthResponse {
        status: "ready".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        backend: state.backend_name().into(),
    }))
}

async fn version(State(state): State<AppState>) -> Json<HealthResponse> {
    health(State(state)).await
}

async fn account_bootstrap(
    State(state): State<AppState>,
    Json(req): Json<AccountBootstrapRequest>,
) -> Result<StatusCode, ApiError> {
    validate_hex(&req.account_handle, 20)?;
    let root = decode_b64_len(&req.root_public_key_b64, 32)?;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            if let Some(existing) = guard.accounts.get(&req.account_handle)
                && existing != &root
            {
                return Err(BackendError::Conflict("account root key mismatch".into()).api());
            }
            guard.accounts.insert(req.account_handle, root);
        }
        Backend::Persistent(p) => {
            sqlx::query("INSERT INTO accounts(account_handle,root_public_key) VALUES ($1,$2) ON CONFLICT (account_handle) DO NOTHING")
                .bind(hex::decode(&req.account_handle).map_err(invalid_hex)?)
                .bind(&root)
                .execute(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
            let existing: Vec<u8> =
                sqlx::query_scalar("SELECT root_public_key FROM accounts WHERE account_handle=$1")
                    .bind(hex::decode(req.account_handle).map_err(invalid_hex)?)
                    .fetch_one(&p.pool)
                    .await
                    .map_err(db_unavailable)
                    .map_err(BackendError::api)?;
            if existing != root {
                return Err(BackendError::Conflict("account root key mismatch".into()).api());
            }
        }
    }
    Ok(StatusCode::CREATED)
}

async fn account_challenge(
    State(state): State<AppState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    validate_hex(&req.account_handle, 20)?;
    validate_hex(&req.device_cert_id, 16)?;
    let challenge_id = Uuid::new_v4();
    let nonce = Uuid::new_v4().as_bytes().to_vec();
    let expires = unix_seconds() + 300;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let active = guard
                .devices
                .get(&req.device_cert_id)
                .is_some_and(|d| d.account_handle == req.account_handle && !d.revoked);
            if !active {
                return Err(BackendError::NotFound("active device".into()).api());
            }
            guard.challenges.insert(
                challenge_id,
                MemoryChallenge {
                    account_handle: req.account_handle.clone(),
                    device_cert_id: req.device_cert_id.clone(),
                    nonce: nonce.clone(),
                    expires,
                    used: false,
                },
            );
        }
        Backend::Persistent(p) => {
            let account = hex::decode(&req.account_handle).map_err(invalid_hex)?;
            let cert = hex::decode(&req.device_cert_id).map_err(invalid_hex)?;
            let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE account_handle=$1 AND cert_id=$2 AND revoked_at IS NULL)")
            .bind(account.clone()).bind(cert.clone()).fetch_one(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
            if !active {
                return Err(BackendError::NotFound("active device".into()).api());
            }
            sqlx::query("INSERT INTO auth_challenges(id,account_handle,device_cert_id,nonce,expires_at) VALUES ($1,$2,$3,$4,to_timestamp($5))")
            .bind(challenge_id).bind(account).bind(cert).bind(&nonce).bind(expires as i64)
            .execute(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
        }
    }
    Ok(Json(ChallengeResponse {
        challenge_id,
        nonce_b64: base64::engine::general_purpose::STANDARD.encode(nonce),
        expires_unix_seconds: expires,
    }))
}

async fn device_register(
    State(state): State<AppState>,
    Json(req): Json<DeviceRegisterRequest>,
) -> Result<StatusCode, ApiError> {
    validate_hex(&req.account_handle, 20)?;
    validate_hex(&req.cert_id, 16)?;
    let public_key = decode_b64_len(&req.device_public_key_b64, 32)?;
    let certificate = decode_b64(&req.certificate_b64)?;
    if certificate.len() > 16 * 1024 {
        return Err(BackendError::Invalid("certificate too large".into()).api());
    }
    let parsed: DeviceCertificate = ciborium::de::from_reader(certificate.as_slice())
        .map_err(|_| BackendError::Invalid("invalid device certificate encoding".into()).api())?;
    verify_device_certificate(&parsed)
        .map_err(|_| BackendError::Invalid("invalid device certificate signature".into()).api())?;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let root = guard
                .accounts
                .get(&req.account_handle)
                .ok_or_else(|| BackendError::NotFound("account".into()).api())?;
            validate_device_certificate(&parsed, root, &req.cert_id, &public_key, unix_seconds())?;
            guard.devices.insert(
                req.cert_id,
                MemoryDevice {
                    account_handle: req.account_handle,
                    public_key,
                    revoked: false,
                },
            );
        }
        Backend::Persistent(p) => {
            let account = hex::decode(&req.account_handle).map_err(invalid_hex)?;
            let cert_id = hex::decode(&req.cert_id).map_err(invalid_hex)?;
            let root: Vec<u8> =
                sqlx::query_scalar("SELECT root_public_key FROM accounts WHERE account_handle=$1")
                    .bind(&account)
                    .fetch_optional(&p.pool)
                    .await
                    .map_err(db_unavailable)
                    .map_err(BackendError::api)?
                    .ok_or_else(|| BackendError::NotFound("account".into()).api())?;
            validate_device_certificate(&parsed, &root, &req.cert_id, &public_key, unix_seconds())?;
            sqlx::query("INSERT INTO devices(cert_id,account_handle,device_public_key,certificate) VALUES ($1,$2,$3,$4) ON CONFLICT (cert_id) DO NOTHING")
                .bind(&cert_id)
                .bind(&account)
                .bind(&public_key).bind(&certificate)
                .execute(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
            let existing = sqlx::query("SELECT account_handle,device_public_key,certificate,revoked_at IS NOT NULL AS revoked FROM devices WHERE cert_id=$1")
                .bind(cert_id).fetch_one(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
            if !device_registration_matches_existing_identity(
                &existing.get::<Vec<u8>, _>("account_handle"),
                &existing.get::<Vec<u8>, _>("device_public_key"),
                existing.get::<bool, _>("revoked"),
                &account,
                &public_key,
            ) {
                return Err(BackendError::Conflict(
                    "device certificate mismatch or revoked".into(),
                )
                .api());
            }
        }
    }
    Ok(StatusCode::CREATED)
}

async fn device_revoke(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(cert_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    validate_hex(&cert_id, 16)?;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let device = guard
                .devices
                .get_mut(&cert_id)
                .filter(|d| d.account_handle == auth.account_handle)
                .ok_or_else(|| BackendError::NotFound("device".into()).api())?;
            device.revoked = true;
        }
        Backend::Persistent(p) => {
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let result = sqlx::query(
                "UPDATE devices SET revoked_at=now() WHERE cert_id=$1 AND account_handle=$2",
            )
            .bind(hex::decode(cert_id).map_err(invalid_hex)?)
            .bind(hex::decode(auth.account_handle).map_err(invalid_hex)?)
            .execute(&p.pool)
            .await
            .map_err(db_unavailable)
            .map_err(BackendError::api)?;
            if result.rows_affected() == 0 {
                return Err(BackendError::NotFound("device".into()).api());
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn begin_snapshot(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Json(req): Json<BeginSnapshotRequest>,
) -> Result<Json<BeginSnapshotResponse>, ApiError> {
    validate_object_id(&req.encrypted_manifest_id)?;
    for id in &req.chunk_ids {
        validate_object_id(id)?;
    }
    validate_parent_shape(req.base_head.as_ref(), &req.parents)?;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let account = req
                .account_handle
                .as_ref()
                .ok_or_else(|| BackendError::Invalid("account_handle required".into()).api())?;
            let device = req
                .device_cert_id
                .as_ref()
                .ok_or_else(|| BackendError::Invalid("device_cert_id required".into()).api())?;
            if account != &auth.account_handle || device != &auth.device_cert_id {
                return Err((
                    StatusCode::FORBIDDEN,
                    "authenticated identity mismatch".into(),
                ));
            }
            validate_parent_set_memory(
                &guard,
                account,
                &req.logical_save_id,
                req.base_head.as_ref(),
                &req.parents,
            )?;
            let upload_id = Uuid::new_v4();
            let missing = req
                .chunk_ids
                .iter()
                .filter(|id| !guard.chunks.contains_key(&scoped_key(account, id)))
                .cloned()
                .collect();
            guard.uploads.insert(
                upload_id,
                UploadSession {
                    account_handle: req.account_handle,
                    device_cert_id: req.device_cert_id,
                    logical_save_id: req.logical_save_id,
                    base_head: req.base_head,
                    parents: req.parents,
                    required_chunks: req.chunk_ids.into_iter().collect(),
                    uploaded_chunks: BTreeSet::new(),
                    manifest_id: Some(req.encrypted_manifest_id),
                },
            );
            Ok(Json(BeginSnapshotResponse {
                upload_id,
                missing_chunk_ids: missing,
            }))
        }
        Backend::Persistent(p) => {
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let account_hex = req
                .account_handle
                .ok_or_else(|| BackendError::Invalid("account_handle required".into()).api())?;
            let cert_hex = req
                .device_cert_id
                .ok_or_else(|| BackendError::Invalid("device_cert_id required".into()).api())?;
            validate_hex(&account_hex, 20)?;
            validate_hex(&cert_hex, 16)?;
            if account_hex != auth.account_handle || cert_hex != auth.device_cert_id {
                return Err((
                    StatusCode::FORBIDDEN,
                    "authenticated identity mismatch".into(),
                ));
            }
            let account = hex::decode(&account_hex).map_err(invalid_hex)?;
            let cert = hex::decode(&cert_hex).map_err(invalid_hex)?;
            let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE account_handle=$1 AND cert_id=$2 AND revoked_at IS NULL)")
                .bind(&account).bind(&cert).fetch_one(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
            if !active {
                return Err(BackendError::NotFound("active device".into()).api());
            }
            sqlx::query("INSERT INTO logical_saves(id,account_handle,encrypted_label) VALUES ($1,$2,$3) ON CONFLICT (account_handle,id) DO NOTHING")
                .bind(&req.logical_save_id).bind(&account).bind(Vec::<u8>::new())
                .execute(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
            let owner: Vec<u8> = sqlx::query_scalar(
                "SELECT account_handle FROM logical_saves WHERE id=$1 AND account_handle=$2",
            )
            .bind(&req.logical_save_id)
            .bind(&account)
            .fetch_one(&p.pool)
            .await
            .map_err(db_unavailable)
            .map_err(BackendError::api)?;
            if owner != account {
                return Err((
                    StatusCode::FORBIDDEN,
                    "logical save belongs to another account".into(),
                ));
            }
            validate_parent_set_persistent(
                p,
                &account,
                &req.logical_save_id,
                req.base_head.as_ref(),
                &req.parents,
            )
            .await?;
            let mut tx = p
                .pool
                .begin()
                .await
                .map_err(db_unavailable)
                .map_err(BackendError::api)?;
            let lock_ids = std::iter::once(req.encrypted_manifest_id.as_str())
                .chain(req.chunk_ids.iter().map(String::as_str))
                .collect::<BTreeSet<_>>();
            for id in lock_ids {
                lock_gc_object_tx(&mut tx, &account, id)
                    .await
                    .map_err(BackendError::api)?;
            }
            let mut missing = Vec::new();
            for id in &req.chunk_ids {
                if !persistent_object_exists_tx(p, &mut tx, &account, id)
                    .await
                    .map_err(BackendError::api)?
                {
                    missing.push(id.clone());
                }
            }
            let upload_id = Uuid::new_v4();
            let parents =
                serde_json::to_value(req.parents.iter().map(|x| x.0.clone()).collect::<Vec<_>>())
                    .unwrap();
            let chunks = serde_json::to_value(&req.chunk_ids).unwrap();
            sqlx::query("INSERT INTO upload_sessions(id,account_handle,device_cert_id,logical_save_id,base_head,parents,required_chunks,manifest_id,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now()+interval '24 hours')")
                .bind(upload_id).bind(&account).bind(cert).bind(req.logical_save_id)
                .bind(req.base_head.map(|x| x.0)).bind(parents).bind(chunks).bind(req.encrypted_manifest_id)
                .execute(&mut *tx).await.map_err(db_unavailable).map_err(BackendError::api)?;
            tx.commit()
                .await
                .map_err(db_unavailable)
                .map_err(BackendError::api)?;
            Ok(Json(BeginSnapshotResponse {
                upload_id,
                missing_chunk_ids: missing,
            }))
        }
    }
}

async fn put_chunk(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(upload_id): Path<Uuid>,
    Json(req): Json<PutChunkRequest>,
) -> Result<StatusCode, ApiError> {
    validate_object_id(&req.chunk_id)?;
    let bytes = decode_and_verify(&req.bytes_b64, &req.sha256)?;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let session = guard
                .uploads
                .get(&upload_id)
                .ok_or_else(|| BackendError::NotFound("upload".into()).api())?;
            if session.account_handle.as_deref() != Some(&auth.account_handle)
                || session.device_cert_id.as_deref() != Some(&auth.device_cert_id)
            {
                return Err((StatusCode::NOT_FOUND, "upload not found".into()));
            }
            let object_key = scoped_key(&auth.account_handle, &req.chunk_id);
            if let Some(existing) = guard.chunks.get(&object_key)
                && sha256_hex(existing) != req.sha256.to_lowercase()
            {
                return Err(BackendError::Conflict(
                    "object id reused with different checksum".into(),
                )
                .api());
            }
            guard.chunks.entry(object_key).or_insert(bytes);
            guard
                .uploads
                .get_mut(&upload_id)
                .unwrap()
                .uploaded_chunks
                .insert(req.chunk_id);
        }
        Backend::Persistent(p) => {
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let row = sqlx::query("SELECT encode(account_handle,'hex') AS account_handle, encode(device_cert_id,'hex') AS device_cert_id, required_chunks FROM upload_sessions WHERE id=$1 AND account_handle=$2 AND device_cert_id=$3 AND expires_at>now()")
                .bind(upload_id).bind(hex::decode(&auth.account_handle).map_err(invalid_hex)?).bind(hex::decode(&auth.device_cert_id).map_err(invalid_hex)?).fetch_optional(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?
                .ok_or_else(|| BackendError::NotFound("upload".into()).api())?;
            let account: String = row.get("account_handle");
            let required: serde_json::Value = row.get("required_chunks");
            if !json_string_array(&required)
                .iter()
                .any(|id| id == &req.chunk_id)
            {
                return Err(BackendError::Invalid("chunk not declared in upload".into()).api());
            }
            persistent_put_object(p, &account, &req.chunk_id, "chunk", &req.sha256, bytes).await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn put_manifest(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(upload_id): Path<Uuid>,
    Json(req): Json<PutManifestRequest>,
) -> Result<StatusCode, ApiError> {
    validate_object_id(&req.manifest_id)?;
    let bytes = decode_and_verify(&req.bytes_b64, &req.sha256)?;
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let session = guard
                .uploads
                .get_mut(&upload_id)
                .ok_or_else(|| BackendError::NotFound("upload".into()).api())?;
            if session.account_handle.as_deref() != Some(&auth.account_handle)
                || session.device_cert_id.as_deref() != Some(&auth.device_cert_id)
            {
                return Err((StatusCode::NOT_FOUND, "upload not found".into()));
            }
            if session.manifest_id.as_deref() != Some(&req.manifest_id) {
                return Err(BackendError::Invalid("manifest id mismatch".into()).api());
            }
            let object_key = scoped_key(&auth.account_handle, &req.manifest_id);
            if let Some(existing) = guard.manifests.get(&object_key)
                && sha256_hex(existing) != req.sha256.to_lowercase()
            {
                return Err(BackendError::Conflict(
                    "manifest id reused with different checksum".into(),
                )
                .api());
            }
            guard.manifests.entry(object_key).or_insert(bytes);
        }
        Backend::Persistent(p) => {
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            let row = sqlx::query("SELECT encode(account_handle,'hex') AS account_handle, manifest_id FROM upload_sessions WHERE id=$1 AND account_handle=$2 AND device_cert_id=$3 AND expires_at>now()")
                .bind(upload_id).bind(hex::decode(&auth.account_handle).map_err(invalid_hex)?).bind(hex::decode(&auth.device_cert_id).map_err(invalid_hex)?).fetch_optional(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?
                .ok_or_else(|| BackendError::NotFound("upload".into()).api())?;
            let account: String = row.get("account_handle");
            let expected: String = row.get("manifest_id");
            if expected != req.manifest_id {
                return Err(BackendError::Invalid("manifest id mismatch".into()).api());
            }
            persistent_put_object(
                p,
                &account,
                &req.manifest_id,
                "manifest",
                &req.sha256,
                bytes,
            )
            .await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn commit_snapshot(
    State(state): State<AppState>,
    auth: Option<Extension<AuthContext>>,
    Path(upload_id): Path<Uuid>,
    Json(req): Json<CommitSnapshotRequest>,
) -> Result<Json<CommitSnapshotResponse>, ApiError> {
    validate_object_id(&req.snapshot_id.0)?;
    match &state.backend {
        Backend::Memory(inner) => {
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            commit_memory(inner, upload_id, req, &auth)
        }
        .map(Json)
        .map_err(BackendError::api),
        Backend::Persistent(p) => {
            let auth = auth
                .ok_or_else(|| (StatusCode::UNAUTHORIZED, "authentication required".into()))?
                .0;
            commit_persistent(p, upload_id, req, &auth)
                .await
                .map(Json)
                .map_err(BackendError::api)
        }
    }
}

fn commit_memory(
    inner: &Arc<Mutex<InMemoryState>>,
    upload_id: Uuid,
    req: CommitSnapshotRequest,
    auth: &AuthContext,
) -> Result<CommitSnapshotResponse, BackendError> {
    let mut guard = inner.lock().unwrap();
    let session = guard
        .uploads
        .remove(&upload_id)
        .ok_or_else(|| BackendError::NotFound("upload".into()))?;
    if session.account_handle.as_deref() != Some(&auth.account_handle)
        || session.device_cert_id.as_deref() != Some(&auth.device_cert_id)
    {
        return Err(BackendError::NotFound("upload".into()));
    }
    for chunk in &session.required_chunks {
        if !guard
            .chunks
            .contains_key(&scoped_key(&auth.account_handle, chunk))
            && !session.uploaded_chunks.contains(chunk)
        {
            return Err(BackendError::Conflict(format!("missing chunk {chunk}")));
        }
    }
    let manifest_id = session
        .manifest_id
        .ok_or_else(|| BackendError::Conflict("missing manifest".into()))?;
    if !guard
        .manifests
        .contains_key(&scoped_key(&auth.account_handle, &manifest_id))
    {
        return Err(BackendError::Conflict("manifest not durable".into()));
    }
    let account = session.account_handle.as_deref().unwrap_or_default();
    let logical_key = scoped_key(account, &session.logical_save_id);
    let snapshot_key = scoped_key(account, &req.snapshot_id.0);
    let current = guard.heads.get(&logical_key).cloned();
    let (kind, head, conflict) = cas_outcome(&session.base_head, &current, &req.snapshot_id);
    guard.snapshots.insert(
        snapshot_key.clone(),
        SnapshotRow {
            snapshot_id: req.snapshot_id.clone(),
            logical_save_id: session.logical_save_id.clone(),
            parents: session.parents,
            manifest_id,
            conflict,
            resolved: false,
        },
    );
    guard
        .snapshot_accounts
        .insert(snapshot_key.clone(), auth.account_handle.clone());
    guard.snapshot_chunks.insert(
        snapshot_key,
        session.required_chunks.iter().cloned().collect(),
    );
    if !conflict {
        guard.heads.insert(logical_key, req.snapshot_id.clone());
    }
    Ok(CommitSnapshotResponse {
        outcome: kind,
        head,
        conflict_snapshot: conflict.then_some(req.snapshot_id),
    })
}

async fn commit_persistent(
    p: &PersistentState,
    upload_id: Uuid,
    req: CommitSnapshotRequest,
    auth: &AuthContext,
) -> Result<CommitSnapshotResponse, BackendError> {
    commit_persistent_with_failpoint(p, upload_id, req, auth, configured_commit_failpoint()).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitFailpoint {
    Disabled,
    #[cfg(test)]
    BeforeDatabaseCommit,
    #[cfg(test)]
    AfterDatabaseCommit,
}

fn configured_commit_failpoint() -> CommitFailpoint {
    CommitFailpoint::Disabled
}

async fn commit_persistent_with_failpoint(
    p: &PersistentState,
    upload_id: Uuid,
    req: CommitSnapshotRequest,
    auth: &AuthContext,
    failpoint: CommitFailpoint,
) -> Result<CommitSnapshotResponse, BackendError> {
    let auth_account = hex::decode(&auth.account_handle)
        .map_err(|_| BackendError::Invalid("invalid auth account".into()))?;
    let auth_device = hex::decode(&auth.device_cert_id)
        .map_err(|_| BackendError::Invalid("invalid auth device".into()))?;
    let pre = sqlx::query("SELECT encode(account_handle,'hex') AS account_hex, logical_save_id, manifest_id, required_chunks FROM upload_sessions WHERE id=$1 AND account_handle=$2 AND device_cert_id=$3 AND expires_at>now()")
        .bind(upload_id).bind(&auth_account).bind(&auth_device).fetch_optional(&p.pool).await.map_err(db_unavailable)?
        .ok_or_else(|| BackendError::NotFound("upload".into()))?;
    let account_hex: String = pre.get("account_hex");
    let manifest_id: String = pre.get("manifest_id");
    let chunks = json_string_array(&pre.get::<serde_json::Value, _>("required_chunks"));
    if !persistent_object_exists(p, &account_hex, &manifest_id).await? {
        return Err(BackendError::Conflict("manifest not durable".into()));
    }
    for chunk in &chunks {
        if !persistent_object_exists(p, &account_hex, chunk).await? {
            return Err(BackendError::Conflict(format!("missing chunk {chunk}")));
        }
    }

    let mut tx = p.pool.begin().await.map_err(db_unavailable)?;
    let row = sqlx::query("SELECT account_handle,device_cert_id,logical_save_id,base_head,parents,required_chunks,manifest_id FROM upload_sessions WHERE id=$1 AND account_handle=$2 AND device_cert_id=$3 AND expires_at>now() FOR UPDATE")
        .bind(upload_id).bind(&auth_account).bind(&auth_device).fetch_optional(&mut *tx).await.map_err(db_unavailable)?
        .ok_or_else(|| BackendError::NotFound("upload".into()))?;
    let account: Vec<u8> = row.get("account_handle");
    let device: Vec<u8> = row.get("device_cert_id");
    let logical_save_id: String = row.get("logical_save_id");
    let base_head: Option<String> = row.get("base_head");
    let parents = json_string_array(&row.get::<serde_json::Value, _>("parents"));
    validate_parent_set_tx(
        &mut tx,
        &account,
        &logical_save_id,
        base_head.as_deref(),
        &parents,
    )
    .await?;
    let current: Option<String> = sqlx::query_scalar(
        "SELECT head_snapshot_id FROM logical_saves WHERE id=$1 AND account_handle=$2 FOR UPDATE",
    )
    .bind(&logical_save_id)
    .bind(&account)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_unavailable)?;
    let base = base_head.as_ref().map(|x| SnapshotId(x.clone()));
    let current_id = current.as_ref().map(|x| SnapshotId(x.clone()));
    let (kind, head, conflict) = cas_outcome(&base, &current_id, &req.snapshot_id);
    sqlx::query("INSERT INTO snapshots(id,account_handle,logical_save_id,encrypted_manifest_object,committing_device_cert_id,conflict) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(&req.snapshot_id.0).bind(&account).bind(&logical_save_id).bind(&manifest_id).bind(device).bind(conflict)
        .execute(&mut *tx).await.map_err(db_unavailable)?;
    for parent in &parents {
        sqlx::query("INSERT INTO snapshot_parents(account_handle,snapshot_id,parent_snapshot_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(&account).bind(&req.snapshot_id.0).bind(parent).execute(&mut *tx).await.map_err(db_unavailable)?;
    }
    for object_id in std::iter::once(&manifest_id).chain(chunks.iter()) {
        sqlx::query("INSERT INTO snapshot_objects(account_handle,snapshot_id,object_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(&account).bind(&req.snapshot_id.0).bind(object_id).execute(&mut *tx).await.map_err(db_unavailable)?;
    }
    if !conflict {
        let affected = match base_head {
            Some(base) => sqlx::query("UPDATE logical_saves SET head_snapshot_id=$1,updated_at=now() WHERE id=$2 AND account_handle=$3 AND head_snapshot_id=$4")
                .bind(&req.snapshot_id.0).bind(&logical_save_id).bind(&account).bind(base).execute(&mut *tx).await.map_err(db_unavailable)?.rows_affected(),
            None => sqlx::query("UPDATE logical_saves SET head_snapshot_id=$1,updated_at=now() WHERE id=$2 AND account_handle=$3 AND head_snapshot_id IS NULL")
                .bind(&req.snapshot_id.0).bind(&logical_save_id).bind(&account).execute(&mut *tx).await.map_err(db_unavailable)?.rows_affected(),
        };
        if affected != 1 {
            return Err(BackendError::Conflict(
                "HEAD changed during transaction".into(),
            ));
        }
    }
    sqlx::query("DELETE FROM upload_sessions WHERE id=$1")
        .bind(upload_id)
        .execute(&mut *tx)
        .await
        .map_err(db_unavailable)?;
    #[cfg(test)]
    if failpoint == CommitFailpoint::BeforeDatabaseCommit {
        return Err(BackendError::Unavailable(
            "test failpoint before database commit".into(),
        ));
    }
    tx.commit().await.map_err(db_unavailable)?;
    #[cfg(test)]
    if failpoint == CommitFailpoint::AfterDatabaseCommit {
        return Err(BackendError::Unavailable(
            "test failpoint after database commit".into(),
        ));
    }
    #[cfg(not(test))]
    let _ = failpoint;
    Ok(CommitSnapshotResponse {
        outcome: kind,
        head,
        conflict_snapshot: conflict.then_some(req.snapshot_id),
    })
}

async fn get_encrypted_bundle(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(snapshot_id): Path<String>,
) -> Result<Json<SnapshotDownloadResponse>, ApiError> {
    validate_object_id(&snapshot_id)?;
    match &state.backend {
        Backend::Memory(inner) => {
            let guard = inner.lock().unwrap();
            let snapshot_key = scoped_key(&auth.account_handle, &snapshot_id);
            let row = guard
                .snapshots
                .get(&snapshot_key)
                .ok_or_else(|| BackendError::NotFound("snapshot".into()).api())?;
            let manifest_bytes = guard
                .manifests
                .get(&scoped_key(&auth.account_handle, &row.manifest_id))
                .ok_or_else(|| BackendError::Unavailable("missing manifest object".into()).api())?;
            let mut chunks = Vec::new();
            for chunk_id in guard
                .snapshot_chunks
                .get(&snapshot_key)
                .cloned()
                .unwrap_or_default()
            {
                let bytes = guard
                    .chunks
                    .get(&scoped_key(&auth.account_handle, &chunk_id))
                    .ok_or_else(|| {
                        BackendError::Unavailable(format!("missing chunk {chunk_id}")).api()
                    })?;
                chunks.push(object_response(&chunk_id, bytes));
            }
            Ok(Json(SnapshotDownloadResponse {
                snapshot_id: row.snapshot_id.clone(),
                encrypted_manifest: object_response(&row.manifest_id, manifest_bytes),
                chunks,
            }))
        }
        Backend::Persistent(p) => {
            let account = hex::decode(&auth.account_handle).map_err(invalid_hex)?;
            let manifest_id: String = sqlx::query_scalar(
                "SELECT encrypted_manifest_object FROM snapshots WHERE id=$1 AND account_handle=$2",
            )
            .bind(&snapshot_id)
            .bind(&account)
            .fetch_optional(&p.pool)
            .await
            .map_err(db_unavailable)
            .map_err(BackendError::api)?
            .ok_or_else(|| BackendError::NotFound("snapshot".into()).api())?;
            let manifest = persistent_get_snapshot_object(
                p,
                &auth.account_handle,
                &snapshot_id,
                &manifest_id,
                "manifest",
            )
            .await
            .map_err(BackendError::api)?;
            let chunk_ids: Vec<String> = sqlx::query_scalar(
                "SELECT so.object_id FROM snapshot_objects so JOIN objects o ON o.account_handle=so.account_handle AND o.object_id=so.object_id WHERE so.snapshot_id=$1 AND so.account_handle=$2 AND o.object_kind='chunk' ORDER BY so.object_id",
            )
            .bind(&snapshot_id)
            .bind(&account)
            .fetch_all(&p.pool)
            .await
            .map_err(db_unavailable)
            .map_err(BackendError::api)?;
            let mut chunks = Vec::new();
            for chunk_id in chunk_ids {
                chunks.push(
                    persistent_get_snapshot_object(
                        p,
                        &auth.account_handle,
                        &snapshot_id,
                        &chunk_id,
                        "chunk",
                    )
                    .await
                    .map_err(BackendError::api)?,
                );
            }
            Ok(Json(SnapshotDownloadResponse {
                snapshot_id: SnapshotId(snapshot_id),
                encrypted_manifest: manifest,
                chunks,
            }))
        }
    }
}

async fn get_head(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(logical_save_id): Path<String>,
) -> Result<Json<SnapshotId>, StatusCode> {
    match &state.backend {
        Backend::Memory(inner) => {
            let guard = inner.lock().unwrap();
            guard
                .heads
                .get(&scoped_key(&auth.account_handle, &logical_save_id))
                .cloned()
                .map(Json)
                .ok_or(StatusCode::NOT_FOUND)
        }
        Backend::Persistent(p) => {
            let account =
                hex::decode(&auth.account_handle).map_err(|_| StatusCode::UNAUTHORIZED)?;
            let head: Option<String> = sqlx::query_scalar(
                "SELECT head_snapshot_id FROM logical_saves WHERE id=$1 AND account_handle=$2",
            )
            .bind(logical_save_id)
            .bind(account)
            .fetch_optional(&p.pool)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .flatten();
            head.map(|x| Json(SnapshotId(x)))
                .ok_or(StatusCode::NOT_FOUND)
        }
    }
}

async fn get_history(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(logical_save_id): Path<String>,
) -> Result<Json<Vec<SnapshotRow>>, StatusCode> {
    history(&state, &auth.account_handle, &logical_save_id, false)
        .await
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

async fn get_conflicts(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(logical_save_id): Path<String>,
) -> Result<Json<Vec<SnapshotRow>>, StatusCode> {
    history(&state, &auth.account_handle, &logical_save_id, true)
        .await
        .map(Json)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

async fn resolve_conflict(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((logical_save_id, snapshot_id)): Path<(String, String)>,
    Json(request): Json<ResolveConflictRequest>,
) -> Result<Json<ResolveConflictResponse>, ApiError> {
    validate_object_id(&snapshot_id)?;
    validate_object_id(&request.chosen_snapshot_id.0)?;
    resolve_conflict_in_backend(
        &state,
        &auth,
        &logical_save_id,
        &SnapshotId(snapshot_id),
        &request,
    )
    .await
    .map(Json)
    .map_err(BackendError::api)
}

async fn resolve_conflict_in_backend(
    state: &AppState,
    auth: &AuthContext,
    logical_save_id: &str,
    conflict_snapshot_id: &SnapshotId,
    request: &ResolveConflictRequest,
) -> Result<ResolveConflictResponse, BackendError> {
    match &state.backend {
        Backend::Memory(inner) => {
            let mut guard = inner.lock().unwrap();
            let logical_key = scoped_key(&auth.account_handle, logical_save_id);
            let current_head = guard
                .heads
                .get(&logical_key)
                .ok_or_else(|| BackendError::NotFound("logical save head".into()))?;
            if current_head != &request.chosen_snapshot_id {
                return Err(BackendError::Conflict(
                    "chosen snapshot must be the current HEAD".into(),
                ));
            }
            let snapshot_key = scoped_key(&auth.account_handle, &conflict_snapshot_id.0);
            let row = guard
                .snapshots
                .get_mut(&snapshot_key)
                .ok_or_else(|| BackendError::NotFound("conflict snapshot".into()))?;
            if row.logical_save_id != logical_save_id || !row.conflict {
                return Err(BackendError::Invalid(
                    "snapshot is not a conflict branch".into(),
                ));
            }
            row.resolved = true;
        }
        Backend::Persistent(p) => {
            let account = hex::decode(&auth.account_handle)
                .map_err(|_| BackendError::Invalid("invalid auth account".into()))?;
            let mut tx = p.pool.begin().await.map_err(db_unavailable)?;
            let current_head: Option<String> = sqlx::query_scalar(
                "SELECT head_snapshot_id FROM logical_saves WHERE id=$1 AND account_handle=$2 FOR UPDATE",
            )
            .bind(logical_save_id)
            .bind(&account)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_unavailable)?
            .flatten();
            if current_head.as_deref() != Some(request.chosen_snapshot_id.0.as_str()) {
                return Err(BackendError::Conflict(
                    "chosen snapshot must be the current HEAD".into(),
                ));
            }
            let result = sqlx::query(
                "UPDATE snapshots SET resolved_at=COALESCE(resolved_at,now()), resolved_by_device_cert_id=COALESCE(resolved_by_device_cert_id,$1), resolution_kind=COALESCE(resolution_kind,$2), chosen_snapshot_id=COALESCE(chosen_snapshot_id,$3) WHERE id=$4 AND account_handle=$5 AND logical_save_id=$6 AND conflict=true",
            )
            .bind(hex::decode(&auth.device_cert_id).map_err(|_| BackendError::Invalid("invalid auth device".into()))?)
            .bind(match request.resolution { ConflictResolutionKind::KeepCloudHead => "keep-cloud-head", ConflictResolutionKind::ReplaceWithLocal => "replace-with-local" })
            .bind(&request.chosen_snapshot_id.0)
            .bind(&conflict_snapshot_id.0)
            .bind(&account)
            .bind(logical_save_id)
            .execute(&mut *tx)
            .await
            .map_err(db_unavailable)?;
            if result.rows_affected() == 0 {
                return Err(BackendError::NotFound("conflict snapshot".into()));
            }
            tx.commit().await.map_err(db_unavailable)?;
        }
    }
    Ok(ResolveConflictResponse {
        conflict_snapshot_id: conflict_snapshot_id.clone(),
        chosen_snapshot_id: request.chosen_snapshot_id.clone(),
        resolution: request.resolution.clone(),
        resolved: true,
    })
}

async fn history(
    state: &AppState,
    account_handle: &str,
    logical_save_id: &str,
    conflicts_only: bool,
) -> Result<Vec<SnapshotRow>, BackendError> {
    match &state.backend {
        Backend::Memory(inner) => {
            let guard = inner.lock().unwrap();
            Ok(guard
                .snapshots
                .iter()
                .filter(|(key, s)| {
                    s.logical_save_id == logical_save_id
                        && guard
                            .snapshot_accounts
                            .get(*key)
                            .is_some_and(|a| a == account_handle)
                        && (!conflicts_only || (s.conflict && !s.resolved))
                })
                .map(|(_, s)| s.clone())
                .collect())
        }
        Backend::Persistent(p) => {
            let account = hex::decode(account_handle)
                .map_err(|_| BackendError::Invalid("invalid auth account".into()))?;
            let rows = sqlx::query("SELECT id,encrypted_manifest_object,conflict,(resolved_at IS NOT NULL) AS resolved FROM snapshots WHERE logical_save_id=$1 AND account_handle=$2 AND ($3=false OR (conflict=true AND resolved_at IS NULL)) ORDER BY created_at DESC")
                .bind(logical_save_id).bind(&account).bind(conflicts_only).fetch_all(&p.pool).await.map_err(db_unavailable)?;
            let mut result = Vec::new();
            for row in rows {
                let id: String = row.get("id");
                let parents: Vec<String> = sqlx::query_scalar("SELECT sp.parent_snapshot_id FROM snapshot_parents sp WHERE sp.snapshot_id=$1 AND sp.account_handle=$2 ORDER BY sp.parent_snapshot_id")
                    .bind(&id).bind(&account).fetch_all(&p.pool).await.map_err(db_unavailable)?;
                result.push(SnapshotRow {
                    snapshot_id: SnapshotId(id),
                    logical_save_id: logical_save_id.into(),
                    parents: parents.into_iter().map(SnapshotId).collect(),
                    manifest_id: row.get("encrypted_manifest_object"),
                    conflict: row.get("conflict"),
                    resolved: row.get("resolved"),
                });
            }
            Ok(result)
        }
    }
}

fn cas_outcome(
    base: &Option<SnapshotId>,
    current: &Option<SnapshotId>,
    new: &SnapshotId,
) -> (CommitOutcomeKind, SnapshotId, bool) {
    match (base, current) {
        (None, None) => (CommitOutcomeKind::FirstSnapshot, new.clone(), false),
        (Some(base), Some(current)) if base == current => {
            (CommitOutcomeKind::FastForward, new.clone(), false)
        }
        _ => (
            CommitOutcomeKind::Conflict,
            current.clone().unwrap_or_else(|| new.clone()),
            true,
        ),
    }
}

fn validate_parent_shape(
    base: Option<&SnapshotId>,
    parents: &[SnapshotId],
) -> Result<(), ApiError> {
    let unique = parents
        .iter()
        .map(|p| p.0.as_str())
        .collect::<BTreeSet<_>>();
    if unique.len() != parents.len() {
        return Err(BackendError::Invalid("duplicate parents".into()).api());
    }
    if let Some(base) = base
        && !unique.contains(base.0.as_str())
    {
        return Err(BackendError::Invalid("base_head must be included in parents".into()).api());
    }
    Ok(())
}

fn validate_parent_set_memory(
    state: &InMemoryState,
    account: &str,
    logical_save_id: &str,
    base: Option<&SnapshotId>,
    parents: &[SnapshotId],
) -> Result<(), ApiError> {
    validate_parent_shape(base, parents)?;
    for parent in parents {
        let key = scoped_key(account, &parent.0);
        let belongs_to_save = state
            .snapshots
            .get(&key)
            .is_some_and(|row| row.logical_save_id == logical_save_id)
            && state
                .snapshot_accounts
                .get(&key)
                .is_some_and(|owner| owner == account);
        if !belongs_to_save {
            return Err(BackendError::Invalid(
                "parent does not belong to this account and logical save".into(),
            )
            .api());
        }
    }
    Ok(())
}

async fn validate_parent_set_persistent(
    p: &PersistentState,
    account: &[u8],
    logical_save_id: &str,
    base: Option<&SnapshotId>,
    parents: &[SnapshotId],
) -> Result<(), ApiError> {
    validate_parent_shape(base, parents)?;
    for parent in parents {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM snapshots s WHERE s.id=$1 AND s.logical_save_id=$2 AND s.account_handle=$3)")
            .bind(&parent.0).bind(logical_save_id).bind(account)
            .fetch_one(&p.pool).await.map_err(db_unavailable).map_err(BackendError::api)?;
        if !exists {
            return Err(BackendError::Invalid(
                "parent does not belong to this account and logical save".into(),
            )
            .api());
        }
    }
    Ok(())
}

async fn validate_parent_set_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account: &[u8],
    logical_save_id: &str,
    base: Option<&str>,
    parents: &[String],
) -> Result<(), BackendError> {
    let parent_ids = parents.iter().cloned().map(SnapshotId).collect::<Vec<_>>();
    validate_parent_shape(base.map(|v| SnapshotId(v.to_owned())).as_ref(), &parent_ids)
        .map_err(|(_, message)| BackendError::Invalid(message))?;
    for parent in parents {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM snapshots s WHERE s.id=$1 AND s.logical_save_id=$2 AND s.account_handle=$3)")
            .bind(parent).bind(logical_save_id).bind(account)
            .fetch_one(&mut **tx).await.map_err(db_unavailable)?;
        if !exists {
            return Err(BackendError::Invalid(
                "parent changed account or logical save before commit".into(),
            ));
        }
    }
    Ok(())
}

async fn persistent_object_exists(
    p: &PersistentState,
    account_hex: &str,
    object_id: &str,
) -> Result<bool, BackendError> {
    let account = hex::decode(account_hex)
        .map_err(|_| BackendError::Invalid("invalid account hex".into()))?;
    let row =
        sqlx::query("SELECT storage_key FROM objects WHERE account_handle=$1 AND object_id=$2")
            .bind(account)
            .bind(object_id)
            .fetch_optional(&p.pool)
            .await
            .map_err(db_unavailable)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let key: String = row.get("storage_key");
    match p.object_store.head(&ObjectPath::from(key)).await {
        Ok(_) => Ok(true),
        Err(object_store::Error::NotFound { .. }) => Ok(false),
        Err(error) => Err(object_store_unavailable(error)),
    }
}

async fn persistent_object_exists_tx(
    p: &PersistentState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account: &[u8],
    object_id: &str,
) -> Result<bool, BackendError> {
    let row =
        sqlx::query("SELECT storage_key FROM objects WHERE account_handle=$1 AND object_id=$2")
            .bind(account)
            .bind(object_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db_unavailable)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let key: String = row.get("storage_key");
    match p.object_store.head(&ObjectPath::from(key)).await {
        Ok(_) => Ok(true),
        Err(object_store::Error::NotFound { .. }) => Ok(false),
        Err(error) => Err(object_store_unavailable(error)),
    }
}

async fn persistent_put_object(
    p: &PersistentState,
    account_hex: &str,
    object_id: &str,
    kind: &str,
    sha256: &str,
    bytes: Vec<u8>,
) -> Result<(), ApiError> {
    let key = format!("accounts/{account_hex}/{kind}s/{object_id}");
    let account = hex::decode(account_hex).map_err(invalid_hex)?;
    let mut tx = p
        .pool
        .begin()
        .await
        .map_err(db_unavailable)
        .map_err(BackendError::api)?;
    lock_gc_object_tx(&mut tx, &account, object_id)
        .await
        .map_err(BackendError::api)?;
    p.object_store
        .put(&ObjectPath::from(key.clone()), bytes.clone().into())
        .await
        .map_err(object_store_unavailable)
        .map_err(BackendError::api)?;
    sqlx::query("INSERT INTO objects(account_handle,object_id,object_kind,storage_key,size_bytes,checksum_sha256) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (account_handle,object_id) DO NOTHING")
        .bind(&account).bind(object_id).bind(kind).bind(&key).bind(bytes.len() as i64).bind(sha256)
        .execute(&mut *tx).await.map_err(db_unavailable).map_err(BackendError::api)?;
    let existing: String = sqlx::query_scalar(
        "SELECT checksum_sha256 FROM objects WHERE account_handle=$1 AND object_id=$2",
    )
    .bind(&account)
    .bind(object_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_unavailable)
    .map_err(BackendError::api)?;
    if existing != sha256 {
        return Err(
            BackendError::Conflict("object id reused with different checksum".into()).api(),
        );
    }
    sqlx::query("DELETE FROM orphan_gc_marks WHERE account_handle=$1 AND storage_key=$2")
        .bind(&account)
        .bind(&key)
        .execute(&mut *tx)
        .await
        .map_err(db_unavailable)
        .map_err(BackendError::api)?;
    tx.commit()
        .await
        .map_err(db_unavailable)
        .map_err(BackendError::api)?;
    Ok(())
}

async fn persistent_get_snapshot_object(
    p: &PersistentState,
    account_handle: &str,
    snapshot_id: &str,
    object_id: &str,
    kind: &str,
) -> Result<SnapshotObjectResponse, BackendError> {
    let row = sqlx::query(
        "SELECT o.storage_key,o.checksum_sha256 FROM snapshot_objects so JOIN objects o ON o.account_handle=so.account_handle AND o.object_id=so.object_id WHERE so.account_handle=decode($1,'hex') AND so.snapshot_id=$2 AND so.object_id=$3 AND o.object_kind=$4",
    )
    .bind(account_handle)
    .bind(snapshot_id)
    .bind(object_id)
    .bind(kind)
    .fetch_optional(&p.pool)
    .await
    .map_err(db_unavailable)?
    .ok_or_else(|| BackendError::NotFound(format!("{kind} object")))?;
    let key: String = row.get("storage_key");
    let expected_sha: String = row.get("checksum_sha256");
    let bytes = p
        .object_store
        .get(&ObjectPath::from(key.as_str()))
        .await
        .map_err(object_store_unavailable)?
        .bytes()
        .await
        .map_err(object_store_unavailable)?
        .to_vec();
    let actual = sha256_hex(&bytes);
    if actual != expected_sha {
        return Err(BackendError::Unavailable(format!(
            "object checksum mismatch:{object_id}"
        )));
    }
    Ok(object_response(object_id, &bytes))
}

fn object_response(object_id: &str, bytes: &[u8]) -> SnapshotObjectResponse {
    SnapshotObjectResponse {
        object_id: object_id.into(),
        sha256: sha256_hex(bytes),
        bytes_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn scoped_key(account_handle: &str, opaque_id: &str) -> String {
    format!("{account_handle}:{opaque_id}")
}

fn database_url_from_env() -> anyhow::Result<String> {
    if let Ok(url) = std::env::var("DATABASE_URL")
        && !url.contains("PASSWORD_FILE_REQUIRED")
    {
        return Ok(url);
    }
    let host = std::env::var("DATABASE_HOST").unwrap_or_else(|_| "postgres".into());
    let port = std::env::var("DATABASE_PORT").unwrap_or_else(|_| "5432".into());
    let user = std::env::var("DATABASE_USER").unwrap_or_else(|_| "mh_save_sync".into());
    let name = std::env::var("DATABASE_NAME").unwrap_or_else(|_| "mh_save_sync".into());
    let password = env_or_file("DATABASE_PASSWORD", "DATABASE_PASSWORD_FILE")?;
    Ok(format!("postgres://{user}:{password}@{host}:{port}/{name}"))
}

fn required_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("missing {name}"))
}

fn env_or_file(value_name: &str, file_name: &str) -> anyhow::Result<String> {
    if let Ok(value) = std::env::var(value_name) {
        return Ok(value);
    }
    let path = required_env(file_name)?;
    Ok(std::fs::read_to_string(path)?.trim_end().to_string())
}

fn device_registration_matches_existing_identity(
    existing_account: &[u8],
    existing_device_public: &[u8],
    revoked: bool,
    account: &[u8],
    device_public: &[u8],
) -> bool {
    !revoked && existing_account == account && existing_device_public == device_public
}

fn validate_device_certificate(
    cert: &DeviceCertificate,
    account_root: &[u8],
    cert_id_hex: &str,
    device_public: &[u8],
    now: u64,
) -> Result<(), ApiError> {
    let cert_id = hex::decode(cert_id_hex).map_err(invalid_hex)?;
    if cert.body.cert_version != 1
        || cert.body.account_root_public != account_root
        || cert.body.cert_id != cert_id
        || cert.body.device_public != device_public
        || !cert.body.crypto_suites.contains(&CRYPTO_SUITE_V1)
    {
        return Err(BackendError::Invalid("device certificate fields mismatch".into()).api());
    }
    if cert.body.issued_at_unix_seconds > now.saturating_add(300)
        || cert.body.expires_at_unix_seconds <= now
        || cert.body.issued_at_unix_seconds >= cert.body.expires_at_unix_seconds
    {
        return Err(
            BackendError::Invalid("device certificate outside validity window".into()).api(),
        );
    }
    Ok(())
}

fn decode_and_verify(encoded: &str, expected_sha256: &str) -> Result<Vec<u8>, ApiError> {
    let bytes = decode_b64(encoded)?;
    let actual = hex::encode(Sha256::digest(&bytes));
    if actual != expected_sha256.to_lowercase() {
        return Err(BackendError::Invalid("checksum mismatch".into()).api());
    }
    Ok(bytes)
}

fn decode_b64(encoded: &str) -> Result<Vec<u8>, ApiError> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| BackendError::Invalid("invalid base64".into()).api())
}

fn decode_b64_len(encoded: &str, len: usize) -> Result<Vec<u8>, ApiError> {
    let bytes = decode_b64(encoded)?;
    if bytes.len() != len {
        return Err(BackendError::Invalid(format!("expected {len} bytes")).api());
    }
    Ok(bytes)
}

fn validate_hex(value: &str, bytes: usize) -> Result<(), ApiError> {
    let decoded = hex::decode(value).map_err(invalid_hex)?;
    if decoded.len() != bytes {
        return Err(BackendError::Invalid(format!("expected {bytes}-byte hex value")).api());
    }
    Ok(())
}

fn validate_object_id(value: &str) -> Result<(), ApiError> {
    validate_hex(value, 32)
}

fn invalid_hex(_: hex::FromHexError) -> ApiError {
    BackendError::Invalid("invalid hex".into()).api()
}
fn db_unavailable(error: sqlx::Error) -> BackendError {
    BackendError::Unavailable(format!("database:{error}"))
}
fn object_store_unavailable<E>(_: E) -> BackendError {
    BackendError::Unavailable("object-store".into())
}
fn json_string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(ToOwned::to_owned))
        .collect()
}
fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use save_crypto::{
        account_root_signing_key, derive_account_keys, issue_device_certificate_with_id,
    };

    fn id(byte: u8) -> String {
        hex::encode([byte; 32])
    }
    fn sha_b64(bytes: &[u8]) -> (String, String) {
        (
            hex::encode(Sha256::digest(bytes)),
            base64::engine::general_purpose::STANDARD.encode(bytes),
        )
    }

    async fn insert_persistent_identity(pool: &PgPool, account_byte: u8, devices: &[u8]) {
        let account = vec![account_byte; 20];
        sqlx::query("INSERT INTO accounts(account_handle,root_public_key) VALUES ($1,$2)")
            .bind(&account)
            .bind(vec![account_byte; 32])
            .execute(pool)
            .await
            .unwrap();
        for device_byte in devices {
            sqlx::query("INSERT INTO devices(cert_id,account_handle,device_public_key,certificate) VALUES ($1,$2,$3,$4)")
                .bind(vec![*device_byte; 16])
                .bind(&account)
                .bind(vec![*device_byte; 32])
                .bind(vec![*device_byte; 64])
                .execute(pool)
                .await
                .unwrap();
        }
    }
    fn test_auth() -> Option<Extension<AuthContext>> {
        Some(Extension(AuthContext {
            account_handle: hex::encode([0x11; 20]),
            device_cert_id: hex::encode([0x22; 16]),
        }))
    }

    fn fixture_auth(device_byte: u8) -> Option<Extension<AuthContext>> {
        fixture_auth_for(0x11, device_byte)
    }

    fn fixture_auth_for(account_byte: u8, device_byte: u8) -> Option<Extension<AuthContext>> {
        Some(Extension(AuthContext {
            account_handle: hex::encode([account_byte; 20]),
            device_cert_id: hex::encode([device_byte; 16]),
        }))
    }

    #[derive(Serialize)]
    struct FixtureManifest<'a> {
        created_unix_ms: u64,
        mutation: &'a str,
    }

    async fn commit_fixture_snapshot(
        state: &AppState,
        snapshot_byte: u8,
        device_byte: u8,
        base: Option<u8>,
        parents: &[u8],
        created_unix_ms: u64,
        mutation: &str,
    ) -> Result<CommitSnapshotResponse, ApiError> {
        commit_fixture_snapshot_for(
            state,
            0x11,
            "three-device-save",
            snapshot_byte,
            device_byte,
            base,
            parents,
            created_unix_ms,
            mutation,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_fixture_snapshot_for(
        state: &AppState,
        account_byte: u8,
        logical_save_id: &str,
        snapshot_byte: u8,
        device_byte: u8,
        base: Option<u8>,
        parents: &[u8],
        created_unix_ms: u64,
        mutation: &str,
    ) -> Result<CommitSnapshotResponse, ApiError> {
        let manifest_id = id(snapshot_byte.wrapping_add(100));
        let auth = fixture_auth_for(account_byte, device_byte);
        let begin = begin_snapshot(
            State(state.clone()),
            auth.clone(),
            Json(BeginSnapshotRequest {
                account_handle: Some(hex::encode([account_byte; 20])),
                device_cert_id: Some(hex::encode([device_byte; 16])),
                logical_save_id: logical_save_id.into(),
                base_head: base.map(|byte| SnapshotId(id(byte))),
                parents: parents.iter().map(|byte| SnapshotId(id(*byte))).collect(),
                encrypted_manifest_id: manifest_id.clone(),
                chunk_ids: vec![],
            }),
        )
        .await?;
        let bytes = serde_json::to_vec(&FixtureManifest {
            created_unix_ms,
            mutation,
        })
        .unwrap();
        let (sha256, bytes_b64) = sha_b64(&bytes);
        put_manifest(
            State(state.clone()),
            auth.clone(),
            Path(begin.0.upload_id),
            Json(PutManifestRequest {
                manifest_id,
                sha256,
                bytes_b64,
            }),
        )
        .await?;
        commit_snapshot(
            State(state.clone()),
            auth,
            Path(begin.0.upload_id),
            Json(CommitSnapshotRequest {
                snapshot_id: SnapshotId(id(snapshot_byte)),
            }),
        )
        .await
        .map(|response| response.0)
    }

    #[test]
    fn reissued_device_certificate_is_idempotent_when_identity_matches() {
        assert!(device_registration_matches_existing_identity(
            &[1, 2],
            &[3, 4],
            false,
            &[1, 2],
            &[3, 4],
        ));
        assert!(!device_registration_matches_existing_identity(
            &[1, 2],
            &[3, 4],
            false,
            &[9],
            &[3, 4],
        ));
        assert!(!device_registration_matches_existing_identity(
            &[1, 2],
            &[3, 4],
            true,
            &[1, 2],
            &[3, 4],
        ));
    }

    #[test]
    fn signed_device_certificate_binds_account_device_and_validity() {
        let keys = derive_account_keys(&[0x42; 32]).unwrap();
        let root = account_root_signing_key(&keys);
        let device = SigningKey::from_bytes(&[0x24; 32]);
        let cert_id = [0x33; 16];
        let cert = issue_device_certificate_with_id(
            &root,
            &device.verifying_key(),
            cert_id,
            100,
            1_000,
            1,
        )
        .unwrap();
        validate_device_certificate(
            &cert,
            &root.verifying_key().to_bytes(),
            &hex::encode(cert_id),
            &device.verifying_key().to_bytes(),
            500,
        )
        .unwrap();
        assert!(
            validate_device_certificate(
                &cert,
                &[0u8; 32],
                &hex::encode(cert_id),
                &device.verifying_key().to_bytes(),
                500,
            )
            .is_err()
        );
        assert!(
            validate_device_certificate(
                &cert,
                &root.verifying_key().to_bytes(),
                &hex::encode(cert_id),
                &device.verifying_key().to_bytes(),
                1_001,
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn challenge_is_consumed_once_and_signature_binds_request() {
        use ed25519_dalek::Signer;
        let state = AppState::default();
        let signing = SigningKey::from_bytes(&[0x55; 32]);
        let account = hex::encode([0x11; 20]);
        let device = hex::encode([0x22; 16]);
        let challenge_id = Uuid::new_v4();
        let nonce = vec![0x33; 16];
        let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(&nonce);
        let timestamp = unix_seconds();
        if let Backend::Memory(inner) = &state.backend {
            let mut guard = inner.lock().unwrap();
            guard.devices.insert(
                device.clone(),
                MemoryDevice {
                    account_handle: account.clone(),
                    public_key: signing.verifying_key().to_bytes().to_vec(),
                    revoked: false,
                },
            );
            guard.challenges.insert(
                challenge_id,
                MemoryChallenge {
                    account_handle: account.clone(),
                    device_cert_id: device.clone(),
                    nonce: nonce.clone(),
                    expires: timestamp + 60,
                    used: false,
                },
            );
        }
        let message = canonical_http_request(
            "POST",
            "/v1/snapshots/begin",
            &sha256_hex(b"{}"),
            &challenge_id.to_string(),
            &nonce_b64,
            timestamp,
        );
        let signature = signing.sign(&message).to_bytes();
        verify_and_consume_challenge(
            &state,
            ChallengeProof {
                account_hex: &account,
                device_hex: &device,
                challenge_id,
                nonce: &nonce,
                timestamp,
                signature_bytes: &signature,
                message: &message,
            },
        )
        .await
        .unwrap();
        let replay = verify_and_consume_challenge(
            &state,
            ChallengeProof {
                account_hex: &account,
                device_hex: &device,
                challenge_id,
                nonce: &nonce,
                timestamp,
                signature_bytes: &signature,
                message: &message,
            },
        )
        .await;
        assert!(matches!(replay, Err((StatusCode::UNAUTHORIZED, _))));
    }

    #[test]
    fn canonical_signature_rejects_path_or_body_tampering() {
        use ed25519_dalek::Signer;
        let signing = SigningKey::from_bytes(&[0x44; 32]);
        let good = canonical_http_request(
            "POST",
            "/v1/snapshots/begin",
            &sha256_hex(b"a"),
            "challenge",
            "nonce",
            123,
        );
        let signature = signing.sign(&good);
        let changed = canonical_http_request(
            "POST",
            "/v1/snapshots/begin",
            &sha256_hex(b"b"),
            "challenge",
            "nonce",
            123,
        );
        assert!(
            signing
                .verifying_key()
                .verify(&changed, &signature)
                .is_err()
        );
    }

    #[test]
    fn parent_shape_rejects_duplicates_and_requires_base_parent() {
        let parent = SnapshotId(id(1));
        assert!(validate_parent_shape(Some(&parent), std::slice::from_ref(&parent)).is_ok());
        assert!(validate_parent_shape(Some(&parent), &[]).is_err());
        assert!(validate_parent_shape(None, &[parent.clone(), parent]).is_err());
    }

    #[test]
    fn object_store_errors_never_expose_storage_keys() {
        let secret_key = "accounts/00112233445566778899aabbccddeeff00112233/chunks/private-object";
        let error = object_store_unavailable(format!("delete failed for {secret_key}"));
        let rendered = error.to_string();
        assert_eq!(rendered, "backend unavailable: object-store");
        assert!(!rendered.contains(secret_key));
    }

    #[tokio::test]
    async fn memory_backend_rejects_unknown_parent_before_upload() {
        let state = AppState::default();
        let result = begin_snapshot(
            State(state),
            fixture_auth(0x31),
            Json(BeginSnapshotRequest {
                account_handle: Some(hex::encode([0x11; 20])),
                device_cert_id: Some(hex::encode([0x31; 16])),
                logical_save_id: "three-device-save".into(),
                base_head: Some(SnapshotId(id(9))),
                parents: vec![SnapshotId(id(9))],
                encrypted_manifest_id: id(109),
                chunk_ids: vec![],
            }),
        )
        .await;

        assert!(matches!(
            result,
            Err((StatusCode::BAD_REQUEST, message))
                if message == "parent does not belong to this account and logical save"
        ));
    }

    #[tokio::test]
    async fn three_devices_retain_fast_forward_and_every_offline_branch() {
        let state = AppState::default();

        let a = commit_fixture_snapshot(&state, 10, 0x31, None, &[], 1_000, "initial")
            .await
            .unwrap();
        assert!(matches!(a.outcome, CommitOutcomeKind::FirstSnapshot));

        let b = commit_fixture_snapshot(&state, 11, 0x32, Some(10), &[10], 2_000, "modify")
            .await
            .unwrap();
        assert!(matches!(b.outcome, CommitOutcomeKind::FastForward));
        assert_eq!(b.head, SnapshotId(id(11)));

        let c = commit_fixture_snapshot(&state, 12, 0x33, Some(10), &[10], 1_500, "modify")
            .await
            .unwrap();
        assert!(matches!(c.outcome, CommitOutcomeKind::Conflict));
        assert_eq!(c.head, SnapshotId(id(11)));
        assert_eq!(c.conflict_snapshot, Some(SnapshotId(id(12))));

        let a_branch = commit_fixture_snapshot(&state, 13, 0x31, Some(10), &[10], 3_000, "modify")
            .await
            .unwrap();
        assert!(matches!(a_branch.outcome, CommitOutcomeKind::Conflict));
        let c_branch = commit_fixture_snapshot(&state, 14, 0x33, Some(12), &[12], 4_000, "modify")
            .await
            .unwrap();
        assert!(matches!(c_branch.outcome, CommitOutcomeKind::Conflict));

        let auth = fixture_auth(0x31).unwrap().0;
        let rows = history(&state, &auth.account_handle, "three-device-save", false)
            .await
            .unwrap();
        assert_eq!(rows.len(), 5, "no branch may be erased or overwritten");
        for (snapshot, parent) in [(11, 10), (12, 10), (13, 10), (14, 12)] {
            assert!(rows.iter().any(|row| {
                row.snapshot_id == SnapshotId(id(snapshot))
                    && row.parents == vec![SnapshotId(id(parent))]
            }));
        }
        assert_eq!(
            get_head(
                State(state),
                Extension(auth),
                Path("three-device-save".into())
            )
            .await
            .unwrap()
            .0,
            SnapshotId(id(11)),
            "offline branches must never replace the fast-forward HEAD"
        );
    }

    #[tokio::test]
    async fn delete_tombstone_and_offline_modify_form_conflict_branches() {
        let state = AppState::default();
        commit_fixture_snapshot(&state, 20, 0x31, None, &[], 10, "initial")
            .await
            .unwrap();
        let delete = commit_fixture_snapshot(&state, 21, 0x32, Some(20), &[20], 20, "tombstone")
            .await
            .unwrap();
        let modify = commit_fixture_snapshot(&state, 22, 0x33, Some(20), &[20], 30, "modify")
            .await
            .unwrap();

        assert!(matches!(delete.outcome, CommitOutcomeKind::FastForward));
        assert!(matches!(modify.outcome, CommitOutcomeKind::Conflict));
        assert_eq!(modify.head, SnapshotId(id(21)));
        assert_eq!(modify.conflict_snapshot, Some(SnapshotId(id(22))));

        let auth = fixture_auth(0x31).unwrap().0;
        let rows = history(&state, &auth.account_handle, "three-device-save", false)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|row| row.snapshot_id == SnapshotId(id(21))));
        assert!(rows.iter().any(|row| {
            row.snapshot_id == SnapshotId(id(22))
                && row.conflict
                && row.parents == vec![SnapshotId(id(20))]
        }));
    }

    #[tokio::test]
    async fn extreme_clock_drift_does_not_change_cas_topology() {
        async fn topology(timestamps: [u64; 3]) -> Vec<(String, String, Option<String>)> {
            let state = AppState::default();
            let first =
                commit_fixture_snapshot(&state, 30, 0x31, None, &[], timestamps[0], "initial")
                    .await
                    .unwrap();
            let fast_forward =
                commit_fixture_snapshot(&state, 31, 0x32, Some(30), &[30], timestamps[1], "modify")
                    .await
                    .unwrap();
            let conflict =
                commit_fixture_snapshot(&state, 32, 0x33, Some(30), &[30], timestamps[2], "modify")
                    .await
                    .unwrap();
            [first, fast_forward, conflict]
                .into_iter()
                .map(|response| {
                    (
                        format!("{:?}", response.outcome),
                        response.head.0,
                        response.conflict_snapshot.map(|id| id.0),
                    )
                })
                .collect()
        }

        let normal = topology([1_000, 2_000, 1_500]).await;
        let drifted = topology([u64::MAX, 0, u64::MAX - 1]).await;
        assert_eq!(normal, drifted);
    }

    #[tokio::test]
    async fn ready_starts_ready() {
        let state = AppState::default();
        let response = ready(State(state)).await.unwrap().0;
        assert_eq!(response.status, "ready");
        assert_eq!(response.backend, "memory");
    }

    #[test]
    fn persistent_readiness_pagination_has_no_total_object_limit() {
        let storage_keys = (0..=2000)
            .map(|index| format!("objects/{index:04}"))
            .collect::<Vec<_>>();
        let mut cursor = None;
        let mut scanned = 0;
        for page in storage_keys.chunks(READINESS_PAGE_SIZE as usize) {
            cursor = readiness_next_cursor(page);
            scanned += page.len();
        }

        assert_eq!(scanned, 2001);
        assert_eq!(cursor.as_deref(), Some("objects/2000"));
        assert!(READINESS_OBJECT_PAGE_SQL.contains("o.storage_key > $1"));
        assert!(READINESS_OBJECT_PAGE_SQL.contains("ORDER BY o.storage_key"));
        assert!(READINESS_OBJECT_PAGE_SQL.contains("LIMIT $2"));
        assert!(!READINESS_OBJECT_PAGE_SQL.contains("LIMIT 2000"));
    }

    #[sqlx::test(migrations = "../../deploy/compose/migrations")]
    #[ignore = "requires PostgreSQL; run scripts/postgres-dag-contract-test.sh"]
    async fn persistent_backend_enforces_multi_device_dag_contract(pool: PgPool) {
        use object_store::memory::InMemory;

        insert_persistent_identity(&pool, 0x11, &[0x31, 0x32, 0x33]).await;
        insert_persistent_identity(&pool, 0x44, &[0x41]).await;
        let state = AppState {
            backend: Backend::Persistent(Arc::new(PersistentState {
                pool,
                object_store: Arc::new(InMemory::new()),
            })),
        };

        let a = commit_fixture_snapshot(&state, 10, 0x31, None, &[], 1_000, "initial")
            .await
            .unwrap();
        let b = commit_fixture_snapshot(&state, 11, 0x32, Some(10), &[10], 2_000, "modify")
            .await
            .unwrap();
        let c = commit_fixture_snapshot(&state, 12, 0x33, Some(10), &[10], 1_500, "modify")
            .await
            .unwrap();
        let a_branch = commit_fixture_snapshot(&state, 13, 0x31, Some(10), &[10], 3_000, "modify")
            .await
            .unwrap();
        let c_branch = commit_fixture_snapshot(&state, 14, 0x33, Some(12), &[12], 4_000, "modify")
            .await
            .unwrap();

        assert!(matches!(a.outcome, CommitOutcomeKind::FirstSnapshot));
        assert!(matches!(b.outcome, CommitOutcomeKind::FastForward));
        assert!(matches!(c.outcome, CommitOutcomeKind::Conflict));
        assert!(matches!(a_branch.outcome, CommitOutcomeKind::Conflict));
        assert!(matches!(c_branch.outcome, CommitOutcomeKind::Conflict));
        assert_eq!(b.head, SnapshotId(id(11)));
        for branch in [&c, &a_branch, &c_branch] {
            assert_eq!(branch.head, SnapshotId(id(11)));
        }

        let auth = fixture_auth(0x31).unwrap().0;
        let rows = history(&state, &auth.account_handle, "three-device-save", false)
            .await
            .unwrap();
        assert_eq!(rows.len(), 5, "PostgreSQL must retain every branch");
        for (snapshot, parent) in [(11, 10), (12, 10), (13, 10), (14, 12)] {
            assert!(rows.iter().any(|row| {
                row.snapshot_id == SnapshotId(id(snapshot))
                    && row.parents == vec![SnapshotId(id(parent))]
            }));
        }

        commit_fixture_snapshot_for(
            &state,
            0x11,
            "delete-save",
            20,
            0x31,
            None,
            &[],
            10,
            "initial",
        )
        .await
        .unwrap();
        let delete = commit_fixture_snapshot_for(
            &state,
            0x11,
            "delete-save",
            21,
            0x32,
            Some(20),
            &[20],
            20,
            "tombstone",
        )
        .await
        .unwrap();
        let modify = commit_fixture_snapshot_for(
            &state,
            0x11,
            "delete-save",
            22,
            0x33,
            Some(20),
            &[20],
            30,
            "modify",
        )
        .await
        .unwrap();
        assert!(matches!(delete.outcome, CommitOutcomeKind::FastForward));
        assert!(matches!(modify.outcome, CommitOutcomeKind::Conflict));
        assert_eq!(modify.head, SnapshotId(id(21)));
        assert_eq!(modify.conflict_snapshot, Some(SnapshotId(id(22))));

        async fn clock_topology(
            state: &AppState,
            logical_save_id: &str,
            ids: [u8; 3],
            timestamps: [u64; 3],
        ) -> [bool; 3] {
            let first = commit_fixture_snapshot_for(
                state,
                0x11,
                logical_save_id,
                ids[0],
                0x31,
                None,
                &[],
                timestamps[0],
                "initial",
            )
            .await
            .unwrap();
            let fast_forward = commit_fixture_snapshot_for(
                state,
                0x11,
                logical_save_id,
                ids[1],
                0x32,
                Some(ids[0]),
                &[ids[0]],
                timestamps[1],
                "modify",
            )
            .await
            .unwrap();
            let conflict = commit_fixture_snapshot_for(
                state,
                0x11,
                logical_save_id,
                ids[2],
                0x33,
                Some(ids[0]),
                &[ids[0]],
                timestamps[2],
                "modify",
            )
            .await
            .unwrap();
            [
                matches!(first.outcome, CommitOutcomeKind::FirstSnapshot),
                matches!(fast_forward.outcome, CommitOutcomeKind::FastForward),
                matches!(conflict.outcome, CommitOutcomeKind::Conflict)
                    && conflict.head == SnapshotId(id(ids[1])),
            ]
        }
        assert_eq!(
            clock_topology(&state, "clock-normal", [30, 31, 32], [1_000, 2_000, 1_500]).await,
            clock_topology(
                &state,
                "clock-drift",
                [40, 41, 42],
                [u64::MAX, 0, u64::MAX - 1],
            )
            .await
        );

        let cross_account = begin_snapshot(
            State(state.clone()),
            fixture_auth_for(0x44, 0x41),
            Json(BeginSnapshotRequest {
                account_handle: Some(hex::encode([0x44; 20])),
                device_cert_id: Some(hex::encode([0x41; 16])),
                logical_save_id: "three-device-save".into(),
                base_head: Some(SnapshotId(id(10))),
                parents: vec![SnapshotId(id(10))],
                encrypted_manifest_id: id(250),
                chunk_ids: vec![],
            }),
        )
        .await;
        assert!(matches!(
            cross_account,
            Err((StatusCode::BAD_REQUEST, message))
                if message == "parent does not belong to this account and logical save"
        ));

        let cross_logical_save = begin_snapshot(
            State(state.clone()),
            fixture_auth(0x31),
            Json(BeginSnapshotRequest {
                account_handle: Some(hex::encode([0x11; 20])),
                device_cert_id: Some(hex::encode([0x31; 16])),
                logical_save_id: "other-save".into(),
                base_head: Some(SnapshotId(id(10))),
                parents: vec![SnapshotId(id(10))],
                encrypted_manifest_id: id(251),
                chunk_ids: vec![],
            }),
        )
        .await;
        assert!(matches!(
            cross_logical_save,
            Err((StatusCode::BAD_REQUEST, message))
                if message == "parent does not belong to this account and logical save"
        ));

        let merge = commit_fixture_snapshot(
            &state,
            15,
            0x31,
            Some(11),
            &[11, 12],
            u64::MAX,
            "explicit-merge",
        )
        .await
        .unwrap();
        assert!(matches!(merge.outcome, CommitOutcomeKind::FastForward));
        assert_eq!(merge.head, SnapshotId(id(15)));
        let merged_history = history(&state, &auth.account_handle, "three-device-save", false)
            .await
            .unwrap();
        assert_eq!(merged_history.len(), 6);
        assert!(merged_history.iter().any(|row| {
            row.snapshot_id == SnapshotId(id(15))
                && row.parents == vec![SnapshotId(id(11)), SnapshotId(id(12))]
        }));
    }

    #[sqlx::test(migrations = "../../deploy/compose/migrations")]
    #[ignore = "requires PostgreSQL; run scripts/readiness-fullscan-test.sh"]
    async fn persistent_ready_checks_referenced_objects_after_the_first_2000(pool: PgPool) {
        use object_store::memory::InMemory;

        let account = vec![0x11_u8; 20];
        let device = vec![0x22_u8; 16];
        sqlx::query("INSERT INTO accounts(account_handle,root_public_key) VALUES ($1,$2)")
            .bind(&account)
            .bind(vec![0x33_u8; 32])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO devices(cert_id,account_handle,device_public_key,certificate) VALUES ($1,$2,$3,$4)")
            .bind(&device)
            .bind(&account)
            .bind(vec![0x44_u8; 32])
            .bind(vec![0x55_u8; 32])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO logical_saves(id,account_handle,encrypted_label) VALUES ('save',$1,$2)",
        )
        .bind(&account)
        .bind(Vec::<u8>::new())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO snapshots(id,account_handle,logical_save_id,encrypted_manifest_object,committing_device_cert_id) VALUES ('snapshot',$1,'save','object-0000',$2)")
            .bind(&account)
            .bind(&device)
            .execute(&pool)
            .await
            .unwrap();

        let mut objects = sqlx::QueryBuilder::new(
            "INSERT INTO objects(account_handle,object_id,object_kind,storage_key,size_bytes,checksum_sha256) ",
        );
        objects.push_values(0..=2000, |mut row, index| {
            let object_id = format!("object-{index:04}");
            let storage_key = format!("objects/{index:04}");
            row.push_bind(account.clone())
                .push_bind(object_id)
                .push_bind("chunk")
                .push_bind(storage_key)
                .push_bind(0_i64)
                .push_bind("00");
        });
        objects.build().execute(&pool).await.unwrap();

        let mut references = sqlx::QueryBuilder::new(
            "INSERT INTO snapshot_objects(account_handle,snapshot_id,object_id) ",
        );
        references.push_values(0..=2000, |mut row, index| {
            row.push_bind(account.clone())
                .push_bind("snapshot")
                .push_bind(format!("object-{index:04}"));
        });
        references.build().execute(&pool).await.unwrap();

        // Force DISTINCT to preserve key ordering so the legacy LIMIT 2000 query
        // deterministically omits the final referenced object.
        sqlx::query("SET enable_hashagg = off")
            .execute(&pool)
            .await
            .unwrap();

        let object_store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        object_store
            .put(
                &ObjectPath::from("__mh_save_sync_readiness__"),
                Vec::new().into(),
            )
            .await
            .unwrap();
        for index in 0..2000 {
            object_store
                .put(
                    &ObjectPath::from(format!("objects/{index:04}")),
                    Vec::new().into(),
                )
                .await
                .unwrap();
        }
        let state = AppState {
            backend: Backend::Persistent(Arc::new(PersistentState { pool, object_store })),
        };

        let (status, response) = ready(State(state.clone())).await.unwrap_err();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.0.status, "backend unavailable: missing-object");

        let Backend::Persistent(persistent) = &state.backend else {
            unreachable!();
        };
        persistent
            .object_store
            .put(&ObjectPath::from("objects/2000"), Vec::new().into())
            .await
            .unwrap();
        let response = ready(State(state)).await.unwrap().0;
        assert_eq!(response.status, "ready");
    }

    #[sqlx::test(migrations = "../../deploy/compose/migrations")]
    #[ignore = "requires PostgreSQL; run scripts/server-crash-gc-test.sh"]
    async fn orphan_gc_is_account_scoped_and_preserves_live_references(pool: PgPool) {
        use object_store::memory::InMemory;
        use std::time::Duration;

        let account_a = vec![0x11_u8; 20];
        let account_b = vec![0x22_u8; 20];
        let device_a = vec![0x31_u8; 16];
        let device_b = vec![0x32_u8; 16];
        for (account, device) in [(&account_a, &device_a), (&account_b, &device_b)] {
            sqlx::query("INSERT INTO accounts(account_handle,root_public_key) VALUES ($1,$2)")
                .bind(account)
                .bind(vec![0x44_u8; 32])
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO devices(cert_id,account_handle,device_public_key,certificate) VALUES ($1,$2,$3,$4)")
                .bind(device)
                .bind(account)
                .bind(vec![0x55_u8; 32])
                .bind(vec![0x66_u8; 32])
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO logical_saves(id,account_handle,encrypted_label) VALUES ('save',$1,$2)")
                .bind(account)
                .bind(Vec::<u8>::new())
                .execute(&pool)
                .await
                .unwrap();
        }

        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let persistent = PersistentState {
            pool: pool.clone(),
            object_store: store.clone(),
        };
        for (account, suffix) in [(&account_a, "a"), (&account_b, "b")] {
            for object_id in ["shared", "active", "young"] {
                let key = format!("accounts/{suffix}/{object_id}");
                store
                    .put(&ObjectPath::from(key.clone()), vec![1].into())
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO objects(account_handle,object_id,object_kind,storage_key,size_bytes,checksum_sha256,created_at) VALUES ($1,$2,'chunk',$3,1,'00',now()-interval '2 days')")
                    .bind(account)
                    .bind(object_id)
                    .bind(key)
                    .execute(&pool)
                    .await
                .unwrap();
            }
        }
        let untracked_key = format!("accounts/{}/chunks/untracked", hex::encode(&account_b));
        store
            .put(&ObjectPath::from(untracked_key), vec![9].into())
            .await
            .unwrap();
        std::thread::sleep(Duration::from_millis(1_100));
        sqlx::query(
            "UPDATE objects SET created_at=now() WHERE account_handle=$1 AND object_id='young'",
        )
        .bind(&account_a)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO snapshots(id,account_handle,logical_save_id,encrypted_manifest_object,committing_device_cert_id) VALUES ('snap',$1,'save','shared',$2)")
            .bind(&account_a)
            .bind(&device_a)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO snapshot_objects(account_handle,snapshot_id,object_id) VALUES ($1,'snap','shared')")
            .bind(&account_a)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO upload_sessions(id,account_handle,device_cert_id,logical_save_id,parents,required_chunks,manifest_id,expires_at) VALUES ($1,$2,$3,'save','[]','[\"active\"]','manifest',now()+interval '1 hour')")
            .bind(Uuid::new_v4())
            .bind(&account_a)
            .bind(&device_a)
            .execute(&pool)
            .await
            .unwrap();

        let dry_run = collect_orphan_objects(&persistent, Duration::from_secs(1), true)
            .await
            .unwrap();
        assert_eq!(dry_run.eligible, 4);
        assert_eq!(dry_run.deleted, 0);

        sqlx::query(
            "INSERT INTO orphan_gc_marks(account_handle,object_id,storage_key,lease_token,lease_until) \
             SELECT account_handle,object_id,storage_key,$1,now()-interval '1 minute' \
             FROM objects WHERE account_handle=$2 AND object_id='shared'",
        )
        .bind(Uuid::new_v4())
        .bind(&account_b)
        .execute(&pool)
        .await
        .unwrap();

        let swept = collect_orphan_objects(&persistent, Duration::from_secs(1), false)
            .await
            .unwrap();
        assert_eq!(swept.eligible, 4);
        assert_eq!(swept.deleted, 4);
        assert!(persistent_object_row_exists(&pool, &account_a, "shared").await);
        assert!(persistent_object_row_exists(&pool, &account_a, "active").await);
        assert!(persistent_object_row_exists(&pool, &account_a, "young").await);
        assert!(!persistent_object_row_exists(&pool, &account_b, "shared").await);
    }

    async fn persistent_object_row_exists(pool: &PgPool, account: &[u8], object_id: &str) -> bool {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM objects WHERE account_handle=$1 AND object_id=$2)",
        )
        .bind(account)
        .bind(object_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "../../deploy/compose/migrations")]
    #[ignore = "requires PostgreSQL; run scripts/server-crash-gc-test.sh"]
    async fn commit_failpoints_prove_transactional_head_and_orphan_recovery(pool: PgPool) {
        use object_store::memory::InMemory;
        use std::time::Duration;

        let account = vec![0x71_u8; 20];
        let device = vec![0x72_u8; 16];
        sqlx::query("INSERT INTO accounts(account_handle,root_public_key) VALUES ($1,$2)")
            .bind(&account)
            .bind(vec![0x73_u8; 32])
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO devices(cert_id,account_handle,device_public_key,certificate) VALUES ($1,$2,$3,$4)")
            .bind(&device)
            .bind(&account)
            .bind(vec![0x74_u8; 32])
            .bind(vec![0x75_u8; 32])
            .execute(&pool)
            .await
            .unwrap();
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());
        let persistent = PersistentState {
            pool: pool.clone(),
            object_store: store.clone(),
        };
        let auth = AuthContext {
            account_handle: hex::encode(&account),
            device_cert_id: hex::encode(&device),
        };

        let before_upload = insert_commit_fixture(
            &pool,
            &store,
            &account,
            &device,
            "before-save",
            "before-manifest",
        )
        .await;
        let before = commit_persistent_with_failpoint(
            &persistent,
            before_upload,
            CommitSnapshotRequest {
                snapshot_id: SnapshotId("before-snapshot".into()),
            },
            &auth,
            CommitFailpoint::BeforeDatabaseCommit,
        )
        .await;
        assert!(matches!(before, Err(BackendError::Unavailable(_))));
        assert_eq!(snapshot_count(&pool, &account, "before-save").await, 0);
        assert_eq!(head(&pool, &account, "before-save").await, None);
        assert!(upload_exists(&pool, before_upload).await);
        sqlx::query("DELETE FROM upload_sessions WHERE id=$1")
            .bind(before_upload)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE objects SET created_at=now()-interval '2 days' WHERE account_handle=$1 AND object_id='before-manifest'")
            .bind(&account)
            .execute(&pool)
            .await
            .unwrap();
        let swept = collect_orphan_objects(&persistent, Duration::from_secs(1), false)
            .await
            .unwrap();
        assert_eq!(swept.deleted, 1);

        let after_upload = insert_commit_fixture(
            &pool,
            &store,
            &account,
            &device,
            "after-save",
            "after-manifest",
        )
        .await;
        let after = commit_persistent_with_failpoint(
            &persistent,
            after_upload,
            CommitSnapshotRequest {
                snapshot_id: SnapshotId("after-snapshot".into()),
            },
            &auth,
            CommitFailpoint::AfterDatabaseCommit,
        )
        .await;
        assert!(matches!(after, Err(BackendError::Unavailable(_))));
        assert_eq!(snapshot_count(&pool, &account, "after-save").await, 1);
        assert_eq!(
            head(&pool, &account, "after-save").await.as_deref(),
            Some("after-snapshot")
        );
        assert!(!upload_exists(&pool, after_upload).await);
        sqlx::query("UPDATE objects SET created_at=now()-interval '2 days' WHERE account_handle=$1 AND object_id='after-manifest'")
            .bind(&account)
            .execute(&pool)
            .await
            .unwrap();
        let retained = collect_orphan_objects(&persistent, Duration::from_secs(1), false)
            .await
            .unwrap();
        assert_eq!(retained.deleted, 0);
        assert!(persistent_object_row_exists(&pool, &account, "after-manifest").await);
    }

    #[sqlx::test(migrations = "../../deploy/compose/migrations")]
    #[ignore = "requires PostgreSQL; run scripts/server-crash-gc-test.sh"]
    async fn slow_delete_lock_does_not_block_unrelated_foreground_object(pool: PgPool) {
        let account = vec![0x61_u8; 20];
        let mut slow_tx = pool.begin().await.unwrap();
        lock_gc_object_tx(&mut slow_tx, &account, "slow-orphan")
            .await
            .unwrap();
        let slow = tokio::spawn(async move {
            sqlx::query("SELECT pg_sleep(0.6)")
                .execute(&mut *slow_tx)
                .await
                .unwrap();
            slow_tx.commit().await.unwrap();
        });

        let started = std::time::Instant::now();
        let mut foreground_tx = pool.begin().await.unwrap();
        lock_gc_object_tx(&mut foreground_tx, &account, "foreground-object")
            .await
            .unwrap();
        foreground_tx.commit().await.unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "unrelated foreground object was blocked by slow orphan deletion"
        );
        slow.await.unwrap();
    }

    async fn insert_commit_fixture(
        pool: &PgPool,
        store: &Arc<dyn ObjectStore>,
        account: &[u8],
        device: &[u8],
        logical_save: &str,
        manifest_id: &str,
    ) -> Uuid {
        sqlx::query(
            "INSERT INTO logical_saves(id,account_handle,encrypted_label) VALUES ($1,$2,$3)",
        )
        .bind(logical_save)
        .bind(account)
        .bind(Vec::<u8>::new())
        .execute(pool)
        .await
        .unwrap();
        let key = format!("fixture/{manifest_id}");
        store
            .put(&ObjectPath::from(key.clone()), vec![1].into())
            .await
            .unwrap();
        sqlx::query("INSERT INTO objects(account_handle,object_id,object_kind,storage_key,size_bytes,checksum_sha256) VALUES ($1,$2,'manifest',$3,1,'00')")
            .bind(account)
            .bind(manifest_id)
            .bind(key)
            .execute(pool)
            .await
            .unwrap();
        let upload_id = Uuid::new_v4();
        sqlx::query("INSERT INTO upload_sessions(id,account_handle,device_cert_id,logical_save_id,parents,required_chunks,manifest_id,expires_at) VALUES ($1,$2,$3,$4,'[]','[]',$5,now()+interval '1 hour')")
            .bind(upload_id)
            .bind(account)
            .bind(device)
            .bind(logical_save)
            .bind(manifest_id)
            .execute(pool)
            .await
            .unwrap();
        upload_id
    }

    async fn snapshot_count(pool: &PgPool, account: &[u8], logical_save: &str) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM snapshots WHERE account_handle=$1 AND logical_save_id=$2",
        )
        .bind(account)
        .bind(logical_save)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn head(pool: &PgPool, account: &[u8], logical_save: &str) -> Option<String> {
        sqlx::query_scalar(
            "SELECT head_snapshot_id FROM logical_saves WHERE account_handle=$1 AND id=$2",
        )
        .bind(account)
        .bind(logical_save)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn upload_exists(pool: &PgPool, upload_id: Uuid) -> bool {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM upload_sessions WHERE id=$1)")
            .bind(upload_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn cas_conflict_preserves_second_snapshot_and_validates_bytes() {
        let state = AppState::default();
        let manifest1 = id(1);
        let begin1 = begin_snapshot(
            State(state.clone()),
            test_auth(),
            Json(BeginSnapshotRequest {
                account_handle: Some(hex::encode([0x11; 20])),
                device_cert_id: Some(hex::encode([0x22; 16])),
                logical_save_id: "ls".into(),
                base_head: None,
                parents: vec![],
                encrypted_manifest_id: manifest1.clone(),
                chunk_ids: vec![],
            }),
        )
        .await
        .unwrap()
        .0;
        let (sha1, b641) = sha_b64(b"abc");
        put_manifest(
            State(state.clone()),
            test_auth(),
            Path(begin1.upload_id),
            Json(PutManifestRequest {
                manifest_id: manifest1,
                sha256: sha1,
                bytes_b64: b641,
            }),
        )
        .await
        .unwrap();
        let c1 = commit_snapshot(
            State(state.clone()),
            test_auth(),
            Path(begin1.upload_id),
            Json(CommitSnapshotRequest {
                snapshot_id: SnapshotId(id(2)),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(matches!(c1.outcome, CommitOutcomeKind::FirstSnapshot));

        let manifest2 = id(3);
        let begin2 = begin_snapshot(
            State(state.clone()),
            test_auth(),
            Json(BeginSnapshotRequest {
                account_handle: Some(hex::encode([0x11; 20])),
                device_cert_id: Some(hex::encode([0x22; 16])),
                logical_save_id: "ls".into(),
                base_head: None,
                parents: vec![],
                encrypted_manifest_id: manifest2.clone(),
                chunk_ids: vec![],
            }),
        )
        .await
        .unwrap()
        .0;
        let (sha2, b642) = sha_b64(b"def");
        put_manifest(
            State(state.clone()),
            test_auth(),
            Path(begin2.upload_id),
            Json(PutManifestRequest {
                manifest_id: manifest2,
                sha256: sha2,
                bytes_b64: b642,
            }),
        )
        .await
        .unwrap();
        let c2 = commit_snapshot(
            State(state.clone()),
            test_auth(),
            Path(begin2.upload_id),
            Json(CommitSnapshotRequest {
                snapshot_id: SnapshotId(id(4)),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(matches!(c2.outcome, CommitOutcomeKind::Conflict));
        assert_eq!(c2.head, SnapshotId(id(2)));
        assert_eq!(c2.conflict_snapshot, Some(SnapshotId(id(4))));

        let auth = AuthContext {
            account_handle: hex::encode([0x11; 20]),
            device_cert_id: hex::encode([0x22; 16]),
        };
        let before = history(&state, &auth.account_handle, "ls", true)
            .await
            .unwrap();
        assert_eq!(before.len(), 1);
        let stale_choice = resolve_conflict_in_backend(
            &state,
            &auth,
            "ls",
            &SnapshotId(id(4)),
            &ResolveConflictRequest {
                chosen_snapshot_id: SnapshotId(id(9)),
                resolution: ConflictResolutionKind::ReplaceWithLocal,
            },
        )
        .await;
        assert!(matches!(stale_choice, Err(BackendError::Conflict(_))));
        assert_eq!(
            history(&state, &auth.account_handle, "ls", true)
                .await
                .unwrap()
                .len(),
            1,
            "a stale explicit choice must not hide the conflict"
        );
        resolve_conflict_in_backend(
            &state,
            &auth,
            "ls",
            &SnapshotId(id(4)),
            &ResolveConflictRequest {
                chosen_snapshot_id: SnapshotId(id(2)),
                resolution: ConflictResolutionKind::KeepCloudHead,
            },
        )
        .await
        .unwrap();
        assert!(
            history(&state, &auth.account_handle, "ls", true)
                .await
                .unwrap()
                .is_empty()
        );
        let all = history(&state, &auth.account_handle, "ls", false)
            .await
            .unwrap();
        assert_eq!(all.len(), 2, "resolution must not erase DAG history");
        assert!(
            all.iter()
                .any(|row| row.snapshot_id == SnapshotId(id(4)) && row.resolved)
        );
    }

    #[tokio::test]
    async fn concurrent_replacements_from_same_observed_head_keep_loser_as_conflict() {
        let base = Some(SnapshotId(id(10)));
        let current_after_winner = Some(SnapshotId(id(11)));
        let losing_snapshot = SnapshotId(id(12));

        let (outcome, preserved_head, conflict) =
            cas_outcome(&base, &current_after_winner, &losing_snapshot);

        assert!(matches!(outcome, CommitOutcomeKind::Conflict));
        assert_eq!(preserved_head, SnapshotId(id(11)));
        assert!(conflict);
    }

    #[tokio::test]
    async fn corrupt_payload_checksum_is_rejected() {
        let state = AppState::default();
        let begin = begin_snapshot(
            State(state.clone()),
            test_auth(),
            Json(BeginSnapshotRequest {
                account_handle: Some(hex::encode([0x11; 20])),
                device_cert_id: Some(hex::encode([0x22; 16])),
                logical_save_id: "ls".into(),
                base_head: None,
                parents: vec![],
                encrypted_manifest_id: id(5),
                chunk_ids: vec![id(6)],
            }),
        )
        .await
        .unwrap()
        .0;
        let result = put_chunk(
            State(state),
            test_auth(),
            Path(begin.upload_id),
            Json(PutChunkRequest {
                chunk_id: id(6),
                sha256: id(7),
                bytes_b64: base64::engine::general_purpose::STANDARD.encode(b"corrupt"),
            }),
        )
        .await;
        assert!(matches!(result, Err((StatusCode::BAD_REQUEST, _))));
    }

    #[tokio::test]
    async fn memory_bundle_objects_are_isolated_by_account() {
        let state = AppState::default();
        let object_id = id(30);
        let snapshot_id = id(31);
        for (account_byte, device_byte, payload) in [
            (0x11, 0x22, b"account-a".as_slice()),
            (0x33, 0x44, b"account-b".as_slice()),
        ] {
            let auth = Some(Extension(AuthContext {
                account_handle: hex::encode([account_byte; 20]),
                device_cert_id: hex::encode([device_byte; 16]),
            }));
            let begin = begin_snapshot(
                State(state.clone()),
                auth.clone(),
                Json(BeginSnapshotRequest {
                    account_handle: Some(hex::encode([account_byte; 20])),
                    device_cert_id: Some(hex::encode([device_byte; 16])),
                    logical_save_id: "same-logical-id".into(),
                    base_head: None,
                    parents: vec![],
                    encrypted_manifest_id: object_id.clone(),
                    chunk_ids: vec![],
                }),
            )
            .await
            .unwrap()
            .0;
            let (sha, encoded) = sha_b64(payload);
            put_manifest(
                State(state.clone()),
                auth.clone(),
                Path(begin.upload_id),
                Json(PutManifestRequest {
                    manifest_id: object_id.clone(),
                    sha256: sha,
                    bytes_b64: encoded,
                }),
            )
            .await
            .unwrap();
            let _ = commit_snapshot(
                State(state.clone()),
                auth,
                Path(begin.upload_id),
                Json(CommitSnapshotRequest {
                    snapshot_id: SnapshotId(snapshot_id.clone()),
                }),
            )
            .await
            .unwrap();
        }
        let bundle_a = get_encrypted_bundle(
            State(state.clone()),
            Extension(AuthContext {
                account_handle: hex::encode([0x11; 20]),
                device_cert_id: hex::encode([0x22; 16]),
            }),
            Path(snapshot_id.clone()),
        )
        .await
        .unwrap()
        .0;
        let bundle_b = get_encrypted_bundle(
            State(state),
            Extension(AuthContext {
                account_handle: hex::encode([0x33; 20]),
                device_cert_id: hex::encode([0x44; 16]),
            }),
            Path(snapshot_id),
        )
        .await
        .unwrap()
        .0;
        assert_ne!(
            bundle_a.encrypted_manifest.bytes_b64,
            bundle_b.encrypted_manifest.bytes_b64
        );
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(bundle_a.encrypted_manifest.bytes_b64)
                .unwrap(),
            b"account-a"
        );
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(bundle_b.encrypted_manifest.bytes_b64)
                .unwrap(),
            b"account-b"
        );
    }
}
