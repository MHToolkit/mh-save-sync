use base64::Engine;
use clap::{Parser, Subcommand, ValueEnum};
use ed25519_dalek::SigningKey;
use save_crypto::{
    EncryptedBlob, account_handle, account_root_signing_key, derive_account_keys,
    deterministic_cbor, issue_device_certificate_with_id, recovery_phrase_from_secret,
};
use save_domain::{
    DeviceId, FileKind, GameKey, LogicalSaveId, SnapshotId, SnapshotManifest, TreeFingerprint,
    stable_logical_save_id,
};
use save_engine::{
    EmulatorState, EncryptedSnapshot, EngineError, GameSaveDiffReport, SnapshotOptions,
    create_snapshot_from_stable_folder, decrypt_manifest, diff_folders_for_game,
    diff_manifests_for_game, export_encrypted_bundle, import_encrypted_bundle,
    restore_snapshot_to_folder,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::BTreeMap;
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
    SaveDiff {
        #[arg(long)]
        left: PathBuf,
        #[arg(long)]
        right: PathBuf,
        #[arg(long, default_value = "mh3g-3ds")]
        game_profile: String,
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
        /// Explicitly make this stable local snapshot replace the HEAD observed
        /// immediately before upload. The server still uses CAS, so a concurrent
        /// HEAD change becomes a conflict branch instead of being overwritten.
        #[arg(long, conflicts_with = "base_head")]
        replace_cloud_head: bool,
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
        #[arg(long, default_value = "mh3g-3ds")]
        game_profile: String,
    },
    ServerRestore {
        #[arg(long, env = "MH_SAVE_SYNC_SERVER_URL")]
        server_url: String,
        #[arg(long)]
        secret_hex: String,
        #[arg(long)]
        logical_save_id: Option<String>,
        #[arg(long)]
        snapshot_id: Option<String>,
        #[arg(long)]
        target: PathBuf,
        #[arg(long, value_enum, default_value_t = CliEmulatorState::Stopped)]
        emulator_state: CliEmulatorState,
    },
    /// Mark one retained conflict as handled after the user has explicitly chosen a side.
    ServerResolveConflict {
        #[arg(long, env = "MH_SAVE_SYNC_SERVER_URL")]
        server_url: String,
        #[arg(long)]
        secret_hex: String,
        #[arg(long)]
        logical_save_id: Option<String>,
        #[arg(long)]
        conflict_snapshot_id: String,
        #[arg(long)]
        chosen_snapshot_id: String,
        #[arg(long, value_enum)]
        resolution: CliConflictResolution,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliEmulatorState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CliConflictResolution {
    KeepCloudHead,
    ReplaceWithLocal,
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
async fn main() {
    if let Err(error) = run().await {
        print_cli_error(&error);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
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
        Commands::SaveDiff {
            left,
            right,
            game_profile,
        } => {
            let descriptor = descriptor_for_game_profile(&game_profile);
            let report = diff_folders_for_game(&left, &right, &descriptor, &game_profile)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::ServerUpload {
            server_url,
            root,
            secret_hex,
            base_head,
            replace_cloud_head,
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
                replace_cloud_head,
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
            game_profile,
        } => {
            let report =
                server_status(server_url, secret_hex, logical_save_id, game_profile).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::ServerRestore {
            server_url,
            secret_hex,
            logical_save_id,
            snapshot_id,
            target,
            emulator_state,
        } => {
            let report = server_restore(
                server_url,
                secret_hex,
                logical_save_id,
                snapshot_id,
                target,
                emulator_state,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::ServerResolveConflict {
            server_url,
            secret_hex,
            logical_save_id,
            conflict_snapshot_id,
            chosen_snapshot_id,
            resolution,
        } => {
            let report = server_resolve_conflict(
                server_url,
                secret_hex,
                logical_save_id,
                conflict_snapshot_id,
                chosen_snapshot_id,
                resolution,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

fn print_cli_error(error: &anyhow::Error) {
    if matches!(
        error.downcast_ref::<EngineError>(),
        Some(EngineError::EmulatorRunning)
    ) {
        eprintln!(
            "{}",
            serde_json::json!({
                "error_code": "emulator_running",
                "message": "restore refused while emulator is running",
                "message_zh": "已拒绝恢复：模拟器仍在运行，没有覆盖本地存档。请先退出游戏/模拟器，再执行云端覆盖本地。"
            })
        );
        return;
    }
    eprintln!("{error:?}");
}

#[derive(Debug)]
struct ServerUploadInput {
    server_url: String,
    root: PathBuf,
    secret_hex: String,
    base_head: Option<String>,
    replace_cloud_head: bool,
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
    game_profile: String,
    cloud_head: Option<SnapshotId>,
    history_count: usize,
    conflict_count: usize,
    conflict_diffs: Vec<ConflictDiffReport>,
    message_zh: String,
}

#[derive(Debug, Serialize)]
struct ConflictDiffReport {
    current_head: SnapshotId,
    conflict_snapshot: SnapshotId,
    diff: GameSaveDiffReport,
    message_zh: String,
}

#[derive(Debug, Serialize)]
struct ServerRestoreReport {
    server_url: String,
    sync_target: String,
    account_handle: String,
    logical_save_id: String,
    snapshot_id: SnapshotId,
    restored: PathBuf,
    backup: PathBuf,
    file_count: u64,
    total_bytes: u64,
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

#[derive(Debug, Serialize)]
struct AccountBootstrapRequest {
    account_handle: String,
    root_public_key_b64: String,
}

#[derive(Debug, Serialize)]
struct DeviceRegisterRequest {
    account_handle: String,
    cert_id: String,
    device_public_key_b64: String,
    certificate_b64: String,
}

struct ClientDeviceIdentity {
    account_handle: String,
    device_cert_id: String,
    signing_key: SigningKey,
}

#[derive(Debug, Serialize)]
struct ChallengeRequest<'a> {
    account_handle: &'a str,
    device_cert_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChallengeResponse {
    challenge_id: String,
    nonce_b64: String,
    expires_unix_seconds: u64,
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

#[derive(Debug, Serialize)]
struct ResolveConflictRequest {
    chosen_snapshot_id: SnapshotId,
    resolution: CliConflictResolution,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResolveConflictResponse {
    conflict_snapshot_id: SnapshotId,
    chosen_snapshot_id: SnapshotId,
    resolution: String,
    resolved: bool,
}

#[derive(Debug, Deserialize)]
struct SnapshotObjectDownload {
    object_id: String,
    sha256: String,
    bytes_b64: String,
}

#[derive(Debug, Deserialize)]
struct SnapshotDownloadResponse {
    snapshot_id: SnapshotId,
    encrypted_manifest: SnapshotObjectDownload,
    chunks: Vec<SnapshotObjectDownload>,
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
    let client = reqwest::Client::new();
    let default_identity =
        ensure_account_device_registered(&client, &server_url, &keys, &computed_account_handle)
            .await?;
    let cloud_head_before =
        get_head(&client, &server_url, &logical_save_id, &default_identity).await?;
    let base_head = if input.replace_cloud_head {
        cloud_head_before.clone()
    } else {
        input.base_head.map(SnapshotId)
    };
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
    let request_account_handle = input
        .account_handle
        .clone()
        .unwrap_or_else(|| default_identity.account_handle.clone());
    let request_device_cert_id = input
        .device_cert_id
        .clone()
        .unwrap_or_else(|| default_identity.device_cert_id.clone());
    if let Some(current_head) = &cloud_head_before
        && let Some(remote_fingerprint) = remote_snapshot_fingerprint(
            &client,
            &server_url,
            &secret,
            &default_identity,
            current_head,
        )
        .await?
        && remote_fingerprint == snapshot.fingerprint
    {
        let message_zh = format!(
            "云端已经是同一份稳定存档：服务器 {} 的逻辑存档 {} 当前 HEAD {} 与本地文件指纹一致；没有重复上传，也没有新增冲突分支。",
            server_url, logical_save_id, current_head
        );
        return Ok(ServerUploadReport {
            server_url: server_url.clone(),
            sync_target: format!("{server_url}/v1/heads/{logical_save_id}"),
            account_handle: computed_account_handle,
            logical_save_id,
            device_id: input.device_id,
            snapshot_id: current_head.clone(),
            cloud_head_before: Some(current_head.clone()),
            cloud_head: current_head.clone(),
            conflict_snapshot: None,
            outcome: "up-to-date".into(),
            missing_chunks_uploaded: 0,
            chunk_count: snapshot.chunks.len(),
            manifest_uploaded: false,
            file_count: snapshot.fingerprint.file_count,
            total_bytes: snapshot.fingerprint.total_bytes,
            message_zh,
        });
    }
    let manifest_bytes = serde_json::to_vec(&snapshot.encrypted_manifest)?;
    let manifest_id = sha256_hex(&manifest_bytes);
    let mut chunk_ids = snapshot.chunks.keys().cloned().collect::<Vec<_>>();
    chunk_ids.sort();
    let begin = signed_post_json::<_, BeginSnapshotResponse>(
        &client,
        &format!("{server_url}/v1/snapshots/begin"),
        &server_url,
        &default_identity,
        &BeginSnapshotRequest {
            account_handle: Some(&request_account_handle),
            device_cert_id: Some(&request_device_cert_id),
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
        signed_post_no_content(
            &client,
            &format!("{server_url}/v1/snapshots/{}/chunks", begin.upload_id),
            &server_url,
            &default_identity,
            &PutChunkRequest {
                chunk_id: chunk_id.clone(),
                sha256: sha256_hex(&bytes),
                bytes_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
            },
        )
        .await?;
    }
    signed_post_no_content(
        &client,
        &format!("{server_url}/v1/snapshots/{}/manifest", begin.upload_id),
        &server_url,
        &default_identity,
        &PutManifestRequest {
            manifest_id,
            sha256: sha256_hex(&manifest_bytes),
            bytes_b64: base64::engine::general_purpose::STANDARD.encode(manifest_bytes),
        },
    )
    .await?;
    let commit = signed_post_json::<_, CommitSnapshotResponse>(
        &client,
        &format!("{server_url}/v1/snapshots/{}/commit", begin.upload_id),
        &server_url,
        &default_identity,
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

async fn ensure_account_device_registered(
    client: &reqwest::Client,
    server_url: &str,
    keys: &save_crypto::AccountKeys,
    computed_account_handle: &str,
) -> anyhow::Result<ClientDeviceIdentity> {
    let root = account_root_signing_key(keys);
    let device = deterministic_cli_device_key(keys);
    let cert_id = deterministic_cli_device_cert_id(&device);
    let cert_id_hex = hex::encode(cert_id);
    let certificate = deterministic_cli_device_certificate(&root, &device, cert_id)?;

    post_no_content(
        client,
        &format!("{server_url}/v1/accounts/bootstrap"),
        &AccountBootstrapRequest {
            account_handle: computed_account_handle.to_owned(),
            root_public_key_b64: base64::engine::general_purpose::STANDARD
                .encode(root.verifying_key().to_bytes()),
        },
    )
    .await?;
    post_no_content(
        client,
        &format!("{server_url}/v1/devices/register"),
        &DeviceRegisterRequest {
            account_handle: computed_account_handle.to_owned(),
            cert_id: cert_id_hex.clone(),
            device_public_key_b64: base64::engine::general_purpose::STANDARD
                .encode(device.verifying_key().to_bytes()),
            certificate_b64: base64::engine::general_purpose::STANDARD
                .encode(deterministic_cbor(&certificate)?),
        },
    )
    .await?;

    Ok(ClientDeviceIdentity {
        account_handle: computed_account_handle.to_owned(),
        device_cert_id: cert_id_hex,
        signing_key: device,
    })
}

fn deterministic_cli_device_certificate(
    root: &SigningKey,
    device: &SigningKey,
    cert_id: [u8; 16],
) -> anyhow::Result<save_crypto::DeviceCertificate> {
    issue_device_certificate_with_id(
        root,
        &device.verifying_key(),
        cert_id,
        1_700_000_000,
        4_102_444_800,
        1,
    )
    .map_err(Into::into)
}

fn deterministic_cli_device_key(keys: &save_crypto::AccountKeys) -> SigningKey {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"mh-save-sync/cli-device-signing-seed/v1");
    hasher.update(keys.auth);
    let seed: [u8; 32] = hasher.finalize().into();
    SigningKey::from_bytes(&seed)
}

fn deterministic_cli_device_cert_id(device: &SigningKey) -> [u8; 16] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"mh-save-sync/cli-device-cert-id/v1");
    hasher.update(device.verifying_key().to_bytes());
    let digest = hasher.finalize();
    let mut cert_id = [0u8; 16];
    cert_id.copy_from_slice(&digest[..16]);
    cert_id
}

async fn server_status(
    server_url: String,
    secret_hex: String,
    logical_save_id: Option<String>,
    game_profile: String,
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
    let identity =
        ensure_account_device_registered(&client, &server_url, &keys, &computed_account_handle)
            .await?;
    let cloud_head = get_head(&client, &server_url, &logical_save_id, &identity).await?;
    let history = get_vec::<SnapshotRowResponse>(
        &client,
        &format!("{server_url}/v1/history/{logical_save_id}"),
        &server_url,
        &identity,
    )
    .await?;
    let conflicts = get_vec::<SnapshotRowResponse>(
        &client,
        &format!("{server_url}/v1/conflicts/{logical_save_id}"),
        &server_url,
        &identity,
    )
    .await?;
    let conflict_diffs = conflict_diff_reports(
        &client,
        &server_url,
        &secret,
        &identity,
        cloud_head.as_ref(),
        &conflicts,
        &game_profile,
    )
    .await?;
    let message_zh = match &cloud_head {
        Some(head) => {
            let parser_note = if conflict_diffs.is_empty() {
                String::new()
            } else {
                format!(
                    " 已按游戏档案 {} 用客户端恢复密钥解析 {} 个冲突分支的文件/字节级差异。",
                    game_profile,
                    conflict_diffs.len()
                )
            };
            format!(
                "云端当前 HEAD 是 {}，服务器 {} 上有 {} 个历史快照、{} 个冲突分支。{}",
                head,
                server_url,
                history.len(),
                conflicts.len(),
                parser_note
            )
        }
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
        game_profile,
        cloud_head,
        history_count: history.len(),
        conflict_count: conflicts.len(),
        conflict_diffs,
        message_zh,
    })
}

async fn server_resolve_conflict(
    server_url: String,
    secret_hex: String,
    logical_save_id: Option<String>,
    conflict_snapshot_id: String,
    chosen_snapshot_id: String,
    resolution: CliConflictResolution,
) -> anyhow::Result<ResolveConflictResponse> {
    let server_url = normalize_server_url(&server_url);
    let secret = secret_from_hex(&secret_hex)?;
    let keys = derive_account_keys(&secret)?;
    let computed_account_handle = account_handle(&keys);
    let descriptor = save_adapters::generic_folder_macos();
    let game_key = GameKey::new("generic", "fixture", "none", "slot1");
    let logical_save_id = logical_save_id
        .unwrap_or_else(|| stable_logical_save_id(&descriptor.emulator_id, &game_key).0);
    let client = reqwest::Client::new();
    let identity =
        ensure_account_device_registered(&client, &server_url, &keys, &computed_account_handle)
            .await?;
    signed_post_json(
        &client,
        &format!("{server_url}/v1/conflicts/{logical_save_id}/{conflict_snapshot_id}/resolve"),
        &server_url,
        &identity,
        &ResolveConflictRequest {
            chosen_snapshot_id: SnapshotId(chosen_snapshot_id),
            resolution,
        },
    )
    .await
}

async fn remote_snapshot_fingerprint(
    client: &reqwest::Client,
    server_url: &str,
    secret: &[u8; 32],
    identity: &ClientDeviceIdentity,
    snapshot_id: &SnapshotId,
) -> anyhow::Result<Option<TreeFingerprint>> {
    let Some(manifest) =
        remote_snapshot_manifest(client, server_url, secret, identity, snapshot_id).await?
    else {
        return Ok(None);
    };
    Ok(Some(fingerprint_manifest_entries(&manifest)?))
}

async fn remote_snapshot_manifest(
    client: &reqwest::Client,
    server_url: &str,
    secret: &[u8; 32],
    identity: &ClientDeviceIdentity,
    snapshot_id: &SnapshotId,
) -> anyhow::Result<Option<SnapshotManifest>> {
    let url = format!(
        "{server_url}/v1/snapshots/{}/encrypted-bundle",
        snapshot_id.0
    );
    let resp = signed_get(client, &url, server_url, identity).await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let bundle: SnapshotDownloadResponse = ensure_success(resp).await?.json().await?;
    let encrypted_manifest = decode_downloaded_blob(&bundle.encrypted_manifest)?;
    let snapshot = EncryptedSnapshot {
        snapshot_id: bundle.snapshot_id,
        encrypted_manifest,
        chunks: BTreeMap::new(),
        fingerprint: TreeFingerprint {
            file_count: 0,
            total_bytes: 0,
            sha256: "remote-before-decrypt".into(),
        },
    };
    Ok(Some(decrypt_manifest(secret, &snapshot)?))
}

async fn conflict_diff_reports(
    client: &reqwest::Client,
    server_url: &str,
    secret: &[u8; 32],
    identity: &ClientDeviceIdentity,
    cloud_head: Option<&SnapshotId>,
    conflicts: &[SnapshotRowResponse],
    game_profile: &str,
) -> anyhow::Result<Vec<ConflictDiffReport>> {
    let Some(current_head) = cloud_head else {
        return Ok(Vec::new());
    };
    if conflicts.is_empty() {
        return Ok(Vec::new());
    }
    let Some(head_manifest) =
        remote_snapshot_manifest(client, server_url, secret, identity, current_head).await?
    else {
        return Ok(Vec::new());
    };
    let mut reports = Vec::new();
    for row in conflicts.iter().take(20) {
        if let Some(conflict_manifest) =
            remote_snapshot_manifest(client, server_url, secret, identity, &row.snapshot_id).await?
        {
            let diff = diff_manifests_for_game(&head_manifest, &conflict_manifest, game_profile)?;
            let message_zh = format!(
                "冲突分支 {} 相对当前云端 HEAD {}（游戏档案 {}）：{}",
                row.snapshot_id, current_head, game_profile, diff.summary_zh
            );
            reports.push(ConflictDiffReport {
                current_head: current_head.clone(),
                conflict_snapshot: row.snapshot_id.clone(),
                diff,
                message_zh,
            });
        }
    }
    Ok(reports)
}

fn fingerprint_manifest_entries(manifest: &SnapshotManifest) -> anyhow::Result<TreeFingerprint> {
    let mut files = manifest
        .entries
        .iter()
        .filter(|entry| entry.kind != FileKind::Tombstone)
        .collect::<Vec<_>>();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut h = sha2::Sha256::new();
    let mut total = 0u64;
    for entry in &files {
        total += entry.size;
        h.update(entry.path.as_bytes());
        h.update([0]);
        h.update(entry.size.to_be_bytes());
        h.update([0]);
        h.update(hex::decode(&entry.plaintext_sha256)?);
    }
    Ok(TreeFingerprint {
        file_count: files.len() as u64,
        total_bytes: total,
        sha256: hex::encode(h.finalize()),
    })
}

async fn server_restore(
    server_url: String,
    secret_hex: String,
    logical_save_id: Option<String>,
    snapshot_id: Option<String>,
    target: PathBuf,
    emulator_state: CliEmulatorState,
) -> anyhow::Result<ServerRestoreReport> {
    let server_url = normalize_server_url(&server_url);
    let secret = secret_from_hex(&secret_hex)?;
    let keys = derive_account_keys(&secret)?;
    let computed_account_handle = account_handle(&keys);
    let descriptor = save_adapters::generic_folder_macos();
    let game_key = GameKey::new("generic", "fixture", "none", "slot1");
    let logical_save_id = logical_save_id
        .unwrap_or_else(|| stable_logical_save_id(&descriptor.emulator_id, &game_key).0);
    let client = reqwest::Client::new();
    let identity =
        ensure_account_device_registered(&client, &server_url, &keys, &computed_account_handle)
            .await?;
    let snapshot_id = match snapshot_id {
        Some(id) => SnapshotId(id),
        None => get_head(&client, &server_url, &logical_save_id, &identity)
            .await?
            .ok_or_else(|| anyhow::anyhow!("cloud head not found for {logical_save_id}"))?,
    };
    let bundle = get_json::<SnapshotDownloadResponse>(
        &client,
        &format!(
            "{server_url}/v1/snapshots/{}/encrypted-bundle",
            snapshot_id.0
        ),
        &server_url,
        &identity,
    )
    .await?;
    anyhow::ensure!(
        bundle.snapshot_id == snapshot_id,
        "server returned different snapshot id"
    );
    let encrypted_manifest = decode_downloaded_blob(&bundle.encrypted_manifest)?;
    let mut chunks = BTreeMap::new();
    for chunk in &bundle.chunks {
        chunks.insert(chunk.object_id.clone(), decode_downloaded_blob(chunk)?);
    }
    let mut snapshot = EncryptedSnapshot {
        snapshot_id: bundle.snapshot_id,
        encrypted_manifest,
        chunks,
        fingerprint: TreeFingerprint {
            file_count: 0,
            total_bytes: 0,
            sha256: "downloaded-before-decrypt".into(),
        },
    };
    let manifest = decrypt_manifest(&secret, &snapshot)?;
    let file_count = manifest
        .entries
        .iter()
        .filter(|entry| entry.kind != FileKind::Tombstone)
        .count() as u64;
    let total_bytes = manifest
        .entries
        .iter()
        .filter(|entry| entry.kind != FileKind::Tombstone)
        .map(|entry| entry.size)
        .sum::<u64>();
    snapshot.fingerprint = TreeFingerprint {
        file_count,
        total_bytes,
        sha256: hex::encode(sha2::Sha256::digest(serde_json::to_vec(&manifest)?)),
    };
    let backup = restore_snapshot_to_folder(
        &secret,
        &snapshot,
        &target,
        EmulatorState::from(emulator_state),
    )?;
    Ok(ServerRestoreReport {
        server_url: server_url.clone(),
        sync_target: format!("{server_url}/v1/heads/{logical_save_id}"),
        account_handle: computed_account_handle,
        logical_save_id,
        snapshot_id: snapshot.snapshot_id,
        restored: target.clone(),
        backup: backup.clone(),
        file_count,
        total_bytes,
        message_zh: format!(
            "已从服务器下载并恢复快照到本地目录；恢复前备份已保留。服务器：{}，快照：{}。",
            server_url, snapshot_id
        ),
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
    identity: &ClientDeviceIdentity,
) -> anyhow::Result<Option<SnapshotId>> {
    let url = format!("{server_url}/v1/heads/{logical_save_id}");
    let resp = signed_get(client, &url, server_url, identity).await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let resp = ensure_success(resp).await?;
    Ok(Some(resp.json().await?))
}

async fn get_vec<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    server_url: &str,
    identity: &ClientDeviceIdentity,
) -> anyhow::Result<Vec<T>> {
    let resp = signed_get(client, url, server_url, identity).await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    let resp = ensure_success(resp).await?;
    Ok(resp.json().await?)
}

async fn get_json<R: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    server_url: &str,
    identity: &ClientDeviceIdentity,
) -> anyhow::Result<R> {
    let resp = signed_get(client, url, server_url, identity).await?;
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

async fn signed_raw_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    server_url: &str,
    identity: &ClientDeviceIdentity,
    body: Vec<u8>,
) -> anyhow::Result<reqwest::Response> {
    let challenge: ChallengeResponse = post_json(
        client,
        &format!("{server_url}/v1/accounts/challenge"),
        &ChallengeRequest {
            account_handle: &identity.account_handle,
            device_cert_id: &identity.device_cert_id,
        },
    )
    .await?;
    anyhow::ensure!(
        challenge.expires_unix_seconds >= unix_seconds(),
        "server returned expired auth challenge"
    );
    let parsed = reqwest::Url::parse(url)?;
    let path = parsed.path().to_owned()
        + parsed
            .query()
            .map(|q| format!("?{q}"))
            .as_deref()
            .unwrap_or("");
    let timestamp = unix_seconds();
    let signature = save_crypto::sign_http_request(
        &identity.signing_key,
        method.as_str(),
        &path,
        &body,
        &challenge.challenge_id,
        &challenge.nonce_b64,
        timestamp,
    );
    Ok(client
        .request(method, url)
        .header("content-type", "application/json")
        .header("x-mh-account", &identity.account_handle)
        .header("x-mh-device-cert", &identity.device_cert_id)
        .header("x-mh-challenge-id", &challenge.challenge_id)
        .header("x-mh-nonce", &challenge.nonce_b64)
        .header("x-mh-timestamp", timestamp.to_string())
        .header(
            "x-mh-signature",
            base64::engine::general_purpose::STANDARD.encode(signature),
        )
        .body(body)
        .send()
        .await?)
}

async fn signed_request<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    server_url: &str,
    identity: &ClientDeviceIdentity,
    body: &T,
) -> anyhow::Result<reqwest::Response> {
    signed_raw_request(
        client,
        reqwest::Method::POST,
        url,
        server_url,
        identity,
        serde_json::to_vec(body)?,
    )
    .await
}

async fn signed_get(
    client: &reqwest::Client,
    url: &str,
    server_url: &str,
    identity: &ClientDeviceIdentity,
) -> anyhow::Result<reqwest::Response> {
    signed_raw_request(
        client,
        reqwest::Method::GET,
        url,
        server_url,
        identity,
        Vec::new(),
    )
    .await
}

async fn signed_post_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    server_url: &str,
    identity: &ClientDeviceIdentity,
    body: &T,
) -> anyhow::Result<R> {
    let response = signed_request(client, url, server_url, identity, body).await?;
    Ok(ensure_success(response).await?.json().await?)
}

async fn signed_post_no_content<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    server_url: &str,
    identity: &ClientDeviceIdentity,
    body: &T,
) -> anyhow::Result<()> {
    ensure_success(signed_request(client, url, server_url, identity, body).await?).await?;
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

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn decode_downloaded_blob(object: &SnapshotObjectDownload) -> anyhow::Result<EncryptedBlob> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(&object.bytes_b64)?;
    anyhow::ensure!(
        sha256_hex(&bytes) == object.sha256.to_lowercase(),
        "downloaded object checksum mismatch: {}",
        object.object_id
    );
    Ok(serde_json::from_slice(&bytes)?)
}

fn normalize_server_url(input: &str) -> String {
    input.trim().trim_end_matches('/').to_string()
}

fn descriptor_for_game_profile(game_profile: &str) -> save_domain::AdapterDescriptor {
    match game_profile {
        "mh3g-3ds" => save_adapters::nemessix_macos(),
        _ => save_adapters::generic_folder_macos(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_fingerprint_matches_tree_fingerprint_for_same_plaintext() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("slot1")).unwrap();
        std::fs::write(tmp.path().join("slot1/main.bin"), b"same-save").unwrap();
        let descriptor = save_adapters::generic_folder_macos();
        let tree = save_engine::fingerprint_tree(tmp.path(), &descriptor.exclude_globs).unwrap();
        let mut options =
            SnapshotOptions::fixture(GameKey::new("generic", "fixture", "none", "slot1"));
        options.created_unix_ms = 1;
        let snapshot =
            create_snapshot_from_stable_folder(tmp.path(), &descriptor, &[0x33; 32], options)
                .unwrap();
        let manifest = decrypt_manifest(&[0x33; 32], &snapshot).unwrap();
        let manifest_fingerprint = fingerprint_manifest_entries(&manifest).unwrap();
        assert_eq!(tree, manifest_fingerprint);
    }

    #[test]
    fn deterministic_cli_certificate_is_idempotent_for_repeated_registration() {
        let keys = derive_account_keys(&[0x33; 32]).unwrap();
        let root = account_root_signing_key(&keys);
        let device = deterministic_cli_device_key(&keys);
        let cert_id = deterministic_cli_device_cert_id(&device);
        let first = deterministic_cli_device_certificate(&root, &device, cert_id).unwrap();
        let second = deterministic_cli_device_certificate(&root, &device, cert_id).unwrap();
        assert_eq!(
            deterministic_cbor(&first).unwrap(),
            deterministic_cbor(&second).unwrap()
        );
    }
}
