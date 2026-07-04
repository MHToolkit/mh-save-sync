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
}
