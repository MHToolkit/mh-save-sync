//! Atomic installation and rollback for complete MH3G ExtData component groups.
//!
//! The Cemu-side components are independently stored files, but game state
//! treats each group as one logical unit.  This module therefore never offers
//! a per-file install entry point.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Component, Path, PathBuf},
};

#[cfg(any(target_os = "android", target_os = "linux", target_os = "macos"))]
use std::{ffi::CString, os::unix::ffi::OsStrExt};

use clap::ValueEnum;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    ConversionError,
    converter::validate_cemu_external_component_named,
    io_at_path,
    process_probe::{PlatformProcessProbe, ProcessProbe},
    transaction::{remove_if_regular_file, sha256_hex, sync_directory, write_new_file},
};

pub const EXTRA_INSTALL_MANIFEST_VERSION: u32 = 3;
const EXTRA_MANIFEST_NAME: &str = ".mh3g-extra-install.json";
const EXTRA_RECOVERY_JOURNAL_NAME: &str = ".mh3g-extra-recovery.json";
const EXTRA_LOCK_NAME: &str = ".mh3g-extra-install.lock";

const GUILD_CARD_COMPONENTS: [&str; 4] = ["card1", "card2", "card3", "cardbox"];
const QUEST_COMPONENTS: [&str; 4] = ["quest1", "quest2", "quest3", "quest4"];

/// A complete ExtData unit that can be installed or rolled back atomically.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum ExtraGroup {
    #[value(name = "guild-cards")]
    GuildCards,
    #[value(name = "quests")]
    Quests,
}

impl ExtraGroup {
    pub const fn components(self) -> &'static [&'static str] {
        match self {
            Self::GuildCards => &GUILD_CARD_COMPONENTS,
            Self::Quests => &QUEST_COMPONENTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraInstallEntry {
    pub group: ExtraGroup,
    pub component: String,
    pub target: PathBuf,
    pub temporary: PathBuf,
    pub before_sha256: Option<String>,
    pub after_sha256: String,
    pub backup: Option<PathBuf>,
    pub target_previously_existed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraInstallManifest {
    pub version: u32,
    pub transaction_id: String,
    pub groups: Vec<ExtraGroup>,
    pub staging_dir: PathBuf,
    pub target_dir: PathBuf,
    pub staging_set_sha256: String,
    pub target_set_sha256: String,
    pub entries: Vec<ExtraInstallEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraInstallReport {
    pub manifest_path: PathBuf,
    pub groups: Vec<ExtraGroup>,
    pub staging_dir: PathBuf,
    pub target_dir: PathBuf,
    pub staging_set_sha256: String,
    pub target_set_sha256: String,
    pub entries: Vec<ExtraInstallEntry>,
}

/// Filesystem seam used by synthetic tests to fail a precise replacement or
/// manifest publication without touching an emulator or real save directory.
pub trait ExtraFileOperations {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError>;
    /// Atomically exchange a staged file with an existing target, but only if
    /// both names still contain the bytes captured by the transaction plan.
    /// Implementations must retain both values if they detect a conflict.
    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError>;
    /// Reverse a prior staged exchange.  This has the same conditional
    /// semantics as `replace_staged`; it must not overwrite a later writer.
    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError>;
    fn remove_regular_file(&self, path: &Path) -> Result<(), ConversionError>;
    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdExtraFileOperations;

impl ExtraFileOperations for StdExtraFileOperations {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        write_new_file(path, bytes)
    }

    fn replace_staged(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        conditional_exchange(
            staged,
            target,
            expected_staged,
            expected_target,
            "replacing staged ExtData component",
        )
    }

    fn restore_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
        expected_target: &[u8],
    ) -> Result<(), ConversionError> {
        conditional_exchange(
            staged,
            target,
            expected_staged,
            expected_target,
            "restoring staged ExtData component",
        )
    }

    fn remove_regular_file(&self, path: &Path) -> Result<(), ConversionError> {
        remove_if_regular_file(path)
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        sync_directory(path)
    }
}

/// Perform a replace-or-restore as a two-name atomic exchange and verify the
/// returned value.  Ordinary `rename` silently replaces the target and leaves
/// no evidence that a non-cooperating writer won the last preflight window.
/// Exchange retains that value under `staged`, allowing us to restore it and
/// fail closed instead of losing it.
fn conditional_exchange(
    staged: &Path,
    target: &Path,
    expected_staged: &[u8],
    expected_target: &[u8],
    operation: &'static str,
) -> Result<(), ConversionError> {
    validate_regular_file_bytes(staged, expected_staged, None, operation)?;
    validate_regular_file_bytes(target, expected_target, None, operation)?;
    atomic_exchange_paths(staged, target, operation)?;

    let target_after = read_regular_file(target, operation);
    let staged_after = read_regular_file(staged, operation);
    match (target_after, staged_after) {
        (Ok(target_after), Ok(staged_after))
            if target_after == expected_staged && staged_after == expected_target =>
        {
            sync_exchange_parent(target, operation)
        }
        (target_after, staged_after) => {
            let observed_target = target_after
                .as_ref()
                .map(|bytes| sha256_hex(bytes))
                .unwrap_or_else(|_| "unreadable".to_owned());
            let observed_staged = staged_after
                .as_ref()
                .map(|bytes| sha256_hex(bytes))
                .unwrap_or_else(|_| "unreadable".to_owned());
            let (Ok(target_after), Ok(staged_after)) = (&target_after, &staged_after) else {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData exchange has an unreadable post-swap path; retain the recovery journal and both paths ({operation}; target={observed_target}, staged={observed_staged})"
                )));
            };
            // Only reverse a conflict when the target still demonstrably holds
            // this transaction's staged value.  If it does not, a later writer
            // has already won the pathname and we leave both paths untouched.
            if target_after != expected_staged {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData exchange detected a competing write after the swap; retain the recovery journal and both paths ({operation}; target={observed_target}, staged={observed_staged})"
                )));
            }
            let target_snapshot = capture_owned_regular_file(
                target,
                target_after,
                "capturing ExtData target before conflict restoration",
            )?;
            let staged_snapshot = capture_owned_regular_file(
                staged,
                staged_after,
                "capturing ExtData staged value before conflict restoration",
            )?;
            if let Err(error) = validate_owned_regular_file(
                &target_snapshot,
                "rechecking ExtData target before conflict restoration",
            ) {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData exchange detected a competing write and the target changed before restoration; retain the recovery journal and both paths ({operation}; target={observed_target}, staged={observed_staged}; recovery={error})"
                )));
            }
            if let Err(error) = validate_owned_regular_file(
                &staged_snapshot,
                "rechecking ExtData staged value before conflict restoration",
            ) {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData exchange detected a competing write and the staged value changed before restoration; retain the recovery journal and both paths ({operation}; target={observed_target}, staged={observed_staged}; recovery={error})"
                )));
            }
            let restore = atomic_exchange_paths(staged, target, operation)
                .and_then(|_| sync_exchange_parent(target, operation));
            match restore {
                Ok(()) => {
                    let restored_target = read_regular_file(
                        target,
                        "verifying ExtData target after conflict restoration",
                    );
                    let restored_staged = read_regular_file(
                        staged,
                        "verifying ExtData staged value after conflict restoration",
                    );
                    if restored_target.as_ref().ok() == Some(staged_after)
                        && restored_staged.as_ref().ok() == Some(target_after)
                    {
                        Err(ConversionError::UnsafeInstall(format!(
                            "ExtData exchange detected a competing write; original names were restored ({operation}; target={observed_target}, staged={observed_staged})"
                        )))
                    } else {
                        Err(ConversionError::UnsafeInstall(format!(
                            "ExtData exchange detected a competing write but conflict restoration changed again; retain the recovery journal and both paths ({operation}; target={observed_target}, staged={observed_staged})"
                        )))
                    }
                }
                Err(restore_error) => Err(ConversionError::UnsafeInstall(format!(
                    "ExtData exchange detected a competing write and could not restore the original names; retain the recovery journal and both paths ({operation}; target={observed_target}, staged={observed_staged}; recovery={restore_error})"
                ))),
            }
        }
    }
}

