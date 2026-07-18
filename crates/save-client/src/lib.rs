use base64::Engine;
use ed25519_dalek::SigningKey;
use save_crypto::{
    EncryptedBlob, account_handle, account_root_signing_key, derive_account_keys,
    deterministic_cbor, issue_device_certificate_with_id,
};
use save_domain::{AdapterDescriptor, SnapshotId};
use save_domain::{DeviceId, GameKey, LogicalSaveId};
#[cfg(target_os = "android")]
use save_engine::import_encrypted_bundle;
use save_engine::{
    EmulatorState, EncryptedSnapshot, HeadUpdate, decide_head_update, decrypt_manifest,
    export_encrypted_bundle, restore_snapshot_to_folder,
};
use save_engine::{SnapshotOptions, create_snapshot_from_stable_folder};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::Path;
#[cfg(target_os = "android")]
use zeroize::Zeroizing;

uniffi::setup_scaffolding!();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPolicy {
    pub wifi_only: bool,
    pub battery_not_low: bool,
    pub charging_required: bool,
    pub auto_download_to_cas: bool,
    pub auto_restore: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self {
            wifi_only: true,
            battery_not_low: true,
            charging_required: false,
            auto_download_to_cas: true,
            auto_restore: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisibleState {
    Ready,
    PendingUpload,
    OfflineQueued,
    Conflict,
    PermissionRequired,
    RestoreBlockedRunning,
    Error(String),
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInfo {
    pub bridge_version: String,
    pub snapshot_format_version: u32,
    pub watcher_behavior: String,
    pub automatic_restore: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeHeadKind {
    FirstSnapshot,
    FastForward,
    Conflict,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeHeadDecision {
    pub kind: BridgeHeadKind,
    pub head: String,
    pub conflict_snapshot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum LaunchGateKind {
    Ready,
    RemoteNewer,
    Conflict,
    CloudUnavailable,
    PermissionRequired,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ConflictSideInfo {
    pub label_zh: String,
    pub device_name: String,
    pub snapshot_id: String,
    pub parent_snapshot_id: Option<String>,
    pub captured_at_zh: String,
    pub size_bytes: u64,
    pub hash_prefix: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct LaunchGateDecisionZh {
    pub kind: LaunchGateKind,
    pub title_zh: String,
    pub summary_zh: String,
    pub primary_action_zh: String,
    pub secondary_action_zh: String,
    pub allows_local_play: bool,
    pub allows_restore_now: bool,
    pub local_side: Option<ConflictSideInfo>,
    pub remote_side: Option<ConflictSideInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum AutomationEventKind {
    DirtyObserved,
    SaveComplete,
    EmulatorExit,
    PeriodicReconcile,
    ManualSync,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AutomationDecisionZh {
    pub event_kind: String,
    pub mark_dirty: bool,
    pub create_snapshot_candidate: bool,
    pub upload_allowed: bool,
    pub download_to_cas_allowed: bool,
    pub restore_allowed: bool,
    pub summary_zh: String,
}

#[uniffi::export]
pub fn decide_automation_event(
    event: AutomationEventKind,
    dirty: bool,
    emulator_stopped: bool,
) -> AutomationDecisionZh {
    match event {
        AutomationEventKind::DirtyObserved => AutomationDecisionZh {
            event_kind: "dirty-observed".into(),
            mark_dirty: true,
            create_snapshot_candidate: false,
            upload_allowed: false,
            download_to_cas_allowed: false,
            restore_allowed: false,
            summary_zh:
                "文件变化/FSEvents/FileObserver 只标记 dirty，不直接上传，也不会覆盖本地存档。"
                    .into(),
        },
        AutomationEventKind::SaveComplete => reconcile_decision(
            "save-complete",
            dirty,
            emulator_stopped,
            "收到明确 save-complete 后才允许进入稳定窗口、staging copy 和 manifest/hash 校验。",
        ),
        AutomationEventKind::EmulatorExit => reconcile_decision(
            "emulator-exit",
            dirty,
            emulator_stopped,
            "模拟器正常退出后执行 session-boundary 对账，形成稳定快照候选。",
        ),
        AutomationEventKind::PeriodicReconcile => reconcile_decision(
            "periodic-reconcile",
            dirty,
            emulator_stopped,
            "定时兜底只处理已 dirty 的目录，无变化时不读全量文件；有变化也必须先形成稳定快照。",
        ),
        AutomationEventKind::ManualSync => reconcile_decision(
            "manual-sync",
            true,
            emulator_stopped,
            "手动同步立即触发稳定快照候选，但仍经过 staging、manifest/hash 和一致性校验。",
        ),
    }
}

fn reconcile_decision(
    event_kind: &str,
    dirty: bool,
    emulator_stopped: bool,
    prefix: &str,
) -> AutomationDecisionZh {
    let create_snapshot_candidate = dirty;
    AutomationDecisionZh {
        event_kind: event_kind.into(),
        mark_dirty: dirty,
        create_snapshot_candidate,
        upload_allowed: create_snapshot_candidate,
        download_to_cas_allowed: true,
        restore_allowed: false,
        summary_zh: if create_snapshot_candidate {
            format!(
                "{} 当前只允许上传已验证稳定快照；恢复仍需单独确认模拟器停止。emulator_stopped={}.",
                prefix, emulator_stopped
            )
        } else {
            format!("{} 当前没有 dirty 标记，不创建快照候选。", prefix)
        },
    }
}

#[uniffi::export]
pub fn decide_restore_event(
    emulator_running: bool,
    remote_available: bool,
) -> AutomationDecisionZh {
    if emulator_running {
        return AutomationDecisionZh {
            event_kind: "restore".into(),
            mark_dirty: false,
            create_snapshot_candidate: false,
            upload_allowed: false,
            download_to_cas_allowed: remote_available,
            restore_allowed: false,
            summary_zh: "模拟器运行中禁止云端覆盖本地；云端内容最多先下载到 local CAS，退出后再由用户确认恢复。".into(),
        };
    }
    AutomationDecisionZh {
        event_kind: "restore".into(),
        mark_dirty: false,
        create_snapshot_candidate: false,
        upload_allowed: false,
        download_to_cas_allowed: remote_available,
        restore_allowed: remote_available,
        summary_zh: "模拟器已停止且云端可用时，恢复前必须先快照/备份当前本地状态，再从 staging 提交到原目录。".into(),
    }
}

#[uniffi::export]
pub fn describe_launch_gate_zh(
    saf_authorized: bool,
    cloud_reachable: bool,
    emulator_running: bool,
    local_head: Option<String>,
    remote_head: Option<String>,
) -> LaunchGateDecisionZh {
    if !saf_authorized {
        return LaunchGateDecisionZh {
            kind: LaunchGateKind::PermissionRequired,
            title_zh: "需要先授权存档目录".into(),
            summary_zh: "还没有 Android SAF 或 macOS 本地目录权限，无法判断本地/云端哪个版本更新。"
                .into(),
            primary_action_zh: "选择存档目录".into(),
            secondary_action_zh: "暂不启动".into(),
            allows_local_play: false,
            allows_restore_now: false,
            local_side: None,
            remote_side: None,
        };
    }
    if !cloud_reachable {
        return LaunchGateDecisionZh {
            kind: LaunchGateKind::CloudUnavailable,
            title_zh: "云端暂时不可用".into(),
            summary_zh: "不会破坏本地原始存档。你可以继续使用本地存档游玩；退出后本地快照会保留在队列里，云端恢复后再上传。".into(),
            primary_action_zh: "继续使用本地".into(),
            secondary_action_zh: "稍后重试同步".into(),
            allows_local_play: true,
            allows_restore_now: false,
            local_side: None,
            remote_side: None,
        };
    }
    match (local_head, remote_head) {
        (Some(local), Some(remote)) if local != remote => LaunchGateDecisionZh {
            kind: LaunchGateKind::Conflict,
            title_zh: "发现本地与云端冲突".into(),
            summary_zh: "本地和云端都从同一历史分叉后发生过修改。不会按最新时间自动覆盖；需要用户选择本地替换云端、云端覆盖本地，或保留为分支。".into(),
            primary_action_zh: "选择云端覆盖本地".into(),
            secondary_action_zh: "选择本地替换云端".into(),
            allows_local_play: true,
            allows_restore_now: !emulator_running,
            local_side: Some(ConflictSideInfo {
                label_zh: "本地".into(),
                device_name: "当前设备".into(),
                snapshot_id: local.clone(),
                parent_snapshot_id: None,
                captured_at_zh: "等待本地元数据".into(),
                size_bytes: 0,
                hash_prefix: local.chars().take(12).collect(),
            }),
            remote_side: Some(ConflictSideInfo {
                label_zh: "云端".into(),
                device_name: "远端设备".into(),
                snapshot_id: remote.clone(),
                parent_snapshot_id: None,
                captured_at_zh: "等待云端元数据".into(),
                size_bytes: 0,
                hash_prefix: remote.chars().take(12).collect(),
            }),
        },
        (local, Some(remote)) if local.as_ref() != Some(&remote) => LaunchGateDecisionZh {
            kind: LaunchGateKind::RemoteNewer,
            title_zh: "云端有可恢复版本".into(),
            summary_zh: "云端存在本机没有的快照。只会先下载到本地 CAS 缓存；真正覆盖模拟器目录前必须确认模拟器已停止，并先备份当前本地状态。".into(),
            primary_action_zh: if emulator_running {
                "先关闭模拟器再恢复".into()
            } else {
                "下载并恢复云端".into()
            },
            secondary_action_zh: "继续使用本地".into(),
            allows_local_play: true,
            allows_restore_now: !emulator_running,
            local_side: local.map(|head| ConflictSideInfo {
                label_zh: "本地".into(),
                device_name: "当前设备".into(),
                snapshot_id: head.clone(),
                parent_snapshot_id: None,
                captured_at_zh: "本机当前 HEAD".into(),
                size_bytes: 0,
                hash_prefix: head.chars().take(12).collect(),
            }),
            remote_side: Some(ConflictSideInfo {
                label_zh: "云端".into(),
                device_name: "远端设备".into(),
                snapshot_id: remote.clone(),
                parent_snapshot_id: None,
                captured_at_zh: "云端 HEAD".into(),
                size_bytes: 0,
                hash_prefix: remote.chars().take(12).collect(),
            }),
        },
        _ => LaunchGateDecisionZh {
            kind: LaunchGateKind::Ready,
            title_zh: "可以启动游戏".into(),
            summary_zh: "本地和云端没有发现需要用户处理的差异。启动后文件变化只会标记 dirty；退出或 save-complete 后才会形成稳定快照。".into(),
            primary_action_zh: "启动 Nemessix".into(),
            secondary_action_zh: "稍后手动同步".into(),
            allows_local_play: true,
            allows_restore_now: false,
            local_side: None,
            remote_side: None,
        },
    }
}

#[uniffi::export]
pub fn bridge_info() -> BridgeInfo {
    BridgeInfo {
        bridge_version: "1.0".into(),
        snapshot_format_version: 1,
        watcher_behavior: "dirty-only".into(),
        automatic_restore: false,
    }
}

#[uniffi::export]
pub fn bridge_head_decision(
    base: Option<String>,
    current: Option<String>,
    new_snapshot: String,
) -> BridgeHeadDecision {
    let base = base.map(SnapshotId);
    let current = current.map(SnapshotId);
    let new = SnapshotId(new_snapshot);
    match decide_head_update(base.as_ref(), current.as_ref(), &new) {
        HeadUpdate::FirstSnapshot { new_head } => BridgeHeadDecision {
            kind: BridgeHeadKind::FirstSnapshot,
            head: new_head.0,
            conflict_snapshot: None,
        },
        HeadUpdate::FastForward { new_head } => BridgeHeadDecision {
            kind: BridgeHeadKind::FastForward,
            head: new_head.0,
            conflict_snapshot: None,
        },
        HeadUpdate::Conflict {
            current_head,
            conflict_head,
        } => BridgeHeadDecision {
            kind: BridgeHeadKind::Conflict,
            head: current_head.0,
            conflict_snapshot: Some(conflict_head.0),
        },
    }
}

pub struct SyncCoordinator {
    pub policy: SyncPolicy,
}

#[derive(Debug, Serialize)]
pub struct AndroidUploadReport {
    pub outcome: String,
    pub snapshot_id: String,
    pub cloud_head: String,
    pub conflict_snapshot: Option<String>,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AndroidQueueReport {
    pub snapshot_id: String,
    pub bundle_path: String,
    pub pending_count: u64,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AndroidQueueDrainReport {
    pub uploaded_count: u64,
    pub conflict_count: u64,
    pub failed_count: u64,
    pub pending_count: u64,
    pub last_snapshot_id: Option<String>,
    pub last_cloud_head: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudHeadReport {
    pub head: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AndroidRestoreStageReport {
    pub snapshot_id: String,
    pub file_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadObject {
    object_id: String,
    sha256: String,
    bytes_b64: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadBundle {
    snapshot_id: SnapshotId,
    encrypted_manifest: DownloadObject,
    chunks: Vec<DownloadObject>,
}

const MAX_BUNDLE_RESPONSE_BYTES: usize = 192 * 1024 * 1024;
const MAX_BUNDLE_OBJECTS: usize = 10_001;
const MAX_ENCODED_OBJECT_BYTES: usize = 180 * 1024 * 1024;

fn validate_encoded_object_size(length: usize) -> anyhow::Result<()> {
    anyhow::ensure!(length <= MAX_ENCODED_OBJECT_BYTES, "object too large");
    Ok(())
}

async fn bounded_json_response<T: for<'de> Deserialize<'de>>(
    mut response: reqwest::Response,
) -> anyhow::Result<T> {
    if let Some(length) = response.content_length() {
        anyhow::ensure!(
            length <= MAX_BUNDLE_RESPONSE_BYTES as u64,
            "response too large"
        );
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= MAX_BUNDLE_RESPONSE_BYTES,
            "response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[derive(Serialize)]
struct Bootstrap<'a> {
    account_handle: &'a str,
    root_public_key_b64: String,
}
#[derive(Serialize)]
struct Register<'a> {
    account_handle: &'a str,
    cert_id: &'a str,
    device_public_key_b64: String,
    certificate_b64: String,
}
#[derive(Serialize)]
struct Begin<'a> {
    account_handle: &'a str,
    device_cert_id: &'a str,
    logical_save_id: &'a str,
    base_head: Option<SnapshotId>,
    parents: Vec<SnapshotId>,
    encrypted_manifest_id: String,
    chunk_ids: Vec<String>,
}
#[derive(Deserialize)]
struct BeginReply {
    upload_id: String,
    missing_chunk_ids: Vec<String>,
}
#[derive(Serialize)]
struct PutObject {
    chunk_id: String,
    sha256: String,
    bytes_b64: String,
}
#[derive(Serialize)]
struct PutManifest {
    manifest_id: String,
    sha256: String,
    bytes_b64: String,
}
#[derive(Serialize)]
struct Commit {
    snapshot_id: SnapshotId,
}
#[derive(Deserialize)]
struct CommitReply {
    outcome: String,
    head: SnapshotId,
    conflict_snapshot: Option<SnapshotId>,
}
#[derive(Serialize)]
struct Challenge<'a> {
    account_handle: &'a str,
    device_cert_id: &'a str,
}
#[derive(Deserialize)]
struct ChallengeReply {
    challenge_id: String,
    nonce_b64: String,
    expires_unix_seconds: u64,
}
struct RequestIdentity<'a> {
    account_handle: &'a str,
    device_cert_id: &'a str,
    signing_key: &'a SigningKey,
}

/// Uploads an already read-only staged SAF tree. The caller must create two
/// matching captures before invoking this function. `base_head` is never
/// guessed from wall-clock time: a stale/missing base becomes a server conflict.
pub async fn upload_android_stable_stage(
    staging_root: &Path,
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    base_head: Option<&str>,
    device_id: &str,
) -> anyhow::Result<AndroidUploadReport> {
    let server = server.trim_end_matches('/');
    anyhow::ensure!(
        server.starts_with("https://") || server.starts_with("http://"),
        "invalid server endpoint"
    );
    let descriptor = save_adapters::nemessix_android();
    let mut options = SnapshotOptions::fixture(GameKey::new("mh3g", "jp", "none", "slot1"));
    options.logical_save_id = LogicalSaveId(logical_save_id.to_owned());
    options.device_id = DeviceId(device_id.to_owned());
    options.parents = base_head
        .map(|v| vec![SnapshotId(v.to_owned())])
        .unwrap_or_default();
    options.created_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let snapshot = create_snapshot_from_stable_folder(staging_root, &descriptor, secret, options)?;
    upload_android_encrypted_snapshot(
        &snapshot,
        server,
        secret,
        logical_save_id,
        base_head,
        device_id,
    )
    .await
}

async fn upload_android_encrypted_snapshot(
    snapshot: &EncryptedSnapshot,
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    base_head: Option<&str>,
    device_id: &str,
) -> anyhow::Result<AndroidUploadReport> {
    let server = server.trim_end_matches('/');
    anyhow::ensure!(
        server.starts_with("https://") || server.starts_with("http://"),
        "invalid server endpoint"
    );
    let keys = derive_account_keys(secret)?;
    let handle = account_handle(&keys);
    let root = account_root_signing_key(&keys);
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-signing-seed/v1");
    h.update(keys.auth);
    h.update(device_id.as_bytes());
    let device = SigningKey::from_bytes(&h.finalize().into());
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-cert-id/v1");
    h.update(device.verifying_key().to_bytes());
    let digest = h.finalize();
    let mut cert_id = [0u8; 16];
    cert_id.copy_from_slice(&digest[..16]);
    let cert_hex = hex::encode(cert_id);
    let cert = issue_device_certificate_with_id(
        &root,
        &device.verifying_key(),
        cert_id,
        1_700_000_000,
        4_102_444_800,
        1,
    )?;
    let identity = RequestIdentity {
        account_handle: &handle,
        device_cert_id: &cert_hex,
        signing_key: &device,
    };
    let client = reqwest::Client::new();
    post_empty(
        &client,
        &format!("{server}/v1/accounts/bootstrap"),
        &Bootstrap {
            account_handle: &handle,
            root_public_key_b64: base64::engine::general_purpose::STANDARD
                .encode(root.verifying_key().to_bytes()),
        },
    )
    .await?;
    post_empty(
        &client,
        &format!("{server}/v1/devices/register"),
        &Register {
            account_handle: &handle,
            cert_id: &cert_hex,
            device_public_key_b64: base64::engine::general_purpose::STANDARD
                .encode(device.verifying_key().to_bytes()),
            certificate_b64: base64::engine::general_purpose::STANDARD
                .encode(deterministic_cbor(&cert)?),
        },
    )
    .await?;
    let manifest_bytes = serde_json::to_vec(&snapshot.encrypted_manifest)?;
    let manifest_id = hex::encode(sha2::Sha256::digest(&manifest_bytes));
    let chunk_ids = snapshot.chunks.keys().cloned().collect::<Vec<_>>();
    let begin: BeginReply = signed_post_json_client(
        &client,
        &format!("{server}/v1/snapshots/begin"),
        server,
        &identity,
        &Begin {
            account_handle: &handle,
            device_cert_id: &cert_hex,
            logical_save_id,
            base_head: base_head.map(|v| SnapshotId(v.to_owned())),
            parents: base_head
                .map(|v| vec![SnapshotId(v.to_owned())])
                .unwrap_or_default(),
            encrypted_manifest_id: manifest_id.clone(),
            chunk_ids,
        },
    )
    .await?;
    for id in &begin.missing_chunk_ids {
        let bytes = serde_json::to_vec(
            snapshot
                .chunks
                .get(id)
                .ok_or_else(|| anyhow::anyhow!("server requested unknown chunk"))?,
        )?;
        signed_post_empty(
            &client,
            &format!("{server}/v1/snapshots/{}/chunks", begin.upload_id),
            server,
            &identity,
            &PutObject {
                chunk_id: id.clone(),
                sha256: hex::encode(sha2::Sha256::digest(&bytes)),
                bytes_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
            },
        )
        .await?;
    }
    signed_post_empty(
        &client,
        &format!("{server}/v1/snapshots/{}/manifest", begin.upload_id),
        server,
        &identity,
        &PutManifest {
            manifest_id,
            sha256: hex::encode(sha2::Sha256::digest(&manifest_bytes)),
            bytes_b64: base64::engine::general_purpose::STANDARD.encode(manifest_bytes),
        },
    )
    .await?;
    let commit: CommitReply = signed_post_json_client(
        &client,
        &format!("{server}/v1/snapshots/{}/commit", begin.upload_id),
        server,
        &identity,
        &Commit {
            snapshot_id: snapshot.snapshot_id.clone(),
        },
    )
    .await?;
    Ok(AndroidUploadReport {
        outcome: commit.outcome,
        snapshot_id: snapshot.snapshot_id.0.clone(),
        cloud_head: commit.head.0,
        conflict_snapshot: commit.conflict_snapshot.map(|v| v.0),
        file_count: snapshot.fingerprint.file_count,
        total_bytes: snapshot.fingerprint.total_bytes,
    })
}

/// Creates an immutable encrypted snapshot and durably records it before any
/// network request. A later worker can retry from the encrypted bundle without
/// reopening the SAF tree or retaining plaintext staging data.
pub fn queue_android_stable_stage(
    staging_root: &Path,
    queue_root: &Path,
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    observed_base_head: Option<&str>,
    device_id: &str,
) -> anyhow::Result<AndroidQueueReport> {
    let server = server.trim_end_matches('/');
    anyhow::ensure!(
        server.starts_with("https://") || server.starts_with("http://"),
        "invalid server endpoint"
    );
    validate_logical_save_id(logical_save_id)?;
    std::fs::create_dir_all(queue_root.join("objects"))?;
    let store = save_engine::local_store::LocalStore::open(&queue_root.join("state.sqlite"))?;
    let parent = store
        .latest_retryable_snapshot(server, logical_save_id)?
        .map(|snapshot| snapshot.0)
        .or_else(|| observed_base_head.map(str::to_owned));
    let descriptor = save_adapters::nemessix_android();
    let mut options = SnapshotOptions::fixture(GameKey::new("mh3g", "jp", "none", "slot1"));
    options.logical_save_id = LogicalSaveId(logical_save_id.to_owned());
    options.device_id = DeviceId(device_id.to_owned());
    options.parents = parent
        .as_deref()
        .map(|value| vec![SnapshotId(value.to_owned())])
        .unwrap_or_default();
    options.created_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let created_unix_ms = options.created_unix_ms;
    let snapshot = create_snapshot_from_stable_folder(staging_root, &descriptor, secret, options)?;
    let relative_bundle = format!("objects/{}.mhsavebundle", snapshot.snapshot_id.0);
    let bundle = queue_root.join(&relative_bundle);
    export_encrypted_bundle(&snapshot, &bundle)?;
    std::fs::File::open(queue_root.join("objects"))?.sync_all()?;
    let encrypted_manifest = serde_json::to_vec(&snapshot.encrypted_manifest)?;
    if let Err(error) = store.enqueue_upload(
        &snapshot.snapshot_id,
        logical_save_id,
        device_id,
        &encrypted_manifest,
        created_unix_ms,
        server,
        logical_save_id,
        parent.as_deref(),
        &relative_bundle,
    ) {
        let _ = std::fs::remove_file(&bundle);
        return Err(error.into());
    }
    Ok(AndroidQueueReport {
        snapshot_id: snapshot.snapshot_id.0,
        bundle_path: relative_bundle,
        pending_count: store.pending_upload_count()?,
        file_count: snapshot.fingerprint.file_count,
        total_bytes: snapshot.fingerprint.total_bytes,
    })
}

/// Consumes encrypted upload jobs in FIFO order. Failures are redacted and
/// remain pending with an incremented attempt count; successful immutable
/// commits are marked complete before their local bundle is removed.
pub async fn drain_android_upload_queue(
    queue_root: &Path,
    server: &str,
    secret: &[u8; 32],
) -> AndroidQueueDrainReport {
    let server = server.trim_end_matches('/');
    let mut report = AndroidQueueDrainReport {
        uploaded_count: 0,
        conflict_count: 0,
        failed_count: 0,
        pending_count: 0,
        last_snapshot_id: None,
        last_cloud_head: None,
        last_error: None,
    };
    let store = match save_engine::local_store::LocalStore::open(&queue_root.join("state.sqlite")) {
        Ok(store) => store,
        Err(_) => {
            report.failed_count = 1;
            report.last_error = Some("local_queue_unavailable".into());
            return report;
        }
    };
    let jobs = match store.retryable_uploads(server, 100) {
        Ok(jobs) => jobs,
        Err(_) => {
            report.failed_count = 1;
            report.last_error = Some("local_queue_unavailable".into());
            return report;
        }
    };
    for job in jobs {
        if store.mark_uploading(job.id).is_err() {
            report.failed_count += 1;
            report.last_error = Some("local_queue_unavailable".into());
            break;
        }
        let bundle = queue_root.join(&job.bundle_path);
        let snapshot = match save_engine::import_encrypted_bundle(&bundle) {
            Ok(snapshot) if snapshot.snapshot_id == job.snapshot_id => snapshot,
            _ => {
                let _ = store.mark_upload_failed(job.id, "local_queue_integrity_failure");
                report.failed_count += 1;
                report.last_error = Some("local_queue_integrity_failure".into());
                break;
            }
        };
        match upload_android_encrypted_snapshot(
            &snapshot,
            server,
            secret,
            &job.logical_save_id,
            job.base_head.as_deref(),
            &job.device_id,
        )
        .await
        {
            Ok(upload) => {
                if store.mark_upload_completed(job.id).is_err() {
                    report.failed_count += 1;
                    report.last_error = Some("local_queue_unavailable".into());
                    break;
                }
                let _ = std::fs::remove_file(bundle);
                report.uploaded_count += 1;
                if upload.outcome == "conflict" {
                    report.conflict_count += 1;
                }
                report.last_snapshot_id = Some(upload.snapshot_id);
                report.last_cloud_head = Some(upload.cloud_head);
            }
            Err(_) => {
                let _ = store.mark_upload_failed(job.id, "network_or_server_failure");
                report.failed_count += 1;
                report.last_error = Some("network_or_server_failure".into());
                break;
            }
        }
    }
    report.pending_count = store.pending_upload_count_for_server(server).unwrap_or(0);
    report
}

async fn post_empty<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    body: &T,
) -> anyhow::Result<()> {
    let response = client.post(url).json(body).send().await?;
    anyhow::ensure!(
        response.status().is_success(),
        "server request failed: {}",
        response.status()
    );
    Ok(())
}
async fn post_json_client<T: Serialize, R: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    body: &T,
) -> anyhow::Result<R> {
    let response = client.post(url).json(body).send().await?;
    anyhow::ensure!(
        response.status().is_success(),
        "server request failed: {}",
        response.status()
    );
    Ok(response.json().await?)
}

async fn signed_request<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    server: &str,
    identity: &RequestIdentity<'_>,
    body: &T,
) -> anyhow::Result<reqwest::Response> {
    signed_raw_request(
        client,
        reqwest::Method::POST,
        url,
        server,
        identity,
        serde_json::to_vec(body)?,
    )
    .await
}

async fn signed_raw_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    server: &str,
    identity: &RequestIdentity<'_>,
    body: Vec<u8>,
) -> anyhow::Result<reqwest::Response> {
    let challenge: ChallengeReply = post_json_client(
        client,
        &format!("{server}/v1/accounts/challenge"),
        &Challenge {
            account_handle: identity.account_handle,
            device_cert_id: identity.device_cert_id,
        },
    )
    .await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    anyhow::ensure!(
        challenge.expires_unix_seconds >= now,
        "expired auth challenge"
    );
    let parsed = reqwest::Url::parse(url)?;
    let path = parsed.path().to_owned()
        + parsed
            .query()
            .map(|q| format!("?{q}"))
            .as_deref()
            .unwrap_or("");
    let signature = save_crypto::sign_http_request(
        identity.signing_key,
        method.as_str(),
        &path,
        &body,
        &challenge.challenge_id,
        &challenge.nonce_b64,
        now,
    );
    Ok(client
        .request(method, url)
        .header("content-type", "application/json")
        .header("x-mh-account", identity.account_handle)
        .header("x-mh-device-cert", identity.device_cert_id)
        .header("x-mh-challenge-id", challenge.challenge_id)
        .header("x-mh-nonce", challenge.nonce_b64)
        .header("x-mh-timestamp", now.to_string())
        .header(
            "x-mh-signature",
            base64::engine::general_purpose::STANDARD.encode(signature),
        )
        .body(body)
        .send()
        .await?)
}

async fn signed_get(
    client: &reqwest::Client,
    url: &str,
    server: &str,
    identity: &RequestIdentity<'_>,
) -> anyhow::Result<reqwest::Response> {
    signed_raw_request(
        client,
        reqwest::Method::GET,
        url,
        server,
        identity,
        Vec::new(),
    )
    .await
}

fn validate_logical_save_id(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "invalid logical save id"
    );
    Ok(())
}

/// Authenticates the account/device and fetches the exact server CAS HEAD.
/// A missing logical save is represented as `head: null`; transport/auth errors
/// remain errors and must never be collapsed into an empty cloud state.
pub async fn fetch_android_cloud_head(
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    device_id: &str,
) -> anyhow::Result<CloudHeadReport> {
    let server = server.trim_end_matches('/');
    anyhow::ensure!(
        server.starts_with("https://") || server.starts_with("http://"),
        "invalid server endpoint"
    );
    validate_logical_save_id(logical_save_id)?;
    anyhow::ensure!(
        !device_id.is_empty() && device_id.len() <= 128,
        "invalid device id"
    );

    let keys = derive_account_keys(secret)?;
    let handle = account_handle(&keys);
    let root = account_root_signing_key(&keys);
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-signing-seed/v1");
    h.update(keys.auth);
    h.update(device_id.as_bytes());
    let device = SigningKey::from_bytes(&h.finalize().into());
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-cert-id/v1");
    h.update(device.verifying_key().to_bytes());
    let digest = h.finalize();
    let mut cert_id = [0u8; 16];
    cert_id.copy_from_slice(&digest[..16]);
    let cert_hex = hex::encode(cert_id);
    let cert = issue_device_certificate_with_id(
        &root,
        &device.verifying_key(),
        cert_id,
        1_700_000_000,
        4_102_444_800,
        1,
    )?;
    let identity = RequestIdentity {
        account_handle: &handle,
        device_cert_id: &cert_hex,
        signing_key: &device,
    };
    let client = reqwest::Client::new();
    post_empty(
        &client,
        &format!("{server}/v1/accounts/bootstrap"),
        &Bootstrap {
            account_handle: &handle,
            root_public_key_b64: base64::engine::general_purpose::STANDARD
                .encode(root.verifying_key().to_bytes()),
        },
    )
    .await?;
    post_empty(
        &client,
        &format!("{server}/v1/devices/register"),
        &Register {
            account_handle: &handle,
            cert_id: &cert_hex,
            device_public_key_b64: base64::engine::general_purpose::STANDARD
                .encode(device.verifying_key().to_bytes()),
            certificate_b64: base64::engine::general_purpose::STANDARD
                .encode(deterministic_cbor(&cert)?),
        },
    )
    .await?;
    let response = signed_get(
        &client,
        &format!("{server}/v1/heads/{logical_save_id}"),
        server,
        &identity,
    )
    .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(CloudHeadReport { head: None });
    }
    anyhow::ensure!(
        response.status().is_success(),
        "server request failed: {}",
        response.status()
    );
    let head: SnapshotId = response.json().await?;
    Ok(CloudHeadReport { head: Some(head.0) })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidConflictSummary {
    pub snapshot_id: String,
    pub cloud_head: String,
    pub branch_device_id: String,
    pub branch_created_unix_ms: u64,
    pub cloud_device_id: String,
    pub cloud_created_unix_ms: u64,
    pub changed_files: u64,
    pub changed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AndroidConflictReport {
    pub cloud_head: Option<String>,
    pub conflicts: Vec<AndroidConflictSummary>,
}

#[derive(Debug, Deserialize)]
struct AndroidSnapshotRow {
    snapshot_id: SnapshotId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum AndroidConflictResolutionKind {
    KeepCloudHead,
    ReplaceWithLocal,
}

#[derive(Debug, Serialize)]
struct AndroidResolveConflictRequest {
    chosen_snapshot_id: SnapshotId,
    resolution: AndroidConflictResolutionKind,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AndroidResolveReport {
    pub resolved: usize,
    pub total: usize,
    pub failed_snapshot_ids: Vec<String>,
}

fn manifest_diff_summary(
    head_entries: &BTreeMap<String, save_domain::ManifestEntry>,
    branch_entries: &BTreeMap<String, save_domain::ManifestEntry>,
) -> (u64, u64) {
    let paths: std::collections::BTreeSet<_> =
        head_entries.keys().chain(branch_entries.keys()).collect();
    let mut changed_files = 0u64;
    let mut changed_bytes = 0u64;
    for path in paths {
        let left = head_entries.get(path);
        let right = branch_entries.get(path);
        if left.map(|v| (&v.kind, v.size, &v.plaintext_sha256))
            != right.map(|v| (&v.kind, v.size, &v.plaintext_sha256))
        {
            changed_files += 1;
            changed_bytes = changed_bytes
                .saturating_add(left.map_or(0, |v| v.size).max(right.map_or(0, |v| v.size)));
        }
    }
    (changed_files, changed_bytes)
}

fn android_request_identity<'a>(
    handle: &'a str,
    cert_hex: &'a str,
    device: &'a SigningKey,
) -> RequestIdentity<'a> {
    RequestIdentity {
        account_handle: handle,
        device_cert_id: cert_hex,
        signing_key: device,
    }
}

fn android_device_identity(
    secret: &[u8; 32],
    device_id: &str,
) -> anyhow::Result<(String, String, SigningKey)> {
    let keys = derive_account_keys(secret)?;
    let handle = account_handle(&keys);
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-signing-seed/v1");
    h.update(keys.auth);
    h.update(device_id.as_bytes());
    let device = SigningKey::from_bytes(&h.finalize().into());
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-cert-id/v1");
    h.update(device.verifying_key().to_bytes());
    let cert_hex = hex::encode(&h.finalize()[..16]);
    Ok((handle, cert_hex, device))
}

async fn fetch_snapshot_for_conflict_diff(
    client: &reqwest::Client,
    server: &str,
    identity: &RequestIdentity<'_>,
    secret: &[u8; 32],
    snapshot_id: &str,
) -> anyhow::Result<EncryptedSnapshot> {
    let response = signed_get(
        client,
        &format!("{server}/v1/snapshots/{snapshot_id}/encrypted-bundle"),
        server,
        identity,
    )
    .await?;
    anyhow::ensure!(response.status().is_success(), "bundle request failed");
    decode_download_bundle(secret, snapshot_id, bounded_json_response(response).await?)
}

/// Reads only unresolved branches and decrypts manifests locally to produce a
/// path-free count/byte summary. The server never receives plaintext paths.
pub async fn fetch_android_conflicts(
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    device_id: &str,
) -> anyhow::Result<AndroidConflictReport> {
    let server = server.trim_end_matches('/');
    validate_logical_save_id(logical_save_id)?;
    let head = fetch_android_cloud_head(server, secret, logical_save_id, device_id)
        .await?
        .head;
    let Some(head_id) = head.clone() else {
        return Ok(AndroidConflictReport {
            cloud_head: None,
            conflicts: Vec::new(),
        });
    };
    let (handle, cert_hex, device) = android_device_identity(secret, device_id)?;
    let identity = android_request_identity(&handle, &cert_hex, &device);
    let client = reqwest::Client::new();
    let response = signed_get(
        &client,
        &format!("{server}/v1/conflicts/{logical_save_id}"),
        server,
        &identity,
    )
    .await?;
    anyhow::ensure!(response.status().is_success(), "conflict request failed");
    let rows: Vec<AndroidSnapshotRow> = bounded_json_response(response).await?;
    let head_snapshot =
        fetch_snapshot_for_conflict_diff(&client, server, &identity, secret, &head_id).await?;
    let head_manifest = decrypt_manifest(secret, &head_snapshot)?;
    let head_device_id = head_manifest.device_id.0.clone();
    let head_created_unix_ms = head_manifest.created_unix_ms;
    let head_entries: BTreeMap<_, _> = head_manifest
        .entries
        .into_iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect();
    let mut conflicts = Vec::with_capacity(rows.len());
    for row in rows {
        let branch_id = row.snapshot_id.0;
        let branch =
            fetch_snapshot_for_conflict_diff(&client, server, &identity, secret, &branch_id)
                .await?;
        let manifest = decrypt_manifest(secret, &branch)?;
        let branch_device_id = manifest.device_id.0.clone();
        let branch_created_unix_ms = manifest.created_unix_ms;
        let branch_entries: BTreeMap<_, _> = manifest
            .entries
            .into_iter()
            .map(|entry| (entry.path.clone(), entry))
            .collect();
        let (changed_files, changed_bytes) = manifest_diff_summary(&head_entries, &branch_entries);
        conflicts.push(AndroidConflictSummary {
            snapshot_id: branch_id,
            cloud_head: head_id.clone(),
            branch_device_id,
            branch_created_unix_ms,
            cloud_device_id: head_device_id.clone(),
            cloud_created_unix_ms: head_created_unix_ms,
            changed_files,
            changed_bytes,
        });
    }
    Ok(AndroidConflictReport {
        cloud_head: head,
        conflicts,
    })
}

pub async fn resolve_android_conflicts(
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    device_id: &str,
    conflict_snapshot_ids: &[String],
    chosen_snapshot_id: &str,
    replace_with_local: bool,
) -> anyhow::Result<AndroidResolveReport> {
    let server = server.trim_end_matches('/');
    validate_logical_save_id(logical_save_id)?;
    let observed = fetch_android_cloud_head(server, secret, logical_save_id, device_id).await?;
    anyhow::ensure!(
        observed.head.as_deref() == Some(chosen_snapshot_id),
        "chosen snapshot is not current head"
    );
    let (handle, cert_hex, device) = android_device_identity(secret, device_id)?;
    let identity = android_request_identity(&handle, &cert_hex, &device);
    let client = reqwest::Client::new();
    let mut failed = Vec::new();
    let resolution = || {
        if replace_with_local {
            AndroidConflictResolutionKind::ReplaceWithLocal
        } else {
            AndroidConflictResolutionKind::KeepCloudHead
        }
    };
    for conflict in conflict_snapshot_ids {
        if conflict.len() != 64 || !conflict.bytes().all(|b| b.is_ascii_hexdigit()) {
            failed.push(conflict.clone());
            continue;
        }
        let response = signed_request(
            &client,
            &format!("{server}/v1/conflicts/{logical_save_id}/{conflict}/resolve"),
            server,
            &identity,
            &AndroidResolveConflictRequest {
                chosen_snapshot_id: SnapshotId(chosen_snapshot_id.to_owned()),
                resolution: resolution(),
            },
        )
        .await;
        if !response.is_ok_and(|response| response.status().is_success()) {
            failed.push(conflict.clone());
        }
    }
    Ok(AndroidResolveReport {
        resolved: conflict_snapshot_ids.len() - failed.len(),
        total: conflict_snapshot_ids.len(),
        failed_snapshot_ids: failed,
    })
}

fn decode_download_bundle(
    secret: &[u8; 32],
    expected_snapshot_id: &str,
    bundle: DownloadBundle,
) -> anyhow::Result<EncryptedSnapshot> {
    anyhow::ensure!(
        bundle.snapshot_id.0 == expected_snapshot_id,
        "snapshot mismatch"
    );
    anyhow::ensure!(
        bundle.chunks.len() <= MAX_BUNDLE_OBJECTS,
        "too many objects"
    );
    let decode = |object: &DownloadObject| -> anyhow::Result<Vec<u8>> {
        validate_encoded_object_size(object.bytes_b64.len())?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(&object.bytes_b64)?;
        anyhow::ensure!(
            hex::encode(sha2::Sha256::digest(&bytes)) == object.sha256,
            "object checksum mismatch"
        );
        Ok(bytes)
    };
    let manifest_bytes = decode(&bundle.encrypted_manifest)?;
    anyhow::ensure!(
        hex::encode(sha2::Sha256::digest(&manifest_bytes)) == bundle.encrypted_manifest.object_id,
        "manifest object id mismatch"
    );
    let encrypted_manifest: EncryptedBlob = serde_json::from_slice(&manifest_bytes)?;
    let mut chunks = BTreeMap::new();
    for object in bundle.chunks {
        anyhow::ensure!(!chunks.contains_key(&object.object_id), "duplicate chunk");
        let bytes = decode(&object)?;
        let blob: EncryptedBlob = serde_json::from_slice(&bytes)?;
        chunks.insert(object.object_id, blob);
    }
    let mut snapshot = EncryptedSnapshot {
        snapshot_id: bundle.snapshot_id,
        encrypted_manifest,
        chunks,
        fingerprint: save_domain::TreeFingerprint {
            file_count: 0,
            total_bytes: 0,
            sha256: String::new(),
        },
    };
    let manifest = verify_snapshot_content_id(secret, &snapshot)?;
    save_domain::validate_manifest_entries(&manifest.entries, 10_000, 128 * 1024 * 1024)?;
    let mut referenced = std::collections::BTreeSet::new();
    for entry in &manifest.entries {
        let mut chunk_total = 0u64;
        for chunk in &entry.chunks {
            anyhow::ensure!(
                chunk.id.len() == 64 && chunk.id.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid chunk id"
            );
            chunk_total = chunk_total
                .checked_add(chunk.plaintext_size)
                .ok_or_else(|| anyhow::anyhow!("chunk size overflow"))?;
            anyhow::ensure!(chunk_total <= entry.size, "chunk size exceeds file size");
            referenced.insert(chunk.id.clone());
        }
        if entry.kind != save_domain::FileKind::Tombstone {
            anyhow::ensure!(
                chunk_total == entry.size,
                "chunk sizes do not match file size"
            );
        }
    }
    anyhow::ensure!(
        referenced == snapshot.chunks.keys().cloned().collect(),
        "bundle chunk set mismatch"
    );
    snapshot.fingerprint.file_count = manifest
        .entries
        .iter()
        .filter(|e| e.kind != save_domain::FileKind::Tombstone)
        .count() as u64;
    snapshot.fingerprint.total_bytes = manifest
        .entries
        .iter()
        .filter(|e| e.kind != save_domain::FileKind::Tombstone)
        .map(|e| e.size)
        .sum();
    Ok(snapshot)
}

fn verify_snapshot_content_id(
    secret: &[u8; 32],
    snapshot: &EncryptedSnapshot,
) -> anyhow::Result<save_domain::SnapshotManifest> {
    let manifest = decrypt_manifest(secret, snapshot)?;
    let parent_bytes: Vec<Vec<u8>> = manifest
        .parents
        .iter()
        .map(|parent| parent.0.as_bytes().to_vec())
        .collect();
    let mut parts: Vec<&[u8]> = vec![b"v1", &snapshot.encrypted_manifest.ciphertext];
    for parent in &parent_bytes {
        parts.push(parent);
    }
    anyhow::ensure!(
        SnapshotId::from_parts(&parts) == snapshot.snapshot_id,
        "snapshot integrity mismatch"
    );
    Ok(manifest)
}

async fn fetch_android_encrypted_snapshot(
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    snapshot_id: &str,
    device_id: &str,
) -> anyhow::Result<EncryptedSnapshot> {
    anyhow::ensure!(
        snapshot_id.len() == 64 && snapshot_id.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid snapshot id"
    );
    let server = server.trim_end_matches('/');
    let observed = fetch_android_cloud_head(server, secret, logical_save_id, device_id).await?;
    anyhow::ensure!(
        observed.head.as_deref() == Some(snapshot_id),
        "cloud head changed"
    );
    let keys = derive_account_keys(secret)?;
    let handle = account_handle(&keys);
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-signing-seed/v1");
    h.update(keys.auth);
    h.update(device_id.as_bytes());
    let device = SigningKey::from_bytes(&h.finalize().into());
    let mut h = sha2::Sha256::new();
    h.update(b"mh-save-sync/android-device-cert-id/v1");
    h.update(device.verifying_key().to_bytes());
    let digest = h.finalize();
    let mut cert_id = [0u8; 16];
    cert_id.copy_from_slice(&digest[..16]);
    let cert_hex = hex::encode(cert_id);
    let identity = RequestIdentity {
        account_handle: &handle,
        device_cert_id: &cert_hex,
        signing_key: &device,
    };
    let client = reqwest::Client::new();
    let response = signed_get(
        &client,
        &format!("{server}/v1/snapshots/{snapshot_id}/encrypted-bundle"),
        server,
        &identity,
    )
    .await?;
    anyhow::ensure!(response.status().is_success(), "bundle request failed");
    decode_download_bundle(secret, snapshot_id, bounded_json_response(response).await?)
}

/// Downloads an authenticated opaque cloud snapshot, verifies every stored
/// object, decrypts it client-side, and materializes it only under the caller's
/// private staging directory. It never writes to an emulator SAF tree.
pub async fn download_android_snapshot_to_stage(
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    snapshot_id: &str,
    device_id: &str,
    stage_target: &Path,
) -> anyhow::Result<AndroidRestoreStageReport> {
    anyhow::ensure!(!stage_target.exists(), "staging target already exists");
    let snapshot =
        fetch_android_encrypted_snapshot(server, secret, logical_save_id, snapshot_id, device_id)
            .await?;
    let report = AndroidRestoreStageReport {
        snapshot_id: snapshot.snapshot_id.0.clone(),
        file_count: snapshot.fingerprint.file_count,
        total_bytes: snapshot.fingerprint.total_bytes,
    };
    if let Err(error) =
        restore_snapshot_to_folder(secret, &snapshot, stage_target, EmulatorState::Stopped)
    {
        let _ = std::fs::remove_dir_all(stage_target);
        return Err(error.into());
    }
    Ok(report)
}

pub async fn download_android_snapshot_to_cache(
    server: &str,
    secret: &[u8; 32],
    logical_save_id: &str,
    snapshot_id: &str,
    device_id: &str,
    destination: &Path,
) -> anyhow::Result<AndroidRestoreStageReport> {
    anyhow::ensure!(!destination.exists(), "cache target already exists");
    let snapshot =
        fetch_android_encrypted_snapshot(server, secret, logical_save_id, snapshot_id, device_id)
            .await?;
    let report = AndroidRestoreStageReport {
        snapshot_id: snapshot.snapshot_id.0.clone(),
        file_count: snapshot.fingerprint.file_count,
        total_bytes: snapshot.fingerprint.total_bytes,
    };
    export_encrypted_bundle(&snapshot, destination)?;
    Ok(report)
}

async fn signed_post_empty<T: Serialize>(
    client: &reqwest::Client,
    url: &str,
    server: &str,
    identity: &RequestIdentity<'_>,
    body: &T,
) -> anyhow::Result<()> {
    let response = signed_request(client, url, server, identity, body).await?;
    anyhow::ensure!(
        response.status().is_success(),
        "server request failed: {}",
        response.status()
    );
    Ok(())
}

async fn signed_post_json_client<T: Serialize, R: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
    server: &str,
    identity: &RequestIdentity<'_>,
    body: &T,
) -> anyhow::Result<R> {
    let response = signed_request(client, url, server, identity, body).await?;
    anyhow::ensure!(
        response.status().is_success(),
        "server request failed: {}",
        response.status()
    );
    Ok(response.json().await?)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_fetchUnresolvedConflicts<
    'local,
>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    logical: jni::objects::JString<'local>,
    device: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let server: String = env.get_string(&server)?.into();
            let logical: String = env.get_string(&logical)?.into();
            let device: String = env.get_string(&device)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "recovery secret must be 32 bytes");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let runtime = tokio::runtime::Runtime::new()?;
            Ok(serde_json::to_string(&runtime.block_on(
                fetch_android_conflicts(&server, &key, &logical, &device),
            )?)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({"error":"conflict_fetch_failed"}).to_string(),
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_resolveConflicts<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    logical: jni::objects::JString<'local>,
    device: jni::objects::JString<'local>,
    conflict_ids_json: jni::objects::JString<'local>,
    chosen: jni::objects::JString<'local>,
    replace_with_local: jni::sys::jboolean,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let server: String = env.get_string(&server)?.into();
            let logical: String = env.get_string(&logical)?.into();
            let device: String = env.get_string(&device)?.into();
            let ids_json: String = env.get_string(&conflict_ids_json)?.into();
            let chosen: String = env.get_string(&chosen)?.into();
            let ids: Vec<String> = serde_json::from_str(&ids_json)?;
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "recovery secret must be 32 bytes");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let runtime = tokio::runtime::Runtime::new()?;
            Ok(serde_json::to_string(&runtime.block_on(
                resolve_android_conflicts(
                    &server,
                    &key,
                    &logical,
                    &device,
                    &ids,
                    &chosen,
                    replace_with_local != 0,
                ),
            )?)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({"error":"conflict_resolve_failed"}).to_string(),
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_fetchCloudHead<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    logical: jni::objects::JString<'local>,
    device: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let server: String = env.get_string(&server)?.into();
            let logical: String = env.get_string(&logical)?.into();
            let device: String = env.get_string(&device)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "recovery secret must be 32 bytes");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let runtime = tokio::runtime::Runtime::new()?;
            let report =
                runtime.block_on(fetch_android_cloud_head(&server, &key, &logical, &device))?;
            Ok(serde_json::to_string(&report)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => {
            // Never surface URLs, account identifiers, device identifiers,
            // recovery material, response bodies, or native panic details.
            serde_json::json!({"error":"cloud_head_failed"}).to_string()
        }
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_downloadCloudSnapshotToStage<
    'local,
>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    logical: jni::objects::JString<'local>,
    snapshot: jni::objects::JString<'local>,
    device: jni::objects::JString<'local>,
    stage: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let server: String = env.get_string(&server)?.into();
            let logical: String = env.get_string(&logical)?.into();
            let snapshot: String = env.get_string(&snapshot)?.into();
            let device: String = env.get_string(&device)?.into();
            let stage: String = env.get_string(&stage)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "invalid secret");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let runtime = tokio::runtime::Runtime::new()?;
            let report = runtime.block_on(download_android_snapshot_to_stage(
                &server,
                &key,
                &logical,
                &snapshot,
                &device,
                Path::new(&stage),
            ))?;
            Ok(serde_json::to_string(&report)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({"error":"restore_stage_failed","message_zh":"云端版本下载或完整性校验失败，未修改本地存档"}).to_string(),
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_encryptStageBackup<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    stage: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    destination: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let stage: String = env.get_string(&stage)?.into();
            let destination: String = env.get_string(&destination)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "invalid secret");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let snapshot = create_snapshot_from_stable_folder(
                Path::new(&stage),
                &save_adapters::nemessix_android(),
                &key,
                SnapshotOptions::fixture(GameKey::new("mh3g", "jp", "none", "pre-restore")),
            )?;
            export_encrypted_bundle(&snapshot, Path::new(&destination))?;
            Ok(serde_json::json!({"snapshot_id":snapshot.snapshot_id.0}).to_string())
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({"error":"backup_encrypt_failed"}).to_string(),
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_verifyEncryptedBundle<
    'local,
>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    bundle: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    expected_snapshot: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let bundle: String = env.get_string(&bundle)?.into();
            let expected_snapshot: String = env.get_string(&expected_snapshot)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "invalid secret");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let snapshot = import_encrypted_bundle(Path::new(&bundle))?;
            anyhow::ensure!(
                snapshot.snapshot_id.0 == expected_snapshot,
                "snapshot mismatch"
            );
            let parent = Path::new(&bundle)
                .parent()
                .unwrap_or_else(|| Path::new("."));
            let scratch = tempfile::tempdir_in(parent)?;
            restore_snapshot_to_folder(
                &key,
                &snapshot,
                &scratch.path().join("verified"),
                EmulatorState::Stopped,
            )?;
            Ok(serde_json::json!({"snapshot_id":snapshot.snapshot_id.0}).to_string())
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({"error":"bundle_verify_failed"}).to_string(),
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_restoreEncryptedBundleToStage<
    'local,
>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    bundle: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    target: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let bundle: String = env.get_string(&bundle)?.into();
            let target: String = env.get_string(&target)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "invalid secret");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let snapshot = import_encrypted_bundle(Path::new(&bundle))?;
            let manifest = verify_snapshot_content_id(&key, &snapshot)?;
            restore_snapshot_to_folder(
                &key,
                &snapshot,
                Path::new(&target),
                EmulatorState::Stopped,
            )?;
            Ok(serde_json::json!({
                "snapshot_id":snapshot.snapshot_id.0,
                "file_count":manifest.entries.iter().filter(|entry| entry.kind != save_domain::FileKind::Tombstone).count(),
                "total_bytes":manifest.entries.iter().filter(|entry| entry.kind != save_domain::FileKind::Tombstone).map(|entry| entry.size).sum::<u64>()
            })
            .to_string())
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({"error":"bundle_restore_failed"}).to_string(),
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_downloadCloudSnapshotToCache<
    'local,
>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    logical: jni::objects::JString<'local>,
    snapshot: jni::objects::JString<'local>,
    device: jni::objects::JString<'local>,
    destination: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let server: String = env.get_string(&server)?.into();
            let logical: String = env.get_string(&logical)?.into();
            let snapshot: String = env.get_string(&snapshot)?.into();
            let device: String = env.get_string(&device)?.into();
            let destination: String = env.get_string(&destination)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "invalid secret");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let runtime = tokio::runtime::Runtime::new()?;
            let report = runtime.block_on(download_android_snapshot_to_cache(
                &server,
                &key,
                &logical,
                &snapshot,
                &device,
                Path::new(&destination),
            ))?;
            Ok(serde_json::to_string(&report)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({"error":"cloud_cache_failed","message_zh":"云端版本下载或完整性校验失败"}).to_string(),
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_queueStableStage<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    staging: jni::objects::JString<'local>,
    queue_root: jni::objects::JString<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    logical: jni::objects::JString<'local>,
    base: jni::objects::JString<'local>,
    device: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let staging: String = env.get_string(&staging)?.into();
            let queue_root: String = env.get_string(&queue_root)?.into();
            let server: String = env.get_string(&server)?.into();
            let logical: String = env.get_string(&logical)?.into();
            let device: String = env.get_string(&device)?.into();
            let base = if base.is_null() {
                None
            } else {
                Some(String::from(env.get_string(&base)?))
            };
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "recovery secret must be 32 bytes");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let report = queue_android_stable_stage(
                Path::new(&staging),
                Path::new(&queue_root),
                &server,
                &key,
                &logical,
                base.as_deref(),
                &device,
            )?;
            Ok(serde_json::to_string(&report)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({
            "error":"queue_failed",
            "message_zh":"无法创建持久加密上传任务；未修改云端或本地原始存档"
        })
        .to_string(),
    };
    env.new_string(output)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_drainUploadQueue<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    queue_root: jni::objects::JString<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let queue_root: String = env.get_string(&queue_root)?.into();
            let server: String = env.get_string(&server)?.into();
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "recovery secret must be 32 bytes");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let runtime = tokio::runtime::Runtime::new()?;
            let report = runtime.block_on(drain_android_upload_queue(
                Path::new(&queue_root),
                &server,
                &key,
            ));
            Ok(serde_json::to_string(&report)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => serde_json::json!({
            "uploaded_count":0,
            "conflict_count":0,
            "failed_count":1,
            "pending_count":0,
            "last_error":"local_queue_unavailable"
        })
        .to_string(),
    };
    env.new_string(output)
        .map(|value| value.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_bridgeHealth<'local>(
    env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
) -> jni::sys::jstring {
    env.new_string("save-client-jni/1;e2ee=xchacha20poly1305;watcher=dirty-only")
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_org_mhtoolkit_savesync_NativeSyncBridge_uploadStableStage<'local>(
    mut env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    staging: jni::objects::JString<'local>,
    server: jni::objects::JString<'local>,
    secret: jni::objects::JByteArray<'local>,
    logical: jni::objects::JString<'local>,
    base: jni::objects::JString<'local>,
    device: jni::objects::JString<'local>,
) -> jni::sys::jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> anyhow::Result<String> {
            let staging: String = env.get_string(&staging)?.into();
            let server: String = env.get_string(&server)?.into();
            let logical: String = env.get_string(&logical)?.into();
            let device: String = env.get_string(&device)?.into();
            let base = if base.is_null() {
                None
            } else {
                Some(String::from(env.get_string(&base)?))
            };
            let bytes = Zeroizing::new(env.convert_byte_array(&secret)?);
            anyhow::ensure!(bytes.len() == 32, "recovery secret must be 32 bytes");
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(&bytes);
            let runtime = tokio::runtime::Runtime::new()?;
            let report = runtime.block_on(upload_android_stable_stage(
                Path::new(&staging),
                &server,
                &key,
                &logical,
                base.as_deref(),
                &device,
            ));
            Ok(serde_json::to_string(&report?)?)
        },
    ));
    let output = match result {
        Ok(Ok(json)) => json,
        Ok(Err(_)) | Err(_) => {
            // Native error chains may contain private staging paths. Keep the JNI
            // boundary redacted; structured diagnostics belong in metadata-only
            // audit events, never UI/logcat.
            serde_json::json!({"error":"sync_failed","message_zh":"同步失败，未修改云端 HEAD 或本地原始存档"}).to_string()
        }
    };
    env.new_string(output)
        .map(|v| v.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

impl SyncCoordinator {
    pub fn new(policy: SyncPolicy) -> Self {
        Self { policy }
    }

    pub fn watcher_event_state(&self) -> VisibleState {
        VisibleState::PendingUpload
    }

    pub fn pre_launch_decision(
        &self,
        local_head: Option<&SnapshotId>,
        remote_head: Option<&SnapshotId>,
    ) -> VisibleState {
        match (local_head, remote_head) {
            (Some(l), Some(r)) if l != r => VisibleState::Conflict,
            (None, Some(_)) if !self.policy.auto_restore => VisibleState::PendingUpload,
            _ => VisibleState::Ready,
        }
    }

    pub fn commit_decision(
        &self,
        base: Option<&SnapshotId>,
        current: Option<&SnapshotId>,
        new: &SnapshotId,
    ) -> (HeadUpdate, VisibleState) {
        let update = decide_head_update(base, current, new);
        let state = match update {
            HeadUpdate::Conflict { .. } => VisibleState::Conflict,
            _ => VisibleState::PendingUpload,
        };
        (update, state)
    }

    pub fn can_restore(
        &self,
        descriptor: &AdapterDescriptor,
        emulator_stopped: bool,
    ) -> VisibleState {
        if descriptor.restore.require_emulator_stopped && !emulator_stopped {
            VisibleState::RestoreBlockedRunning
        } else {
            VisibleState::Ready
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_upload_server(
        listener: std::net::TcpListener,
        snapshot_id: String,
    ) -> std::thread::JoinHandle<Vec<String>> {
        std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let mut paths = Vec::new();
            let mut idle = 0;
            loop {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => {
                        idle = 0;
                        value
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        idle += 1;
                        if idle > 500 {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("accept failed: {error}"),
                };
                use std::io::{Read, Write};
                let mut request = Vec::new();
                let mut buffer = [0u8; 4096];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    request.extend_from_slice(&buffer[..read]);
                    let header_end = request.windows(4).position(|v| v == b"\r\n\r\n");
                    if let Some(header_end) = header_end {
                        let headers = String::from_utf8_lossy(&request[..header_end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|v| v.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= header_end + 4 + length {
                            break;
                        }
                    }
                }
                let first = String::from_utf8_lossy(&request)
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned();
                paths.push(first.clone());
                let is_commit = first.contains("/commit ");
                let body = if first.contains("/v1/accounts/challenge ") {
                    serde_json::json!({
                        "challenge_id":"fixture-challenge",
                        "nonce_b64":base64::engine::general_purpose::STANDARD.encode([3u8;32]),
                        "expires_unix_seconds":4_102_444_800u64
                    })
                    .to_string()
                } else if first.contains("/v1/snapshots/begin ") {
                    serde_json::json!({"upload_id":"upload-fixture","missing_chunk_ids":[]})
                        .to_string()
                } else if first.contains("/commit ") {
                    serde_json::json!({
                        "outcome":"created",
                        "head":snapshot_id,
                        "conflict_snapshot":null
                    })
                    .to_string()
                } else {
                    String::new()
                };
                write!(stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body,
                ).unwrap();
                if is_commit {
                    break;
                }
            }
            paths
        })
    }

    #[test]
    fn android_durable_queue_survives_failed_upload_attempt() {
        let source = tempfile::tempdir().unwrap();
        let queue = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("system"), b"offline-save").unwrap();
        let secret = [21u8; 32];
        let queued = queue_android_stable_stage(
            source.path(),
            queue.path(),
            "http://127.0.0.1:9",
            &secret,
            "mh3g-nemessix-jp-slot1",
            None,
            "android-fixture",
        )
        .unwrap();
        assert_eq!(queued.pending_count, 1);
        assert!(queue.path().join(&queued.bundle_path).is_file());

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let drained = runtime.block_on(drain_android_upload_queue(
            queue.path(),
            "http://127.0.0.1:9",
            &secret,
        ));
        assert_eq!(drained.uploaded_count, 0);
        assert_eq!(drained.pending_count, 1);
        assert_eq!(drained.failed_count, 1);
        assert_eq!(
            drained.last_error.as_deref(),
            Some("network_or_server_failure")
        );

        let store =
            save_engine::local_store::LocalStore::open(&queue.path().join("state.sqlite")).unwrap();
        let retry = store.retryable_uploads("http://127.0.0.1:9", 10).unwrap();
        assert_eq!(retry[0].attempts, 1);
        assert_eq!(
            retry[0].last_error.as_deref(),
            Some("network_or_server_failure")
        );
    }

    #[test]
    fn android_durable_queue_chains_offline_snapshots_without_mtime_lww() {
        let source = tempfile::tempdir().unwrap();
        let queue = tempfile::tempdir().unwrap();
        let secret = [22u8; 32];
        std::fs::write(source.path().join("system"), b"first").unwrap();
        let first = queue_android_stable_stage(
            source.path(),
            queue.path(),
            "https://sync.example.test",
            &secret,
            "mh3g-nemessix-jp-slot1",
            Some("cloud-base"),
            "android-fixture",
        )
        .unwrap();
        std::fs::write(source.path().join("system"), b"second").unwrap();
        let second = queue_android_stable_stage(
            source.path(),
            queue.path(),
            "https://sync.example.test",
            &secret,
            "mh3g-nemessix-jp-slot1",
            Some("cloud-base"),
            "android-fixture",
        )
        .unwrap();
        assert_ne!(first.snapshot_id, second.snapshot_id);
        let store =
            save_engine::local_store::LocalStore::open(&queue.path().join("state.sqlite")).unwrap();
        let jobs = store
            .retryable_uploads("https://sync.example.test", 10)
            .unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].base_head.as_deref(), Some("cloud-base"));
        assert_eq!(
            jobs[1].base_head.as_deref(),
            Some(first.snapshot_id.as_str())
        );
    }

    #[test]
    fn android_durable_queue_retries_to_real_upload_and_removes_bundle() {
        let source = tempfile::tempdir().unwrap();
        let queue = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        std::fs::write(source.path().join("system"), b"retry-success").unwrap();
        let secret = [23u8; 32];
        let queued = queue_android_stable_stage(
            source.path(),
            queue.path(),
            &server,
            &secret,
            "mh3g-nemessix-jp-slot1",
            None,
            "android-fixture",
        )
        .unwrap();
        let bundle = queue.path().join(&queued.bundle_path);
        let store =
            save_engine::local_store::LocalStore::open(&queue.path().join("state.sqlite")).unwrap();
        let first_attempt = store.retryable_uploads(&server, 1).unwrap().remove(0);
        store
            .mark_upload_failed(first_attempt.id, "network_or_server_failure")
            .unwrap();
        let server_thread = spawn_upload_server(listener, queued.snapshot_id.clone());
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let drained = runtime.block_on(drain_android_upload_queue(queue.path(), &server, &secret));
        let paths = server_thread.join().unwrap();
        assert_eq!(
            drained.uploaded_count, 1,
            "drained={drained:?} paths={paths:?}"
        );
        assert_eq!(drained.pending_count, 0);
        assert_eq!(
            drained.last_cloud_head.as_deref(),
            Some(queued.snapshot_id.as_str())
        );
        assert!(!bundle.exists());
        assert!(paths.iter().any(|path| path.contains("/commit ")));
        assert!(store.retryable_uploads(&server, 1).unwrap().is_empty());
    }

    #[test]
    fn cloud_head_json_is_strict_and_null_is_not_an_error() {
        assert_eq!(
            serde_json::to_string(&CloudHeadReport { head: None }).unwrap(),
            r#"{"head":null}"#
        );
        assert_eq!(
            serde_json::to_string(&CloudHeadReport {
                head: Some("abc123".into())
            })
            .unwrap(),
            r#"{"head":"abc123"}"#
        );
    }

    #[test]
    fn cloud_head_rejects_path_injection_ids() {
        assert!(validate_logical_save_id("mh3g-save_01").is_ok());
        for invalid in ["", "../heads/other", "save/id", "save?id=x", "存档"] {
            assert!(validate_logical_save_id(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn bridge_uses_shared_conflict_engine() {
        let decision =
            bridge_head_decision(Some("base".into()), Some("other".into()), "incoming".into());
        assert_eq!(decision.kind, BridgeHeadKind::Conflict);
        assert_eq!(decision.head, "other");
        assert_eq!(decision.conflict_snapshot.as_deref(), Some("incoming"));
        assert!(!bridge_info().automatic_restore);
    }

    #[test]
    fn auto_restore_is_off_and_conflict_visible() {
        let c = SyncCoordinator::new(SyncPolicy::default());
        assert!(!c.policy.auto_restore);
        assert_eq!(
            c.pre_launch_decision(Some(&SnapshotId("a".into())), Some(&SnapshotId("b".into()))),
            VisibleState::Conflict
        );
    }

    #[test]
    fn launch_gate_keeps_cloud_unavailable_local_safe() {
        let decision = describe_launch_gate_zh(true, false, false, Some("local".into()), None);
        assert_eq!(decision.kind, LaunchGateKind::CloudUnavailable);
        assert!(decision.allows_local_play);
        assert!(!decision.allows_restore_now);
        assert!(decision.summary_zh.contains("不会破坏本地原始存档"));
    }

    #[test]
    fn launch_gate_lists_conflict_sides_without_last_write_wins() {
        let decision = describe_launch_gate_zh(
            true,
            true,
            true,
            Some("local-head-abcdef".into()),
            Some("remote-head-123456".into()),
        );
        assert_eq!(decision.kind, LaunchGateKind::Conflict);
        assert!(decision.summary_zh.contains("不会按最新时间自动覆盖"));
        assert!(decision.allows_local_play);
        assert!(!decision.allows_restore_now);
        assert_eq!(decision.local_side.as_ref().unwrap().label_zh, "本地");
        assert_eq!(decision.remote_side.as_ref().unwrap().label_zh, "云端");
    }

    #[test]
    fn launch_gate_remote_newer_downloads_before_restore() {
        let decision = describe_launch_gate_zh(true, true, false, None, Some("remote".into()));
        assert_eq!(decision.kind, LaunchGateKind::RemoteNewer);
        assert!(decision.allows_restore_now);
        assert!(decision.summary_zh.contains("先下载到本地 CAS 缓存"));
    }

    #[test]
    fn watcher_marks_dirty_but_never_uploads() {
        let decision = decide_automation_event(AutomationEventKind::DirtyObserved, false, true);
        assert!(decision.mark_dirty);
        assert!(!decision.create_snapshot_candidate);
        assert!(!decision.upload_allowed);
        assert!(decision.summary_zh.contains("只标记 dirty"));
    }

    #[test]
    fn exit_and_save_complete_reconcile_dirty_session() {
        for event in [
            AutomationEventKind::SaveComplete,
            AutomationEventKind::EmulatorExit,
            AutomationEventKind::PeriodicReconcile,
            AutomationEventKind::ManualSync,
        ] {
            let decision = decide_automation_event(event, true, true);
            assert!(decision.create_snapshot_candidate, "{event:?}");
            assert!(decision.upload_allowed, "{event:?}");
            assert!(!decision.restore_allowed, "{event:?}");
            assert!(decision.summary_zh.contains("稳定快照"));
        }
    }

    #[test]
    fn running_emulator_blocks_restore_even_when_remote_newer() {
        let decision = decide_restore_event(true, true);
        assert!(!decision.restore_allowed);
        assert!(!decision.upload_allowed);
        assert!(decision.summary_zh.contains("禁止云端覆盖本地"));
    }

    #[test]
    fn downloaded_bundle_is_verified_before_restore_staging() {
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("system"), b"fixture-save").unwrap();
        let secret = [9u8; 32];
        let snapshot = create_snapshot_from_stable_folder(
            source.path(),
            &save_adapters::nemessix_android(),
            &secret,
            SnapshotOptions::fixture(GameKey::new("mh3g", "jp", "none", "slot1")),
        )
        .unwrap();
        assert!(verify_snapshot_content_id(&secret, &snapshot).is_ok());
        let mut forged_id = snapshot.clone();
        forged_id.snapshot_id = SnapshotId::from_parts(&[b"forged-bundle-id"]);
        assert!(verify_snapshot_content_id(&secret, &forged_id).is_err());
        let manifest_bytes = serde_json::to_vec(&snapshot.encrypted_manifest).unwrap();
        let object = |id: String, bytes: Vec<u8>| DownloadObject {
            object_id: id,
            sha256: hex::encode(sha2::Sha256::digest(&bytes)),
            bytes_b64: base64::engine::general_purpose::STANDARD.encode(bytes),
        };
        let bundle = DownloadBundle {
            snapshot_id: snapshot.snapshot_id.clone(),
            encrypted_manifest: object(
                hex::encode(sha2::Sha256::digest(&manifest_bytes)),
                manifest_bytes,
            ),
            chunks: snapshot
                .chunks
                .iter()
                .map(|(id, blob)| object(id.clone(), serde_json::to_vec(blob).unwrap()))
                .collect(),
        };
        assert_eq!(
            decode_download_bundle(&secret, &snapshot.snapshot_id.0, bundle.clone())
                .unwrap()
                .snapshot_id,
            snapshot.snapshot_id
        );
        let mut corrupt = bundle.clone();
        corrupt.chunks[0].sha256 = "00".repeat(32);
        assert!(decode_download_bundle(&secret, &snapshot.snapshot_id.0, corrupt).is_err());
        let mut duplicate = bundle.clone();
        duplicate.chunks.push(duplicate.chunks[0].clone());
        assert!(decode_download_bundle(&secret, &snapshot.snapshot_id.0, duplicate).is_err());
        let mut wrong_nonce = bundle;
        let mut blob: EncryptedBlob = base64::engine::general_purpose::STANDARD
            .decode(&wrong_nonce.encrypted_manifest.bytes_b64)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap();
        blob.nonce[0] ^= 1;
        let bytes = serde_json::to_vec(&blob).unwrap();
        wrong_nonce.encrypted_manifest.object_id = hex::encode(sha2::Sha256::digest(&bytes));
        wrong_nonce.encrypted_manifest.sha256 = wrong_nonce.encrypted_manifest.object_id.clone();
        wrong_nonce.encrypted_manifest.bytes_b64 =
            base64::engine::general_purpose::STANDARD.encode(bytes);
        assert!(decode_download_bundle(&secret, &snapshot.snapshot_id.0, wrong_nonce).is_err());
        assert!(validate_encoded_object_size(MAX_ENCODED_OBJECT_BYTES + 1).is_err());
    }

    #[test]
    fn conflict_summary_counts_changed_paths_without_exposing_them() {
        let entry = |path: &str, size: u64, hash: &str| save_domain::ManifestEntry {
            path: path.into(),
            kind: save_domain::FileKind::Regular,
            size,
            plaintext_sha256: hash.into(),
            chunks: Vec::new(),
        };
        let head = BTreeMap::from([
            ("slot/a".into(), entry("slot/a", 10, "aa")),
            ("slot/b".into(), entry("slot/b", 20, "bb")),
        ]);
        let branch = BTreeMap::from([
            ("slot/a".into(), entry("slot/a", 10, "changed")),
            ("slot/c".into(), entry("slot/c", 30, "cc")),
        ]);
        assert_eq!(manifest_diff_summary(&head, &branch), (3, 60));
    }
}
