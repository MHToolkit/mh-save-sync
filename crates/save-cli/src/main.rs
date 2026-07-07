use base64::Engine;
use clap::{Parser, Subcommand, ValueEnum};
use ed25519_dalek::SigningKey;
use save_crypto::{
    account_handle, account_root_signing_key, derive_account_keys, deterministic_cbor,
    issue_device_certificate_with_id, recovery_phrase_from_secret,
};
use save_domain::{DeviceId, GameKey, LogicalSaveId, SnapshotId, stable_logical_save_id};
use save_engine::{
    EmulatorState, EncryptedSnapshot, SnapshotOptions, create_snapshot_from_stable_folder,
    decrypt_manifest, export_encrypted_bundle, import_encrypted_bundle, restore_snapshot_to_folder,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "mh-save", about = "MH Save Sync research/phase1 CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Adapters,
    CryptoVector,
    CryptoDeviceFixture,
    SnapshotFixture {
        root: PathBuf,
    },
    SnapshotExport {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        secret_hex: String,
    },
    BundleRestore {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        target: PathBuf,
        #[arg(long)]
        secret_hex: String,
        #[arg(long, value_enum, default_value_t = CliEmulatorState::Stopped)]
        emulator_state: CliEmulatorState,
    },
    ServerUpload {
        #[arg(long, env = "MH_SAVE_SYNC_SERVER_URL")]
        server_url: String,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        secret_hex: String,
        #[arg(long)]
        base_head: Option<String>,
        #[arg(long)]
        logical_save_id: Option<String>,
        #[arg(long, default_value = "cli-device")]
        device_id: String,
        #[arg(long)]
        account_handle: Option<String>,
        #[arg(long)]
        device_cert_id: Option<String>,
    },
    ServerStatus {
        #[arg(long, env = "MH_SAVE_SYNC_SERVER_URL")]
        server_url: String,
        #[arg(long)]
        secret_hex: String,
        #[arg(long)]
        logical_save_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliEmulatorState {
    Stopped,
    Running,
}

impl From<CliEmulatorState> for EmulatorState {
    fn from(value: CliEmulatorState) -> Self {
        match value {
            CliEmulatorState::Stopped => EmulatorState::Stopped,
            CliEmulatorState::Running => EmulatorState::Running,
        }
    }
}

fn secret_from_hex(input: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = hex::decode(input.trim())?;
    anyhow::ensure!(bytes.len() == 32, "secret-hex must encode exactly 32 bytes");
    let mut secret = [0u8; 32];
    secret.copy_from_slice(&bytes);
    Ok(secret)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Adapters => {
            let descriptors = save_adapters::all_descriptors();
            println!("{}", serde_json::to_string_pretty(&descriptors)?);
        }
        Commands::CryptoVector => {
            let secret = [0x42u8; 32];
            let keys = derive_account_keys(&secret)?;
            let phrase = recovery_phrase_from_secret(&secret)?;
            println!(
                "{{\"suite\":1,\"secret_sha256\":\"{}\",\"account_handle\":\"{}\",\"phrase_word_count\":{}}}",
                hex::encode(sha2::Sha256::digest(secret)),
                account_handle(&keys),
                phrase.split_whitespace().count()
            );
        }
        Commands::CryptoDeviceFixture => {
            let keys = derive_account_keys(&[0x42; 32])?;
            let root = account_root_signing_key(&keys);
            let device = SigningKey::from_bytes(&[0x24; 32]);
            let cert_id = [0x33; 16];
            let certificate = issue_device_certificate_with_id(
                &root,
                &device.verifying_key(),
                cert_id,
                1_700_000_000,
                4_102_444_800,
                1,
            )?;
            println!(
                "{}",
                serde_json::json!({
                    "account_handle": account_handle(&keys),
                    "root_public_key_b64": base64::engine::general_purpose::STANDARD
                        .encode(root.verifying_key().to_bytes()),
                    "cert_id": hex::encode(cert_id),
                    "device_public_key_b64": base64::engine::general_purpose::STANDARD
                        .encode(device.verifying_key().to_bytes()),
                    "certificate_b64": base64::engine::general_purpose::STANDARD
                        .encode(deterministic_cbor(&certificate)?),
                })
            );
        }
        Commands::SnapshotFixture { root } => {
            let descriptor = save_adapters::generic_folder_macos();
            let game_key = GameKey::new("generic", "fixture", "none", "slot1");
            let mut options = SnapshotOptions::fixture(game_key.clone());
            options.logical_save_id = stable_logical_save_id(&descriptor.emulator_id, &game_key);
            let secret = [0x11u8; 32];
            let snapshot =
                create_snapshot_from_stable_folder(&root, &descriptor, &secret, options)?;
            let manifest = decrypt_manifest(&secret, &snapshot)?;
            println!(
                "{}",
                serde_json::json!({
                    "snapshot_id": snapshot.snapshot_id,
                    "file_count": snapshot.fingerprint.file_count,
                    "total_bytes": snapshot.fingerprint.total_bytes,
                    "manifest_entries": manifest.entries.len(),
                    "chunk_count": snapshot.chunks.len()
                })
            );
        }
        Commands::SnapshotExport {
            root,
            bundle,
            secret_hex,
        } => {
            let descriptor = save_adapters::generic_folder_macos();
            let game_key = GameKey::new("generic", "fixture", "none", "slot1");
            let mut options = SnapshotOptions::fixture(game_key.clone());
            options.logical_save_id = stable_logical_save_id(&descriptor.emulator_id, &game_key);
            let secret = secret_from_hex(&secret_hex)?;
            let snapshot =
                create_snapshot_from_stable_folder(&root, &descriptor, &secret, options)?;
            let manifest = decrypt_manifest(&secret, &snapshot)?;
            export_encrypted_bundle(&snapshot, &bundle)?;
            println!(
                "{}",
                serde_json::json!({
                    "bundle": bundle,
                    "encrypted": true,
                    "snapshot_id": snapshot.snapshot_id,
                    "file_count": snapshot.fingerprint.file_count,
                    "total_bytes": snapshot.fingerprint.total_bytes,
                    "manifest_entries": manifest.entries.len(),
                    "chunk_count": snapshot.chunks.len()
                })
            );
        }
        Commands::BundleRestore {
            bundle,
            target,
            secret_hex,
            emulator_state,
        } => {
            let secret = secret_from_hex(&secret_hex)?;
            let snapshot = import_encrypted_bundle(&bundle)?;
            let backup = restore_snapshot_to_folder(
                &secret,
                &snapshot,
                &target,
                EmulatorState::from(emulator_state),
            )?;
            println!(
                "{}",
                serde_json::json!({
                    "restored": target,
                    "backup": backup,
                    "snapshot_id": snapshot.snapshot_id
                })
            );
        }
        Commands::ServerUpload {
            server_url,
            root,
            secret_hex,
            base_head,
            logical_save_id,
            device_id,
            account_handle,
            device_cert_id,
        } => {
            let report = server_upload(ServerUploadInput {
                server_url,
                root,
                secret_hex,
                base_head,
                logical_save_id,
                device_id,
                account_handle,
                device_cert_id,
            })
            .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::ServerStatus {
            server_url,
            secret_hex,
            logical_save_id,
        } => {
            let report = server_status(server_url, secret_hex, logical_save_id).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ServerUploadInput {
    server_url: String,
    root: PathBuf,
    secret_hex: String,
    base_head: Option<String>,
    logical_save_id: Option<String>,
    device_id: String,
    account_handle: Option<String>,
    device_cert_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ServerUploadReport {
    server_url: String,
    sync_target: String,
    account_handle: String,
    logical_save_id: String,
    device_id: String,
    snapshot_id: SnapshotId,
    cloud_head_before: Option<SnapshotId>,
    cloud_head: SnapshotId,
    conflict_snapshot: Option<SnapshotId>,
    outcome: String,
    missing_chunks_uploaded: usize,
    chunk_count: usize,
    manifest_uploaded: bool,
    file_count: u64,
    total_bytes: u64,
    message_zh: String,
}

#[derive(Debug, Serialize)]
struct ServerStatusReport {
    server_url: String,
    sync_target: String,
    account_handle: String,
    logical_save_id: String,
    cloud_head: Option<SnapshotId>,
    history_count: usize,
    conflict_count: usize,
    message_zh: String,
}

#[derive(Debug, Serialize)]
struct BeginSnapshotRequest<'a> {
    account_handle: Option<&'a str>,
    device_cert_id: Option<&'a str>,
    logical_save_id: &'a str,
    base_head: Option<SnapshotId>,
    parents: Vec<SnapshotId>,
    encrypted_manifest_id: String,
    chunk_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BeginSnapshotResponse {
    upload_id: String,
    missing_chunk_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PutChunkRequest {
    chunk_id: String,
    sha256: String,
    bytes_b64: String,
}

#[derive(Debug, Serialize)]
struct PutManifestRequest {
    manifest_id: String,
    sha256: String,
    bytes_b64: String,
}

#[derive(Debug, Serialize)]
struct CommitSnapshotRequest {
    snapshot_id: SnapshotId,
}

#[derive(Debug, Deserialize)]
struct CommitSnapshotResponse {
    outcome: String,
    head: SnapshotId,
    conflict_snapshot: Option<SnapshotId>,
}

#[derive(Debug, Deserialize)]
struct SnapshotRowResponse {
    #[allow(dead_code)]
    snapshot_id: SnapshotId,
}

async fn server_upload(input: ServerUploadInput) -> anyhow::Result<ServerUploadReport> {
    let server_url = normalize_server_url(&input.server_url);
    let secret = secret_from_hex(&input.secret_hex)?;
    let keys = derive_account_keys(&secret)?;
    let computed_account_handle = account_handle(&keys);
    let descriptor = save_adapters::generic_folder_macos();
    let game_key = GameKey::new("generic", "fixture", "none", "slot1");
    let logical_save_id = input
        .logical_save_id
        .unwrap_or_else(|| stable_logical_save_id(&descriptor.emulator_id, &game_key).0);
    let base_head = input.base_head.map(SnapshotId);
    let mut parents = Vec::new();
    if let Some(base) = &base_head {
        parents.push(base.clone());
    }
    let mut options = SnapshotOptions::fixture(game_key);
    options.logical_save_id = LogicalSaveId(logical_save_id.clone());
    options.device_id = DeviceId(input.device_id.clone());
    options.parents = parents.clone();
    options.created_unix_ms = unix_millis();
    let snapshot = create_snapshot_from_stable_folder(&input.root, &descriptor, &secret, options)?;
    let client = reqwest::Client::new();
    let cloud_head_before = get_head(&client, &server_url, &logical_save_id).await?;
    let manifest_bytes = serde_json::to_vec(&snapshot.encrypted_manifest)?;
    let manifest_id = sha256_hex(&manifest_bytes);
    let mut chunk_ids = snapshot.chunks.keys().cloned().collect::<Vec<_>>();
    chunk_ids.sort();
    let begin = post_json::<_, BeginSnapshotResponse>(
        &client,
        &format!("{server_url}/v1/snapshots/begin"),
        &BeginSnapshotRequest {
            account_handle: input.account_handle.as_deref(),
            device_cert_id: input.device_cert_id.as_deref(),
            logical_save_id: &logical_save_id,
            base_head,
            parents,
            encrypted_manifest_id: manifest_id.clone(),
            chunk_ids: chunk_ids.clone(),
        },
    )
    .await?;
    for chunk_id in &begin.missing_chunk_ids {
        let blob = snapshot
            .chunks
            .get(chunk_id)
            .ok_or_else(|| anyhow::anyhow!("server requested unknown chunk {chunk_id}"))?;
        let bytes = serde_json::to_vec(blob)?;
        post_no_content(
            &client,
            &format!("{server_url}/v1/snapshots/{}/chunks", begin.upload_id),
            &PutChunkRequest {
                chunk_id: chunk_id.clone(),
                sha256: sha256_hex(&bytes),
                bytes_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
            },
        )
        .await?;
    }
    post_no_content(
        &client,
        &format!("{server_url}/v1/snapshots/{}/manifest", begin.upload_id),
        &PutManifestRequest {
            manifest_id,
            sha256: sha256_hex(&manifest_bytes),
            bytes_b64: base64::engine::general_purpose::STANDARD.encode(manifest_bytes),
        },
    )
    .await?;
    let commit = post_json::<_, CommitSnapshotResponse>(
        &client,
        &format!("{server_url}/v1/snapshots/{}/commit", begin.upload_id),
        &CommitSnapshotRequest {
            snapshot_id: snapshot.snapshot_id.clone(),
        },
    )
    .await?;
    let message_zh = upload_message_zh(&server_url, &logical_save_id, &commit, &snapshot);
    Ok(ServerUploadReport {
        server_url: server_url.clone(),
        sync_target: format!("{server_url}/v1/heads/{logical_save_id}"),
        account_handle: computed_account_handle,
        logical_save_id,
        device_id: input.device_id,
        snapshot_id: snapshot.snapshot_id,
        cloud_head_before,
        cloud_head: commit.head,
        conflict_snapshot: commit.conflict_snapshot,
        outcome: commit.outcome,
        missing_chunks_uploaded: begin.missing_chunk_ids.len(),
        chunk_count: chunk_ids.len(),
        manifest_uploaded: true,
        file_count: snapshot.fingerprint.file_count,
        total_bytes: snapshot.fingerprint.total_bytes,
        message_zh,
    })
}

async fn server_status(
    server_url: String,
    secret_hex: String,
    logical_save_id: Option<String>,
) -> anyhow::Result<ServerStatusReport> {
    let server_url = normalize_server_url(&server_url);
    let secret = secret_from_hex(&secret_hex)?;
    let keys = derive_account_keys(&secret)?;
    let computed_account_handle = account_handle(&keys);
    let descriptor = save_adapters::generic_folder_macos();
    let game_key = GameKey::new("generic", "fixture", "none", "slot1");
    let logical_save_id = logical_save_id
        .unwrap_or_else(|| stable_logical_save_id(&descriptor.emulator_id, &game_key).0);
    let client = reqwest::Client::new();
    let cloud_head = get_head(&client, &server_url, &logical_save_id).await?;
    let history = get_vec::<SnapshotRowResponse>(
        &client,
        &format!("{server_url}/v1/history/{logical_save_id}"),
    )
    .await?;
    let conflicts = get_vec::<SnapshotRowResponse>(
        &client,
        &format!("{server_url}/v1/conflicts/{logical_save_id}"),
    )
    .await?;
    let message_zh = match &cloud_head {
        Some(head) => format!(
            "云端当前 HEAD 是 {}，服务器 {} 上有 {} 个历史快照、{} 个冲突分支。",
            head,
            server_url,
            history.len(),
            conflicts.len()
        ),
        None => format!(
            "云端还没有 HEAD；当前逻辑存档 {} 尚未在服务器 {} 完成首次上传。",
            logical_save_id, server_url
        ),
    };
    Ok(ServerStatusReport {
        server_url: server_url.clone(),
        sync_target: format!("{server_url}/v1/heads/{logical_save_id}"),
        account_handle: computed_account_handle,
        logical_save_id,
        cloud_head,
        history_count: history.len(),
        conflict_count: conflicts.len(),
        message_zh,
    })
}

fn upload_message_zh(
    server_url: &str,
    logical_save_id: &str,
    commit: &CommitSnapshotResponse,
    snapshot: &EncryptedSnapshot,
) -> String {
    match commit.outcome.as_str() {
        "conflict" => format!(
            "检测到冲突：本地快照 {} 已上传到服务器 {} 作为冲突分支，不会覆盖云端 HEAD {}；请在客户端选择本地替换云端、云端覆盖本地或保留分支。逻辑存档 {}。",
            snapshot.snapshot_id, server_url, commit.head, logical_save_id
        ),
        _ => format!(
            "已上传到服务器 {}，逻辑存档 {} 的云端 HEAD 已更新为 {}。",
            server_url, logical_save_id, commit.head
        ),
    }
}

async fn get_head(
    client: &reqwest::Client,
    server_url: &str,
    logical_save_id: &str,
) -> anyhow::Result<Option<SnapshotId>> {
    let resp = client
        .get(format!("{server_url}/v1/heads/{logical_save_id}"))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let resp = ensure_success(resp).await?;
    Ok(Some(resp.json().await?))
}

async fn get_vec<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<Vec<T>> {
    let resp = client.get(url).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let resp = ensure_success(resp).await?;
    Ok(resp.json().await?)
}

async fn post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    body: &T,
) -> anyhow::Result<R> {
    let resp = client.post(url).json(body).send().await?;
    let resp = ensure_success(resp).await?;
    Ok(resp.json().await?)
}

async fn post_no_content<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    body: &T,
) -> anyhow::Result<()> {
    let resp = client.post(url).json(body).send().await?;
    ensure_success(resp).await?;
    Ok(())
}

async fn ensure_success(resp: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    anyhow::bail!("server returned {status}: {body}");
}

fn normalize_server_url(input: &str) -> String {
    input.trim().trim_end_matches('/').to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
