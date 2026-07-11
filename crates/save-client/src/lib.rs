use base64::Engine;
use ed25519_dalek::SigningKey;
use save_crypto::{
    account_handle, account_root_signing_key, derive_account_keys, deterministic_cbor,
    issue_device_certificate_with_id,
};
use save_domain::{AdapterDescriptor, SnapshotId};
use save_domain::{DeviceId, GameKey, LogicalSaveId};
use save_engine::{HeadUpdate, decide_head_update};
use save_engine::{SnapshotOptions, create_snapshot_from_stable_folder};
use serde::{Deserialize, Serialize};
use sha2::Digest;
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
pub struct CloudHeadReport {
    pub head: Option<String>,
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
        snapshot_id: snapshot.snapshot_id.0,
        cloud_head: commit.head.0,
        conflict_snapshot: commit.conflict_snapshot.map(|v| v.0),
        file_count: snapshot.fingerprint.file_count,
        total_bytes: snapshot.fingerprint.total_bytes,
    })
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
}