fn sync_exchange_parent(target: &Path, operation: &'static str) -> Result<(), ConversionError> {
    let parent = target.parent().ok_or_else(|| {
        ConversionError::UnsafeInstall(format!(
            "ExtData exchange target has no parent ({operation}): {}",
            target.display()
        ))
    })?;
    sync_directory(parent)
}

#[cfg(any(target_os = "android", target_os = "linux", target_os = "macos"))]
fn c_path(path: &Path, operation: &'static str) -> Result<CString, ConversionError> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        ConversionError::InvalidSave(format!(
            "ExtData exchange path contains an embedded NUL ({operation}): {}",
            path.display()
        ))
    })
}

#[cfg(target_os = "macos")]
fn atomic_exchange_paths(
    staged: &Path,
    target: &Path,
    operation: &'static str,
) -> Result<(), ConversionError> {
    let staged_c = c_path(staged, operation)?;
    let target_c = c_path(target, operation)?;
    let result =
        unsafe { libc::renamex_np(staged_c.as_ptr(), target_c.as_ptr(), libc::RENAME_SWAP) };
    if result == 0 {
        Ok(())
    } else {
        io_at_path(Err(std::io::Error::last_os_error()), operation, target)
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn atomic_exchange_paths(
    staged: &Path,
    target: &Path,
    operation: &'static str,
) -> Result<(), ConversionError> {
    let staged_c = c_path(staged, operation)?;
    let target_c = c_path(target, operation)?;
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2 as libc::c_long,
            libc::AT_FDCWD,
            staged_c.as_ptr(),
            libc::AT_FDCWD,
            target_c.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        io_at_path(Err(std::io::Error::last_os_error()), operation, target)
    }
}

#[cfg(not(any(target_os = "android", target_os = "linux", target_os = "macos")))]
fn atomic_exchange_paths(
    _staged: &Path,
    _target: &Path,
    _operation: &'static str,
) -> Result<(), ConversionError> {
    Err(ConversionError::UnsafeInstall(
        "atomic ExtData exchange is unavailable on this platform".to_owned(),
    ))
}

#[derive(Debug)]
struct ExtraInstallLock {
    _file: File,
}

impl ExtraInstallLock {
    fn acquire(target_dir: &Path) -> Result<Self, ConversionError> {
        let path = target_dir.join(EXTRA_LOCK_NAME);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        for _ in 0..2 {
            let file = io_at_path(options.open(&path), "opening ExtData install lock", &path)?;
            let file_metadata = io_at_path(file.metadata(), "reading ExtData install lock", &path)?;
            let path_metadata = io_at_path(
                fs::symlink_metadata(&path),
                "reading ExtData install lock metadata",
                &path,
            )?;
            if !file_metadata.file_type().is_file()
                || !path_metadata.file_type().is_file()
                || !same_file_identity(&file_metadata, &path_metadata)
            {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData install lock is not a stable regular file: {}",
                    path.display()
                )));
            }
            match file.try_lock_exclusive() {
                Ok(()) => {
                    // The lock is advisory and bound to the opened inode. Re-read
                    // the pathname after locking so a replaced path cannot create
                    // two independently locked transactions.
                    let final_path_metadata = io_at_path(
                        fs::symlink_metadata(&path),
                        "rechecking ExtData install lock metadata",
                        &path,
                    )?;
                    if final_path_metadata.file_type().is_file()
                        && same_file_identity(&file_metadata, &final_path_metadata)
                    {
                        return Ok(Self { _file: file });
                    }
                    // Dropping the handle releases the advisory lock before a
                    // bounded retry against the replacement pathname.
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "ExtData group installation is already locked: {}",
                        path.display()
                    )));
                }
                Err(error) => return io_at_path(Err(error), "locking ExtData install lock", &path),
            }
        }
        Err(ConversionError::UnsafeInstall(format!(
            "ExtData install lock changed while being acquired: {}",
            path.display()
        )))
    }
}

#[derive(Debug, Clone)]
struct PreparedEntry {
    group: ExtraGroup,
    component: &'static str,
    staging_bytes: Vec<u8>,
    target: PathBuf,
    temporary: PathBuf,
    previous: Option<Vec<u8>>,
    backup: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct OwnedArtifact {
    path: PathBuf,
    bytes: Vec<u8>,
    identity: FileIdentity,
}

#[cfg(unix)]
type FileIdentity = (u64, u64);

#[cfg(not(unix))]
type FileIdentity = (u64, Option<u128>);

#[derive(Debug, Clone)]
struct ExtraInstallPlan {
    staging_dir: PathBuf,
    target_dir: PathBuf,
    groups: Vec<ExtraGroup>,
    manifest_path: PathBuf,
    recovery_journal_path: PathBuf,
    transaction_id: String,
    staging_set_sha256: String,
    target_set_sha256: String,
    entries: Vec<PreparedEntry>,
}

impl ExtraInstallPlan {
    fn report(&self) -> ExtraInstallReport {
        ExtraInstallReport {
            // A successful transaction deliberately retains its original
            // create-new journal as the active rollback record.  Unlike a
            // journal-to-manifest promotion, this has no later unlink window
            // where an independently-created active record could be removed.
            manifest_path: self.recovery_journal_path.clone(),
            groups: self.groups.clone(),
            staging_dir: self.staging_dir.clone(),
            target_dir: self.target_dir.clone(),
            staging_set_sha256: self.staging_set_sha256.clone(),
            target_set_sha256: self.target_set_sha256.clone(),
            entries: self
                .entries
                .iter()
                .map(install_entry_from_prepared)
                .collect(),
        }
    }

