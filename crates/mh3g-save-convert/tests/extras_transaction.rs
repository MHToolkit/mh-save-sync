use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[cfg(not(windows))]
use std::fs::OpenOptions;

#[cfg(not(windows))]
use fs2::FileExt;
use mh3g_save_convert::{
    ConversionError,
    extras_transaction::{
        ExtraFileOperations, ExtraGroup, ExtraInstallManifest, StdExtraFileOperations,
        dry_run_extra_groups_with, install_extra_groups_with, rollback_extra_groups_with,
    },
    process_probe::ProcessProbe,
    profile::build_jp_cemu_header,
    transaction::sha256_hex,
};
use tempfile::tempdir;

struct Stopped;

impl ProcessProbe for Stopped {
    fn matching_process(&self) -> Result<Option<String>, ConversionError> {
        Ok(None)
    }
}

struct Running;

impl ProcessProbe for Running {
    fn matching_process(&self) -> Result<Option<String>, ConversionError> {
        Ok(Some("Cemu_release".to_owned()))
    }
}

struct ProbeFailure;

impl ProcessProbe for ProbeFailure {
    fn matching_process(&self) -> Result<Option<String>, ConversionError> {
        Err(ConversionError::UnsafeInstall(
            "injected process enumeration failure".to_owned(),
        ))
    }
}

fn payload_size(component: &str) -> usize {
    match component {
        "card1" | "card2" | "card3" => 0x57_FFC,
        "cardbox" => 0x2F_FFC,
        "quest1" | "quest2" | "quest3" | "quest4" => 0x28_FFC,
        _ => panic!("unknown synthetic component: {component}"),
    }
}

fn cemu_component(component: &str, marker: u8) -> Vec<u8> {
    let payload_size = payload_size(component);
    let mut bytes = build_jp_cemu_header(component, payload_size)
        .unwrap()
        .to_vec();
    bytes.resize(bytes.len() + payload_size, marker);
    bytes
}

fn write_group(directory: &Path, group: ExtraGroup, marker: u8) {
    fs::create_dir_all(directory).unwrap();
    for (offset, component) in group.components().iter().enumerate() {
        fs::write(
            directory.join(component),
            cemu_component(component, marker.wrapping_add(offset as u8)),
        )
        .unwrap();
    }
}

fn target_bytes(directory: &Path, group: ExtraGroup) -> Vec<(String, Option<Vec<u8>>)> {
    group
        .components()
        .iter()
        .map(|component| {
            let path = directory.join(component);
            let bytes = match fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => panic!("read target fixture: {error}"),
            };
            ((*component).to_owned(), bytes)
        })
        .collect()
}

fn assert_target_bytes(directory: &Path, expected: &[(String, Option<Vec<u8>>)]) {
    for (component, expected) in expected {
        let actual = match fs::read(directory.join(component)) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("read target fixture: {error}"),
        };
        assert_eq!(&actual, expected, "component {component}");
    }
}

fn assert_no_transaction_artifacts(directory: &Path) {
    let leftovers = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        // The advisory lock inode intentionally survives process crashes. It
        // is not transaction material: fs2 releases its OS lock with the
        // process, so a later install can reuse this empty path safely.
        .filter(|name| name.contains("mh3g-extra-") && name != ".mh3g-extra-install.lock")
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "unexpected transaction files: {leftovers:?}"
    );
}

fn transaction_directories(directory: &Path) -> Vec<PathBuf> {
    let mut paths = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".mh3g-extra-transaction-"))
                && path.is_dir()
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn only_transaction_directory(directory: &Path) -> PathBuf {
    let paths = transaction_directories(directory);
    assert_eq!(
        paths.len(),
        1,
        "expected one transaction directory: {paths:?}"
    );
    paths.into_iter().next().unwrap()
}

fn transaction_journal(directory: &Path) -> PathBuf {
    let journal = only_transaction_directory(directory).join(".mh3g-extra-recovery.json");
    assert!(
        journal.is_file(),
        "expected retained recovery journal: {journal:?}"
    );
    journal.canonicalize().unwrap()
}

struct FailingOperations {
    replacements: AtomicUsize,
    fail_second_replace: bool,
    fail_recovery_journal_create: bool,
}

struct RecoveryJournalCollision {
    collided: AtomicBool,
    external_bytes: Vec<u8>,
}

struct FailsAfterMaterializingFirstAppendOnlyArtifact {
    writes: AtomicUsize,
}

struct FailsBeforeFirstExchangeAndReplacesCleanupPath {
    sync_calls: AtomicUsize,
}

impl ExtraFileOperations for FailsBeforeFirstExchangeAndReplacesCleanupPath {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        if self.sync_calls.fetch_add(1, Ordering::SeqCst) == 1 {
            return Err(ConversionError::UnsafeInstall(
                "injected pre-exchange durability failure".to_owned(),
            ));
        }
        StdExtraFileOperations.sync_directory(path)
    }
}

impl ExtraFileOperations for RecoveryJournalCollision {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == ".mh3g-extra-recovery.json")
            && !self.collided.swap(true, Ordering::SeqCst)
        {
            fs::write(path, &self.external_bytes).unwrap();
        }
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

impl ExtraFileOperations for FailsAfterMaterializingFirstAppendOnlyArtifact {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)?;
        if self.writes.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err(ConversionError::UnsafeInstall(
                "injected failure after append-only artifact materialization".to_owned(),
            ));
        }
        Ok(())
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct RequiresPreparedMaterialSync {
    created_files: AtomicUsize,
    materials_synced: AtomicBool,
}

impl ExtraFileOperations for RequiresPreparedMaterialSync {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        let result = StdExtraFileOperations.write_new_file(path, bytes);
        if result.is_ok() {
            self.created_files.fetch_add(1, Ordering::SeqCst);
        }
        result
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        if !self.materials_synced.load(Ordering::SeqCst) {
            return Err(ConversionError::UnsafeInstall(
                "first target replacement started before all recovery material was synced"
                    .to_owned(),
            ));
        }
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        // POSIX writes four durable backups before the first exchange.  On
        // Windows, `ReplaceFileW` must create each backup as part of that
        // exchange, so the pre-exchange material is the journal plus four
        // staged files; the existing target is the remaining recovery value
        // until ReplaceFileW moves it atomically into its controlled backup.
        let required_pre_exchange_files = if cfg!(windows) { 5 } else { 9 };
        if self.created_files.load(Ordering::SeqCst) >= required_pre_exchange_files {
            self.materials_synced.store(true, Ordering::SeqCst);
        }
        StdExtraFileOperations.sync_directory(path)
    }
}

