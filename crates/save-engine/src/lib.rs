use save_crypto::{EncryptedBlob, chunk_id, decrypt_bytes, derive_account_keys, encrypt_bytes};
use save_domain::{
    AdapterDescriptor, ChunkRef, DEFAULT_CHUNK_SIZE, DeviceId, FileKind, GameKey, LogicalSaveId,
    ManifestEntry, SNAPSHOT_FORMAT_VERSION, SnapshotId, SnapshotManifest, TreeFingerprint,
    validate_manifest_entries,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use walkdir::WalkDir;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),
    #[error("domain error: {0}")]
    Domain(#[from] save_domain::DomainError),
    #[error("crypto error: {0}")]
    Crypto(#[from] save_crypto::CryptoError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("staging fingerprint changed")]
    StagingChanged,
    #[error("symlink or non-regular file rejected: {0}")]
    RejectedFile(String),
    #[error("missing chunk: {0}")]
    MissingChunk(String),
    #[error("restore refused while emulator is running")]
    EmulatorRunning,
    #[error("invalid encrypted chunk metadata")]
    InvalidChunkMetadata,
    #[error("encrypted chunk identifier does not match plaintext")]
    ChunkIdMismatch,
    #[error("restored file failed integrity verification")]
    FileIntegrityMismatch,
}

const AEAD_TAG_SIZE: u64 = 16;

fn max_compressed_chunk_size() -> u64 {
    zstd::zstd_safe::compress_bound(DEFAULT_CHUNK_SIZE) as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSnapshot {
    pub snapshot_id: SnapshotId,
    pub encrypted_manifest: EncryptedBlob,
    pub chunks: BTreeMap<String, EncryptedBlob>,
    pub fingerprint: TreeFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SaveDiffChange {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRangeDiff {
    pub offset: u64,
    pub len: u64,
    pub left_sha256: Option<String>,
    pub right_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveDiffEntry {
    pub path: String,
    pub change: SaveDiffChange,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
    pub left_sha256: Option<String>,
    pub right_sha256: Option<String>,
    pub byte_ranges: Vec<ByteRangeDiff>,
    pub notes_zh: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSaveDiffReport {
    pub game_profile: String,
    pub parser_id: String,
    pub parser_support: String,
    pub semantic_available: bool,
    pub summary_zh: String,
    pub changed_files: usize,
    pub added_files: usize,
    pub removed_files: usize,
    pub modified_files: usize,
    pub total_left_bytes: u64,
    pub total_right_bytes: u64,
    pub entries: Vec<SaveDiffEntry>,
}

#[derive(Debug, Clone)]
struct FileSummary {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadUpdate {
    FastForward {
        new_head: SnapshotId,
    },
    Conflict {
        current_head: SnapshotId,
        conflict_head: SnapshotId,
    },
    FirstSnapshot {
        new_head: SnapshotId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmulatorState {
    Stopped,
    Running,
}

#[derive(Debug, Clone)]
pub struct SnapshotOptions {
    pub device_id: DeviceId,
    pub logical_save_id: LogicalSaveId,
    pub game_key: GameKey,
    pub parents: Vec<SnapshotId>,
    pub created_unix_ms: u64,
    pub max_files: usize,
    pub max_total_bytes: u64,
}

impl SnapshotOptions {
    pub fn fixture(game_key: GameKey) -> Self {
        Self {
            device_id: DeviceId("fixture-device".into()),
            logical_save_id: LogicalSaveId("fixture-logical-save".into()),
            game_key,
            parents: vec![],
            created_unix_ms: 1,
            max_files: 10_000,
            max_total_bytes: 128 * 1024 * 1024,
        }
    }
}

pub fn fingerprint_tree(
    root: &Path,
    exclude_prefixes: &[String],
) -> Result<TreeFingerprint, EngineError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.path() == root {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if exclude_prefixes
            .iter()
            .any(|p| rel == *p || rel.starts_with(&format!("{}/", p.trim_end_matches('/'))))
        {
            continue;
        }
        let ty = entry.file_type();
        if ty.is_symlink() {
            return Err(EngineError::RejectedFile(rel));
        }
        if ty.is_file() {
            files.push((rel, entry.path().to_path_buf()));
        } else if !ty.is_dir() {
            return Err(EngineError::RejectedFile(rel));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = Sha256::new();
    let mut total = 0u64;
    for (rel, path) in &files {
        let bytes = fs::read(path)?;
        total += bytes.len() as u64;
        h.update(rel.as_bytes());
        h.update([0]);
        h.update((bytes.len() as u64).to_be_bytes());
        h.update([0]);
        h.update(Sha256::digest(&bytes));
    }
    Ok(TreeFingerprint {
        file_count: files.len() as u64,
        total_bytes: total,
        sha256: hex::encode(h.finalize()),
    })
}

pub fn create_snapshot_from_stable_folder(
    root: &Path,
    descriptor: &AdapterDescriptor,
    secret: &[u8; 32],
    options: SnapshotOptions,
) -> Result<EncryptedSnapshot, EngineError> {
    let before = fingerprint_tree(root, &descriptor.exclude_globs)?;
    let stage = tempfile::tempdir()?;
    copy_tree(root, stage.path(), &descriptor.exclude_globs)?;
    let staged = fingerprint_tree(stage.path(), &[])?;
    if before != staged {
        return Err(EngineError::StagingChanged);
    }
    let snapshot = encrypt_staged_tree(stage.path(), secret, options, before)?;
    Ok(snapshot)
}

fn copy_tree(source: &Path, dest: &Path, excludes: &[String]) -> Result<(), EngineError> {
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry?;
        if entry.path() == source {
            continue;
        }
        let rel = entry.path().strip_prefix(source).unwrap();
        let rel_string = rel.to_string_lossy().replace('\\', "/");
        if excludes.iter().any(|p| {
            rel_string == *p || rel_string.starts_with(&format!("{}/", p.trim_end_matches('/')))
        }) {
            continue;
        }
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        } else {
            return Err(EngineError::RejectedFile(rel_string));
        }
    }
    Ok(())
}

fn encrypt_staged_tree(
    stage: &Path,
    secret: &[u8; 32],
    options: SnapshotOptions,
    fingerprint: TreeFingerprint,
) -> Result<EncryptedSnapshot, EngineError> {
    let keys = derive_account_keys(secret)?;
    let mut entries = Vec::new();
    let mut chunks = BTreeMap::new();
    let mut files = Vec::<(String, PathBuf)>::new();
    for entry in WalkDir::new(stage).follow_links(false) {
        let entry = entry?;
        if entry.path() == stage {
            continue;
        }
        if entry.file_type().is_file() {
            let rel = entry
                .path()
                .strip_prefix(stage)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            files.push((rel, entry.path().to_path_buf()));
        } else if entry.file_type().is_symlink() || !entry.file_type().is_dir() {
            return Err(EngineError::RejectedFile(
                entry.path().display().to_string(),
            ));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (rel, path) in files {
        let mut file = fs::File::open(&path)?;
        let mut all = Vec::new();
        file.read_to_end(&mut all)?;
        let file_hash = hex::encode(Sha256::digest(&all));
        let mut refs = Vec::new();
        for chunk in all.chunks(DEFAULT_CHUNK_SIZE) {
            let id = chunk_id(&keys, chunk);
            let compressed = zstd::bulk::compress(chunk, 3)?;
            let aad = format!("mh-save-sync/chunk/v1/{id}");
            let encrypted = encrypt_bytes(&keys, aad.as_bytes(), &compressed)?;
            refs.push(ChunkRef {
                id: id.clone(),
                plaintext_size: chunk.len() as u64,
                compressed_size: compressed.len() as u64,
                ciphertext_size: encrypted.ciphertext.len() as u64,
            });
            chunks.entry(id).or_insert(encrypted);
        }
        entries.push(ManifestEntry {
            path: rel,
            kind: FileKind::Regular,
            size: all.len() as u64,
            plaintext_sha256: file_hash,
            chunks: refs,
        });
    }
    validate_manifest_entries(&entries, options.max_files, options.max_total_bytes)?;
    let manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        game_key: options.game_key,
        logical_save_id: options.logical_save_id,
        device_id: options.device_id,
        parents: options.parents,
        entries,
        created_unix_ms: options.created_unix_ms,
    };
    let manifest_plain = serde_json::to_vec(&manifest)?;
    let encrypted_manifest = encrypt_bytes(&keys, b"mh-save-sync/manifest/v1", &manifest_plain)?;
    let parent_bytes: Vec<Vec<u8>> = manifest
        .parents
        .iter()
        .map(|p| p.0.as_bytes().to_vec())
        .collect();
    let mut parts: Vec<&[u8]> = vec![b"v1", &encrypted_manifest.ciphertext];
    for p in &parent_bytes {
        parts.push(p);
    }
    let snapshot_id = SnapshotId::from_parts(&parts);
    Ok(EncryptedSnapshot {
        snapshot_id,
        encrypted_manifest,
        chunks,
        fingerprint,
    })
}

pub fn decrypt_manifest(
    secret: &[u8; 32],
    snapshot: &EncryptedSnapshot,
) -> Result<SnapshotManifest, EngineError> {
    let keys = derive_account_keys(secret)?;
    let bytes = decrypt_bytes(
        &keys,
        b"mh-save-sync/manifest/v1",
        &snapshot.encrypted_manifest,
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn decide_head_update(
    base_head: Option<&SnapshotId>,
    current_head: Option<&SnapshotId>,
    new_snapshot: &SnapshotId,
) -> HeadUpdate {
    match (base_head, current_head) {
        (None, None) => HeadUpdate::FirstSnapshot {
            new_head: new_snapshot.clone(),
        },
        (Some(base), Some(current)) if base == current => HeadUpdate::FastForward {
            new_head: new_snapshot.clone(),
        },
        (None, Some(current)) => HeadUpdate::Conflict {
            current_head: current.clone(),
            conflict_head: new_snapshot.clone(),
        },
        (Some(_), None) => HeadUpdate::Conflict {
            current_head: SnapshotId("missing-current-head".into()),
            conflict_head: new_snapshot.clone(),
        },
        (Some(_), Some(current)) => HeadUpdate::Conflict {
            current_head: current.clone(),
            conflict_head: new_snapshot.clone(),
        },
    }
}

pub fn restore_snapshot_to_folder(
    secret: &[u8; 32],
    snapshot: &EncryptedSnapshot,
    target: &Path,
    emulator_state: EmulatorState,
) -> Result<PathBuf, EngineError> {
    restore_snapshot_to_folder_with_failpoint(secret, snapshot, target, emulator_state, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RestorePhase {
    StageComplete,
    TargetBackedUp,
    StageInstalled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RestoreJournal {
    version: u16,
    phase: RestorePhase,
    target_name: String,
    original_target_existed: bool,
    expected_fingerprint: TreeFingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreFailpoint {
    StageComplete,
    TargetBackedUp,
    StageInstalled,
    ReceiptLost,
}

#[derive(Debug)]
struct RestorePaths {
    parent: PathBuf,
    target: PathBuf,
    backup: PathBuf,
    stage: PathBuf,
    journal: PathBuf,
    journal_tmp: PathBuf,
    target_name: String,
}

static RESTORE_PROCESS_LOCK: Mutex<()> = Mutex::new(());

struct RestoreLockGuard {
    _process_guard: MutexGuard<'static, ()>,
    #[cfg(unix)]
    directory: fs::File,
}

#[cfg(unix)]
impl Drop for RestoreLockGuard {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        const LOCK_UN: i32 = 8;
        // SAFETY: `directory` owns a valid open descriptor for the lifetime of this guard.
        let _ = unsafe { flock(self.directory.as_raw_fd(), LOCK_UN) };
    }
}

#[cfg(unix)]
unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

impl RestorePaths {
    fn for_target(target: &Path) -> Result<Self, EngineError> {
        let target_name = target
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .ok_or_else(|| EngineError::RejectedFile(target.display().to_string()))?
            .to_string();
        let parent = target
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Ok(Self {
            target: target.to_path_buf(),
            backup: parent.join(format!("{target_name}.mhsave-backup")),
            stage: parent.join(format!(".{target_name}.mhsave-restore-stage")),
            journal: parent.join(format!(".{target_name}.mhsave-restore-journal.json")),
            journal_tmp: parent.join(format!(".{target_name}.mhsave-restore-journal.tmp")),
            parent,
            target_name,
        })
    }
}

fn restore_snapshot_to_folder_with_failpoint(
    secret: &[u8; 32],
    snapshot: &EncryptedSnapshot,
    target: &Path,
    emulator_state: EmulatorState,
    failpoint: Option<RestoreFailpoint>,
) -> Result<PathBuf, EngineError> {
    if emulator_state != EmulatorState::Stopped {
        return Err(EngineError::EmulatorRunning);
    }
    let paths = RestorePaths::for_target(target)?;
    fs::create_dir_all(&paths.parent)?;
    reject_symlink_or_wrong_kind(&paths.parent, true)?;
    let _restore_lock = acquire_restore_lock(target)?;
    recover_interrupted_restore_locked(&paths)?;
    reject_symlink_or_wrong_kind(&paths.target, true)?;

    let keys = derive_account_keys(secret)?;
    let manifest = decrypt_manifest(secret, snapshot)?;
    validate_manifest_entries(&manifest.entries, 10_000, 128 * 1024 * 1024)?;
    validate_restore_manifest_chunks(&manifest, snapshot)?;

    remove_restore_dir_if_present(&paths.stage)?;
    fs::create_dir(&paths.stage)?;
    let stage_result = (|| -> Result<(), EngineError> {
        for entry in &manifest.entries {
            if entry.kind == FileKind::Tombstone {
                continue;
            }
            let out = checked_restore_path(&paths.stage, &entry.path)?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut writer = fs::File::create(&out)?;
            let mut hasher = Sha256::new();
            let mut total = 0u64;
            for cref in &entry.chunks {
                let blob = snapshot
                    .chunks
                    .get(&cref.id)
                    .ok_or_else(|| EngineError::MissingChunk(cref.id.clone()))?;
                validate_chunk_metadata(cref, blob)?;
                let compressed = decrypt_bytes(
                    &keys,
                    format!("mh-save-sync/chunk/v1/{}", cref.id).as_bytes(),
                    blob,
                )?;
                if compressed.len() as u64 != cref.compressed_size {
                    return Err(EngineError::InvalidChunkMetadata);
                }
                let plaintext_limit = usize::try_from(cref.plaintext_size)
                    .map_err(|_| EngineError::InvalidChunkMetadata)?;
                let plaintext = zstd::bulk::decompress(&compressed, plaintext_limit)?;
                if plaintext.len() as u64 != cref.plaintext_size {
                    return Err(EngineError::InvalidChunkMetadata);
                }
                if chunk_id(&keys, &plaintext) != cref.id {
                    return Err(EngineError::ChunkIdMismatch);
                }
                total = total
                    .checked_add(plaintext.len() as u64)
                    .ok_or(EngineError::InvalidChunkMetadata)?;
                hasher.update(&plaintext);
                writer.write_all(&plaintext)?;
            }
            writer.sync_all()?;
            if total != entry.size || hex::encode(hasher.finalize()) != entry.plaintext_sha256 {
                return Err(EngineError::FileIntegrityMismatch);
            }
        }
        sync_tree(&paths.stage)?;
        Ok(())
    })();
    if let Err(e) = stage_result {
        let _ = remove_restore_dir_if_present(&paths.stage);
        return Err(e);
    }

    let journal = RestoreJournal {
        version: 1,
        phase: RestorePhase::StageComplete,
        target_name: paths.target_name.clone(),
        original_target_existed: paths.target.exists(),
        expected_fingerprint: fingerprint_tree(&paths.stage, &[])?,
    };
    write_restore_journal(&paths, &journal)?;
    interrupt_if(failpoint, RestoreFailpoint::StageComplete, "stage-complete")?;

    remove_restore_dir_if_present(&paths.backup)?;
    if paths.target.exists() {
        fs::rename(&paths.target, &paths.backup)?;
        sync_dir(&paths.parent)?;
    }
    interrupt_if(
        failpoint,
        RestoreFailpoint::TargetBackedUp,
        "target-backed-up",
    )?;
    let mut journal = journal;
    journal.phase = RestorePhase::TargetBackedUp;
    write_restore_journal(&paths, &journal)?;

    fs::rename(&paths.stage, &paths.target)?;
    sync_dir(&paths.parent)?;
    interrupt_if(
        failpoint,
        RestoreFailpoint::StageInstalled,
        "stage-installed",
    )?;
    journal.phase = RestorePhase::StageInstalled;
    write_restore_journal(&paths, &journal)?;

    remove_restore_file_if_present(&paths.journal)?;
    sync_dir(&paths.parent)?;
    interrupt_if(failpoint, RestoreFailpoint::ReceiptLost, "receipt-lost")?;
    Ok(paths.backup)
}

fn validate_restore_manifest_chunks(
    manifest: &SnapshotManifest,
    snapshot: &EncryptedSnapshot,
) -> Result<(), EngineError> {
    for entry in &manifest.entries {
        if entry.kind == FileKind::Tombstone {
            if entry.size != 0 || !entry.chunks.is_empty() {
                return Err(EngineError::InvalidChunkMetadata);
            }
            continue;
        }
        if entry.size == 0 {
            if !entry.chunks.is_empty() {
                return Err(EngineError::InvalidChunkMetadata);
            }
            continue;
        }
        let expected_chunks = entry
            .size
            .checked_add(DEFAULT_CHUNK_SIZE as u64 - 1)
            .ok_or(EngineError::InvalidChunkMetadata)?
            / DEFAULT_CHUNK_SIZE as u64;
        if entry.chunks.len() as u64 != expected_chunks {
            return Err(EngineError::InvalidChunkMetadata);
        }
        let mut declared_total = 0u64;
        for (index, chunk) in entry.chunks.iter().enumerate() {
            let is_last = index + 1 == entry.chunks.len();
            let expected_plaintext_size = if is_last {
                let remainder = entry.size % DEFAULT_CHUNK_SIZE as u64;
                if remainder == 0 {
                    DEFAULT_CHUNK_SIZE as u64
                } else {
                    remainder
                }
            } else {
                DEFAULT_CHUNK_SIZE as u64
            };
            if chunk.plaintext_size != expected_plaintext_size {
                return Err(EngineError::InvalidChunkMetadata);
            }
            let blob = snapshot
                .chunks
                .get(&chunk.id)
                .ok_or_else(|| EngineError::MissingChunk(chunk.id.clone()))?;
            validate_chunk_metadata(chunk, blob)?;
            declared_total = declared_total
                .checked_add(chunk.plaintext_size)
                .ok_or(EngineError::InvalidChunkMetadata)?;
        }
        if declared_total != entry.size {
            return Err(EngineError::InvalidChunkMetadata);
        }
    }
    Ok(())
}

fn validate_chunk_metadata(
    chunk: &save_domain::ChunkRef,
    blob: &EncryptedBlob,
) -> Result<(), EngineError> {
    if chunk.id.len() != 64 || !chunk.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EngineError::InvalidChunkMetadata);
    }
    if chunk.plaintext_size == 0 || chunk.plaintext_size > DEFAULT_CHUNK_SIZE as u64 {
        return Err(EngineError::InvalidChunkMetadata);
    }
    if chunk.compressed_size == 0 || chunk.compressed_size > max_compressed_chunk_size() {
        return Err(EngineError::InvalidChunkMetadata);
    }
    let expected_ciphertext_size = chunk
        .compressed_size
        .checked_add(AEAD_TAG_SIZE)
        .ok_or(EngineError::InvalidChunkMetadata)?;
    if chunk.ciphertext_size != expected_ciphertext_size
        || chunk.ciphertext_size != blob.ciphertext.len() as u64
    {
        return Err(EngineError::InvalidChunkMetadata);
    }
    Ok(())
}

/// Recovers one native-folder restore transaction after process or power loss.
///
/// Clients must call this for every configured native save target during startup, before allowing
/// an emulator launch. `restore_snapshot_to_folder` also calls it while holding the same
/// transaction lock, so retrying a restore remains safe and idempotent.
pub fn recover_interrupted_restore(target: &Path) -> Result<(), EngineError> {
    let paths = RestorePaths::for_target(target)?;
    if !paths.parent.exists() {
        return Ok(());
    }
    let _restore_lock = acquire_restore_lock(target)?;
    recover_interrupted_restore_locked(&paths)
}

fn recover_interrupted_restore_locked(paths: &RestorePaths) -> Result<(), EngineError> {
    reject_symlink_or_wrong_kind(&paths.parent, true)?;
    reject_symlink_or_wrong_kind(&paths.target, true)?;
    reject_symlink_or_wrong_kind(&paths.backup, true)?;
    reject_symlink_or_wrong_kind(&paths.stage, true)?;
    reject_symlink_or_wrong_kind(&paths.journal, false)?;
    reject_symlink_or_wrong_kind(&paths.journal_tmp, false)?;

    remove_restore_file_if_present(&paths.journal_tmp)?;
    if !paths.journal.exists() {
        remove_restore_dir_if_present(&paths.stage)?;
        return Ok(());
    }

    let bytes = fs::read(&paths.journal)?;
    let journal: RestoreJournal = match serde_json::from_slice(&bytes) {
        Ok(journal) => journal,
        Err(_error) if paths.backup.exists() => {
            recover_from_corrupt_journal(paths)?;
            remove_restore_dir_if_present(&paths.stage)?;
            remove_restore_file_if_present(&paths.journal)?;
            sync_dir(&paths.parent)?;
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    if journal.version != 1 || journal.target_name != paths.target_name {
        if paths.backup.exists() {
            recover_from_corrupt_journal(paths)?;
            remove_restore_dir_if_present(&paths.stage)?;
            remove_restore_file_if_present(&paths.journal)?;
            sync_dir(&paths.parent)?;
            return Ok(());
        }
        return Err(EngineError::RejectedFile(
            "restore journal does not match target".into(),
        ));
    }

    let installed_new_tree = paths.target.exists()
        && !paths.stage.exists()
        && fingerprint_tree(&paths.target, &[])? == journal.expected_fingerprint;
    match journal.phase {
        RestorePhase::StageComplete => recover_stage_complete(paths, &journal)?,
        RestorePhase::StageInstalled if installed_new_tree => {}
        RestorePhase::TargetBackedUp if installed_new_tree => {}
        _ => rollback_interrupted_restore(paths, journal.original_target_existed)?,
    }
    remove_restore_dir_if_present(&paths.stage)?;
    remove_restore_file_if_present(&paths.journal)?;
    sync_dir(&paths.parent)?;
    Ok(())
}

fn acquire_restore_lock(target: &Path) -> Result<RestoreLockGuard, EngineError> {
    let paths = RestorePaths::for_target(target)?;
    let process_guard = RESTORE_PROCESS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        const LOCK_EX: i32 = 2;
        let directory = fs::File::open(&paths.parent)?;
        loop {
            // SAFETY: `directory` remains open in the returned guard and `LOCK_EX` is a valid
            // `flock(2)` operation on macOS, Linux, and Android.
            if unsafe { flock(directory.as_raw_fd(), LOCK_EX) } == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error.into());
            }
        }
        Ok(RestoreLockGuard {
            _process_guard: process_guard,
            directory,
        })
    }
    #[cfg(not(unix))]
    {
        Ok(RestoreLockGuard {
            _process_guard: process_guard,
        })
    }
}

fn recover_stage_complete(
    paths: &RestorePaths,
    journal: &RestoreJournal,
) -> Result<(), EngineError> {
    if journal.original_target_existed {
        if paths.target.exists() {
            return Ok(());
        }
        if paths.backup.exists() {
            fs::rename(&paths.backup, &paths.target)?;
            sync_dir(&paths.parent)?;
            return Ok(());
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "restore journal expected original target but neither target nor backup exists",
        )
        .into());
    }
    if paths.target.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "restore journal expected absent original target but target exists",
        )
        .into());
    }
    Ok(())
}

fn rollback_interrupted_restore(
    paths: &RestorePaths,
    original_target_existed: bool,
) -> Result<(), EngineError> {
    if original_target_existed && paths.backup.exists() {
        remove_restore_dir_if_present(&paths.target)?;
        fs::rename(&paths.backup, &paths.target)?;
        sync_dir(&paths.parent)?;
    } else if original_target_existed {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "restore rollback requires the original backup",
        )
        .into());
    } else if paths.target.exists() {
        remove_restore_dir_if_present(&paths.target)?;
        sync_dir(&paths.parent)?;
    }
    Ok(())
}

fn recover_from_corrupt_journal(paths: &RestorePaths) -> Result<(), EngineError> {
    if paths.target.exists() && paths.stage.exists() {
        return Ok(());
    }
    if paths.backup.exists() {
        remove_restore_dir_if_present(&paths.target)?;
        fs::rename(&paths.backup, &paths.target)?;
        sync_dir(&paths.parent)?;
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "corrupt restore journal has no complete rollback source",
    )
    .into())
}

fn checked_restore_path(stage: &Path, manifest_path: &str) -> Result<PathBuf, EngineError> {
    let relative = Path::new(manifest_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(EngineError::RejectedFile(manifest_path.to_string()));
    }
    Ok(stage.join(relative))
}

fn reject_symlink_or_wrong_kind(path: &Path, expect_directory: bool) -> Result<(), EngineError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink()
        || (expect_directory && !metadata.is_dir())
        || (!expect_directory && !metadata.is_file())
    {
        return Err(EngineError::RejectedFile(path.display().to_string()));
    }
    Ok(())
}

fn remove_restore_dir_if_present(path: &Path) -> Result<(), EngineError> {
    reject_symlink_or_wrong_kind(path, true)?;
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_restore_file_if_present(path: &Path) -> Result<(), EngineError> {
    reject_symlink_or_wrong_kind(path, false)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn write_restore_journal(
    paths: &RestorePaths,
    journal: &RestoreJournal,
) -> Result<(), EngineError> {
    remove_restore_file_if_present(&paths.journal_tmp)?;
    let bytes = serde_json::to_vec(journal)?;
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&paths.journal_tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs::rename(&paths.journal_tmp, &paths.journal)?;
    sync_dir(&paths.parent)?;
    Ok(())
}

fn sync_tree(root: &Path) -> Result<(), EngineError> {
    let mut directories = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            return Err(EngineError::RejectedFile(
                entry.path().display().to_string(),
            ));
        }
        if entry.file_type().is_file() {
            fs::File::open(entry.path())?.sync_all()?;
        } else if entry.file_type().is_dir() {
            directories.push(entry.path().to_path_buf());
        } else {
            return Err(EngineError::RejectedFile(
                entry.path().display().to_string(),
            ));
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        sync_dir(&directory)?;
    }
    Ok(())
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<(), EngineError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<(), EngineError> {
    Ok(())
}

fn interrupt_if(
    actual: Option<RestoreFailpoint>,
    expected: RestoreFailpoint,
    label: &'static str,
) -> Result<(), EngineError> {
    if actual == Some(expected) {
        Err(std::io::Error::new(std::io::ErrorKind::Interrupted, label).into())
    } else {
        Ok(())
    }
}

pub fn diff_manifests_for_game(
    left: &SnapshotManifest,
    right: &SnapshotManifest,
    game_profile: &str,
) -> Result<GameSaveDiffReport, EngineError> {
    let left_files = manifest_file_summaries(left)?;
    let right_files = manifest_file_summaries(right)?;
    Ok(build_diff_report(
        game_profile,
        &left_files,
        &right_files,
        BTreeMap::new(),
    ))
}

pub fn diff_folders_for_game(
    left_root: &Path,
    right_root: &Path,
    descriptor: &AdapterDescriptor,
    game_profile: &str,
) -> Result<GameSaveDiffReport, EngineError> {
    let left_files = folder_file_summaries(left_root, &descriptor.exclude_globs)?;
    let right_files = folder_file_summaries(right_root, &descriptor.exclude_globs)?;
    let byte_ranges = diff_folder_byte_ranges(left_root, right_root, &left_files, &right_files)?;
    Ok(build_diff_report(
        game_profile,
        &left_files,
        &right_files,
        byte_ranges,
    ))
}

fn manifest_file_summaries(
    manifest: &SnapshotManifest,
) -> Result<BTreeMap<String, FileSummary>, EngineError> {
    validate_manifest_entries(&manifest.entries, 10_000, 128 * 1024 * 1024)?;
    let mut out = BTreeMap::new();
    for entry in &manifest.entries {
        if entry.kind == FileKind::Tombstone {
            continue;
        }
        out.insert(
            entry.path.clone(),
            FileSummary {
                path: entry.path.clone(),
                size: entry.size,
                sha256: entry.plaintext_sha256.clone(),
            },
        );
    }
    Ok(out)
}

fn folder_file_summaries(
    root: &Path,
    exclude_prefixes: &[String],
) -> Result<BTreeMap<String, FileSummary>, EngineError> {
    let mut out = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.path() == root {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if exclude_prefixes
            .iter()
            .any(|p| rel == *p || rel.starts_with(&format!("{}/", p.trim_end_matches('/'))))
        {
            continue;
        }
        let ty = entry.file_type();
        if ty.is_symlink() {
            return Err(EngineError::RejectedFile(rel));
        }
        if ty.is_file() {
            let bytes = fs::read(entry.path())?;
            out.insert(
                rel.clone(),
                FileSummary {
                    path: rel,
                    size: bytes.len() as u64,
                    sha256: hex::encode(Sha256::digest(bytes)),
                },
            );
        } else if !ty.is_dir() {
            return Err(EngineError::RejectedFile(rel));
        }
    }
    Ok(out)
}

fn diff_folder_byte_ranges(
    left_root: &Path,
    right_root: &Path,
    left_files: &BTreeMap<String, FileSummary>,
    right_files: &BTreeMap<String, FileSummary>,
) -> Result<BTreeMap<String, Vec<ByteRangeDiff>>, EngineError> {
    let mut out = BTreeMap::new();
    for path in left_files.keys().filter(|p| right_files.contains_key(*p)) {
        let left = left_files.get(path).unwrap();
        let right = right_files.get(path).unwrap();
        if left.sha256 == right.sha256 && left.size == right.size {
            continue;
        }
        let left_bytes = fs::read(left_root.join(path))?;
        let right_bytes = fs::read(right_root.join(path))?;
        out.insert(
            path.clone(),
            byte_ranges_for_changed_content(&left_bytes, &right_bytes),
        );
    }
    Ok(out)
}

fn byte_ranges_for_changed_content(left: &[u8], right: &[u8]) -> Vec<ByteRangeDiff> {
    const MAX_RANGES: usize = 8;
    let min_len = left.len().min(right.len());
    let mut ranges = Vec::new();
    let mut i = 0usize;
    while i < min_len && ranges.len() < MAX_RANGES {
        if left[i] == right[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < min_len && left[i] != right[i] {
            i += 1;
        }
        ranges.push(ByteRangeDiff {
            offset: start as u64,
            len: (i - start) as u64,
            left_sha256: Some(hex::encode(Sha256::digest(&left[start..i]))),
            right_sha256: Some(hex::encode(Sha256::digest(&right[start..i]))),
        });
    }
    if left.len() != right.len() && ranges.len() < MAX_RANGES {
        let offset = min_len as u64;
        let left_tail = if left.len() > min_len {
            &left[min_len..]
        } else {
            &[]
        };
        let right_tail = if right.len() > min_len {
            &right[min_len..]
        } else {
            &[]
        };
        ranges.push(ByteRangeDiff {
            offset,
            len: left_tail.len().max(right_tail.len()) as u64,
            left_sha256: (!left_tail.is_empty()).then(|| hex::encode(Sha256::digest(left_tail))),
            right_sha256: (!right_tail.is_empty()).then(|| hex::encode(Sha256::digest(right_tail))),
        });
    }
    ranges
}

fn build_diff_report(
    game_profile: &str,
    left_files: &BTreeMap<String, FileSummary>,
    right_files: &BTreeMap<String, FileSummary>,
    byte_ranges: BTreeMap<String, Vec<ByteRangeDiff>>,
) -> GameSaveDiffReport {
    let mut all_paths = BTreeSet::new();
    all_paths.extend(left_files.keys().cloned());
    all_paths.extend(right_files.keys().cloned());
    let mut entries = Vec::new();
    for path in all_paths {
        match (left_files.get(&path), right_files.get(&path)) {
            (Some(left), Some(right)) if left.sha256 == right.sha256 && left.size == right.size => {
            }
            (Some(left), Some(right)) => entries.push(SaveDiffEntry {
                path: path.clone(),
                change: SaveDiffChange::Modified,
                left_size: Some(left.size),
                right_size: Some(right.size),
                left_sha256: Some(left.sha256.clone()),
                right_sha256: Some(right.sha256.clone()),
                byte_ranges: byte_ranges.get(&path).cloned().unwrap_or_default(),
                notes_zh: game_specific_notes(game_profile, &path),
            }),
            (None, Some(right)) => entries.push(SaveDiffEntry {
                path: path.clone(),
                change: SaveDiffChange::Added,
                left_size: None,
                right_size: Some(right.size),
                left_sha256: None,
                right_sha256: Some(right.sha256.clone()),
                byte_ranges: Vec::new(),
                notes_zh: game_specific_notes(game_profile, &path),
            }),
            (Some(left), None) => entries.push(SaveDiffEntry {
                path: left.path.clone(),
                change: SaveDiffChange::Removed,
                left_size: Some(left.size),
                right_size: None,
                left_sha256: Some(left.sha256.clone()),
                right_sha256: None,
                byte_ranges: Vec::new(),
                notes_zh: game_specific_notes(game_profile, &path),
            }),
            (None, None) => {}
        }
    }
    let added_files = entries
        .iter()
        .filter(|e| e.change == SaveDiffChange::Added)
        .count();
    let removed_files = entries
        .iter()
        .filter(|e| e.change == SaveDiffChange::Removed)
        .count();
    let modified_files = entries
        .iter()
        .filter(|e| e.change == SaveDiffChange::Modified)
        .count();
    let total_left_bytes = left_files.values().map(|f| f.size).sum();
    let total_right_bytes = right_files.values().map(|f| f.size).sum();
    let (parser_id, parser_support, semantic_available) = parser_capability(game_profile);
    let summary_zh = if entries.is_empty() {
        format!(
            "{} 没有发现存档文件差异。",
            parser_display_name(game_profile)
        )
    } else if semantic_available {
        format!(
            "{} 发现 {} 个文件有差异：新增 {}、删除 {}、修改 {}。可展示语义摘要与文件差异，恢复前仍会保留两边版本。",
            parser_display_name(game_profile),
            entries.len(),
            added_files,
            removed_files,
            modified_files
        )
    } else {
        format!(
            "{} 发现 {} 个文件有差异：新增 {}、删除 {}、修改 {}。当前解析器只做文件/字节级差异，不解读猎人名、装备或道具语义；选择覆盖前会保留两边快照。",
            parser_display_name(game_profile),
            entries.len(),
            added_files,
            removed_files,
            modified_files
        )
    };
    GameSaveDiffReport {
        game_profile: game_profile.to_string(),
        parser_id,
        parser_support,
        semantic_available,
        summary_zh,
        changed_files: entries.len(),
        added_files,
        removed_files,
        modified_files,
        total_left_bytes,
        total_right_bytes,
        entries,
    }
}

fn parser_capability(game_profile: &str) -> (String, String, bool) {
    match game_profile {
        "mh3g-3ds" => (
            "mh3g-3ds-binary-v0".into(),
            "game-specific-file-and-byte-diff-only".into(),
            false,
        ),
        _ => (
            "generic-binary-v0".into(),
            "file-and-byte-diff-only".into(),
            false,
        ),
    }
}

fn parser_display_name(game_profile: &str) -> &'static str {
    match game_profile {
        "mh3g-3ds" => "MH3G/3U 3DS 存档解析器",
        _ => "通用存档解析器",
    }
}

fn game_specific_notes(game_profile: &str, path: &str) -> Vec<String> {
    match game_profile {
        "mh3g-3ds" => vec![format!(
            "{} 是 MH3G 3DS 逻辑存档内的二进制文件；当前版本只展示文件大小、hash 和变更字节段，不声称能语义解析猎人/装备/道具。",
            path
        )],
        _ => vec!["通用二进制差异：只能说明文件内容不同，不能解释游戏语义。".into()],
    }
}

pub fn should_upload_from_watcher_event() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MhsaveBundle {
    pub bundle_version: u16,
    pub encrypted: bool,
    pub snapshot: EncryptedSnapshot,
    pub checksum_sha256: String,
}

pub fn export_encrypted_bundle(
    snapshot: &EncryptedSnapshot,
    destination: &Path,
) -> Result<(), EngineError> {
    let snapshot_json = serde_json::to_vec(snapshot)?;
    let checksum = hex::encode(Sha256::digest(&snapshot_json));
    let bundle = MhsaveBundle {
        bundle_version: 1,
        encrypted: true,
        snapshot: snapshot.clone(),
        checksum_sha256: checksum,
    };
    let bytes = serde_json::to_vec_pretty(&bundle)?;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = destination.with_extension("mhsavebundle.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, destination)?;
    Ok(())
}

pub fn import_encrypted_bundle(path: &Path) -> Result<EncryptedSnapshot, EngineError> {
    let bytes = fs::read(path)?;
    let bundle: MhsaveBundle = serde_json::from_slice(&bytes)?;
    if bundle.bundle_version != 1 || !bundle.encrypted {
        return Err(EngineError::MissingChunk(
            "unsupported bundle version".into(),
        ));
    }
    let snapshot_json = serde_json::to_vec(&bundle.snapshot)?;
    let actual = hex::encode(Sha256::digest(&snapshot_json));
    if actual != bundle.checksum_sha256 {
        return Err(EngineError::MissingChunk("bundle checksum mismatch".into()));
    }
    Ok(bundle.snapshot)
}

pub mod local_store {
    use rusqlite::{Connection, OptionalExtension, params};
    use save_domain::SnapshotId;
    use std::path::Path;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UploadQueueItem {
        pub id: i64,
        pub snapshot_id: SnapshotId,
        pub server_endpoint: String,
        pub logical_save_id: String,
        pub base_head: Option<String>,
        pub device_id: String,
        pub bundle_path: String,
        pub attempts: u32,
        pub last_error: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DurableConsistencyBaseline {
        pub server_endpoint: String,
        pub logical_save_id: String,
        pub tree_uri: String,
        pub device_id: String,
        pub established_remote_head: String,
        pub local_fingerprint: String,
        pub established_at_millis: u64,
        pub mode: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CaptureGenerationLease {
        pub generation: u64,
        pub owner: String,
    }

    #[derive(Debug, thiserror::Error)]
    pub enum LocalStoreError {
        #[error("sqlite error: {0}")]
        Sqlite(#[from] rusqlite::Error),
        #[error("capture lease lost")]
        CaptureLeaseLost,
    }

    pub struct LocalStore {
        conn: Connection,
    }

    impl LocalStore {
        pub fn open(path: &Path) -> Result<Self, LocalStoreError> {
            let conn = Connection::open(path)?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
            let store = Self { conn };
            store.migrate()?;
            Ok(store)
        }

        fn migrate(&self) -> Result<(), LocalStoreError> {
            self.conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS devices (
                    id TEXT PRIMARY KEY,
                    label TEXT NOT NULL,
                    public_key BLOB NOT NULL,
                    revoked_at INTEGER
                );
                CREATE TABLE IF NOT EXISTS profiles (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS game_keys (
                    id TEXT PRIMARY KEY,
                    family TEXT NOT NULL,
                    title_id TEXT NOT NULL,
                    region TEXT NOT NULL,
                    update_label TEXT,
                    slot TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS slots (
                    id TEXT PRIMARY KEY,
                    profile_id TEXT NOT NULL REFERENCES profiles(id),
                    game_key_id TEXT NOT NULL REFERENCES game_keys(id),
                    adapter_id TEXT NOT NULL,
                    local_root_hint TEXT,
                    support_level TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS snapshots (
                    id TEXT PRIMARY KEY,
                    slot_id TEXT NOT NULL REFERENCES slots(id),
                    device_id TEXT NOT NULL,
                    encrypted_manifest BLOB NOT NULL,
                    created_at INTEGER NOT NULL,
                    pinned INTEGER NOT NULL DEFAULT 0,
                    tombstone INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS snapshot_parents (
                    snapshot_id TEXT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
                    parent_snapshot_id TEXT NOT NULL,
                    PRIMARY KEY(snapshot_id, parent_snapshot_id)
                );
                CREATE TABLE IF NOT EXISTS conflicts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    slot_id TEXT NOT NULL REFERENCES slots(id),
                    current_head TEXT NOT NULL,
                    conflict_head TEXT NOT NULL,
                    resolved_at INTEGER
                );
                CREATE TABLE IF NOT EXISTS upload_queue (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    snapshot_id TEXT NOT NULL REFERENCES snapshots(id),
                    state TEXT NOT NULL,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    last_error TEXT,
                    server_endpoint TEXT,
                    logical_save_id TEXT,
                    base_head TEXT,
                    device_id TEXT,
                    bundle_path TEXT,
                    tree_uri TEXT,
                    local_fingerprint TEXT,
                    lease_owner TEXT,
                    lease_expires_at INTEGER
                );
                CREATE TABLE IF NOT EXISTS leases (
                    key TEXT PRIMARY KEY,
                    owner TEXT NOT NULL,
                    expires_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS capture_state (
                    key TEXT PRIMARY KEY,
                    dirty_generation INTEGER NOT NULL DEFAULT 0,
                    captured_generation INTEGER NOT NULL DEFAULT 0,
                    lease_owner TEXT,
                    lease_expires_at INTEGER
                );
                CREATE TABLE IF NOT EXISTS sync_consistency (
                    server_endpoint TEXT NOT NULL,
                    logical_save_id TEXT NOT NULL,
                    tree_uri TEXT NOT NULL,
                    device_id TEXT NOT NULL,
                    established_remote_head TEXT NOT NULL,
                    local_fingerprint TEXT NOT NULL,
                    established_at_millis INTEGER NOT NULL,
                    mode TEXT NOT NULL CHECK(mode IN ('upload','restore')),
                    PRIMARY KEY(server_endpoint,logical_save_id,tree_uri,device_id)
                );
                CREATE TABLE IF NOT EXISTS audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_type TEXT NOT NULL,
                    redacted_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                "#,
            )?;
            self.ensure_upload_queue_column("server_endpoint", "TEXT")?;
            self.ensure_upload_queue_column("logical_save_id", "TEXT")?;
            self.ensure_upload_queue_column("base_head", "TEXT")?;
            self.ensure_upload_queue_column("device_id", "TEXT")?;
            self.ensure_upload_queue_column("bundle_path", "TEXT")?;
            self.ensure_upload_queue_column("tree_uri", "TEXT")?;
            self.ensure_upload_queue_column("local_fingerprint", "TEXT")?;
            self.ensure_upload_queue_column("lease_owner", "TEXT")?;
            self.ensure_upload_queue_column("lease_expires_at", "INTEGER")?;
            self.conn.execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS upload_queue_snapshot_unique ON upload_queue(snapshot_id)",
                [],
            )?;
            Ok(())
        }

        fn ensure_upload_queue_column(
            &self,
            name: &str,
            declaration: &str,
        ) -> Result<(), LocalStoreError> {
            let mut statement = self.conn.prepare("PRAGMA table_info(upload_queue)")?;
            let names = statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            if !names.iter().any(|candidate| candidate == name) {
                self.conn.execute(
                    &format!("ALTER TABLE upload_queue ADD COLUMN {name} {declaration}"),
                    [],
                )?;
            }
            Ok(())
        }

        pub fn journal_mode(&self) -> Result<String, LocalStoreError> {
            Ok(self
                .conn
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))?)
        }

        pub fn mark_capture_dirty(&self, key: &str) -> Result<u64, LocalStoreError> {
            self.conn.execute(
                "INSERT OR IGNORE INTO capture_state(key) VALUES (?1)",
                params![key],
            )?;
            self.conn.execute(
                "UPDATE capture_state SET dirty_generation=CASE WHEN dirty_generation=9223372036854775807 THEN dirty_generation ELSE dirty_generation+1 END WHERE key=?1",
                params![key],
            )?;
            Ok(self.conn.query_row(
                "SELECT dirty_generation FROM capture_state WHERE key=?1",
                params![key],
                |row| row.get::<_, i64>(0),
            )? as u64)
        }

        pub fn claim_capture_generation(
            &self,
            key: &str,
            owner: &str,
            now_unix_ms: u64,
            lease_expires_at: u64,
        ) -> Result<Option<CaptureGenerationLease>, LocalStoreError> {
            self.conn.execute(
                "INSERT OR IGNORE INTO capture_state(key) VALUES (?1)",
                params![key],
            )?;
            let mut statement = self.conn.prepare(
                r#"UPDATE capture_state
                   SET lease_owner=?2, lease_expires_at=?4
                   WHERE key=?1
                     AND dirty_generation>captured_generation
                     AND (lease_owner IS NULL OR COALESCE(lease_expires_at,0)<=?3)
                   RETURNING dirty_generation,lease_owner"#,
            )?;
            let mut rows = statement.query(params![
                key,
                owner,
                now_unix_ms as i64,
                lease_expires_at as i64,
            ])?;
            rows.next()?
                .map(|row| {
                    Ok(CaptureGenerationLease {
                        generation: row.get::<_, i64>(0)? as u64,
                        owner: row.get(1)?,
                    })
                })
                .transpose()
        }

        pub fn complete_capture_generation(
            &self,
            key: &str,
            owner: &str,
            generation: u64,
        ) -> Result<bool, LocalStoreError> {
            Ok(self.conn.execute(
                "UPDATE capture_state SET captured_generation=MAX(captured_generation,?3),lease_owner=NULL,lease_expires_at=NULL WHERE key=?1 AND lease_owner=?2 AND captured_generation<?3",
                params![key, owner, generation as i64],
            )? == 1)
        }

        pub fn release_capture_generation(
            &self,
            key: &str,
            owner: &str,
        ) -> Result<bool, LocalStoreError> {
            Ok(self.conn.execute(
                "UPDATE capture_state SET lease_owner=NULL,lease_expires_at=NULL WHERE key=?1 AND lease_owner=?2",
                params![key, owner],
            )? == 1)
        }

        pub fn enqueue_snapshot(
            &self,
            snapshot_id: &SnapshotId,
            slot_id: &str,
            device_id: &str,
            encrypted_manifest: &[u8],
            created_at: u64,
        ) -> Result<(), LocalStoreError> {
            self.conn.execute(
                "INSERT OR IGNORE INTO devices(id,label,public_key) VALUES (?1,'local',X'00')",
                params![device_id],
            )?;
            self.conn.execute("INSERT OR IGNORE INTO profiles(id,name,created_at) VALUES ('default','Default',?1)", params![created_at as i64])?;
            self.conn.execute("INSERT OR IGNORE INTO game_keys(id,family,title_id,region,slot) VALUES ('generic','generic','fixture','none','slot1')", [])?;
            self.conn.execute("INSERT OR IGNORE INTO slots(id,profile_id,game_key_id,adapter_id,support_level) VALUES (?1,'default','generic','generic-folder','FixtureVerified')", params![slot_id])?;
            self.conn.execute("INSERT INTO snapshots(id,slot_id,device_id,encrypted_manifest,created_at) VALUES (?1,?2,?3,?4,?5)", params![snapshot_id.0, slot_id, device_id, encrypted_manifest, created_at as i64])?;
            self.conn.execute(
                "INSERT INTO upload_queue(snapshot_id,state) VALUES (?1,'pending')",
                params![snapshot_id.0],
            )?;
            Ok(())
        }

        #[allow(clippy::too_many_arguments)]
        pub fn enqueue_upload(
            &self,
            snapshot_id: &SnapshotId,
            slot_id: &str,
            device_id: &str,
            encrypted_manifest: &[u8],
            created_at: u64,
            server_endpoint: &str,
            logical_save_id: &str,
            base_head: Option<&str>,
            bundle_path: &str,
            tree_uri: &str,
            local_fingerprint: &str,
            capture_claim: Option<(&str, &str, u64)>,
        ) -> Result<(), LocalStoreError> {
            let transaction = self.conn.unchecked_transaction()?;
            transaction.execute(
                "INSERT OR IGNORE INTO devices(id,label,public_key) VALUES (?1,'local',X'00')",
                params![device_id],
            )?;
            transaction.execute("INSERT OR IGNORE INTO profiles(id,name,created_at) VALUES ('default','Default',?1)", params![created_at as i64])?;
            transaction.execute("INSERT OR IGNORE INTO game_keys(id,family,title_id,region,slot) VALUES ('generic','generic','fixture','none','slot1')", [])?;
            transaction.execute("INSERT OR IGNORE INTO slots(id,profile_id,game_key_id,adapter_id,support_level) VALUES (?1,'default','generic','generic-folder','FixtureVerified')", params![slot_id])?;
            transaction.execute(
                "INSERT OR IGNORE INTO snapshots(id,slot_id,device_id,encrypted_manifest,created_at) VALUES (?1,?2,?3,?4,?5)",
                params![snapshot_id.0, slot_id, device_id, encrypted_manifest, created_at as i64],
            )?;
            transaction.execute(
                r#"INSERT OR IGNORE INTO upload_queue(
                    snapshot_id,state,server_endpoint,logical_save_id,base_head,device_id,bundle_path,
                    tree_uri,local_fingerprint
                ) VALUES (?1,'pending',?2,?3,?4,?5,?6,?7,?8)"#,
                params![
                    snapshot_id.0,
                    server_endpoint,
                    logical_save_id,
                    base_head,
                    device_id,
                    bundle_path,
                    tree_uri,
                    local_fingerprint,
                ],
            )?;
            if let Some((capture_key, capture_owner, generation)) = capture_claim {
                let updated = transaction.execute(
                    "UPDATE capture_state SET captured_generation=MAX(captured_generation,?3),lease_owner=NULL,lease_expires_at=NULL WHERE key=?1 AND lease_owner=?2 AND captured_generation<?3",
                    params![capture_key, capture_owner, generation as i64],
                )?;
                if updated != 1 {
                    return Err(LocalStoreError::CaptureLeaseLost);
                }
            }
            transaction.commit()?;
            Ok(())
        }

        pub fn retryable_uploads(
            &self,
            server_endpoint: &str,
            limit: usize,
        ) -> Result<Vec<UploadQueueItem>, LocalStoreError> {
            let mut statement = self.conn.prepare(
                r#"SELECT id,snapshot_id,server_endpoint,logical_save_id,base_head,
                          device_id,bundle_path,attempts,last_error
                   FROM upload_queue
                   WHERE state IN ('pending','uploading') AND server_endpoint=?1
                   ORDER BY id ASC LIMIT ?2"#,
            )?;
            let rows = statement.query_map(params![server_endpoint, limit as i64], |row| {
                Ok(UploadQueueItem {
                    id: row.get(0)?,
                    snapshot_id: SnapshotId(row.get(1)?),
                    server_endpoint: row.get(2)?,
                    logical_save_id: row.get(3)?,
                    base_head: row.get(4)?,
                    device_id: row.get(5)?,
                    bundle_path: row.get(6)?,
                    attempts: row.get::<_, i64>(7)? as u32,
                    last_error: row.get(8)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        }

        pub fn latest_retryable_snapshot(
            &self,
            server_endpoint: &str,
            logical_save_id: &str,
        ) -> Result<Option<SnapshotId>, LocalStoreError> {
            let mut statement = self.conn.prepare(
                r#"SELECT snapshot_id FROM upload_queue
                   WHERE state IN ('pending','uploading')
                     AND server_endpoint=?1 AND logical_save_id=?2
                   ORDER BY id DESC LIMIT 1"#,
            )?;
            let mut rows = statement.query(params![server_endpoint, logical_save_id])?;
            Ok(rows
                .next()?
                .map(|row| row.get::<_, String>(0).map(SnapshotId))
                .transpose()?)
        }

        /// Atomically changes one pending or expired upload into an owned lease.
        /// The UPDATE predicate and RETURNING row are a single SQLite statement,
        /// so two WorkManager instances cannot both consume the same bundle.
        pub fn claim_retryable_upload(
            &self,
            server_endpoint: Option<&str>,
            owner: &str,
            now_unix_ms: u64,
            lease_expires_at: u64,
        ) -> Result<Option<UploadQueueItem>, LocalStoreError> {
            let mut statement = self.conn.prepare(
                r#"UPDATE upload_queue
                   SET state='uploading', lease_owner=?2, lease_expires_at=?4
                   WHERE id=(
                       SELECT id FROM upload_queue
                       WHERE (?1 IS NULL OR server_endpoint=?1)
                         AND server_endpoint IS NOT NULL
                         AND logical_save_id IS NOT NULL
                         AND device_id IS NOT NULL
                         AND bundle_path IS NOT NULL
                         AND (
                           state='pending' OR
                           (state='uploading' AND COALESCE(lease_expires_at,0)<=?3)
                         )
                       ORDER BY id ASC LIMIT 1
                   )
                   AND (
                     state='pending' OR
                     (state='uploading' AND COALESCE(lease_expires_at,0)<=?3)
                   )
                   RETURNING id,snapshot_id,server_endpoint,logical_save_id,base_head,
                             device_id,bundle_path,attempts,last_error"#,
            )?;
            let mut rows = statement.query(params![
                server_endpoint,
                owner,
                now_unix_ms as i64,
                lease_expires_at as i64,
            ])?;
            rows.next()?
                .map(|row| {
                    Ok(UploadQueueItem {
                        id: row.get(0)?,
                        snapshot_id: SnapshotId(row.get(1)?),
                        server_endpoint: row.get(2)?,
                        logical_save_id: row.get(3)?,
                        base_head: row.get(4)?,
                        device_id: row.get(5)?,
                        bundle_path: row.get(6)?,
                        attempts: row.get::<_, i64>(7)? as u32,
                        last_error: row.get(8)?,
                    })
                })
                .transpose()
        }

        pub fn mark_upload_failed(
            &self,
            id: i64,
            owner: &str,
            error: &str,
        ) -> Result<bool, LocalStoreError> {
            Ok(self.conn.execute(
                "UPDATE upload_queue SET state='pending',attempts=attempts+1,last_error=?3,lease_owner=NULL,lease_expires_at=NULL WHERE id=?1 AND state='uploading' AND lease_owner=?2",
                params![id, owner, error],
            )? == 1)
        }

        pub fn renew_upload_lease(
            &self,
            id: i64,
            owner: &str,
            lease_expires_at: u64,
        ) -> Result<bool, LocalStoreError> {
            Ok(self.conn.execute(
                "UPDATE upload_queue SET lease_expires_at=?3 WHERE id=?1 AND state='uploading' AND lease_owner=?2",
                params![id, owner, lease_expires_at as i64],
            )? == 1)
        }

        pub fn mark_upload_completed(
            &self,
            id: i64,
            owner: &str,
            cloud_head: &str,
            establish_consistency: bool,
            established_at_millis: u64,
        ) -> Result<bool, LocalStoreError> {
            let transaction = self.conn.unchecked_transaction()?;
            let row = transaction.query_row(
                "SELECT snapshot_id,server_endpoint,logical_save_id,tree_uri,device_id,local_fingerprint FROM upload_queue WHERE id=?1 AND state='uploading' AND lease_owner=?2",
                params![id, owner],
                |row| Ok((
                    row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?, row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?, row.get::<_, Option<String>>(5)?,
                )),
            ).optional()?;
            let Some((snapshot_id, server, logical, tree, device, fingerprint)) = row else {
                return Ok(false);
            };
            if establish_consistency {
                let (Some(server), Some(logical), Some(tree), Some(device), Some(fingerprint)) =
                    (server, logical, tree, device, fingerprint)
                else {
                    return Ok(false);
                };
                if snapshot_id != cloud_head {
                    return Ok(false);
                }
                transaction.execute(
                    r#"INSERT INTO sync_consistency(
                        server_endpoint,logical_save_id,tree_uri,device_id,
                        established_remote_head,local_fingerprint,established_at_millis,mode
                    ) VALUES (?1,?2,?3,?4,?5,?6,?7,'upload')
                    ON CONFLICT(server_endpoint,logical_save_id,tree_uri,device_id) DO UPDATE SET
                        established_remote_head=excluded.established_remote_head,
                        local_fingerprint=excluded.local_fingerprint,
                        established_at_millis=excluded.established_at_millis,
                        mode='upload'"#,
                    params![
                        server,
                        logical,
                        tree,
                        device,
                        cloud_head,
                        fingerprint,
                        established_at_millis as i64
                    ],
                )?;
            }
            let updated = transaction.execute(
                "UPDATE upload_queue SET state='completed',last_error=NULL,lease_owner=NULL,lease_expires_at=NULL WHERE id=?1 AND state='uploading' AND lease_owner=?2",
                params![id, owner],
            )?;
            transaction.commit()?;
            Ok(updated == 1)
        }

        pub fn consistency_baseline(
            &self,
            server_endpoint: &str,
            logical_save_id: &str,
            tree_uri: &str,
            device_id: &str,
        ) -> Result<Option<DurableConsistencyBaseline>, LocalStoreError> {
            Ok(self
                .conn
                .query_row(
                    r#"SELECT established_remote_head,local_fingerprint,established_at_millis,mode
                   FROM sync_consistency WHERE server_endpoint=?1 AND logical_save_id=?2
                     AND tree_uri=?3 AND device_id=?4"#,
                    params![server_endpoint, logical_save_id, tree_uri, device_id],
                    |row| {
                        Ok(DurableConsistencyBaseline {
                            server_endpoint: server_endpoint.to_owned(),
                            logical_save_id: logical_save_id.to_owned(),
                            tree_uri: tree_uri.to_owned(),
                            device_id: device_id.to_owned(),
                            established_remote_head: row.get(0)?,
                            local_fingerprint: row.get(1)?,
                            established_at_millis: row.get::<_, i64>(2)? as u64,
                            mode: row.get(3)?,
                        })
                    },
                )
                .optional()?)
        }

        pub fn attach_upload_consistency(
            &self,
            snapshot_id: &str,
            server_endpoint: &str,
            logical_save_id: &str,
            tree_uri: &str,
            device_id: &str,
            local_fingerprint: &str,
        ) -> Result<bool, LocalStoreError> {
            Ok(self.conn.execute(
                r#"UPDATE upload_queue SET tree_uri=?4,local_fingerprint=?6
                   WHERE snapshot_id=?1 AND server_endpoint=?2 AND logical_save_id=?3
                     AND device_id=?5 AND state IN ('pending','uploading')
                     AND (tree_uri IS NULL OR tree_uri=?4)
                     AND (local_fingerprint IS NULL OR local_fingerprint=?6)"#,
                params![
                    snapshot_id,
                    server_endpoint,
                    logical_save_id,
                    tree_uri,
                    device_id,
                    local_fingerprint
                ],
            )? == 1)
        }

        pub fn pending_upload_count(&self) -> Result<u64, LocalStoreError> {
            Ok(self.conn.query_row(
                "SELECT COUNT(*) FROM upload_queue WHERE state IN ('pending','uploading')",
                [],
                |row| row.get::<_, i64>(0),
            )? as u64)
        }

        pub fn pending_upload_count_for_server(
            &self,
            server_endpoint: &str,
        ) -> Result<u64, LocalStoreError> {
            Ok(self.conn.query_row(
                "SELECT COUNT(*) FROM upload_queue WHERE state IN ('pending','uploading') AND server_endpoint=?1",
                params![server_endpoint],
                |row| row.get::<_, i64>(0),
            )? as u64)
        }

        pub fn pending_upload_endpoint_count(&self) -> Result<u64, LocalStoreError> {
            Ok(self.conn.query_row(
                "SELECT COUNT(DISTINCT server_endpoint) FROM upload_queue WHERE state IN ('pending','uploading') AND server_endpoint IS NOT NULL AND logical_save_id IS NOT NULL AND device_id IS NOT NULL AND bundle_path IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0),
            )? as u64)
        }

        pub fn pending_upload_endpoints(&self) -> Result<Vec<String>, LocalStoreError> {
            let mut statement = self.conn.prepare(
                r#"SELECT server_endpoint
                   FROM upload_queue
                   WHERE state IN ('pending','uploading')
                     AND server_endpoint IS NOT NULL
                     AND logical_save_id IS NOT NULL
                     AND device_id IS NOT NULL
                     AND bundle_path IS NOT NULL
                   GROUP BY server_endpoint
                   ORDER BY MIN(id) ASC"#,
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        }

        pub fn durable_pending_upload_count(&self) -> Result<u64, LocalStoreError> {
            Ok(self.conn.query_row(
                "SELECT COUNT(*) FROM upload_queue WHERE state IN ('pending','uploading') AND server_endpoint IS NOT NULL AND logical_save_id IS NOT NULL AND device_id IS NOT NULL AND bundle_path IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0),
            )? as u64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use save_domain::{
        AdapterCapabilities, Platform, RestorePolicy, RootAcquisition, StabilityPolicy,
        SupportLevel,
    };

    fn descriptor() -> AdapterDescriptor {
        AdapterDescriptor {
            emulator_id: "generic-folder".into(),
            platform: Platform::Generic,
            bundle_ids: vec![],
            package_ids: vec![],
            process_names: vec![],
            root_acquisition: RootAcquisition::UserSelectedFolder,
            user_root_hint: None,
            game_key_contract: "fixture".into(),
            include_globs: vec!["**".into()],
            exclude_globs: vec!["cache".into(), "shaders".into(), "load/textures".into()],
            capabilities: AdapterCapabilities {
                save_complete_event: false,
                launch_gate: false,
                exit_reconcile: true,
                dirty_observer: true,
                saf_restore_journal: false,
            },
            stability: StabilityPolicy::default(),
            restore: RestorePolicy {
                require_emulator_stopped: true,
                require_pre_restore_snapshot: true,
                atomic_replace: true,
                saf_journal: false,
            },
            support_level: SupportLevel::FixtureVerified,
            evidence_fingerprint: "unit-test".into(),
        }
    }

    fn snapshot_with_new_tree(secret: &[u8; 32]) -> (tempfile::TempDir, EncryptedSnapshot) {
        let source = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("slot")).unwrap();
        fs::write(source.path().join("slot/main.bin"), b"new-main").unwrap();
        fs::write(source.path().join("slot/extra.bin"), b"new-extra").unwrap();
        let snapshot = create_snapshot_from_stable_folder(
            source.path(),
            &descriptor(),
            secret,
            SnapshotOptions::fixture(GameKey::new("generic", "fixture", "none", "slot1")),
        )
        .unwrap();
        (source, snapshot)
    }

    fn write_old_tree(target: &Path) {
        fs::create_dir_all(target.join("slot")).unwrap();
        fs::write(target.join("slot/main.bin"), b"old-main").unwrap();
        fs::write(target.join("slot/legacy.bin"), b"old-legacy").unwrap();
    }

    fn assert_old_tree(target: &Path) {
        assert_eq!(fs::read(target.join("slot/main.bin")).unwrap(), b"old-main");
        assert_eq!(
            fs::read(target.join("slot/legacy.bin")).unwrap(),
            b"old-legacy"
        );
        assert!(!target.join("slot/extra.bin").exists());
    }

    fn assert_new_tree(target: &Path) {
        assert_eq!(fs::read(target.join("slot/main.bin")).unwrap(), b"new-main");
        assert_eq!(
            fs::read(target.join("slot/extra.bin")).unwrap(),
            b"new-extra"
        );
        assert!(!target.join("slot/legacy.bin").exists());
    }

    fn crash_restore_at(failpoint: RestoreFailpoint) -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [42u8; 32];
        let (_source, snapshot) = snapshot_with_new_tree(&secret);
        let error = restore_snapshot_to_folder_with_failpoint(
            &secret,
            &snapshot,
            &target,
            EmulatorState::Stopped,
            Some(failpoint),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EngineError::Io(ref source) if source.kind() == std::io::ErrorKind::Interrupted
        ));
        (root, target)
    }

    #[test]
    fn restore_recovers_old_tree_after_crash_when_stage_is_complete() {
        let (_root, target) = crash_restore_at(RestoreFailpoint::StageComplete);
        recover_interrupted_restore(&target).unwrap();
        assert_old_tree(&target);
    }

    #[test]
    fn stage_complete_recovery_preserves_old_target_when_stage_is_lost() {
        let (_root, target) = crash_restore_at(RestoreFailpoint::StageComplete);
        let paths = RestorePaths::for_target(&target).unwrap();
        fs::remove_dir_all(&paths.stage).unwrap();

        recover_interrupted_restore(&target).unwrap();

        assert_old_tree(&target);
    }

    #[test]
    fn stage_complete_recovery_ignores_stale_backup_when_old_target_still_exists() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let backup = root.path().join("active-save.mhsave-backup");
        fs::create_dir_all(backup.join("slot")).unwrap();
        fs::write(backup.join("slot/main.bin"), b"ancient-main").unwrap();
        let secret = [46u8; 32];
        let (_source, snapshot) = snapshot_with_new_tree(&secret);
        restore_snapshot_to_folder_with_failpoint(
            &secret,
            &snapshot,
            &target,
            EmulatorState::Stopped,
            Some(RestoreFailpoint::StageComplete),
        )
        .unwrap_err();

        recover_interrupted_restore(&target).unwrap();

        assert_old_tree(&target);
    }

    #[test]
    fn restore_recovers_old_tree_after_crash_when_target_is_backed_up() {
        let (_root, target) = crash_restore_at(RestoreFailpoint::TargetBackedUp);
        recover_interrupted_restore(&target).unwrap();
        assert_old_tree(&target);
    }

    #[test]
    fn restore_recovers_complete_tree_after_crash_when_stage_is_installed() {
        let (_root, target) = crash_restore_at(RestoreFailpoint::StageInstalled);
        recover_interrupted_restore(&target).unwrap();
        assert_new_tree(&target);
    }

    #[test]
    fn restore_recovers_new_tree_when_terminal_receipt_is_lost() {
        let (_root, target) = crash_restore_at(RestoreFailpoint::ReceiptLost);
        recover_interrupted_restore(&target).unwrap();
        assert_new_tree(&target);
    }

    #[test]
    fn recovery_without_journal_does_not_resurrect_stale_backup() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        let backup = root.path().join("active-save.mhsave-backup");
        write_old_tree(&backup);

        recover_interrupted_restore(&target).unwrap();

        assert!(!target.exists());
        assert_old_tree(&backup);
    }

    #[test]
    fn corrupt_journal_with_complete_backup_rolls_back_old_tree() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        let paths = RestorePaths::for_target(&target).unwrap();
        write_old_tree(&paths.backup);
        fs::create_dir_all(paths.stage.join("slot")).unwrap();
        fs::write(paths.stage.join("slot/main.bin"), b"partial-new").unwrap();
        fs::write(&paths.journal, b"{truncated").unwrap();

        recover_interrupted_restore(&target).unwrap();

        assert_old_tree(&target);
        assert!(!paths.stage.exists());
        assert!(!paths.journal.exists());
    }

    #[test]
    fn restore_waits_for_existing_same_parent_transaction_lock() {
        use std::sync::mpsc;
        use std::time::Duration;

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [45u8; 32];
        let (_source, snapshot) = snapshot_with_new_tree(&secret);
        let held_lock = acquire_restore_lock(&target).unwrap();
        let (tx, rx) = mpsc::channel();
        let thread_target = target.clone();
        std::thread::spawn(move || {
            tx.send(restore_snapshot_to_folder(
                &secret,
                &snapshot,
                &thread_target,
                EmulatorState::Stopped,
            ))
            .unwrap();
        });

        assert!(matches!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        drop(held_lock);
        rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_new_tree(&target);
    }

    #[test]
    fn restore_restart_recovery_is_idempotent() {
        for failpoint in [
            RestoreFailpoint::StageComplete,
            RestoreFailpoint::TargetBackedUp,
            RestoreFailpoint::StageInstalled,
            RestoreFailpoint::ReceiptLost,
        ] {
            let (_root, target) = crash_restore_at(failpoint);
            recover_interrupted_restore(&target).unwrap();
            recover_interrupted_restore(&target).unwrap();
            let main = fs::read(target.join("slot/main.bin")).unwrap();
            assert!(main == b"old-main" || main == b"new-main");
            if main == b"old-main" {
                assert_old_tree(&target);
            } else {
                assert_new_tree(&target);
            }
        }
    }

    #[test]
    fn restore_with_initially_absent_target_recovers_absent_or_complete_new_tree() {
        for failpoint in [
            RestoreFailpoint::StageComplete,
            RestoreFailpoint::TargetBackedUp,
            RestoreFailpoint::StageInstalled,
            RestoreFailpoint::ReceiptLost,
        ] {
            let root = tempfile::tempdir().unwrap();
            let target = root.path().join("active-save");
            let secret = [47u8; 32];
            let (_source, snapshot) = snapshot_with_new_tree(&secret);
            restore_snapshot_to_folder_with_failpoint(
                &secret,
                &snapshot,
                &target,
                EmulatorState::Stopped,
                Some(failpoint),
            )
            .unwrap_err();

            recover_interrupted_restore(&target).unwrap();

            match failpoint {
                RestoreFailpoint::StageComplete | RestoreFailpoint::TargetBackedUp => {
                    assert!(!target.exists());
                }
                RestoreFailpoint::StageInstalled | RestoreFailpoint::ReceiptLost => {
                    assert_new_tree(&target);
                }
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn restore_rejects_symlink_target_without_touching_destination() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("real-save");
        write_old_tree(&destination);
        let target = root.path().join("linked-save");
        std::os::unix::fs::symlink(&destination, &target).unwrap();
        let secret = [43u8; 32];
        let (_source, snapshot) = snapshot_with_new_tree(&secret);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::RejectedFile(_)));
        assert_old_tree(&destination);
    }

    #[test]
    fn restore_rejects_manifest_path_traversal_before_moving_target() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [44u8; 32];
        let (_source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        manifest.entries[0].path = "../escaped.bin".into();
        let keys = derive_account_keys(&secret).unwrap();
        snapshot.encrypted_manifest = encrypt_bytes(
            &keys,
            b"mh-save-sync/manifest/v1",
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        assert!(
            restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped,)
                .is_err()
        );
        assert_old_tree(&target);
        assert!(!root.path().join("escaped.bin").exists());
    }

    fn replace_encrypted_manifest(
        secret: &[u8; 32],
        snapshot: &mut EncryptedSnapshot,
        manifest: &SnapshotManifest,
    ) {
        let keys = derive_account_keys(secret).unwrap();
        snapshot.encrypted_manifest = encrypt_bytes(
            &keys,
            b"mh-save-sync/manifest/v1",
            &serde_json::to_vec(manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn restore_rejects_chunk_plaintext_size_above_protocol_limit_before_moving_target() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [54u8; 32];
        let (_source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        manifest.entries[0].chunks[0].plaintext_size = DEFAULT_CHUNK_SIZE as u64 + 1;
        replace_encrypted_manifest(&secret, &mut snapshot, &manifest);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::InvalidChunkMetadata));
        assert_old_tree(&target);
    }

    #[test]
    fn restore_rejects_chunk_size_metadata_that_does_not_match_ciphertext() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [55u8; 32];
        let (_source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        manifest.entries[0].chunks[0].compressed_size += 1;
        replace_encrypted_manifest(&secret, &mut snapshot, &manifest);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::InvalidChunkMetadata));
        assert_old_tree(&target);
    }

    #[test]
    fn restore_rejects_oversized_compressed_chunk_before_decryption() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [57u8; 32];
        let (_source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        manifest.entries[0].chunks[0].compressed_size = max_compressed_chunk_size() + 1;
        manifest.entries[0].chunks[0].ciphertext_size =
            manifest.entries[0].chunks[0].compressed_size + AEAD_TAG_SIZE;
        replace_encrypted_manifest(&secret, &mut snapshot, &manifest);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::InvalidChunkMetadata));
        assert_old_tree(&target);
    }

    #[test]
    fn restore_rejects_manifest_chunk_total_before_creating_staging_files() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [58u8; 32];
        let (_source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        manifest.entries[0].size = 1;
        replace_encrypted_manifest(&secret, &mut snapshot, &manifest);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::InvalidChunkMetadata));
        assert_old_tree(&target);
        assert!(
            !root
                .path()
                .join(".active-save.mhsave-restore-stage")
                .exists()
        );
    }

    #[test]
    fn restore_rejects_non_final_short_chunk_even_when_declared_total_matches() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [59u8; 32];
        let (source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        let entry = &mut manifest.entries[0];
        let original = fs::read(source.path().join(&entry.path)).unwrap();
        entry.chunks.push(entry.chunks[0].clone());
        entry.size = (original.len() * 2) as u64;
        let mut repeated = original.clone();
        repeated.extend_from_slice(&original);
        entry.plaintext_sha256 = hex::encode(Sha256::digest(&repeated));
        replace_encrypted_manifest(&secret, &mut snapshot, &manifest);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::InvalidChunkMetadata));
        assert_old_tree(&target);
    }

    #[test]
    fn restore_integrity_error_never_contains_decrypted_manifest_path() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [60u8; 32];
        let (_source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        let sensitive_path = manifest.entries[0].path.clone();
        manifest.entries[0].plaintext_sha256 = "00".repeat(32);
        replace_encrypted_manifest(&secret, &mut snapshot, &manifest);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::FileIntegrityMismatch));
        assert!(!error.to_string().contains(&sensitive_path));
        assert_old_tree(&target);
    }

    #[test]
    fn restore_rejects_chunk_id_not_bound_to_plaintext() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("active-save");
        write_old_tree(&target);
        let secret = [56u8; 32];
        let (_source, mut snapshot) = snapshot_with_new_tree(&secret);
        let mut manifest = decrypt_manifest(&secret, &snapshot).unwrap();
        let original_id = manifest.entries[0].chunks[0].id.clone();
        let fake_id = "a".repeat(64);
        let keys = derive_account_keys(&secret).unwrap();
        let original_blob = snapshot.chunks.remove(&original_id).unwrap();
        let compressed = decrypt_bytes(
            &keys,
            format!("mh-save-sync/chunk/v1/{original_id}").as_bytes(),
            &original_blob,
        )
        .unwrap();
        let replacement = encrypt_bytes(
            &keys,
            format!("mh-save-sync/chunk/v1/{fake_id}").as_bytes(),
            &compressed,
        )
        .unwrap();
        manifest.entries[0].chunks[0].id = fake_id.clone();
        manifest.entries[0].chunks[0].ciphertext_size = replacement.ciphertext.len() as u64;
        snapshot.chunks.insert(fake_id, replacement);
        replace_encrypted_manifest(&secret, &mut snapshot, &manifest);

        let error = restore_snapshot_to_folder(&secret, &snapshot, &target, EmulatorState::Stopped)
            .unwrap_err();

        assert!(matches!(error, EngineError::ChunkIdMismatch));
        assert_old_tree(&target);
    }

    #[test]
    fn watcher_never_uploads_directly() {
        assert!(!should_upload_from_watcher_event());
    }

    #[test]
    fn snapshot_restore_round_trip_and_excludes_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("slot1")).unwrap();
        fs::write(dir.path().join("slot1/main.bin"), b"hunter").unwrap();
        fs::create_dir_all(dir.path().join("cache")).unwrap();
        fs::write(dir.path().join("cache/not-save.bin"), b"ignore").unwrap();
        let secret = [9u8; 32];
        let snap = create_snapshot_from_stable_folder(
            dir.path(),
            &descriptor(),
            &secret,
            SnapshotOptions::fixture(GameKey::new("generic", "fixture", "none", "slot1")),
        )
        .unwrap();
        let manifest = decrypt_manifest(&secret, &snap).unwrap();
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].path, "slot1/main.bin");
        fs::write(dir.path().join("slot1/main.bin"), b"damaged").unwrap();
        let backup =
            restore_snapshot_to_folder(&secret, &snap, dir.path(), EmulatorState::Stopped).unwrap();
        assert!(backup.exists());
        assert_eq!(
            fs::read(dir.path().join("slot1/main.bin")).unwrap(),
            b"hunter"
        );
    }

    #[test]
    fn running_emulator_blocks_restore_and_conflict_preserves_branch() {
        let new = SnapshotId("new".into());
        let current = SnapshotId("current".into());
        let base = SnapshotId("base".into());
        assert_eq!(
            decide_head_update(Some(&base), Some(&current), &new),
            HeadUpdate::Conflict {
                current_head: current,
                conflict_head: new
            }
        );
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), b"a").unwrap();
        let secret = [8u8; 32];
        let snap = create_snapshot_from_stable_folder(
            dir.path(),
            &descriptor(),
            &secret,
            SnapshotOptions::fixture(GameKey::new("generic", "fixture", "none", "slot1")),
        )
        .unwrap();
        assert!(matches!(
            restore_snapshot_to_folder(&secret, &snap, dir.path(), EmulatorState::Running),
            Err(EngineError::EmulatorRunning)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("real.bin"), b"a").unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.bin"), dir.path().join("link.bin"))
            .unwrap();
        let secret = [1u8; 32];
        let err = create_snapshot_from_stable_folder(
            dir.path(),
            &descriptor(),
            &secret,
            SnapshotOptions::fixture(GameKey::new("generic", "fixture", "none", "slot1")),
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::RejectedFile(_)));
    }

    #[test]
    fn thousand_dirty_candidates_do_not_create_half_written_snapshot() {
        let desc = descriptor();
        let secret = [5u8; 32];
        for i in 0..1000u32 {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join("save.bin"), format!("half-{i}")).unwrap();
            let before = fingerprint_tree(dir.path(), &desc.exclude_globs).unwrap();
            fs::write(dir.path().join("save.bin"), format!("complete-{i}")).unwrap();
            let after = fingerprint_tree(dir.path(), &desc.exclude_globs).unwrap();
            assert_ne!(before, after);
            let snap = create_snapshot_from_stable_folder(
                dir.path(),
                &desc,
                &secret,
                SnapshotOptions::fixture(GameKey::new("generic", "fixture", "none", "slot1")),
            )
            .unwrap();
            let manifest = decrypt_manifest(&secret, &snap).unwrap();
            assert_eq!(
                manifest.entries[0].size,
                format!("complete-{i}").len() as u64
            );
        }
    }
    #[test]
    fn sqlite_wal_store_tracks_pending_upload_queue() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::local_store::LocalStore::open(&tmp.path().join("state.sqlite")).unwrap();
        assert_eq!(store.journal_mode().unwrap().to_lowercase(), "wal");
        store
            .enqueue_snapshot(
                &SnapshotId("snap-local".into()),
                "slot-a",
                "device-a",
                b"encrypted-manifest",
                1,
            )
            .unwrap();
        assert_eq!(store.pending_upload_count().unwrap(), 1);
        assert_eq!(store.durable_pending_upload_count().unwrap(), 0);
        assert!(
            store
                .claim_retryable_upload(None, "worker", 1, 2)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn sqlite_upload_queue_preserves_retry_metadata_until_completed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::local_store::LocalStore::open(&tmp.path().join("state.sqlite")).unwrap();
        let snapshot = SnapshotId("snap-durable".into());
        store
            .enqueue_upload(
                &snapshot,
                "slot-a",
                "device-a",
                b"encrypted-manifest",
                7,
                "https://sync.example.test",
                "mh3g-nemessix-jp-slot1",
                Some("base-a"),
                "objects/snap-durable.mhsavebundle",
                "content://fixture/tree",
                "fingerprint-a",
                None,
            )
            .unwrap();

        let queued = store
            .retryable_uploads("https://sync.example.test", 10)
            .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].snapshot_id, snapshot);
        assert_eq!(queued[0].attempts, 0);
        assert_eq!(queued[0].base_head.as_deref(), Some("base-a"));

        let claimed = store
            .claim_retryable_upload(Some("https://sync.example.test"), "test-worker", 10, 20)
            .unwrap()
            .unwrap();
        assert!(
            store
                .mark_upload_failed(claimed.id, "test-worker", "network unavailable")
                .unwrap()
        );
        let retry = store
            .retryable_uploads("https://sync.example.test", 10)
            .unwrap();
        assert_eq!(retry[0].attempts, 1);
        assert_eq!(retry[0].last_error.as_deref(), Some("network unavailable"));

        let claimed = store
            .claim_retryable_upload(Some("https://sync.example.test"), "test-worker-2", 20, 30)
            .unwrap()
            .unwrap();
        assert!(
            store
                .mark_upload_completed(claimed.id, "test-worker-2", "snap-durable", true, 30)
                .unwrap()
        );
        assert!(
            store
                .retryable_uploads("https://sync.example.test", 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(store.pending_upload_count().unwrap(), 0);
        let baseline = store
            .consistency_baseline(
                "https://sync.example.test",
                "mh3g-nemessix-jp-slot1",
                "content://fixture/tree",
                "device-a",
            )
            .unwrap()
            .expect("upload completion and consistency receipt must commit atomically");
        assert_eq!(baseline.established_remote_head, "snap-durable");
        assert_eq!(baseline.local_fingerprint, "fingerprint-a");
        drop(store);
        let reopened =
            crate::local_store::LocalStore::open(&tmp.path().join("state.sqlite")).unwrap();
        assert_eq!(
            reopened
                .consistency_baseline(
                    "https://sync.example.test",
                    "mh3g-nemessix-jp-slot1",
                    "content://fixture/tree",
                    "device-a",
                )
                .unwrap()
                .unwrap()
                .established_remote_head,
            "snap-durable",
            "process restart after HTTP success must retain the established HEAD",
        );
    }

    #[test]
    fn sqlite_migrates_legacy_queue_receipt_into_durable_consistency() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.sqlite");
        let legacy = rusqlite::Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                r#"CREATE TABLE upload_queue (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    snapshot_id TEXT NOT NULL,
                    state TEXT NOT NULL,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    last_error TEXT,
                    server_endpoint TEXT,
                    logical_save_id TEXT,
                    base_head TEXT,
                    device_id TEXT,
                    bundle_path TEXT,
                    lease_owner TEXT,
                    lease_expires_at INTEGER
                );
                INSERT INTO upload_queue(
                    snapshot_id,state,server_endpoint,logical_save_id,device_id,bundle_path
                ) VALUES (
                    'legacy-snapshot','pending','https://sync.example.test',
                    'mh3g-nemessix-jp-slot1','device-a','objects/legacy.mhsavebundle'
                );"#,
            )
            .unwrap();
        drop(legacy);

        let store = crate::local_store::LocalStore::open(&path).unwrap();
        assert!(
            store
                .attach_upload_consistency(
                    "legacy-snapshot",
                    "https://sync.example.test",
                    "mh3g-nemessix-jp-slot1",
                    "content://legacy/tree",
                    "device-a",
                    "legacy-fingerprint",
                )
                .unwrap()
        );
        let claimed = store
            .claim_retryable_upload(None, "migration-worker", 1, 2)
            .unwrap()
            .unwrap();
        assert!(
            store
                .mark_upload_completed(claimed.id, "migration-worker", "legacy-snapshot", true, 99,)
                .unwrap()
        );
        drop(store);
        let reopened = crate::local_store::LocalStore::open(&path).unwrap();
        let baseline = reopened
            .consistency_baseline(
                "https://sync.example.test",
                "mh3g-nemessix-jp-slot1",
                "content://legacy/tree",
                "device-a",
            )
            .unwrap()
            .unwrap();
        assert_eq!(baseline.established_remote_head, "legacy-snapshot");
        assert_eq!(baseline.local_fingerprint, "legacy-fingerprint");
    }

    #[test]
    fn sqlite_upload_queue_claim_is_atomic_and_owner_scoped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.sqlite");
        let first = crate::local_store::LocalStore::open(&path).unwrap();
        first
            .enqueue_upload(
                &SnapshotId("snap-claimed".into()),
                "slot-a",
                "device-a",
                b"encrypted-manifest",
                7,
                "https://sync.example.test",
                "mh3g-nemessix-jp-slot1",
                None,
                "objects/snap-claimed.mhsavebundle",
                "content://fixture/tree",
                "fingerprint-claimed",
                None,
            )
            .unwrap();
        let second = crate::local_store::LocalStore::open(&path).unwrap();

        let claimed = first
            .claim_retryable_upload(Some("https://sync.example.test"), "worker-a", 100, 200)
            .unwrap()
            .unwrap();
        assert_eq!(claimed.snapshot_id, SnapshotId("snap-claimed".into()));
        assert!(
            second
                .claim_retryable_upload(Some("https://sync.example.test"), "worker-b", 100, 200,)
                .unwrap()
                .is_none()
        );
        assert!(
            !second
                .mark_upload_completed(claimed.id, "worker-b", "snap-claimed", true, 300)
                .unwrap()
        );
        assert!(
            !second
                .renew_upload_lease(claimed.id, "worker-b", 500)
                .unwrap()
        );
        assert!(
            first
                .renew_upload_lease(claimed.id, "worker-a", 500)
                .unwrap()
        );
        assert!(
            first
                .mark_upload_completed(claimed.id, "worker-a", "snap-claimed", true, 300)
                .unwrap()
        );
    }

    #[test]
    fn sqlite_upload_queue_recovers_expired_claim_after_worker_crash() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.sqlite");
        let crashed = crate::local_store::LocalStore::open(&path).unwrap();
        crashed
            .enqueue_upload(
                &SnapshotId("snap-crashed".into()),
                "slot-a",
                "device-a",
                b"encrypted-manifest",
                7,
                "https://old.example.test",
                "mh3g-nemessix-jp-slot1",
                None,
                "objects/snap-crashed.mhsavebundle",
                "content://fixture/tree",
                "fingerprint-crashed",
                None,
            )
            .unwrap();
        let first_claim = crashed
            .claim_retryable_upload(None, "crashed-worker", 100, 150)
            .unwrap()
            .unwrap();
        drop(crashed);

        let restarted = crate::local_store::LocalStore::open(&path).unwrap();
        assert!(
            restarted
                .claim_retryable_upload(None, "new-worker", 149, 300)
                .unwrap()
                .is_none()
        );
        let recovered = restarted
            .claim_retryable_upload(None, "new-worker", 150, 300)
            .unwrap()
            .unwrap();
        assert_eq!(recovered.id, first_claim.id);
        assert!(
            restarted
                .mark_upload_failed(recovered.id, "new-worker", "network unavailable")
                .unwrap()
        );
    }

    #[test]
    fn capture_generation_is_claimed_once_and_new_dirty_survives_ack() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.sqlite");
        let first = crate::local_store::LocalStore::open(&path).unwrap();
        let second = crate::local_store::LocalStore::open(&path).unwrap();
        assert_eq!(first.mark_capture_dirty("mh3g").unwrap(), 1);
        let claim = first
            .claim_capture_generation("mh3g", "worker-a", 100, 200)
            .unwrap()
            .unwrap();
        assert_eq!(claim.generation, 1);
        assert!(
            second
                .claim_capture_generation("mh3g", "worker-b", 100, 200)
                .unwrap()
                .is_none()
        );
        assert_eq!(second.mark_capture_dirty("mh3g").unwrap(), 2);
        assert!(
            first
                .complete_capture_generation("mh3g", "worker-a", 1)
                .unwrap()
        );
        let next = second
            .claim_capture_generation("mh3g", "worker-b", 201, 300)
            .unwrap()
            .unwrap();
        assert_eq!(next.generation, 2);
    }

    #[test]
    fn capture_generation_lease_is_reclaimed_after_crash() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.sqlite");
        let crashed = crate::local_store::LocalStore::open(&path).unwrap();
        crashed.mark_capture_dirty("mh3g").unwrap();
        crashed
            .claim_capture_generation("mh3g", "dead", 10, 20)
            .unwrap()
            .unwrap();
        drop(crashed);
        let restarted = crate::local_store::LocalStore::open(&path).unwrap();
        assert!(
            restarted
                .claim_capture_generation("mh3g", "new", 19, 30)
                .unwrap()
                .is_none()
        );
        assert!(
            restarted
                .claim_capture_generation("mh3g", "new", 20, 30)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn sqlite_upload_queue_is_idempotent_for_same_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::local_store::LocalStore::open(&tmp.path().join("state.sqlite")).unwrap();
        for _ in 0..2 {
            store
                .enqueue_upload(
                    &SnapshotId("snap-one".into()),
                    "slot-a",
                    "device-a",
                    b"encrypted-manifest",
                    7,
                    "https://sync.example.test",
                    "mh3g-nemessix-jp-slot1",
                    None,
                    "objects/snap-one.mhsavebundle",
                    "content://fixture/tree",
                    "fingerprint-one",
                    None,
                )
                .unwrap();
        }
        assert_eq!(store.pending_upload_count().unwrap(), 1);
    }
    #[test]
    fn encrypted_bundle_round_trip_without_server() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("save.bin"), b"portable").unwrap();
        let secret = [6u8; 32];
        let snap = create_snapshot_from_stable_folder(
            dir.path(),
            &descriptor(),
            &secret,
            SnapshotOptions::fixture(GameKey::new("generic", "fixture", "none", "slot1")),
        )
        .unwrap();
        let bundle = dir.path().join("backup.mhsavebundle");
        export_encrypted_bundle(&snap, &bundle).unwrap();
        fs::remove_file(dir.path().join("save.bin")).unwrap();
        let imported = import_encrypted_bundle(&bundle).unwrap();
        restore_snapshot_to_folder(&secret, &imported, dir.path(), EmulatorState::Stopped).unwrap();
        assert_eq!(fs::read(dir.path().join("save.bin")).unwrap(), b"portable");
    }

    #[test]
    fn mh3g_diff_parser_reports_file_and_byte_differences_without_semantic_claims() {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        fs::create_dir_all(left.path().join("slot1")).unwrap();
        fs::create_dir_all(right.path().join("slot1")).unwrap();
        fs::write(left.path().join("slot1/main.bin"), b"hunter-rank-001").unwrap();
        fs::write(right.path().join("slot1/main.bin"), b"hunter-rank-002").unwrap();
        fs::write(right.path().join("slot1/extra.bin"), b"new").unwrap();
        let report =
            diff_folders_for_game(left.path(), right.path(), &descriptor(), "mh3g-3ds").unwrap();
        assert_eq!(report.game_profile, "mh3g-3ds");
        assert_eq!(report.parser_id, "mh3g-3ds-binary-v0");
        assert!(!report.semantic_available);
        assert_eq!(report.changed_files, 2);
        assert_eq!(report.modified_files, 1);
        assert_eq!(report.added_files, 1);
        assert!(report.summary_zh.contains("文件/字节级差异"));
        assert!(report.summary_zh.contains("不解读猎人名"));
        let changed = report
            .entries
            .iter()
            .find(|e| e.path == "slot1/main.bin")
            .unwrap();
        assert_eq!(changed.change, SaveDiffChange::Modified);
        assert!(!changed.byte_ranges.is_empty());
        assert!(
            changed
                .notes_zh
                .iter()
                .any(|n| n.contains("不声称能语义解析"))
        );
    }

    #[test]
    fn manifest_diff_reports_conflict_ready_file_summary() {
        let mut left = SnapshotManifest {
            format_version: save_domain::SNAPSHOT_FORMAT_VERSION,
            game_key: GameKey::new("monster-hunter", "0004000000048100", "jp-3g", "slot1"),
            logical_save_id: save_domain::LogicalSaveId("mh3g".into()),
            device_id: save_domain::DeviceId("mac".into()),
            parents: vec![],
            entries: vec![ManifestEntry {
                path: "system".into(),
                kind: FileKind::Regular,
                size: 4,
                plaintext_sha256: hex::encode(Sha256::digest(b"aaaa")),
                chunks: vec![],
            }],
            created_unix_ms: 1,
        };
        let mut right = left.clone();
        right.device_id = save_domain::DeviceId("android".into());
        right.entries[0].plaintext_sha256 = hex::encode(Sha256::digest(b"bbbb"));
        let report = diff_manifests_for_game(&left, &right, "mh3g-3ds").unwrap();
        assert_eq!(report.changed_files, 1);
        assert_eq!(report.entries[0].path, "system");
        assert!(report.entries[0].byte_ranges.is_empty());
        assert!(report.summary_zh.contains("选择覆盖前会保留两边快照"));
        left.entries[0].plaintext_sha256 = right.entries[0].plaintext_sha256.clone();
        let no_diff = diff_manifests_for_game(&left, &right, "mh3g-3ds").unwrap();
        assert_eq!(no_diff.changed_files, 0);
    }
}