    fn manifest(&self) -> ExtraInstallManifest {
        ExtraInstallManifest {
            version: EXTRA_INSTALL_MANIFEST_VERSION,
            transaction_id: self.transaction_id.clone(),
            groups: self.groups.clone(),
            staging_dir: self.staging_dir.clone(),
            target_dir: self.target_dir.clone(),
            staging_set_sha256: self.staging_set_sha256.clone(),
            target_set_sha256: self.target_set_sha256.clone(),
            entries: self
                .entries
                .iter()
                .map(install_entry_from_prepared)
                .collect(),
        }
    }
}

fn install_entry_from_prepared(entry: &PreparedEntry) -> ExtraInstallEntry {
    ExtraInstallEntry {
        group: entry.group,
        component: entry.component.to_owned(),
        target: entry.target.clone(),
        temporary: entry.temporary.clone(),
        before_sha256: entry.previous.as_deref().map(sha256_hex),
        after_sha256: sha256_hex(&entry.staging_bytes),
        backup: entry.backup.clone(),
        target_previously_existed: entry.previous.is_some(),
    }
}

/// Return the controlled recovery journal location for an ExtData target
/// directory. A journal is only retained if an installation cannot prove that
/// every selected component reached its final state.
pub fn recovery_journal_path_for_target_dir(
    target_dir: impl AsRef<Path>,
) -> Result<PathBuf, ConversionError> {
    Ok(
        normalize_directory(target_dir.as_ref(), "target ExtData directory")?
            .join(EXTRA_RECOVERY_JOURNAL_NAME),
    )
}

/// Read and validate a complete staged group without creating a lock, backup,
/// temporary file, target update, or manifest.  It is the core CLI/UI dry-run
/// operation; install re-runs the same preflight while holding its directory
/// lock to recheck the fingerprints before mutation.
pub fn dry_run_extra_groups_with(
    staging_dir: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
    groups: &[ExtraGroup],
    expected_staging_set_sha256: Option<&str>,
    expected_target_set_sha256: Option<&str>,
    probe: &dyn ProcessProbe,
) -> Result<ExtraInstallReport, ConversionError> {
    let plan = prepare_extra_install(
        staging_dir.as_ref(),
        target_dir.as_ref(),
        groups,
        expected_staging_set_sha256,
        expected_target_set_sha256,
        probe,
    )?;
    validate_install_artifacts_absent(&plan)?;
    Ok(plan.report())
}

pub fn dry_run_extra_groups(
    staging_dir: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
    groups: &[ExtraGroup],
    expected_staging_set_sha256: Option<&str>,
    expected_target_set_sha256: Option<&str>,
) -> Result<ExtraInstallReport, ConversionError> {
    dry_run_extra_groups_with(
        staging_dir,
        target_dir,
        groups,
        expected_staging_set_sha256,
        expected_target_set_sha256,
        &PlatformProcessProbe::default(),
    )
}

pub fn install_extra_groups(
    staging_dir: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
    groups: &[ExtraGroup],
    expected_staging_set_sha256: Option<&str>,
    expected_target_set_sha256: Option<&str>,
) -> Result<ExtraInstallReport, ConversionError> {
    install_extra_groups_with(
        staging_dir,
        target_dir,
        groups,
        expected_staging_set_sha256,
        expected_target_set_sha256,
        &PlatformProcessProbe::default(),
        &StdExtraFileOperations,
    )
}

/// Install one or more complete ExtData groups as a compensated transaction.
/// All validation and optional dry-run fingerprints are rechecked before any
/// backup, temporary file, or target replacement is created.
pub fn install_extra_groups_with(
    staging_dir: impl AsRef<Path>,
    target_dir: impl AsRef<Path>,
    groups: &[ExtraGroup],
    expected_staging_set_sha256: Option<&str>,
    expected_target_set_sha256: Option<&str>,
    probe: &dyn ProcessProbe,
    operations: &dyn ExtraFileOperations,
) -> Result<ExtraInstallReport, ConversionError> {
    let target_dir = normalize_directory(target_dir.as_ref(), "target ExtData directory")?;
    let _lock = ExtraInstallLock::acquire(&target_dir)?;
    require_durable_extra_transaction_support()?;
    let plan = prepare_extra_install(
        staging_dir.as_ref(),
        &target_dir,
        groups,
        expected_staging_set_sha256,
        expected_target_set_sha256,
        probe,
    )?;
    validate_install_artifacts_absent(&plan)?;

    let mut backups_created = Vec::<OwnedArtifact>::new();
    let mut temporary_paths = vec![None; plan.entries.len()];
    let mut replacement_attempts = Vec::new();
    let mut recovery_journal = None::<OwnedArtifact>;
    let mut replacement_started = false;

    let result = (|| {
        // Persist a complete, rollback-valid journal before creating any
        // backup or temporary file.  A hard interruption at any later point
        // can therefore recover by inspecting before/after states.
        let recovery_bytes = serde_json::to_vec_pretty(&plan.manifest())?;
        // This must have create-new semantics: a second writer that reaches
        // this path after preflight owns its journal, not us.
        operations.write_new_file(&plan.recovery_journal_path, &recovery_bytes)?;
        recovery_journal = Some(capture_owned_regular_file(
            &plan.recovery_journal_path,
            &recovery_bytes,
            "capturing ExtData recovery journal",
        )?);
        operations.sync_directory(&plan.target_dir)?;

        for entry in &plan.entries {
            if let (Some(backup), Some(previous)) = (&entry.backup, entry.previous.as_deref()) {
                operations.write_new_file(backup, previous)?;
                let staged_backup = read_regular_file(backup, "reading staged ExtData backup")?;
                if staged_backup != previous {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "staged ExtData backup does not match its target snapshot: {}",
                        backup.display()
                    )));
                }
                backups_created.push(capture_owned_regular_file(
                    backup,
                    previous,
                    "capturing staged ExtData backup",
                )?);
            }
        }

        for (index, entry) in plan.entries.iter().enumerate() {
            operations.write_new_file(&entry.temporary, &entry.staging_bytes)?;
            temporary_paths[index] = Some(capture_owned_regular_file(
                &entry.temporary,
                &entry.staging_bytes,
                "capturing staged ExtData component",
            )?);
        }

        for (index, _) in plan.entries.iter().enumerate() {
            let temporary = temporary_paths[index]
                .as_ref()
                .expect("temporary path is recorded immediately after creation");
            validate_owned_regular_file(temporary, "reading staged ExtData component")?;
        }

        // The journal, every backup, and every staged replacement are now
        // durable before the first target name can be changed.
        operations.sync_directory(&plan.target_dir)?;

        for (index, entry) in plan.entries.iter().enumerate() {
            let temporary = temporary_paths[index]
                .as_ref()
                .expect("temporary survives until replacement");
            revalidate_before_replace(&plan, index, temporary, probe)?;
            let previous = entry
                .previous
                .as_deref()
                .expect("new ExtData installation requires initialized targets");
            // Once the platform exchange primitive begins, its commit state is
            // deliberately treated as uncertain on any error.  Do not run
            // ordinary compensation after that point: a non-cooperating
            // writer may have occupied either pathname between validations.
            replacement_started = true;
            operations.replace_staged(
                &temporary.path,
                &entry.target,
                &entry.staging_bytes,
                previous,
            )?;
            replacement_attempts.push(index);
            // An atomic exchange leaves the former target at the controlled
            // temporary path.  Retain its identity for compensated failure
            // handling and for a later explicit rollback.
            temporary_paths[index] = Some(capture_owned_regular_file(
                &entry.temporary,
                previous,
                "capturing exchanged ExtData target snapshot",
            )?);
            operations.sync_directory(&plan.target_dir)?;
        }

        revalidate_fully_installed_plan(&plan, probe)?;
        validate_owned_regular_file(
            recovery_journal
                .as_ref()
                .expect("journal is captured before any transaction artifact"),
            "rechecking retained ExtData recovery journal after installation",
        )?;
        operations.sync_directory(&plan.target_dir)?;
        Ok(plan.report())
    })();

    match result {
        Ok(report) => Ok(report),
        Err(install_error) => {
            if replacement_started {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData group installation reached an atomic target exchange but did not finish cleanly: {install_error}; stop the emulator and roll back the retained recovery journal"
                )));
            }
            let cleanup_errors = cleanup_failed_install(
                &plan,
                operations,
                probe,
                &mut temporary_paths,
                &replacement_attempts,
                &backups_created,
                recovery_journal.as_ref(),
            );
            if cleanup_errors.is_empty() {
                Err(install_error)
            } else {
                Err(ConversionError::UnsafeInstall(format!(
                    "ExtData group installation failed: {install_error}; cleanup also failed: {}",
                    cleanup_errors.join("; ")
                )))
            }
        }
    }
}

fn revalidate_before_replace(
    plan: &ExtraInstallPlan,
    completed_replacements: usize,
    temporary: &OwnedArtifact,
    probe: &dyn ProcessProbe,
) -> Result<(), ConversionError> {
    revalidate_plan_state(plan, completed_replacements, probe)?;

    let entry = &plan.entries[completed_replacements];
    if temporary.bytes != entry.staging_bytes {
        return Err(ConversionError::UnsafeInstall(format!(
            "staged ExtData component changed before replacement: {}",
            temporary.path.display()
        )));
    }
    validate_owned_regular_file(
        temporary,
        "rechecking staged ExtData component before replacement",
    )?;
    // Keep the process probe as close as possible to the destructive rename.
    reject_running_emulator(probe)?;
    Ok(())
}

fn revalidate_plan_state(
    plan: &ExtraInstallPlan,
    completed_replacements: usize,
    probe: &dyn ProcessProbe,
) -> Result<(), ConversionError> {
    reject_running_emulator(probe)?;

    for (index, entry) in plan.entries.iter().enumerate() {
        let path = plan.staging_dir.join(entry.component);
        let bytes = read_required_staged_component(&path)?;
        validate_cemu_external_component_named(&bytes, entry.component)?;
        if bytes != entry.staging_bytes {
            return Err(ConversionError::UnsafeInstall(format!(
                "staged ExtData component changed after planning: {}",
                path.display()
            )));
        }

        let bytes = read_optional_regular_file(
            &entry.target,
            "rechecking target ExtData component before replacement",
        )?;
        if let Some(bytes) = bytes.as_deref() {
            validate_cemu_external_component_named(bytes, entry.component)?;
        }
        let expected = if index < completed_replacements {
            Some(entry.staging_bytes.as_slice())
        } else {
            entry.previous.as_deref()
        };
        if bytes.as_deref() != expected {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData component changed after planning or a prior replacement: {}",
                entry.target.display()
            )));
        }
    }
    reject_running_emulator(probe)?;
    Ok(())
}

