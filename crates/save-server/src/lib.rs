use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use save_domain::SnapshotId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct AppState {
    inner: Arc<Mutex<InMemoryState>>,
}

#[derive(Default)]
struct InMemoryState {
    chunks: BTreeMap<String, String>,
    manifests: BTreeSet<String>,
    uploads: BTreeMap<Uuid, UploadSession>,
    heads: BTreeMap<String, SnapshotId>,
    snapshots: BTreeMap<String, SnapshotRow>,
}

#[derive(Debug, Clone)]
struct UploadSession {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeginSnapshotRequest {
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

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/version", get(version))
        .route("/v1/snapshots/begin", post(begin_snapshot))
        .route("/v1/snapshots/{upload_id}/chunks", post(put_chunk))
        .route("/v1/snapshots/{upload_id}/manifest", post(put_manifest))
        .route("/v1/snapshots/{upload_id}/commit", post(commit_snapshot))
        .route("/v1/heads/{logical_save_id}", get(get_head))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

async fn ready(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<HealthResponse>)> {
    let guard = state.inner.lock().unwrap();
    for row in guard.snapshots.values() {
        if !guard.manifests.contains(&row.manifest_id) {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "missing-manifest".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                }),
            ));
        }
    }
    Ok(Json(HealthResponse {
        status: "ready".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }))
}

async fn version() -> Json<HealthResponse> {
    health().await
}

async fn begin_snapshot(
    State(state): State<AppState>,
    Json(req): Json<BeginSnapshotRequest>,
) -> Json<BeginSnapshotResponse> {
    let mut guard = state.inner.lock().unwrap();
    let upload_id = Uuid::new_v4();
    let missing: Vec<String> = req
        .chunk_ids
        .iter()
        .filter(|id| !guard.chunks.contains_key(*id))
        .cloned()
        .collect();
    guard.uploads.insert(
        upload_id,
        UploadSession {
            logical_save_id: req.logical_save_id,
            base_head: req.base_head,
            parents: req.parents,
            required_chunks: req.chunk_ids.into_iter().collect(),
            uploaded_chunks: BTreeSet::new(),
            manifest_id: Some(req.encrypted_manifest_id),
        },
    );
    Json(BeginSnapshotResponse {
        upload_id,
        missing_chunk_ids: missing,
    })
}

async fn put_chunk(
    State(state): State<AppState>,
    Path(upload_id): Path<Uuid>,
    Json(req): Json<PutChunkRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut guard = state.inner.lock().unwrap();
    let session = guard
        .uploads
        .get_mut(&upload_id)
        .ok_or((StatusCode::NOT_FOUND, "unknown upload".into()))?;
    let actual = sha256_hex(req.bytes_b64.as_bytes());
    if actual != req.sha256 {
        return Err((StatusCode::BAD_REQUEST, "checksum mismatch".into()));
    }
    session.uploaded_chunks.insert(req.chunk_id.clone());
    guard.chunks.insert(req.chunk_id, req.sha256);
    Ok(StatusCode::NO_CONTENT)
}

async fn put_manifest(
    State(state): State<AppState>,
    Path(upload_id): Path<Uuid>,
    Json(req): Json<PutManifestRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let mut guard = state.inner.lock().unwrap();
    let session = guard
        .uploads
        .get_mut(&upload_id)
        .ok_or((StatusCode::NOT_FOUND, "unknown upload".into()))?;
    let actual = sha256_hex(req.bytes_b64.as_bytes());
    if actual != req.sha256 {
        return Err((StatusCode::BAD_REQUEST, "checksum mismatch".into()));
    }
    session.manifest_id = Some(req.manifest_id.clone());
    guard.manifests.insert(req.manifest_id);
    Ok(StatusCode::NO_CONTENT)
}