impl FailingOperations {
    fn second_replace() -> Self {
        Self {
            replacements: AtomicUsize::new(0),
            fail_second_replace: true,
            fail_recovery_journal_create: false,
        }
    }

    fn recovery_journal_create() -> Self {
        Self {
            replacements: AtomicUsize::new(0),
            fail_second_replace: false,
            fail_recovery_journal_create: true,
        }
    }
}

impl ExtraFileOperations for FailingOperations {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        if self.fail_recovery_journal_create
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == ".mh3g-extra-recovery.json")
        {
            return Err(ConversionError::UnsafeInstall(
                "injected recovery journal creation failure".to_owned(),
            ));
        }
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        let replacement = self.replacements.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_second_replace && replacement == 2 {
            return Err(ConversionError::UnsafeInstall(
                "injected second replacement failure".to_owned(),
            ));
        }
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct PanicAfterFirstReplacement {
    replacements: AtomicUsize,
}

/// Simulates a non-cooperating writer that replaces a target after the
/// transaction has finished its preflight but immediately before the first
/// destructive filesystem operation.  The installation must fail closed and
/// leave this writer's value intact.
struct ExternalWriteAtFirstReplacement {
    replacements: AtomicUsize,
    target: PathBuf,
    replacement: Vec<u8>,
}

/// Mirrors the final replacement-window race during rollback.  A later
/// writer must survive even when rollback has already validated the manifest.
struct ExternalWriteAtFirstRestore {
    restores: AtomicUsize,
    target: PathBuf,
    replacement: Vec<u8>,
}

impl PanicAfterFirstReplacement {
    fn new() -> Self {
        Self {
            replacements: AtomicUsize::new(0),
        }
    }
}

impl ExtraFileOperations for PanicAfterFirstReplacement {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        let replacement = self.replacements.fetch_add(1, Ordering::SeqCst);
        if replacement == 1 {
            panic!("simulated process interruption after the first target replacement");
        }
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

impl ExtraFileOperations for ExternalWriteAtFirstReplacement {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        if self.replacements.fetch_add(1, Ordering::SeqCst) == 0 {
            fs::write(&self.target, &self.replacement).unwrap();
        }
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

impl ExtraFileOperations for ExternalWriteAtFirstRestore {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        if self.restores.fetch_add(1, Ordering::SeqCst) == 0 {
            fs::write(&self.target, &self.replacement).unwrap();
        }
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct MutatingTargetDuringStaging {
    target: PathBuf,
    replacement: Vec<u8>,
    writes: AtomicUsize,
}

impl ExtraFileOperations for MutatingTargetDuringStaging {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        let writes = self.writes.fetch_add(1, Ordering::SeqCst);
        let result = StdExtraFileOperations.write_new_file(path, bytes);
        if writes == 4 {
            fs::write(&self.target, &self.replacement).unwrap();
        }
        result
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct BackupCollision {
    collided: AtomicUsize,
}

impl ExtraFileOperations for BackupCollision {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if filename.starts_with(".card1.mh3g-extra-backup-")
            && self.collided.fetch_add(1, Ordering::SeqCst) == 0
        {
            fs::write(path, bytes).unwrap();
            return Err(ConversionError::UnsafeInstall(
                "simulated external backup collision".to_owned(),
            ));
        }
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct PanicAfterFirstBackup {
    backups: AtomicUsize,
}

impl ExtraFileOperations for PanicAfterFirstBackup {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if filename.contains("mh3g-extra-backup-")
            && self.backups.fetch_add(1, Ordering::SeqCst) == 1
        {
            panic!("simulated interruption after the first ExtData backup");
        }
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct ToggleProbe {
    running: Arc<AtomicBool>,
}

impl ProcessProbe for ToggleProbe {
    fn matching_process(&self) -> Result<Option<String>, ConversionError> {
        Ok(self
            .running
            .load(Ordering::SeqCst)
            .then(|| "Cemu_release".to_owned()))
    }
}

#[cfg(unix)]
struct ReplacesTemporaryAtWriteGate {
    target_dir: PathBuf,
    replacement: PathBuf,
    swapped: AtomicBool,
}

#[cfg(unix)]
impl ProcessProbe for ReplacesTemporaryAtWriteGate {
    fn matching_process(&self) -> Result<Option<String>, ConversionError> {
        if !self.swapped.load(Ordering::SeqCst) {
            let temporary = transaction_directories(&self.target_dir)
                .into_iter()
                .flat_map(|directory| {
                    fs::read_dir(directory)
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .collect::<Vec<_>>()
                })
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.contains("card1.mh3g-extra-tmp-"))
                });
            if let Some(temporary) = temporary {
                fs::remove_file(&temporary).unwrap();
                std::os::unix::fs::symlink(&self.replacement, &temporary).unwrap();
                self.swapped.store(true, Ordering::SeqCst);
            }
        }
        Ok(None)
    }
}

struct StartsEmulatorAfterFirstReplacement {
    replacements: AtomicUsize,
    running: Arc<AtomicBool>,
}

impl ExtraFileOperations for StartsEmulatorAfterFirstReplacement {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        let result =
            StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target);
        if result.is_ok() && self.replacements.fetch_add(1, Ordering::SeqCst) == 0 {
            self.running.store(true, Ordering::SeqCst);
        }
        result
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct MutatesCompletedTargetAfterFirstReplacement {
    target: PathBuf,
    replacement: Vec<u8>,
    replacements: AtomicUsize,
}

impl ExtraFileOperations for MutatesCompletedTargetAfterFirstReplacement {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        let result =
            StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target);
        if result.is_ok() && self.replacements.fetch_add(1, Ordering::SeqCst) == 0 {
            fs::write(&self.target, &self.replacement).unwrap();
        }
        result
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct WritesExternalTargetAndFailsSecondReplacement {
    replacements: AtomicUsize,
    target: PathBuf,
    replacement: Vec<u8>,
}

#[cfg(unix)]
struct ReplacesTargetDirectoryAtJournalWrite {
    target_dir: PathBuf,
    moved_target_dir: PathBuf,
    external_target_dir: PathBuf,
    swapped: AtomicBool,
}

#[cfg(unix)]
struct ReplacesTransactionDirectoryAtTemporaryWrite {
    moved_transaction_dir: PathBuf,
    external_transaction_dir: PathBuf,
    swapped: AtomicBool,
}

impl ExtraFileOperations for WritesExternalTargetAndFailsSecondReplacement {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        if self.replacements.fetch_add(1, Ordering::SeqCst) == 1 {
            fs::write(&self.target, &self.replacement).unwrap();
            return Err(ConversionError::UnsafeInstall(
                "simulated second replacement with external target write".to_owned(),
            ));
        }
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

#[cfg(unix)]
impl ExtraFileOperations for ReplacesTargetDirectoryAtJournalWrite {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == ".mh3g-extra-recovery.json")
            && !self.swapped.swap(true, Ordering::SeqCst)
        {
            fs::rename(&self.target_dir, &self.moved_target_dir).unwrap();
            std::os::unix::fs::symlink(&self.external_target_dir, &self.target_dir).unwrap();
        }
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

#[cfg(unix)]
impl ExtraFileOperations for ReplacesTransactionDirectoryAtTemporaryWrite {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if filename.contains("mh3g-extra-tmp-") && !self.swapped.swap(true, Ordering::SeqCst) {
            let transaction_dir = path.parent().unwrap();
            fs::rename(transaction_dir, &self.moved_transaction_dir).unwrap();
            std::os::unix::fs::symlink(&self.external_transaction_dir, transaction_dir).unwrap();
        }
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

struct StartsEmulatorDuringRollback {
    restores: AtomicUsize,
    running: Arc<AtomicBool>,
}

impl ExtraFileOperations for StartsEmulatorDuringRollback {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        StdExtraFileOperations.replace_staged(staged, target, expected_staged, expected_target)
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        let result =
            StdExtraFileOperations.restore_target(staged, target, expected_staged, expected_target);
        if result.is_ok() && self.restores.fetch_add(1, Ordering::SeqCst) == 0 {
            self.running.store(true, Ordering::SeqCst);
        }
        result
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.sync_directory(path)
    }
}

#[test]
fn rejects_a_partial_group_before_creating_transaction_artifacts() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::write(staging.join("card1"), cemu_component("card1", 0x11)).unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::InvalidSave(_)));
    assert_no_transaction_artifacts(&target);
}

#[test]
fn rejects_a_quests_group_missing_quest4_before_creating_transaction_artifacts() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::Quests, 0x18);
    write_group(&target, ExtraGroup::Quests, 0x98);
    fs::remove_file(staging.join("quest4")).unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::Quests],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::InvalidSave(message) if message.contains("incomplete"))
    );
    assert_no_transaction_artifacts(&target);
}