fn revalidate_fully_installed_plan(
    plan: &ExtraInstallPlan,
    probe: &dyn ProcessProbe,
) -> Result<(), ConversionError> {
    revalidate_plan_state(plan, plan.entries.len(), probe)
}

pub fn rollback_extra_groups(manifest_path: impl AsRef<Path>) -> Result<(), ConversionError> {
    rollback_extra_groups_with(
        manifest_path,
        &PlatformProcessProbe::default(),
        &StdExtraFileOperations,
    )
}

/// Roll back a complete ExtData manifest.  Every entry is validated before a
/// target changes, and backups/manifest are only consumed after all targets
/// are restored and re-verified.
pub fn rollback_extra_groups_with(
    manifest_path: impl AsRef<Path>,
    probe: &dyn ProcessProbe,
    operations: &dyn ExtraFileOperations,
) -> Result<(), ConversionError> {
    reject_running_emulator(probe)?;
    let (manifest_path, target_dir) = normalize_manifest_path(manifest_path.as_ref())?;
    let _lock = ExtraInstallLock::acquire(&target_dir)?;
    require_durable_extra_transaction_support()?;
    reject_running_emulator(probe)?;
    let manifest_bytes = read_regular_file(&manifest_path, "reading ExtData rollback manifest")?;
    let manifest: ExtraInstallManifest = serde_json::from_slice(&manifest_bytes)?;
    let entries = validate_rollback_manifest(&manifest, &manifest_path, &target_dir)?;
    let companion = matching_transaction_record(&manifest_path, &target_dir, &manifest)?;
    let mut transaction_records = vec![TransactionArtifact {
        path: manifest_path.clone(),
        bytes: manifest_bytes.clone(),
    }];
    if let Some(companion) = companion {
        transaction_records.push(companion);
    }

    let rollback_states = prepare_rollback_states(&entries)?;
    for state in &rollback_states {
        if !state.needs_restore {
            continue;
        }
        reject_running_emulator(probe)?;
        let current = read_regular_file(
            &state.entry.entry.target,
            "rechecking ExtData rollback target hash before restore",
        )?;
        validate_cemu_external_component_named(&current, &state.entry.entry.component)?;
        if sha256_hex(&current) != state.entry.entry.after_sha256 {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData rollback target changed after preflight: {}",
                state.entry.entry.target.display()
            )));
        }
        let previous = state
            .entry
            .previous
            .as_deref()
            .expect("validated ExtData rollback manifest requires initialized targets");
        let temporary = read_regular_file(
            &state.entry.entry.temporary,
            "reading exchanged ExtData target snapshot before rollback",
        )?;
        if temporary != previous {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData rollback temporary does not hold the pre-install target: {}",
                state.entry.entry.temporary.display()
            )));
        }
        let result = operations.restore_target(
            &state.entry.entry.temporary,
            &state.entry.entry.target,
            previous,
            state
                .current
                .as_deref()
                .expect("rollback state requiring restore has an installed target"),
        );
        if let Err(error) = result {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData rollback reached an atomic target exchange but did not finish cleanly at {}: {error}; retain the recovery journal and run rollback again only after resolving the conflict",
                state.entry.entry.target.display(),
            )));
        }
    }
    operations.sync_directory(&target_dir)?;
    verify_restored_targets(&entries)?;
    remove_recovery_temporaries(&entries, probe, operations)?;

    let backups = rollback_states
        .iter()
        .filter_map(|state| {
            state
                .entry
                .entry
                .backup
                .as_ref()
                .zip(state.entry.previous.as_ref())
                .map(|(backup, previous)| (backup.clone(), previous.clone()))
        })
        .collect::<Vec<(PathBuf, Vec<u8>)>>();
    let mut removed_backups = Vec::new();
    for (backup, _) in &backups {
        reject_running_emulator(probe)?;
        let expected = backups
            .iter()
            .find(|(candidate, _)| candidate == backup)
            .map(|(_, bytes)| bytes.as_slice())
            .expect("backup must be present in rollback snapshot");
        if let Err(error) = remove_owned_regular_file(
            backup,
            expected,
            "removing consumed ExtData rollback backup",
            operations,
        ) {
            let mut potentially_removed = removed_backups.clone();
            potentially_removed.push(backup.clone());
            let recovery_errors = restore_consumed_artifacts(
                &backups,
                &potentially_removed,
                &transaction_records,
                operations,
            );
            return Err(consumption_error("backup", error, recovery_errors));
        }
        removed_backups.push(backup.clone());
    }
    for record in &transaction_records {
        reject_running_emulator(probe)?;
        if let Err(error) = remove_owned_regular_file(
            &record.path,
            &record.bytes,
            "removing consumed ExtData transaction record",
            operations,
        ) {
            let recovery_errors = restore_consumed_artifacts(
                &backups,
                &removed_backups,
                &transaction_records,
                operations,
            );
            return Err(consumption_error(
                "transaction record",
                error,
                recovery_errors,
            ));
        }
    }
    if let Err(error) = operations.sync_directory(&target_dir) {
        let recovery_errors = restore_consumed_artifacts(
            &backups,
            &removed_backups,
            &transaction_records,
            operations,
        );
        return Err(consumption_error("directory sync", error, recovery_errors));
    }
    Ok(())
}

fn prepare_extra_install(
    staging_dir: &Path,
    target_dir: &Path,
    groups: &[ExtraGroup],
    expected_staging_set_sha256: Option<&str>,
    expected_target_set_sha256: Option<&str>,
    probe: &dyn ProcessProbe,
) -> Result<ExtraInstallPlan, ConversionError> {
    let staging_dir = normalize_directory(staging_dir, "staged ExtData directory")?;
    let target_dir = normalize_directory(target_dir, "target ExtData directory")?;
    if staging_dir == target_dir {
        return Err(ConversionError::InvalidSave(
            "staged and target ExtData directories alias the same directory".to_owned(),
        ));
    }
    reject_running_emulator(probe)?;
    let groups = normalize_groups(groups)?;
    let manifest_path = target_dir.join(EXTRA_MANIFEST_NAME);
    let recovery_journal_path = target_dir.join(EXTRA_RECOVERY_JOURNAL_NAME);
    let transaction_id = Uuid::new_v4().hyphenated().to_string();
    let mut entries = selected_components(&groups)
        .into_iter()
        .map(|(group, component)| {
            let staging_path = staging_dir.join(component);
            let staging_bytes = read_required_staged_component(&staging_path)?;
            validate_cemu_external_component_named(&staging_bytes, component)?;

            let target = target_dir.join(component);
            let previous = read_optional_regular_file(&target, "reading target ExtData component")?
                .ok_or_else(|| {
                    ConversionError::InvalidSave(format!(
                        "target ExtData component is missing; initialize the Wii U/Cemu save first: {}",
                        target.display()
                    ))
                })?;
            validate_cemu_external_component_named(&previous, component)?;
            if files_alias(&staging_path, &target)? {
                return Err(ConversionError::InvalidSave(format!(
                    "staged and target ExtData component paths alias: {}",
                    component
                )));
            }
            let backup = Some(controlled_backup_path(
                &target_dir,
                component,
                &sha256_hex(&previous),
            )?);
            let temporary = controlled_temporary_path(&target_dir, component, &transaction_id)?;
            Ok(PreparedEntry {
                group,
                component,
                staging_bytes,
                target,
                temporary,
                previous: Some(previous),
                backup,
            })
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    entries.sort_by_key(|entry| entry.component);

    let staging_set_sha256 = set_sha256(
        entries
            .iter()
            .map(|entry| (entry.component, Some(sha256_hex(&entry.staging_bytes)))),
    );
    let target_set_sha256 = set_sha256(
        entries
            .iter()
            .map(|entry| (entry.component, entry.previous.as_deref().map(sha256_hex))),
    );
    verify_expected_set_hash(expected_staging_set_sha256, &staging_set_sha256, "staging")?;
    verify_expected_set_hash(expected_target_set_sha256, &target_set_sha256, "target")?;

    Ok(ExtraInstallPlan {
        staging_dir,
        target_dir,
        groups,
        manifest_path,
        recovery_journal_path,
        transaction_id,
        staging_set_sha256,
        target_set_sha256,
        entries,
    })
}

fn normalize_groups(groups: &[ExtraGroup]) -> Result<Vec<ExtraGroup>, ConversionError> {
    if groups.is_empty() {
        return Err(ConversionError::InvalidSave(
            "at least one complete ExtData group must be selected".to_owned(),
        ));
    }
    let mut groups = groups.to_vec();
    groups.sort_unstable();
    if groups.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ConversionError::InvalidSave(
            "duplicate ExtData group selection is not allowed".to_owned(),
        ));
    }
    Ok(groups)
}

