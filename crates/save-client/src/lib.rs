use save_domain::{AdapterDescriptor, SnapshotId};
use save_engine::{HeadUpdate, decide_head_update};
use serde::{Deserialize, Serialize};

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
}