#[test]
fn dry_run_is_read_only_for_a_complete_group() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x20);
    write_group(&target, ExtraGroup::GuildCards, 0x80);

    let report = dry_run_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
    )
    .unwrap();

    assert_eq!(report.entries.len(), 4);
    assert_eq!(report.groups, vec![ExtraGroup::GuildCards]);
    assert!(report.staging_dir.is_absolute());
    assert!(report.target_dir.is_absolute());
    assert_no_transaction_artifacts(&staging);
    assert_no_transaction_artifacts(&target);
}

#[test]
fn reports_and_persists_groups_in_canonical_order() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x21);
    write_group(&staging, ExtraGroup::Quests, 0x31);
    write_group(&target, ExtraGroup::GuildCards, 0x81);
    write_group(&target, ExtraGroup::Quests, 0x91);

    let report = dry_run_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::Quests, ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
    )
    .unwrap();
    assert_eq!(
        report.groups,
        vec![ExtraGroup::GuildCards, ExtraGroup::Quests]
    );

    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::Quests, ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let manifest: ExtraInstallManifest =
        serde_json::from_slice(&fs::read(installed.manifest_path).unwrap()).unwrap();
    assert_eq!(
        manifest.groups,
        vec![ExtraGroup::GuildCards, ExtraGroup::Quests]
    );
    #[cfg(unix)]
    {
        assert!(manifest.target_dir_identity.is_some());
        assert!(manifest.transaction_dir_identity.is_some());
    }
}

#[test]
fn installs_every_component_from_a_valid_complete_staged_group() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x30);
    write_group(&target, ExtraGroup::GuildCards, 0x90);

    let report = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();

    assert!(report.manifest_path.is_file());
    assert_eq!(report.manifest_path, transaction_journal(&target));
    assert!(!target.join(".mh3g-extra-install.json").exists());
    for component in ExtraGroup::GuildCards.components() {
        assert_eq!(
            fs::read(target.join(component)).unwrap(),
            fs::read(staging.join(component)).unwrap(),
            "component {component}"
        );
    }
}

#[test]
fn refuses_to_overwrite_a_recovery_journal_created_after_preflight() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x32);
    write_group(&target, ExtraGroup::GuildCards, 0x92);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let external_journal = b"external transaction record".to_vec();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &RecoveryJournalCollision {
            collided: AtomicBool::new(false),
            external_bytes: external_journal.clone(),
        },
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::UnsafeInstall(_)));
    assert_target_bytes(&target, &before);
    let journal = transaction_journal(&target);
    assert_eq!(fs::read(&journal).unwrap(), external_journal);
    assert!(!target.join(".mh3g-extra-install.json").exists());
    assert!(
        fs::read_dir(journal.parent().unwrap())
            .unwrap()
            .all(|entry| {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                name == ".mh3g-extra-recovery.json"
            })
    );
}

#[test]
fn append_only_journal_is_retained_when_the_write_call_fails_after_materialization() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x32);
    write_group(&target, ExtraGroup::GuildCards, 0x92);
    let before = target_bytes(&target, ExtraGroup::GuildCards);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &FailsAfterMaterializingFirstAppendOnlyArtifact {
            writes: AtomicUsize::new(0),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("materialization"))
    );
    assert_target_bytes(&target, &before);
    let journal = transaction_journal(&target);
    assert!(journal.is_file());
    assert!(
        fs::read_dir(journal.parent().unwrap())
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == ".mh3g-extra-recovery.json"),
        "the failed append-only write must not trigger cleanup of retained recovery material"
    );
}

#[test]
fn syncs_all_recovery_material_before_the_first_target_replacement() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x33);
    write_group(&target, ExtraGroup::GuildCards, 0x93);

    install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &RequiresPreparedMaterialSync {
            created_files: AtomicUsize::new(0),
            materials_synced: AtomicBool::new(false),
        },
    )
    .unwrap();
}