fn selected_components(groups: &[ExtraGroup]) -> Vec<(ExtraGroup, &'static str)> {
    let mut components = groups
        .iter()
        .flat_map(|group| {
            group
                .components()
                .iter()
                .map(move |component| (*group, *component))
        })
        .collect::<Vec<_>>();
    components.sort_by_key(|(_, component)| *component);
    components
}

fn validate_install_artifacts_absent(plan: &ExtraInstallPlan) -> Result<(), ConversionError> {
    reject_existing_path(&plan.manifest_path, "ExtData install manifest")?;
    reject_existing_path(&plan.recovery_journal_path, "ExtData recovery journal")?;
    for entry in &plan.entries {
        if let Some(backup) = entry.backup.as_deref() {
            reject_existing_path(backup, "ExtData backup")?;
        }
        reject_existing_path(&entry.temporary, "ExtData staged component")?;
    }
    Ok(())
}

fn reject_existing_path(path: &Path, label: &str) -> Result<(), ConversionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(ConversionError::UnsafeInstall(format!(
            "{label} already exists: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => io_at_path(Err(error), "reading ExtData transaction artifact", path),
    }
}

fn cleanup_failed_install(
    plan: &ExtraInstallPlan,
    operations: &dyn ExtraFileOperations,
    probe: &dyn ProcessProbe,
    temporary_paths: &mut [Option<OwnedArtifact>],
    replacement_attempts: &[usize],
    backups_created: &[OwnedArtifact],
    recovery_journal: Option<&OwnedArtifact>,
) -> Vec<String> {
    let mut errors = Vec::new();
    if !cleanup_process_gate(probe, &mut errors) {
        return errors;
    }

    for index in replacement_attempts.iter().rev().copied() {
        let entry = &plan.entries[index];
        if !cleanup_process_gate(probe, &mut errors) {
            return errors;
        }
        if let Err(error) = require_current_bytes(
            &entry.target,
            Some(entry.staging_bytes.as_slice()),
            "rechecking ExtData target before failed-install compensation",
        ) {
            errors.push(format!(
                "retain ExtData recovery journal because target is no longer owned by this transaction: {error}"
            ));
            return errors;
        }
        let previous = entry
            .previous
            .as_deref()
            .expect("new ExtData installation requires initialized targets");
        let temporary = temporary_paths[index]
            .as_ref()
            .expect("successful ExtData exchange retains its controlled temporary");
        if let Err(error) = validate_owned_regular_file(
            temporary,
            "rechecking exchanged ExtData target before failed-install compensation",
        ) {
            errors.push(format!(
                "retain ExtData recovery journal because exchanged target snapshot changed externally: {error}"
            ));
            return errors;
        }
        let result = operations.restore_target(
            &temporary.path,
            &entry.target,
            previous,
            &entry.staging_bytes,
        );
        if let Err(error) = result {
            errors.push(format!("restore prior ExtData component: {error}"));
            return errors;
        }
        temporary_paths[index] = match capture_owned_regular_file(
            &entry.temporary,
            &entry.staging_bytes,
            "capturing compensated ExtData staged component",
        ) {
            Ok(temporary) => Some(temporary),
            Err(error) => {
                errors.push(format!(
                    "retain ExtData recovery journal because compensated staged component changed externally: {error}"
                ));
                return errors;
            }
        };
    }
    if !cleanup_process_gate(probe, &mut errors) {
        return errors;
    }
    if let Err(error) = operations.sync_directory(&plan.target_dir) {
        errors.push(format!(
            "sync restored ExtData transaction directory: {error}"
        ));
        return errors;
    }

    for temporary in temporary_paths.iter().flatten() {
        if !cleanup_process_gate(probe, &mut errors) {
            return errors;
        }
        if let Err(error) = remove_owned_artifact(
            temporary,
            "removing staged ExtData component after failed installation",
            operations,
        ) {
            errors.push(format!("remove staged ExtData component: {error}"));
            return errors;
        }
    }
    for backup in backups_created {
        if !cleanup_process_gate(probe, &mut errors) {
            return errors;
        }
        if let Err(error) = remove_owned_artifact(
            backup,
            "removing ExtData backup after failed installation",
            operations,
        ) {
            errors.push(format!("remove new ExtData backup: {error}"));
            return errors;
        }
    }
    if let Some(recovery_journal) = recovery_journal {
        if !cleanup_process_gate(probe, &mut errors) {
            return errors;
        }
        if let Err(error) = remove_owned_artifact(
            recovery_journal,
            "removing ExtData recovery journal after failed installation",
            operations,
        ) {
            errors.push(format!("remove ExtData recovery journal: {error}"));
            return errors;
        }
    }
    record_cleanup(
        &mut errors,
        "sync ExtData transaction directory",
        operations.sync_directory(&plan.target_dir),
    );
    errors
}

fn cleanup_process_gate(probe: &dyn ProcessProbe, errors: &mut Vec<String>) -> bool {
    match reject_running_emulator(probe) {
        Ok(()) => true,
        Err(error) => {
            errors.push(format!(
                "retain ExtData recovery material because failed-install compensation is unsafe: {error}"
            ));
            false
        }
    }
}

fn require_current_bytes(
    path: &Path,
    expected: Option<&[u8]>,
    operation: &'static str,
) -> Result<(), ConversionError> {
    let actual = read_optional_regular_file(path, operation)?;
    if actual.as_deref() == expected {
        Ok(())
    } else {
        Err(ConversionError::UnsafeInstall(format!(
            "ExtData target changed outside this transaction: {}",
            path.display()
        )))
    }
}

fn capture_owned_regular_file(
    path: &Path,
    expected: &[u8],
    operation: &'static str,
) -> Result<OwnedArtifact, ConversionError> {
    let bytes = read_regular_file(path, operation)?;
    if bytes != expected {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction artifact has unexpected bytes: {}",
            path.display()
        )));
    }
    Ok(OwnedArtifact {
        path: path.to_path_buf(),
        bytes,
        identity: regular_file_identity(path, operation)?,
    })
}

fn validate_owned_regular_file(
    artifact: &OwnedArtifact,
    operation: &'static str,
) -> Result<(), ConversionError> {
    validate_regular_file_bytes(
        &artifact.path,
        &artifact.bytes,
        Some(&artifact.identity),
        operation,
    )
}

fn remove_owned_artifact<O: ExtraFileOperations + ?Sized>(
    artifact: &OwnedArtifact,
    operation: &'static str,
    operations: &O,
) -> Result<(), ConversionError> {
    match fs::symlink_metadata(&artifact.path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => {
            validate_owned_regular_file(artifact, operation)?;
            operations.remove_regular_file(&artifact.path)
        }
        Err(error) => io_at_path(Err(error), operation, &artifact.path),
    }
}

fn remove_owned_regular_file(
    path: &Path,
    expected: &[u8],
    operation: &'static str,
    operations: &dyn ExtraFileOperations,
) -> Result<(), ConversionError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => {
            validate_regular_file_bytes(path, expected, None, operation)?;
            operations.remove_regular_file(path)
        }
        Err(error) => io_at_path(Err(error), operation, path),
    }
}

