use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path};

pub const SNAPSHOT_FORMAT_VERSION: u16 = 1;
pub const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Macos,
    Android,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportLevel {
    RuntimeVerified,
    PathVerified,
    FixtureVerified,
    Experimental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RootAcquisition {
    NativePath,
    SafTree,
    AuthenticatedIpc,
    UserSelectedFolder,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub String);

impl SnapshotId {
    pub fn from_parts(parts: &[&[u8]]) -> Self {
        let mut h = Sha256::new();
        h.update(b"mh-save-sync/snapshot-id/v1\0");
        for p in parts {
            h.update((p.len() as u64).to_be_bytes());
            h.update(p);
        }
        Self(hex::encode(h.finalize()))
    }
}

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LogicalSaveId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameKey {
    pub family: String,
    pub title_id: String,
    pub region: String,
    pub update: Option<String>,
    pub slot: String,
}

impl GameKey {
    pub fn new(
        family: impl Into<String>,
        title_id: impl Into<String>,
        region: impl Into<String>,
        slot: impl Into<String>,
    ) -> Self {
        Self {
            family: family.into(),
            title_id: title_id.into(),
            region: region.into(),
            update: None,
            slot: slot.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterDescriptor {
    pub emulator_id: String,
    pub platform: Platform,
    pub bundle_ids: Vec<String>,
    pub package_ids: Vec<String>,
    pub process_names: Vec<String>,
    pub root_acquisition: RootAcquisition,
    pub user_root_hint: Option<String>,
    pub game_key_contract: String,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
    pub capabilities: AdapterCapabilities,
    pub stability: StabilityPolicy,
    pub restore: RestorePolicy,
    pub support_level: SupportLevel,
    pub evidence_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    pub save_complete_event: bool,
    pub launch_gate: bool,
    pub exit_reconcile: bool,
    pub dirty_observer: bool,
    pub saf_restore_journal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StabilityPolicy {
    pub debounce_ms: u64,
    pub observations: u8,
    pub observation_gap_ms: u64,
    pub max_wait_ms: u64,
}

impl Default for StabilityPolicy {
    fn default() -> Self {
        Self {
            debounce_ms: 2_000,
            observations: 2,
            observation_gap_ms: 500,
            max_wait_ms: 10_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePolicy {
    pub require_emulator_stopped: bool,
    pub require_pre_restore_snapshot: bool,
    pub atomic_replace: bool,
    pub saf_journal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    Regular,
    Tombstone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRef {
    pub id: String,
    pub plaintext_size: u64,
    pub compressed_size: u64,
    pub ciphertext_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub plaintext_sha256: String,
    pub chunks: Vec<ChunkRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub format_version: u16,
    pub game_key: GameKey,
    pub logical_save_id: LogicalSaveId,
    pub device_id: DeviceId,
    pub parents: Vec<SnapshotId>,
    pub entries: Vec<ManifestEntry>,
    pub created_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeFingerprint {
    pub file_count: u64,
    pub total_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("path is absolute: {0}")]
    AbsolutePath(String),
    #[error("path escapes root: {0}")]
    ParentTraversal(String),
    #[error("path is empty or current directory")]
    EmptyPath,
    #[error("path contains platform prefix: {0}")]
    PrefixPath(String),
    #[error("duplicate manifest path: {0}")]
    DuplicatePath(String),
    #[error("case-insensitive path collision: {0}")]
    CaseCollision(String),
    #[error("unsupported file kind for manifest path: {0}")]
    UnsupportedFileKind(String),
    #[error("file count limit exceeded")]
    FileCountLimit,
    #[error("total byte limit exceeded")]
    TotalBytesLimit,
}

pub fn validate_relative_path(path: &str) -> Result<(), DomainError> {
    if path.is_empty() || path == "." {
        return Err(DomainError::EmptyPath);
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(DomainError::AbsolutePath(path.to_string()));
    }
    for component in p.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => return Err(DomainError::EmptyPath),
            Component::ParentDir => return Err(DomainError::ParentTraversal(path.to_string())),
            Component::RootDir => return Err(DomainError::AbsolutePath(path.to_string())),
            Component::Prefix(_) => return Err(DomainError::PrefixPath(path.to_string())),
        }
    }
    Ok(())
}

pub fn validate_manifest_entries(
    entries: &[ManifestEntry],
    max_files: usize,
    max_total_bytes: u64,
) -> Result<(), DomainError> {
    if entries.len() > max_files {
        return Err(DomainError::FileCountLimit);
    }
    let mut exact = BTreeSet::new();
    let mut folded = BTreeSet::new();
    let mut total = 0u64;
    for entry in entries {
        validate_relative_path(&entry.path)?;
        if !exact.insert(entry.path.clone()) {
            return Err(DomainError::DuplicatePath(entry.path.clone()));
        }
        let lower = entry.path.to_lowercase();
        if !folded.insert(lower) {
            return Err(DomainError::CaseCollision(entry.path.clone()));
        }
        total = total
            .checked_add(entry.size)
            .ok_or(DomainError::TotalBytesLimit)?;
        if total > max_total_bytes {
            return Err(DomainError::TotalBytesLimit);
        }
    }
    Ok(())
}

pub fn stable_logical_save_id(adapter: &str, game_key: &GameKey) -> LogicalSaveId {
    let mut h = Sha256::new();
    h.update(b"mh-save-sync/logical-save/v1\0");
    h.update(adapter.as_bytes());
    h.update(b"\0");
    h.update(game_key.family.as_bytes());
    h.update(b"\0");
    h.update(game_key.title_id.as_bytes());
    h.update(b"\0");
    h.update(game_key.region.as_bytes());
    h.update(b"\0");
    h.update(game_key.slot.as_bytes());
    LogicalSaveId(hex::encode(h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_absolute_and_case_collision() {
        assert!(matches!(
            validate_relative_path("../x"),
            Err(DomainError::ParentTraversal(_))
        ));
        assert!(matches!(
            validate_relative_path("/x"),
            Err(DomainError::AbsolutePath(_))
        ));
        let entries = vec![
            ManifestEntry {
                path: "SAVEDATA.bin".into(),
                kind: FileKind::Regular,
                size: 1,
                plaintext_sha256: "00".into(),
                chunks: vec![],
            },
            ManifestEntry {
                path: "savedata.bin".into(),
                kind: FileKind::Regular,
                size: 1,
                plaintext_sha256: "00".into(),
                chunks: vec![],
            },
        ];
        assert!(matches!(
            validate_manifest_entries(&entries, 10, 100),
            Err(DomainError::CaseCollision(_))
        ));
    }
}