#[test]
fn successful_install_retains_its_create_new_recovery_journal_as_active_record() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x34);
    write_group(&target, ExtraGroup::GuildCards, 0x94);

    let report = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();

    assert_eq!(report.manifest_path, transaction_journal(&target));
    assert!(report.manifest_path.is_file());
    assert!(!target.join(".mh3g-extra-install.json").exists());
}

#[test]
fn pre_exchange_failure_never_invokes_a_path_based_cleanup_delete() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    let external_source = target.join("unrelated-external-file");
    write_group(&staging, ExtraGroup::GuildCards, 0x35);
    write_group(&target, ExtraGroup::GuildCards, 0x95);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    fs::write(&external_source, b"external cleanup artifact").unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &FailsBeforeFirstExchangeAndReplacesCleanupPath {
            sync_calls: AtomicUsize::new(0),
        },
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::UnsafeInstall(_)));
    assert_target_bytes(&target, &before);
    assert_eq!(
        fs::read(&external_source).unwrap(),
        b"external cleanup artifact"
    );
    let transaction_dir = only_transaction_directory(&target);
    assert!(transaction_dir.join(".mh3g-extra-recovery.json").is_file());
}

#[test]
fn rollback_retains_a_completed_transaction_directory_and_next_install_uses_a_new_one() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x36);
    write_group(&target, ExtraGroup::GuildCards, 0x96);
    let before = target_bytes(&target, ExtraGroup::GuildCards);

    let first = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let first_transaction_dir = first.manifest_path.parent().unwrap().to_path_buf();
    assert!(
        first_transaction_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".mh3g-extra-transaction-"))
    );

    rollback_extra_groups_with(&first.manifest_path, &Stopped, &StdExtraFileOperations).unwrap();
    assert_target_bytes(&target, &before);
    assert!(first.manifest_path.is_file());
    assert!(first_transaction_dir.is_dir());

    let second = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    assert_ne!(
        second.manifest_path.parent(),
        Some(first_transaction_dir.as_path())
    );
    assert_eq!(transaction_directories(&target).len(), 2);
}

#[test]
fn second_replacement_failure_retains_recovery_material_for_explicit_rollback() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x40);
    write_group(&target, ExtraGroup::GuildCards, 0xA0);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let operations = FailingOperations::second_replace();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &operations,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("second replacement"))
    );
    assert_eq!(
        fs::read(target.join("card1")).unwrap(),
        fs::read(staging.join("card1")).unwrap()
    );
    let journal = transaction_journal(&target);
    assert!(journal.is_file());

    rollback_extra_groups_with(&journal, &Stopped, &StdExtraFileOperations).unwrap();
    assert_target_bytes(&target, &before);
    assert!(journal.is_file());
}

#[test]
fn failed_replacement_does_not_delete_or_overwrite_the_failed_external_target() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x42);
    write_group(&target, ExtraGroup::GuildCards, 0xA2);
    let external = cemu_component("card2", 0xF2);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &WritesExternalTargetAndFailsSecondReplacement {
            replacements: AtomicUsize::new(0),
            target: target.join("card2"),
            replacement: external.clone(),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("second replacement"))
    );
    assert_eq!(fs::read(target.join("card2")).unwrap(), external);
    assert_eq!(
        fs::read(target.join("card1")).unwrap(),
        fs::read(staging.join("card1")).unwrap()
    );
    assert!(transaction_journal(&target).is_file());
}

#[test]
fn interrupted_install_publishes_a_recovery_journal_before_any_replacement() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x45);
    write_group(&target, ExtraGroup::GuildCards, 0xA5);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let operations = PanicAfterFirstReplacement::new();

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        install_extra_groups_with(
            &staging,
            &target,
            &[ExtraGroup::GuildCards],
            None,
            None,
            &Stopped,
            &operations,
        )
    }));
    assert!(interrupted.is_err());

    let journal = transaction_journal(&target);
    assert!(journal.is_file());
    assert!(fs::read(target.join("card1")).unwrap() != before[0].1.clone().unwrap());

    rollback_extra_groups_with(&journal, &Stopped, &StdExtraFileOperations).unwrap();

    assert_target_bytes(&target, &before);
    assert!(journal.is_file());
}

#[test]
fn rollback_retains_owned_temporaries_and_leaves_foreign_paths_untouched() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x48);
    write_group(&target, ExtraGroup::GuildCards, 0xA8);
    let before = target_bytes(&target, ExtraGroup::GuildCards);

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        install_extra_groups_with(
            &staging,
            &target,
            &[ExtraGroup::GuildCards],
            None,
            None,
            &Stopped,
            &PanicAfterFirstReplacement::new(),
        )
    }));
    assert!(interrupted.is_err());

    let foreign_temporary = target.join(".card1.mh3g-extra-tmp-foreign-transaction");
    let foreign_bytes = fs::read(staging.join("card1")).unwrap();
    fs::write(&foreign_temporary, &foreign_bytes).unwrap();

    let journal = transaction_journal(&target);
    rollback_extra_groups_with(&journal, &Stopped, &StdExtraFileOperations).unwrap();

    assert_target_bytes(&target, &before);
    assert_eq!(fs::read(&foreign_temporary).unwrap(), foreign_bytes);
    assert!(journal.exists());
    assert!(
        fs::read_dir(journal.parent().unwrap())
            .unwrap()
            .any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("mh3g-extra-tmp-")
            })
    );
    assert!(!target.join(".mh3g-extra-install.json").exists());
}

#[test]
fn interruption_after_first_backup_is_recoverable_from_the_prepared_journal() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4A);
    write_group(&target, ExtraGroup::GuildCards, 0xAA);
    let before = target_bytes(&target, ExtraGroup::GuildCards);

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        install_extra_groups_with(
            &staging,
            &target,
            &[ExtraGroup::GuildCards],
            None,
            None,
            &Stopped,
            &PanicAfterFirstBackup {
                backups: AtomicUsize::new(0),
            },
        )
    }));
    assert!(interrupted.is_err());

    let journal = transaction_journal(&target);
    assert!(journal.is_file());
    assert!(
        fs::read_dir(journal.parent().unwrap())
            .unwrap()
            .any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("mh3g-extra-backup-")
            })
    );
    assert_target_bytes(&target, &before);

    rollback_extra_groups_with(&journal, &Stopped, &StdExtraFileOperations).unwrap();
    assert_target_bytes(&target, &before);
    assert!(journal.is_file());
}