fn validate_regular_file_bytes(
    path: &Path,
    expected: &[u8],
    expected_identity: Option<&FileIdentity>,
    operation: &'static str,
) -> Result<(), ConversionError> {
    let before_identity = regular_file_identity(path, operation)?;
    if expected_identity.is_some_and(|identity| identity != &before_identity) {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction artifact identity changed: {}",
            path.display()
        )));
    }
    let actual = read_regular_file(path, operation)?;
    if actual != expected {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction artifact has unexpected bytes: {}",
            path.display()
        )));
    }
    let after_identity = regular_file_identity(path, operation)?;
    if before_identity != after_identity
        || expected_identity.is_some_and(|identity| identity != &after_identity)
    {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction artifact identity changed while being read: {}",
            path.display()
        )));
    }
    Ok(())
}

fn record_cleanup(errors: &mut Vec<String>, step: &str, result: Result<(), ConversionError>) {
    if let Err(error) = result {
        errors.push(format!("{step}: {error}"));
    }
}

fn normalize_directory(path: &Path, label: &str) -> Result<PathBuf, ConversionError> {
    let metadata = io_at_path(
        fs::symlink_metadata(path),
        "reading ExtData directory metadata",
        path,
    )?;
    if !metadata.file_type().is_dir() {
        return Err(ConversionError::InvalidSave(format!(
            "{label} must be a real directory, not a symlink or file: {}",
            path.display()
        )));
    }
    io_at_path(fs::canonicalize(path), "resolving ExtData directory", path)
}

fn read_regular_file(path: &Path, operation: &'static str) -> Result<Vec<u8>, ConversionError> {
    let initial_path_metadata = io_at_path(fs::symlink_metadata(path), operation, path)?;
    if !initial_path_metadata.file_type().is_file() {
        return Err(ConversionError::InvalidSave(format!(
            "ExtData component must be a regular non-symlink file: {}",
            path.display()
        )));
    }
    let mut file = open_regular_file_no_follow(path, operation)?;
    let opened_metadata = io_at_path(file.metadata(), operation, path)?;
    let opened_path_metadata = io_at_path(fs::symlink_metadata(path), operation, path)?;
    if !opened_metadata.file_type().is_file()
        || !opened_path_metadata.file_type().is_file()
        || !same_file_identity(&opened_metadata, &opened_path_metadata)
    {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData component changed while opening it: {}",
            path.display()
        )));
    }

    let mut bytes = Vec::with_capacity(opened_metadata.len().try_into().unwrap_or(0));
    io_at_path(file.read_to_end(&mut bytes), operation, path)?;
    let final_path_metadata = io_at_path(fs::symlink_metadata(path), operation, path)?;
    if !final_path_metadata.file_type().is_file()
        || !same_file_identity(&opened_metadata, &final_path_metadata)
    {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData component changed while reading it: {}",
            path.display()
        )));
    }
    Ok(bytes)
}

fn read_required_staged_component(path: &Path) -> Result<Vec<u8>, ConversionError> {
    match read_regular_file(path, "reading staged ExtData component") {
        Ok(bytes) => Ok(bytes),
        Err(error) if is_not_found_error(&error) => Err(ConversionError::InvalidSave(format!(
            "selected ExtData group is incomplete; missing staged component: {}",
            path.display()
        ))),
        Err(error) => Err(error),
    }
}

