use save_crypto::{EncryptedBlob, chunk_id, decrypt_bytes, derive_account_keys, encrypt_bytes};
use save_domain::{
    AdapterDescriptor, ChunkRef, DEFAULT_CHUNK_SIZE, DeviceId, FileKind, GameKey, LogicalSaveId,
    ManifestEntry, SNAPSHOT_FORMAT_VERSION, SnapshotId, SnapshotManifest, TreeFingerprint,
    validate_manifest_entries,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSnapshot {
    pub snapshot_id: SnapshotId,
    pub encrypted_manifest: EncryptedBlob,
    pub chunks: BTreeMap<String, EncryptedBlob>,
    pub fingerprint: TreeFingerprint,
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
    if emulator_state != EmulatorState::Stopped {
        return Err(EngineError::EmulatorRunning);
    }
    let keys = derive_account_keys(secret)?;
    let manifest = decrypt_manifest(secret, snapshot)?;
    validate_manifest_entries(&manifest.entries, 10_000, 128 * 1024 * 1024)?;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let backup = parent.join(format!(
        "{}.mhsave-backup",
        target.file_name().unwrap_or_default().to_string_lossy()
    ));
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    if target.exists() {
        fs::rename(target, &backup)?;
    }
    let stage = tempfile::tempdir_in(parent)?;
    let result = (|| -> Result<(), EngineError> {
        for entry in &manifest.entries {
            if entry.kind == FileKind::Tombstone {
                continue;
            }
            let out = stage.path().join(&entry.path);
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
                let compressed = decrypt_bytes(
                    &keys,
                    format!("mh-save-sync/chunk/v1/{}", cref.id).as_bytes(),
                    blob,
                )?;
                let plaintext = zstd::bulk::decompress(&compressed, cref.plaintext_size as usize)?;
                total += plaintext.len() as u64;
                hasher.update(&plaintext);
                writer.write_all(&plaintext)?;
            }
            writer.sync_all()?;
            if total != entry.size || hex::encode(hasher.finalize()) != entry.plaintext_sha256 {
                return Err(EngineError::MissingChunk(entry.path.clone()));
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        if target.exists() {
            let _ = fs::remove_dir_all(target);
        }
        if backup.exists() {
            let _ = fs::rename(&backup, target);
        }
        return Err(e);
    }
    fs::rename(stage.path(), target)?;
    Ok(backup)
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
    use rusqlite::{Connection, params};
    use save_domain::SnapshotId;
    use std::path::Path;

    #[derive(Debug, thiserror::Error)]
    pub enum LocalStoreError {
        #[error("sqlite error: {0}")]
        Sqlite(#[from] rusqlite::Error),
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
                    last_error TEXT
                );
                CREATE TABLE IF NOT EXISTS leases (
                    key TEXT PRIMARY KEY,
                    owner TEXT NOT NULL,
                    expires_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_type TEXT NOT NULL,
                    redacted_json TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                "#,
            )?;
            Ok(())
        }

        pub fn journal_mode(&self) -> Result<String, LocalStoreError> {
            Ok(self
                .conn
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))?)
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

        pub fn pending_upload_count(&self) -> Result<u64, LocalStoreError> {
            Ok(self.conn.query_row(
                "SELECT COUNT(*) FROM upload_queue WHERE state='pending'",
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
}