async fn commit_snapshot(
    State(state): State<AppState>,
    Path(upload_id): Path<Uuid>,
    Json(req): Json<CommitSnapshotRequest>,
) -> Result<Json<CommitSnapshotResponse>, (StatusCode, String)> {
    let mut guard = state.inner.lock().unwrap();
    let session = guard
        .uploads
        .remove(&upload_id)
        .ok_or((StatusCode::NOT_FOUND, "unknown upload".into()))?;
    for chunk in &session.required_chunks {
        if !guard.chunks.contains_key(chunk) && !session.uploaded_chunks.contains(chunk) {
            return Err((StatusCode::CONFLICT, format!("missing chunk {chunk}")));
        }
    }
    let manifest_id = session
        .manifest_id
        .ok_or((StatusCode::CONFLICT, "missing manifest".into()))?;
    if !guard.manifests.contains(&manifest_id) {
        return Err((StatusCode::CONFLICT, "manifest not durable".into()));
    }
    let current = guard.heads.get(&session.logical_save_id).cloned();
    let (kind, head, conflict) = match (&session.base_head, &current) {
        (None, None) => (
            CommitOutcomeKind::FirstSnapshot,
            req.snapshot_id.clone(),
            false,
        ),
        (Some(base), Some(cur)) if base == cur => (
            CommitOutcomeKind::FastForward,
            req.snapshot_id.clone(),
            false,
        ),
        _ => (
            CommitOutcomeKind::Conflict,
            current.clone().unwrap_or_else(|| req.snapshot_id.clone()),
            true,
        ),
    };
    guard.snapshots.insert(
        req.snapshot_id.0.clone(),
        SnapshotRow {
            snapshot_id: req.snapshot_id.clone(),
            logical_save_id: session.logical_save_id.clone(),
            parents: session.parents,
            manifest_id,
            conflict,
        },
    );
    if !conflict {
        guard
            .heads
            .insert(session.logical_save_id, req.snapshot_id.clone());
    }
    Ok(Json(CommitSnapshotResponse {
        outcome: kind,
        head,
        conflict_snapshot: conflict.then_some(req.snapshot_id),
    }))
}

async fn get_head(
    State(state): State<AppState>,
    Path(logical_save_id): Path<String>,
) -> Result<Json<SnapshotId>, StatusCode> {
    let guard = state.inner.lock().unwrap();
    guard
        .heads
        .get(&logical_save_id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ready_starts_ready() {
        let state = AppState::default();
        let response = ready(State(state)).await.unwrap().0;
        assert_eq!(response.status, "ready");
    }

    #[tokio::test]
    async fn cas_conflict_preserves_second_snapshot() {
        let state = AppState::default();
        let begin1 = begin_snapshot(
            State(state.clone()),
            Json(BeginSnapshotRequest {
                logical_save_id: "ls".into(),
                base_head: None,
                parents: vec![],
                encrypted_manifest_id: "m1".into(),
                chunk_ids: vec![],
            }),
        )
        .await
        .0;
        put_manifest(
            State(state.clone()),
            Path(begin1.upload_id),
            Json(PutManifestRequest {
                manifest_id: "m1".into(),
                sha256: sha256_hex(b"abc"),
                bytes_b64: "abc".into(),
            }),
        )
        .await
        .unwrap();
        let c1 = commit_snapshot(
            State(state.clone()),
            Path(begin1.upload_id),
            Json(CommitSnapshotRequest {
                snapshot_id: SnapshotId("s1".into()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(matches!(c1.outcome, CommitOutcomeKind::FirstSnapshot));

        let begin2 = begin_snapshot(
            State(state.clone()),
            Json(BeginSnapshotRequest {
                logical_save_id: "ls".into(),
                base_head: None,
                parents: vec![],
                encrypted_manifest_id: "m2".into(),
                chunk_ids: vec![],
            }),
        )
        .await
        .0;
        put_manifest(
            State(state.clone()),
            Path(begin2.upload_id),
            Json(PutManifestRequest {
                manifest_id: "m2".into(),
                sha256: sha256_hex(b"def"),
                bytes_b64: "def".into(),
            }),
        )
        .await
        .unwrap();
        let c2 = commit_snapshot(
            State(state.clone()),
            Path(begin2.upload_id),
            Json(CommitSnapshotRequest {
                snapshot_id: SnapshotId("s2".into()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(matches!(c2.outcome, CommitOutcomeKind::Conflict));
        assert_eq!(c2.head, SnapshotId("s1".into()));
        assert_eq!(c2.conflict_snapshot, Some(SnapshotId("s2".into())));
    }
}