#[test]
fn does_not_compensate_when_a_completed_target_is_externally_rewritten() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4B);
    write_group(&target, ExtraGroup::GuildCards, 0xAB);
    let external = cemu_component("card1", 0xFB);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &MutatesCompletedTargetAfterFirstReplacement {
            target: target.join("card1"),
            replacement: external.clone(),
            replacements: AtomicUsize::new(0),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("atomic target exchange"))
    );
    assert_eq!(fs::read(target.join("card1")).unwrap(), external);
    assert!(transaction_journal(&target).is_file());
    assert!(!target.join(".mh3g-extra-install.json").exists());
}

#[test]
fn leaves_recovery_material_untouched_when_cemu_starts_between_replacements() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4C);
    write_group(&target, ExtraGroup::GuildCards, 0xAC);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let running = Arc::new(AtomicBool::new(false));
    let probe = ToggleProbe {
        running: Arc::clone(&running),
    };

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &probe,
        &StartsEmulatorAfterFirstReplacement {
            replacements: AtomicUsize::new(0),
            running: Arc::clone(&running),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("atomic target exchange"))
    );
    assert_eq!(
        fs::read(target.join("card1")).unwrap(),
        fs::read(staging.join("card1")).unwrap()
    );
    let journal = transaction_journal(&target);
    assert!(journal.is_file());

    running.store(false, Ordering::SeqCst);
    rollback_extra_groups_with(&journal, &probe, &StdExtraFileOperations).unwrap();
    assert_target_bytes(&target, &before);
    assert!(journal.is_file());
}

#[test]
fn rollback_refuses_a_pre_v4_root_manifest_without_directory_identities() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4D);
    write_group(&target, ExtraGroup::GuildCards, 0xAD);

    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let mut legacy: ExtraInstallManifest =
        serde_json::from_slice(&fs::read(&installed.manifest_path).unwrap()).unwrap();
    legacy.version = 3;
    legacy.transaction_dir = None;
    legacy.target_dir_identity = None;
    legacy.transaction_dir_identity = None;
    let canonical_target = target.canonicalize().unwrap();
    for entry in &mut legacy.entries {
        let legacy_temporary = canonical_target.join(entry.temporary.file_name().unwrap());
        // Rejection of the legacy v3 manifest happens before any recovery
        // artifact is trusted.  Windows `ReplaceFileW` has already moved the
        // temporary value to its controlled backup, so no POSIX-style
        // temporary pathname exists to hard-link here.
        entry.temporary = legacy_temporary;
        let legacy_backup =
            canonical_target.join(entry.backup.as_ref().unwrap().file_name().unwrap());
        entry.backup = Some(legacy_backup);
    }
    let mut legacy_json = serde_json::to_value(&legacy).unwrap();
    let legacy_object = legacy_json.as_object_mut().unwrap();
    legacy_object.remove("target_dir_identity");
    legacy_object.remove("transaction_dir_identity");
    let legacy_bytes = serde_json::to_vec_pretty(&legacy_json).unwrap();
    let manifest = target.join(".mh3g-extra-install.json");
    let journal = target.join(".mh3g-extra-recovery.json");
    fs::write(&manifest, &legacy_bytes).unwrap();
    fs::write(&journal, &legacy_bytes).unwrap();
    assert!(manifest.is_file());
    assert!(journal.is_file());
    let installed_target = target_bytes(&target, ExtraGroup::GuildCards);

    let error =
        rollback_extra_groups_with(&manifest, &Stopped, &StdExtraFileOperations).unwrap_err();
    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("lacks persisted"))
    );
    assert_target_bytes(&target, &installed_target);
    assert!(manifest.is_file());
    assert!(journal.is_file());
    assert!(installed.manifest_path.is_file());
}

#[test]
fn rollback_refuses_a_v4_transaction_manifest_without_directory_identities() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4E);
    write_group(&target, ExtraGroup::GuildCards, 0xAE);

    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let mut v4: ExtraInstallManifest =
        serde_json::from_slice(&fs::read(&installed.manifest_path).unwrap()).unwrap();
    v4.version = 4;
    v4.target_dir_identity = None;
    v4.transaction_dir_identity = None;
    let mut v4_json = serde_json::to_value(&v4).unwrap();
    let v4_object = v4_json.as_object_mut().unwrap();
    v4_object.remove("target_dir_identity");
    v4_object.remove("transaction_dir_identity");
    fs::write(
        &installed.manifest_path,
        serde_json::to_vec_pretty(&v4_json).unwrap(),
    )
    .unwrap();
    let installed_target = target_bytes(&target, ExtraGroup::GuildCards);

    let error =
        rollback_extra_groups_with(&installed.manifest_path, &Stopped, &StdExtraFileOperations)
            .unwrap_err();
    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("lacks persisted"))
    );
    assert_target_bytes(&target, &installed_target);
    assert!(installed.manifest_path.is_file());
}

#[test]
fn a_stale_lock_inode_does_not_block_install_or_rollback() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4E);
    write_group(&target, ExtraGroup::GuildCards, 0xAE);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    fs::write(
        target.join(".mh3g-extra-install.lock"),
        b"stale crash marker",
    )
    .unwrap();

    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    rollback_extra_groups_with(&installed.manifest_path, &Stopped, &StdExtraFileOperations)
        .unwrap();

    assert_target_bytes(&target, &before);
    assert!(installed.manifest_path.is_file());
    assert!(target.join(".mh3g-extra-install.lock").is_file());
}

// `LockFileEx` locks are owned by a Windows process, so a second handle opened
// by this very test process does not conflict with the first one.  Real
// competing Cemu/converter processes still contend through the OS advisory
// lock; this in-process contention assertion is only meaningful on platforms
// whose file-lock API reports it as a conflict.
#[cfg(not(windows))]
#[test]
fn a_live_advisory_lock_refuses_a_second_install() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4E);
    write_group(&target, ExtraGroup::GuildCards, 0xAE);
    let lock_path = target.join(".mh3g-extra-install.lock");
    let held_lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    held_lock.try_lock_exclusive().unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();
    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("already locked"))
    );

    FileExt::unlock(&held_lock).unwrap();
}

