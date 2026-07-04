use save_domain::{AdapterDescriptor, SnapshotId};
use save_engine::{HeadUpdate, decide_head_update};
use serde::{Deserialize, Serialize};

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
    fn auto_restore_is_off_and_conflict_visible() {
        let c = SyncCoordinator::new(SyncPolicy::default());
        assert!(!c.policy.auto_restore);
        assert_eq!(
            c.pre_launch_decision(Some(&SnapshotId("a".into())), Some(&SnapshotId("b".into()))),
            VisibleState::Conflict
        );
    }
}