fn read_optional_regular_file(
    path: &Path,
    operation: &'static str,
) -> Result<Option<Vec<u8>>, ConversionError> {
    match read_regular_file(path, operation) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if is_not_found_error(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn open_regular_file_no_follow(
    path: &Path,
    operation: &'static str,
) -> Result<File, ConversionError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        // Ask CreateFileW to open a reparse point itself, never its target.
        // The regular-file checks immediately after open then reject it.
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    io_at_path(options.open(path), operation, path)
}

fn is_not_found_error(error: &ConversionError) -> bool {
    matches!(error, ConversionError::IoAtPath { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

#[cfg(unix)]
fn same_file_identity(opened: &fs::Metadata, path: &fs::Metadata) -> bool {
    file_identity(opened) == file_identity(path)
}

#[cfg(not(unix))]
fn same_file_identity(opened: &fs::Metadata, path: &fs::Metadata) -> bool {
    file_identity(opened) == file_identity(path)
}

fn regular_file_identity(
    path: &Path,
    operation: &'static str,
) -> Result<FileIdentity, ConversionError> {
    let metadata = io_at_path(fs::symlink_metadata(path), operation, path)?;
    if !metadata.file_type().is_file() {
        return Err(ConversionError::InvalidSave(format!(
            "ExtData transaction artifact must be a regular non-symlink file: {}",
            path.display()
        )));
    }
    Ok(file_identity(&metadata))
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt;

    (metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_nanos());
    (metadata.len(), modified)
}

fn files_alias(staging: &Path, target: &Path) -> Result<bool, ConversionError> {
    let staging_canonical = io_at_path(
        fs::canonicalize(staging),
        "resolving staged ExtData component identity",
        staging,
    )?;
    let target_canonical = io_at_path(
        fs::canonicalize(target),
        "resolving target ExtData component identity",
        target,
    )?;
    if staging_canonical == target_canonical {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let staging_metadata = io_at_path(
            fs::metadata(staging),
            "reading staged ExtData component identity",
            staging,
        )?;
        let target_metadata = io_at_path(
            fs::metadata(target),
            "reading target ExtData component identity",
            target,
        )?;
        Ok(staging_metadata.dev() == target_metadata.dev()
            && staging_metadata.ino() == target_metadata.ino())
    }
    #[cfg(not(unix))]
    {
        Ok(false)
    }
}

fn controlled_backup_path(
    target_dir: &Path,
    component: &str,
    previous_sha256: &str,
) -> Result<PathBuf, ConversionError> {
    validate_sha256(previous_sha256, "previous")?;
    Ok(target_dir.join(format!(".{component}.mh3g-extra-backup-{previous_sha256}")))
}

fn controlled_temporary_path(
    target_dir: &Path,
    component: &str,
    transaction_id: &str,
) -> Result<PathBuf, ConversionError> {
    validate_transaction_id(transaction_id)?;
    Ok(target_dir.join(format!(".{component}.mh3g-extra-tmp-{transaction_id}")))
}

fn validate_transaction_id(transaction_id: &str) -> Result<(), ConversionError> {
    let parsed = Uuid::parse_str(transaction_id).map_err(|_| {
        ConversionError::InvalidSave("ExtData transaction ID must be a UUID".to_owned())
    })?;
    if parsed.hyphenated().to_string() != transaction_id {
        return Err(ConversionError::InvalidSave(
            "ExtData transaction ID must use canonical hyphenated UUID form".to_owned(),
        ));
    }
    Ok(())
}

// Set fingerprints are SHA-256 over lexicographically sorted component names,
// each followed by NUL, its SHA-256 or the literal `missing`, and newline.
// This is deterministic and distinguishes an absent target from a zero file.
fn set_sha256<'a>(values: impl Iterator<Item = (&'a str, Option<String>)>) -> String {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by_key(|(component, _)| *component);
    let mut hasher = Sha256::new();
    for (component, hash) in values {
        hasher.update(component.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_deref().unwrap_or("missing").as_bytes());
        hasher.update([b'\n']);
    }
    hex::encode(hasher.finalize())
}

fn verify_expected_set_hash(
    expected: Option<&str>,
    observed: &str,
    label: &str,
) -> Result<(), ConversionError> {
    if let Some(expected) = expected {
        validate_sha256(expected, label)?;
        if expected != observed {
            return Err(ConversionError::UnsafeInstall(format!(
                "{label} ExtData component set changed after dry-run planning"
            )));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str, label: &str) -> Result<(), ConversionError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ConversionError::InvalidSave(format!(
            "{label} SHA-256 must be a 64-character hexadecimal digest"
        )))
    }
}

fn reject_running_emulator(probe: &dyn ProcessProbe) -> Result<(), ConversionError> {
    if let Some(name) = probe.matching_process()? {
        return Err(ConversionError::UnsafeInstall(format!(
            "emulator process is running: {name}"
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn require_durable_extra_transaction_support() -> Result<(), ConversionError> {
    // Win32 exposes no directory fsync equivalent. The primary save converter
    // remains available, but optional multi-file ExtData replacement must not
    // claim crash recovery without a durable directory metadata barrier.
    Err(ConversionError::UnsafeInstall(
        "multi-file ExtData installation is unavailable on Windows because durable directory metadata sync is unsupported"
            .to_owned(),
    ))
}

#[cfg(not(windows))]
fn require_durable_extra_transaction_support() -> Result<(), ConversionError> {
    Ok(())
}

fn normalize_manifest_path(manifest_path: &Path) -> Result<(PathBuf, PathBuf), ConversionError> {
    let filename = manifest_path.file_name().and_then(|name| name.to_str());
    if !matches!(
        filename,
        Some(EXTRA_MANIFEST_NAME | EXTRA_RECOVERY_JOURNAL_NAME)
    ) {
        return Err(ConversionError::InvalidSave(format!(
            "ExtData rollback manifest has an unexpected name: {}",
            manifest_path.display()
        )));
    }
    let parent = manifest_path.parent().ok_or_else(|| {
        ConversionError::InvalidSave(format!(
            "ExtData rollback manifest has no parent: {}",
            manifest_path.display()
        ))
    })?;
    let target_dir = normalize_directory(parent, "ExtData rollback target directory")?;
    Ok((
        target_dir.join(filename.expect("filename was validated above")),
        target_dir,
    ))
}

fn validate_rollback_manifest(
    manifest: &ExtraInstallManifest,
    manifest_path: &Path,
    target_dir: &Path,
) -> Result<Vec<RollbackEntry>, ConversionError> {
    if manifest.version != EXTRA_INSTALL_MANIFEST_VERSION {
        return Err(ConversionError::InvalidSave(format!(
            "unsupported ExtData install manifest version: {}",
            manifest.version
        )));
    }
    if !is_normalized_absolute(&manifest.staging_dir)
        || !is_normalized_absolute(&manifest.target_dir)
        || manifest.target_dir != target_dir
        || !matches!(
            manifest_path.file_name().and_then(|name| name.to_str()),
            Some(EXTRA_MANIFEST_NAME | EXTRA_RECOVERY_JOURNAL_NAME)
        )
    {
        return Err(ConversionError::InvalidSave(
            "ExtData manifest paths must be normalized absolute controlled paths".to_owned(),
        ));
    }
    validate_sha256(&manifest.staging_set_sha256, "manifest staging set")?;
    validate_sha256(&manifest.target_set_sha256, "manifest target set")?;
    validate_transaction_id(&manifest.transaction_id)?;
    validate_manifest_groups(&manifest.groups)?;

    let mut entry_groups = BTreeMap::<ExtraGroup, BTreeSet<String>>::new();
    let mut rollback_entries = Vec::with_capacity(manifest.entries.len());
    let mut last_component = None::<&str>;
    for entry in &manifest.entries {
        let expected_group = component_group(&entry.component).ok_or_else(|| {
            ConversionError::InvalidSave(format!(
                "ExtData manifest has unsupported component: {}",
                entry.component
            ))
        })?;
        if entry.group != expected_group {
            return Err(ConversionError::InvalidSave(format!(
                "ExtData manifest group does not match component: {}",
                entry.component
            )));
        }
        if last_component.is_some_and(|last| last >= entry.component.as_str()) {
            return Err(ConversionError::InvalidSave(
                "ExtData manifest entries must be sorted with no duplicates".to_owned(),
            ));
        }
        last_component = Some(&entry.component);
        if !is_normalized_absolute(&entry.target)
            || entry.target != target_dir.join(&entry.component)
        {
            return Err(ConversionError::InvalidSave(
                "ExtData manifest target is not bound to its target directory".to_owned(),
            ));
        }
        let expected_temporary =
            controlled_temporary_path(target_dir, &entry.component, &manifest.transaction_id)?;
        if !is_normalized_absolute(&entry.temporary) || entry.temporary != expected_temporary {
            return Err(ConversionError::InvalidSave(
                "ExtData manifest temporary is not the controlled transaction path".to_owned(),
            ));
        }
        validate_sha256(&entry.after_sha256, "manifest installed")?;
        match (
            entry.target_previously_existed,
            &entry.before_sha256,
            &entry.backup,
        ) {
            (true, Some(before_sha256), Some(backup)) => {
                validate_sha256(before_sha256, "manifest previous")?;
                let expected_backup =
                    controlled_backup_path(target_dir, &entry.component, before_sha256)?;
                if !is_normalized_absolute(backup) || backup != &expected_backup {
                    return Err(ConversionError::InvalidSave(
                        "ExtData manifest backup is not the controlled backup path".to_owned(),
                    ));
                }
            }
            (false, None, None) => {
                return Err(ConversionError::InvalidSave(
                    "ExtData rollback manifest contains a target that was not initialized before installation"
                        .to_owned(),
                ));
            }
            _ => {
                return Err(ConversionError::InvalidSave(
                    "ExtData manifest backup fields are inconsistent".to_owned(),
                ));
            }
        }
        entry_groups
            .entry(entry.group)
            .or_default()
            .insert(entry.component.clone());
        rollback_entries.push(RollbackEntry {
            entry: entry.clone(),
            previous: None,
        });
    }
    for (group, components) in &entry_groups {
        let expected = group
            .components()
            .iter()
            .map(|component| (*component).to_owned())
            .collect::<BTreeSet<_>>();
        if components != &expected {
            return Err(ConversionError::InvalidSave(format!(
                "ExtData manifest does not contain a complete {:?} group",
                group
            )));
        }
    }
    let complete_entry_groups = entry_groups.keys().copied().collect::<Vec<_>>();
    if manifest.groups != complete_entry_groups {
        return Err(ConversionError::InvalidSave(
            "ExtData manifest groups do not exactly match complete entries".to_owned(),
        ));
    }
    let observed_staging = set_sha256(
        manifest
            .entries
            .iter()
            .map(|entry| (entry.component.as_str(), Some(entry.after_sha256.clone()))),
    );
    let observed_target = set_sha256(
        manifest
            .entries
            .iter()
            .map(|entry| (entry.component.as_str(), entry.before_sha256.clone())),
    );
    if observed_staging != manifest.staging_set_sha256
        || observed_target != manifest.target_set_sha256
    {
        return Err(ConversionError::InvalidSave(
            "ExtData manifest set fingerprints do not match its entries".to_owned(),
        ));
    }
    Ok(rollback_entries)
}

fn validate_manifest_groups(groups: &[ExtraGroup]) -> Result<(), ConversionError> {
    if groups.is_empty() {
        return Err(ConversionError::InvalidSave(
            "ExtData manifest groups must not be empty".to_owned(),
        ));
    }
    if groups.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ConversionError::InvalidSave(
            "ExtData manifest groups must be unique and in canonical order".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct TransactionArtifact {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn matching_transaction_record(
    manifest_path: &Path,
    target_dir: &Path,
    manifest: &ExtraInstallManifest,
) -> Result<Option<TransactionArtifact>, ConversionError> {
    let companion_name = match manifest_path.file_name().and_then(|name| name.to_str()) {
        Some(EXTRA_MANIFEST_NAME) => EXTRA_RECOVERY_JOURNAL_NAME,
        Some(EXTRA_RECOVERY_JOURNAL_NAME) => EXTRA_MANIFEST_NAME,
        _ => unreachable!("normalized rollback manifest names are validated"),
    };
    let companion_path = target_dir.join(companion_name);
    let Some(bytes) = read_optional_regular_file(
        &companion_path,
        "reading companion ExtData transaction record",
    )?
    else {
        return Ok(None);
    };
    let companion: ExtraInstallManifest = serde_json::from_slice(&bytes).map_err(|error| {
        ConversionError::InvalidSave(format!(
            "companion ExtData transaction record is invalid: {error}"
        ))
    })?;
    validate_rollback_manifest(&companion, &companion_path, target_dir)?;
    if &companion != manifest {
        return Err(ConversionError::UnsafeInstall(
            "ExtData active manifest and recovery journal do not describe the same transaction"
                .to_owned(),
        ));
    }
    Ok(Some(TransactionArtifact {
        path: companion_path,
        bytes,
    }))
}

#[derive(Debug, Clone)]
struct RollbackEntry {
    entry: ExtraInstallEntry,
    previous: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct RollbackState {
    entry: RollbackEntry,
    current: Option<Vec<u8>>,
    needs_restore: bool,
}

fn prepare_rollback_states(
    entries: &[RollbackEntry],
) -> Result<Vec<RollbackState>, ConversionError> {
    entries
        .iter()
        .cloned()
        .map(|mut rollback_entry| {
            let entry = &rollback_entry.entry;
            let current =
                read_optional_regular_file(&entry.target, "reading ExtData rollback target")?;
            if let Some(current) = current.as_deref() {
                validate_cemu_external_component_named(current, &entry.component)?;
            }
            let current_sha256 = current.as_deref().map(sha256_hex);
            let (needs_restore, previous) = match (
                entry.before_sha256.as_deref(),
                entry.backup.as_deref(),
                current_sha256.as_deref(),
            ) {
                (Some(before_sha256), Some(backup), Some(current_sha256))
                    if current_sha256 == entry.after_sha256 =>
                {
                    (
                        true,
                        Some(read_and_validate_rollback_backup(
                            backup,
                            before_sha256,
                            &entry.component,
                        )?),
                    )
                }
                (Some(before_sha256), Some(backup), Some(current_sha256))
                    if current_sha256 == before_sha256 =>
                {
                    (
                        false,
                        read_optional_and_validate_rollback_backup(
                            backup,
                            before_sha256,
                            &entry.component,
                        )?,
                    )
                }
                (None, None, Some(current_sha256)) if current_sha256 == entry.after_sha256 => {
                    (true, None)
                }
                (None, None, None) => (false, None),
                _ => {
                    return Err(ConversionError::InvalidSave(format!(
                        "ExtData rollback target hash does not match manifest: {}",
                        entry.target.display()
                    )));
                }
            };
            rollback_entry.previous = previous;
            Ok(RollbackState {
                entry: rollback_entry,
                current,
                needs_restore,
            })
        })
        .collect()
}

fn read_and_validate_rollback_backup(
    backup: &Path,
    before_sha256: &str,
    component: &str,
) -> Result<Vec<u8>, ConversionError> {
    let backup_bytes = read_regular_file(backup, "reading ExtData rollback backup")?;
    validate_rollback_backup_bytes(&backup_bytes, backup, before_sha256, component)?;
    Ok(backup_bytes)
}

fn read_optional_and_validate_rollback_backup(
    backup: &Path,
    before_sha256: &str,
    component: &str,
) -> Result<Option<Vec<u8>>, ConversionError> {
    let backup_bytes = read_optional_regular_file(backup, "reading ExtData rollback backup")?;
    if let Some(backup_bytes) = backup_bytes.as_deref() {
        validate_rollback_backup_bytes(backup_bytes, backup, before_sha256, component)?;
    }
    Ok(backup_bytes)
}

fn validate_rollback_backup_bytes(
    backup_bytes: &[u8],
    backup: &Path,
    before_sha256: &str,
    component: &str,
) -> Result<(), ConversionError> {
    if sha256_hex(backup_bytes) != before_sha256 {
        return Err(ConversionError::InvalidSave(format!(
            "ExtData rollback backup hash does not match manifest: {}",
            backup.display()
        )));
    }
    validate_cemu_external_component_named(backup_bytes, component)
}

fn verify_restored_targets(entries: &[RollbackEntry]) -> Result<(), ConversionError> {
    for rollback_entry in entries {
        let entry = &rollback_entry.entry;
        let current =
            read_optional_regular_file(&entry.target, "verifying restored ExtData target")?;
        match (entry.before_sha256.as_deref(), current.as_deref()) {
            (Some(before_sha256), Some(bytes)) if sha256_hex(bytes) == before_sha256 => {}
            (None, None) => {}
            _ => {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData rollback did not restore expected target state: {}",
                    entry.target.display()
                )));
            }
        }
    }
    Ok(())
}

fn remove_recovery_temporaries(
    entries: &[RollbackEntry],
    probe: &dyn ProcessProbe,
    operations: &dyn ExtraFileOperations,
) -> Result<(), ConversionError> {
    for rollback_entry in entries {
        reject_running_emulator(probe)?;
        let entry = &rollback_entry.entry;
        let Some(bytes) = read_optional_regular_file(
            &entry.temporary,
            "reading ExtData recovery temporary file",
        )?
        else {
            continue;
        };
        let temporary = capture_owned_regular_file(
            &entry.temporary,
            &bytes,
            "capturing ExtData recovery temporary file",
        )?;
        let temporary_sha256 = sha256_hex(&temporary.bytes);
        let is_after = temporary_sha256 == entry.after_sha256;
        let is_before = entry
            .before_sha256
            .as_deref()
            .is_some_and(|before_sha256| temporary_sha256 == before_sha256);
        if !is_after && !is_before {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData recovery temporary file has unexpected bytes for {}: {}",
                entry.component,
                entry.temporary.display()
            )));
        }
        remove_owned_artifact(
            &temporary,
            "removing ExtData recovery temporary file",
            operations,
        )?;
    }
    Ok(())
}

fn restore_consumed_artifacts(
    backups: &[(PathBuf, Vec<u8>)],
    removed_backups: &[PathBuf],
    transaction_records: &[TransactionArtifact],
    operations: &dyn ExtraFileOperations,
) -> Vec<String> {
    let mut errors = Vec::new();
    for removed_backup in removed_backups {
        let restored = backups
            .iter()
            .find(|(backup, _)| backup == removed_backup)
            .ok_or_else(|| {
                ConversionError::UnsafeInstall(format!(
                    "missing cached ExtData backup bytes: {}",
                    removed_backup.display()
                ))
            })
            .and_then(|(backup, bytes)| restore_artifact_file(backup, bytes, operations));
        record_cleanup(&mut errors, "recreate consumed ExtData backup", restored);
    }
    for record in transaction_records {
        record_cleanup(
            &mut errors,
            "recreate consumed ExtData transaction record",
            restore_artifact_file(&record.path, &record.bytes, operations),
        );
    }
    errors
}

fn restore_artifact_file(
    path: &Path,
    expected: &[u8],
    operations: &dyn ExtraFileOperations,
) -> Result<(), ConversionError> {
    match read_optional_regular_file(path, "reading ExtData recovery artifact")? {
        None => operations.write_new_file(path, expected),
        Some(actual) if actual == expected => Ok(()),
        Some(_) => Err(ConversionError::UnsafeInstall(format!(
            "ExtData recovery artifact has unexpected bytes: {}",
            path.display()
        ))),
    }
}

fn consumption_error(
    phase: &str,
    error: ConversionError,
    recovery_errors: Vec<String>,
) -> ConversionError {
    if recovery_errors.is_empty() {
        ConversionError::UnsafeInstall(format!(
            "ExtData rollback could not consume {phase}: {error}; restored rollback metadata"
        ))
    } else {
        ConversionError::UnsafeInstall(format!(
            "ExtData rollback could not consume {phase}: {error}; recovery also failed: {}",
            recovery_errors.join("; ")
        ))
    }
}

fn component_group(component: &str) -> Option<ExtraGroup> {
    if GUILD_CARD_COMPONENTS.contains(&component) {
        Some(ExtraGroup::GuildCards)
    } else if QUEST_COMPONENTS.contains(&component) {
        Some(ExtraGroup::Quests)
    } else {
        None
    }
}

fn is_normalized_absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
}