#[cfg(unix)]
#[test]
fn a_temporary_replaced_by_a_symlink_is_not_followed_or_removed() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    let external = temp.path().join("external-component");
    write_group(&staging, ExtraGroup::GuildCards, 0x4F);
    write_group(&target, ExtraGroup::GuildCards, 0xAF);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let external_bytes = cemu_component("card1", 0xEF);
    fs::write(&external, &external_bytes).unwrap();
    let probe = ReplacesTemporaryAtWriteGate {
        target_dir: target.clone(),
        replacement: external.clone(),
        swapped: AtomicBool::new(false),
    };

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &probe,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::UnsafeInstall(_)));
    assert_target_bytes(&target, &before);
    assert_eq!(fs::read(&external).unwrap(), external_bytes);
    let temporary = fs::read_dir(only_transaction_directory(&target))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains("mh3g-extra-tmp-"))
                && fs::symlink_metadata(path)
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false)
        })
        .expect("swapped temporary exists");
    assert!(
        fs::symlink_metadata(temporary)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(transaction_journal(&target).is_file());
}

#[cfg(unix)]
#[test]
fn a_target_directory_replaced_by_a_symlink_never_receives_transaction_material() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    let moved_target = temp.path().join("moved-target");
    let external_target = temp.path().join("external-target");
    write_group(&staging, ExtraGroup::GuildCards, 0x3A);
    write_group(&target, ExtraGroup::GuildCards, 0x4A);
    fs::create_dir(&external_target).unwrap();
    let sentinel = external_target.join("sentinel");
    fs::write(&sentinel, b"external target remains untouched").unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &ReplacesTargetDirectoryAtJournalWrite {
            target_dir: target.clone(),
            moved_target_dir: moved_target,
            external_target_dir: external_target.clone(),
            swapped: AtomicBool::new(false),
        },
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::UnsafeInstall(_)));
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"external target remains untouched"
    );
    assert!(
        fs::read_dir(&external_target)
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == "sentinel"),
        "target-directory replacement must not create a journal, backup, or temporary in the external directory"
    );
}

#[cfg(unix)]
#[test]
fn a_transaction_directory_replaced_by_a_symlink_never_receives_transaction_material() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    let moved_transaction = temp.path().join("moved-transaction");
    let external_transaction = temp.path().join("external-transaction");
    write_group(&staging, ExtraGroup::GuildCards, 0x3B);
    write_group(&target, ExtraGroup::GuildCards, 0x4B);
    fs::create_dir(&external_transaction).unwrap();
    let sentinel = external_transaction.join("sentinel");
    fs::write(&sentinel, b"external transaction remains untouched").unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &ReplacesTransactionDirectoryAtTemporaryWrite {
            moved_transaction_dir: moved_transaction,
            external_transaction_dir: external_transaction.clone(),
            swapped: AtomicBool::new(false),
        },
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::UnsafeInstall(_)));
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"external transaction remains untouched"
    );
    assert!(
        fs::read_dir(&external_transaction)
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == "sentinel"),
        "transaction-directory replacement must not create a backup or temporary in the external directory"
    );
}

#[cfg(unix)]
#[test]
fn rollback_rejects_a_recreated_target_directory_even_with_copied_transaction_artifacts() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    let moved_target = temp.path().join("moved-target");
    write_group(&staging, ExtraGroup::GuildCards, 0x3C);
    write_group(&target, ExtraGroup::GuildCards, 0x4C);

    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let original_transaction = installed.manifest_path.parent().unwrap().to_path_buf();
    let transaction_name = original_transaction.file_name().unwrap().to_owned();

    fs::rename(&target, &moved_target).unwrap();
    write_group(&target, ExtraGroup::GuildCards, 0x3C);
    let copied_transaction = target.join(transaction_name);
    fs::create_dir(&copied_transaction).unwrap();
    for entry in fs::read_dir(moved_target.join(original_transaction.file_name().unwrap())).unwrap()
    {
        let entry = entry.unwrap();
        fs::hard_link(entry.path(), copied_transaction.join(entry.file_name())).unwrap();
    }
    let replacement_before = target_bytes(&target, ExtraGroup::GuildCards);

    let error = rollback_extra_groups_with(
        copied_transaction.join(".mh3g-extra-recovery.json"),
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(ref message) if message.contains("identity")),
        "unexpected rollback result: {error:?}"
    );
    assert_target_bytes(&target, &replacement_before);
    assert!(
        !target.join(".mh3g-extra-install.lock").exists(),
        "directory identity rejection must occur before creating a lock in the replacement target"
    );
}

#[cfg(unix)]
#[test]
fn rollback_rejects_a_recreated_transaction_directory_even_with_copied_artifacts() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    let moved_transaction = temp.path().join("moved-transaction");
    write_group(&staging, ExtraGroup::GuildCards, 0x3D);
    write_group(&target, ExtraGroup::GuildCards, 0x4D);

    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let original_transaction = installed.manifest_path.parent().unwrap().to_path_buf();
    let transaction_name = original_transaction.file_name().unwrap().to_owned();

    fs::rename(&original_transaction, &moved_transaction).unwrap();
    let copied_transaction = target.join(transaction_name);
    fs::create_dir(&copied_transaction).unwrap();
    for entry in fs::read_dir(&moved_transaction).unwrap() {
        let entry = entry.unwrap();
        fs::hard_link(entry.path(), copied_transaction.join(entry.file_name())).unwrap();
    }
    let installed_target = target_bytes(&target, ExtraGroup::GuildCards);

    let error = rollback_extra_groups_with(
        copied_transaction.join(".mh3g-extra-recovery.json"),
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(ref message) if message.contains("transaction directory identity")),
        "unexpected rollback result: {error:?}"
    );
    assert_target_bytes(&target, &installed_target);
}

#[test]
fn rollback_stops_writing_when_cemu_starts_after_its_first_restore() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4F);
    write_group(&target, ExtraGroup::GuildCards, 0xAF);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let running = Arc::new(AtomicBool::new(false));
    let probe = ToggleProbe {
        running: Arc::clone(&running),
    };

    let error = rollback_extra_groups_with(
        &installed.manifest_path,
        &probe,
        &StartsEmulatorDuringRollback {
            restores: AtomicUsize::new(0),
            running: Arc::clone(&running),
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("emulator process is running"))
    );
    assert_eq!(
        fs::read(target.join("card1")).unwrap(),
        before
            .iter()
            .find(|(component, _)| component == "card1")
            .and_then(|(_, bytes)| bytes.as_ref())
            .unwrap()
            .clone()
    );
    assert!(installed.manifest_path.is_file());

    running.store(false, Ordering::SeqCst);
    rollback_extra_groups_with(&installed.manifest_path, &probe, &StdExtraFileOperations).unwrap();
    assert_target_bytes(&target, &before);
    assert!(installed.manifest_path.is_file());
}

#[test]
fn failed_backup_creation_never_removes_a_file_this_transaction_did_not_create() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x46);
    write_group(&target, ExtraGroup::GuildCards, 0xA6);
    let original = fs::read(target.join("card1")).unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &BackupCollision {
            collided: AtomicUsize::new(0),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("collision"))
    );
    let external_backup = only_transaction_directory(&target).join(format!(
        ".card1.mh3g-extra-backup-{}",
        sha256_hex(&original)
    ));
    assert_eq!(fs::read(&external_backup).unwrap(), original);
    assert_eq!(fs::read(target.join("card1")).unwrap(), original);
    assert!(transaction_journal(&target).is_file());
    assert!(!target.join(".mh3g-extra-install.json").exists());
}

#[test]
fn rechecks_target_state_immediately_before_replacing_any_component() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x47);
    write_group(&target, ExtraGroup::GuildCards, 0xA7);
    let external = cemu_component("card1", 0xF7);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &MutatingTargetDuringStaging {
            target: target.join("card1"),
            replacement: external.clone(),
            writes: AtomicUsize::new(0),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("changed"))
    );
    assert_eq!(fs::read(target.join("card1")).unwrap(), external);
    assert!(!target.join(".mh3g-extra-install.json").exists());
    assert!(transaction_journal(&target).is_file());
}

#[test]
fn refuses_a_target_write_in_the_last_replace_window_without_losing_it() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x49);
    write_group(&target, ExtraGroup::GuildCards, 0xA9);
    let external = cemu_component("card1", 0xF9);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &ExternalWriteAtFirstReplacement {
            replacements: AtomicUsize::new(0),
            target: target.join("card1"),
            replacement: external.clone(),
        },
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::UnsafeInstall(_)));
    assert_eq!(fs::read(target.join("card1")).unwrap(), external);
    assert!(transaction_journal(&target).is_file());
}

#[test]
fn rollback_refuses_a_target_write_in_its_last_restore_window_without_losing_it() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x4A);
    write_group(&target, ExtraGroup::GuildCards, 0xAA);
    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let external = cemu_component("card1", 0xFA);

    let error = rollback_extra_groups_with(
        &installed.manifest_path,
        &Stopped,
        &ExternalWriteAtFirstRestore {
            restores: AtomicUsize::new(0),
            target: target.join("card1"),
            replacement: external.clone(),
        },
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::UnsafeInstall(_)));
    assert_eq!(fs::read(target.join("card1")).unwrap(), external);
    assert!(installed.manifest_path.is_file());
}

#[test]
fn rejects_uninitialized_target_components_before_creating_transaction_artifacts() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x50);
    fs::create_dir_all(&target).unwrap();
    for (index, component) in ExtraGroup::GuildCards
        .components()
        .iter()
        .take(2)
        .enumerate()
    {
        fs::write(
            target.join(component),
            cemu_component(component, 0xB0 + index as u8),
        )
        .unwrap();
    }
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::InvalidSave(message) if message.contains("missing")));
    assert_target_bytes(&target, &before);
    assert_no_transaction_artifacts(&target);
}

#[test]
fn rollback_resumes_after_a_restored_target_backup_was_already_consumed() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x55);
    write_group(&target, ExtraGroup::GuildCards, 0xB5);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let manifest: ExtraInstallManifest =
        serde_json::from_slice(&fs::read(&installed.manifest_path).unwrap()).unwrap();
    let restored = manifest
        .entries
        .iter()
        .find(|entry| entry.component == "card1")
        .unwrap();
    let original = before
        .iter()
        .find(|(component, _)| component == "card1")
        .and_then(|(_, bytes)| bytes.as_deref())
        .unwrap();
    fs::write(&restored.target, original).unwrap();
    fs::remove_file(restored.backup.as_ref().unwrap()).unwrap();

    rollback_extra_groups_with(&installed.manifest_path, &Stopped, &StdExtraFileOperations)
        .unwrap();

    assert_target_bytes(&target, &before);
    assert!(installed.manifest_path.is_file());
}

#[test]
fn rollback_refuses_an_after_state_when_its_required_backup_is_missing() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x56);
    write_group(&target, ExtraGroup::GuildCards, 0xB6);
    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let manifest: ExtraInstallManifest =
        serde_json::from_slice(&fs::read(&installed.manifest_path).unwrap()).unwrap();
    let missing_backup = manifest
        .entries
        .iter()
        .find(|entry| entry.component == "card1")
        .unwrap()
        .backup
        .as_ref()
        .unwrap();
    let before_rollback = target_bytes(&target, ExtraGroup::GuildCards);
    fs::remove_file(missing_backup).unwrap();

    let error =
        rollback_extra_groups_with(&installed.manifest_path, &Stopped, &StdExtraFileOperations)
            .unwrap_err();

    assert!(matches!(error, ConversionError::IoAtPath { .. }));
    assert_target_bytes(&target, &before_rollback);
    assert!(installed.manifest_path.is_file());
}

#[test]
fn rejects_a_running_emulator_before_changing_the_target() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x60);
    write_group(&target, ExtraGroup::GuildCards, 0xC0);
    let before = target_bytes(&target, ExtraGroup::GuildCards);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Running,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("Cemu_release"))
    );
    assert_target_bytes(&target, &before);
    assert_no_transaction_artifacts(&target);
}

#[test]
fn rejects_changed_expected_staging_or_target_set_before_changing_the_target() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x70);
    write_group(&target, ExtraGroup::GuildCards, 0xD0);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let stale_hash = "00".repeat(32);

    for (expected_staging, expected_target) in [
        (Some(stale_hash.as_str()), None),
        (None, Some(stale_hash.as_str())),
    ] {
        let error = install_extra_groups_with(
            &staging,
            &target,
            &[ExtraGroup::GuildCards],
            expected_staging,
            expected_target,
            &Stopped,
            &StdExtraFileOperations,
        )
        .unwrap_err();
        assert!(matches!(error, ConversionError::UnsafeInstall(_)));
        assert_target_bytes(&target, &before);
        assert_no_transaction_artifacts(&target);
    }
}

#[test]
fn rejects_a_tampered_manifest_without_changing_installed_targets() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x81);
    write_group(&target, ExtraGroup::GuildCards, 0xE1);
    let installed = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap();
    let before_rollback = target_bytes(&target, ExtraGroup::GuildCards);
    let mut manifest: ExtraInstallManifest =
        serde_json::from_slice(&fs::read(&installed.manifest_path).unwrap()).unwrap();
    manifest.entries.pop();
    fs::write(
        &installed.manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let error =
        rollback_extra_groups_with(&installed.manifest_path, &Stopped, &StdExtraFileOperations)
            .unwrap_err();

    assert!(matches!(error, ConversionError::InvalidSave(_)));
    assert_target_bytes(&target, &before_rollback);
    assert!(installed.manifest_path.is_file());
}

#[test]
fn rejects_manifests_without_a_controlled_transaction_temporary_path() {
    for mutation in 0..4 {
        let temp = tempdir().unwrap();
        let staging = temp.path().join("staging");
        let target = temp.path().join("target");
        write_group(&staging, ExtraGroup::GuildCards, 0x83);
        write_group(&target, ExtraGroup::GuildCards, 0xE3);
        let installed = install_extra_groups_with(
            &staging,
            &target,
            &[ExtraGroup::GuildCards],
            None,
            None,
            &Stopped,
            &StdExtraFileOperations,
        )
        .unwrap();
        let before_rollback = target_bytes(&target, ExtraGroup::GuildCards);
        let mut manifest: ExtraInstallManifest =
            serde_json::from_slice(&fs::read(&installed.manifest_path).unwrap()).unwrap();

        match mutation {
            0 => manifest.version = 2,
            1 => manifest.transaction_id = "not-a-uuid".to_owned(),
            2 => {
                manifest.entries[0].temporary =
                    target.join(".card1.mh3g-extra-tmp-00000000-0000-0000-0000-000000000000");
            }
            3 => manifest.entries[0].temporary = temp.path().join("outside-temporary"),
            _ => unreachable!(),
        }
        fs::write(
            &installed.manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error =
            rollback_extra_groups_with(&installed.manifest_path, &Stopped, &StdExtraFileOperations)
                .unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert_target_bytes(&target, &before_rollback);
    }
}

#[test]
fn rejects_manifest_groups_that_are_missing_duplicated_or_mismatched() {
    for groups in [
        vec![],
        vec![ExtraGroup::GuildCards, ExtraGroup::GuildCards],
        vec![ExtraGroup::Quests],
    ] {
        let temp = tempdir().unwrap();
        let staging = temp.path().join("staging");
        let target = temp.path().join("target");
        write_group(&staging, ExtraGroup::GuildCards, 0x84);
        write_group(&target, ExtraGroup::GuildCards, 0xE4);
        let installed = install_extra_groups_with(
            &staging,
            &target,
            &[ExtraGroup::GuildCards],
            None,
            None,
            &Stopped,
            &StdExtraFileOperations,
        )
        .unwrap();
        let mut manifest: ExtraInstallManifest =
            serde_json::from_slice(&fs::read(&installed.manifest_path).unwrap()).unwrap();
        manifest.groups = groups;
        fs::write(
            &installed.manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let error =
            rollback_extra_groups_with(&installed.manifest_path, &Stopped, &StdExtraFileOperations)
                .unwrap_err();
        assert!(
            matches!(error, ConversionError::InvalidSave(message) if message.contains("groups"))
        );
        assert!(installed.manifest_path.is_file());
    }
}

#[test]
fn recovery_journal_creation_failure_retains_an_empty_transaction_directory() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x82);
    write_group(&target, ExtraGroup::GuildCards, 0xE2);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let operations = FailingOperations::recovery_journal_create();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &operations,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("recovery journal creation"))
    );
    assert_target_bytes(&target, &before);
    let transaction_dir = only_transaction_directory(&target);
    assert!(fs::read_dir(transaction_dir).unwrap().next().is_none());
}

#[test]
fn rejects_target_and_staging_directory_aliases() {
    let temp = tempdir().unwrap();
    let shared = temp.path().join("shared");
    write_group(&shared, ExtraGroup::GuildCards, 0x91);

    let error = dry_run_extra_groups_with(
        &shared,
        &shared,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::InvalidSave(message) if message.contains("alias")));
}

#[test]
fn target_component_with_an_invalid_cemu_wrapper_is_rejected_before_backup() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x92);
    write_group(&target, ExtraGroup::GuildCards, 0xF2);
    fs::write(target.join("card2"), b"not a Cemu component").unwrap();

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::InvalidSave(_)));
    assert_no_transaction_artifacts(&target);
}

#[test]
fn staged_component_with_an_invalid_cemu_wrapper_is_rejected_before_target_change() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x92);
    write_group(&target, ExtraGroup::GuildCards, 0xF2);
    fs::write(staging.join("card2"), b"not a Cemu component").unwrap();
    let before = target_bytes(&target, ExtraGroup::GuildCards);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::InvalidSave(_)));
    assert_target_bytes(&target, &before);
    assert_no_transaction_artifacts(&target);
}

#[test]
fn process_probe_failure_fails_closed_before_target_change() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x92);
    write_group(&target, ExtraGroup::GuildCards, 0xF2);
    let before = target_bytes(&target, ExtraGroup::GuildCards);

    let error = install_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &ProbeFailure,
        &StdExtraFileOperations,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("enumeration"))
    );
    assert_target_bytes(&target, &before);
    assert_no_transaction_artifacts(&target);
}

#[test]
fn rejects_duplicate_group_requests() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x93);
    write_group(&target, ExtraGroup::GuildCards, 0xF3);

    let error = dry_run_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards, ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
    )
    .unwrap_err();

    assert!(
        matches!(error, ConversionError::InvalidSave(message) if message.contains("duplicate"))
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_staging_components() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x94);
    fs::create_dir_all(&target).unwrap();
    let card1 = staging.join("card1");
    let linked = staging.join("linked-card1");
    fs::rename(&card1, &linked).unwrap();
    symlink(&linked, &card1).unwrap();

    let error = dry_run_extra_groups_with(
        &staging,
        &target,
        &[ExtraGroup::GuildCards],
        None,
        None,
        &Stopped,
    )
    .unwrap_err();

    assert!(matches!(error, ConversionError::InvalidSave(message) if message.contains("regular")));
}

#[test]
fn report_paths_are_absolute_and_bound_to_the_target_directory() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::Quests, 0x95);
    write_group(&target, ExtraGroup::Quests, 0xE5);

    let report = dry_run_extra_groups_with(
        PathBuf::from(&staging),
        PathBuf::from(&target),
        &[ExtraGroup::Quests],
        None,
        None,
        &Stopped,
    )
    .unwrap();

    assert!(report.manifest_path.is_absolute());
    assert!(
        report
            .entries
            .iter()
            .all(|entry| entry.target.is_absolute())
    );
}
