//! Atomic installation and rollback for complete MH3G ExtData component groups.
//!
//! The Cemu-side components are independently stored files, but game state
//! treats each group as one logical unit.  This module therefore never offers
//! a per-file install entry point.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ConversionError,
    converter::validate_cemu_external_component_named,
    io_at_path,
    process_probe::{PlatformProcessProbe, ProcessProbe},
    transaction::{
        atomic_replace, remove_if_regular_file, sha256_hex, sync_directory, unique_path,
        write_new_file,
    },
};

pub const EXTRA_INSTALL_MANIFEST_VERSION: u32 = 2;
const EXTRA_MANIFEST_NAME: &str = ".mh3g-extra-install.json";
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
    pub before_sha256: Option<String>,
    pub after_sha256: String,
    pub backup: Option<PathBuf>,
    pub target_previously_existed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtraInstallManifest {
    pub version: u32,
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
    fn replace_staged(&self, staged: &Path, target: &Path) -> Result<(), ConversionError>;
    fn restore_target(&self, target: &Path, bytes: &[u8]) -> Result<(), ConversionError>;
    fn remove_regular_file(&self, path: &Path) -> Result<(), ConversionError>;
    fn publish_manifest(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError>;
    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdExtraFileOperations;

impl ExtraFileOperations for StdExtraFileOperations {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        write_new_file(path, bytes)
    }

    fn replace_staged(&self, staged: &Path, target: &Path) -> Result<(), ConversionError> {
        io_at_path(
            fs::rename(staged, target),
            "replacing staged ExtData component",
            target,
        )
    }

    fn restore_target(&self, target: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        atomic_replace(target, bytes)
    }

    fn remove_regular_file(&self, path: &Path) -> Result<(), ConversionError> {
        remove_if_regular_file(path)
    }

    fn publish_manifest(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        let temporary = unique_path(path, "extra-manifest-tmp");
        match write_new_file(&temporary, bytes).and_then(|_| {
            io_at_path(
                fs::rename(&temporary, path),
                "publishing ExtData install manifest",
                path,
            )
        }) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(error)
            }
        }
    }

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        sync_directory(path)
    }
}

#[derive(Debug)]
struct ExtraInstallLock {
    path: PathBuf,
    _file: File,
}

impl ExtraInstallLock {
    fn acquire(target_dir: &Path) -> Result<Self, ConversionError> {
        let path = target_dir.join(EXTRA_LOCK_NAME);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData group installation is already locked: {}",
                    path.display()
                )));
            }
            Err(error) => return io_at_path(Err(error), "creating ExtData install lock", &path),
        };
        if let Err(error) =
            writeln!(file, "pid={}", std::process::id()).and_then(|_| file.sync_all())
        {
            drop(file);
            let _ = fs::remove_file(&path);
            return io_at_path(Err(error), "writing ExtData install lock", &path);
        }
        Ok(Self { path, _file: file })
    }
}

impl Drop for ExtraInstallLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone)]
struct PreparedEntry {
    group: ExtraGroup,
    component: &'static str,
    staging_bytes: Vec<u8>,
    target: PathBuf,
    previous: Option<Vec<u8>>,
    backup: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ExtraInstallPlan {
    staging_dir: PathBuf,
    target_dir: PathBuf,
    groups: Vec<ExtraGroup>,
    manifest_path: PathBuf,
    staging_set_sha256: String,
    target_set_sha256: String,
    entries: Vec<PreparedEntry>,
}

impl ExtraInstallPlan {
    fn report(&self) -> ExtraInstallReport {
        ExtraInstallReport {
            manifest_path: self.manifest_path.clone(),
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
        before_sha256: entry.previous.as_deref().map(sha256_hex),
        after_sha256: sha256_hex(&entry.staging_bytes),
        backup: entry.backup.clone(),
        target_previously_existed: entry.previous.is_some(),
    }
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
    let plan = prepare_extra_install(
        staging_dir.as_ref(),
        &target_dir,
        groups,
        expected_staging_set_sha256,
        expected_target_set_sha256,
        probe,
    )?;
    validate_install_artifacts_absent(&plan)?;

    let mut backups_created = Vec::new();
    let mut temporary_paths = vec![None; plan.entries.len()];
    let mut replacement_attempts = Vec::new();
    let mut manifest_publish_attempted = false;
    let mut manifest_created = false;

    let result = (|| {
        for entry in &plan.entries {
            if let (Some(backup), Some(previous)) = (&entry.backup, entry.previous.as_deref()) {
                backups_created.push(backup.clone());
                operations.write_new_file(backup, previous)?;
                let staged_backup = read_regular_file(backup, "reading staged ExtData backup")?;
                if staged_backup != previous {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "staged ExtData backup does not match its target snapshot: {}",
                        backup.display()
                    )));
                }
            }
        }

        for (index, entry) in plan.entries.iter().enumerate() {
            let temporary = unique_path(&entry.target, "extra-tmp");
            temporary_paths[index] = Some(temporary.clone());
            operations.write_new_file(&temporary, &entry.staging_bytes)?;
        }

        for (index, entry) in plan.entries.iter().enumerate() {
            let temporary = temporary_paths[index]
                .as_deref()
                .expect("temporary path is recorded immediately after creation");
            let staged = read_regular_file(temporary, "reading staged ExtData component")?;
            if staged != entry.staging_bytes {
                return Err(ConversionError::UnsafeInstall(format!(
                    "staged ExtData component does not match source bytes: {}",
                    temporary.display()
                )));
            }
            validate_cemu_external_component_named(&staged, entry.component)?;
        }

        for (index, entry) in plan.entries.iter().enumerate() {
            let temporary = temporary_paths[index]
                .as_deref()
                .expect("temporary survives until replacement");
            replacement_attempts.push(index);
            operations.replace_staged(temporary, &entry.target)?;
            temporary_paths[index] = None;
            operations.sync_directory(&plan.target_dir)?;
        }

        let manifest_bytes = serde_json::to_vec_pretty(&plan.manifest())?;
        manifest_publish_attempted = true;
        operations.publish_manifest(&plan.manifest_path, &manifest_bytes)?;
        manifest_created = true;
        operations.sync_directory(&plan.target_dir)?;
        Ok(plan.report())
    })();

    match result {
        Ok(report) => Ok(report),
        Err(install_error) => {
            let cleanup_errors = cleanup_failed_install(
                &plan,
                operations,
                &temporary_paths,
                &replacement_attempts,
                &backups_created,
                manifest_publish_attempted,
                manifest_created,
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
    reject_running_emulator(probe)?;
    let manifest_bytes = read_regular_file(&manifest_path, "reading ExtData rollback manifest")?;
    let manifest: ExtraInstallManifest = serde_json::from_slice(&manifest_bytes)?;
    let entries = validate_rollback_manifest(&manifest, &manifest_path, &target_dir)?;

    let rollback_states = prepare_rollback_states(&entries)?;
    let mut restored = Vec::new();
    for (index, state) in rollback_states.iter().enumerate() {
        if !state.needs_restore {
            continue;
        }
        restored.push(index);
        let result = match state.entry.previous.as_deref() {
            Some(previous) => operations.restore_target(&state.entry.entry.target, previous),
            None => operations.remove_regular_file(&state.entry.entry.target),
        };
        if let Err(error) = result {
            let compensation_errors = compensate_rollback(&rollback_states, &restored, operations);
            let detail = if compensation_errors.is_empty() {
                "all prior rollback changes were compensated".to_owned()
            } else {
                format!(
                    "compensation also failed: {}",
                    compensation_errors.join("; ")
                )
            };
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData rollback failed at {}: {error}; {detail}",
                state.entry.entry.target.display()
            )));
        }
    }
    operations.sync_directory(&target_dir)?;
    verify_restored_targets(&entries)?;

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
        if let Err(error) = operations.remove_regular_file(backup) {
            let mut potentially_removed = removed_backups.clone();
            potentially_removed.push(backup.clone());
            let recovery_errors =
                restore_consumed_artifacts(&backups, &potentially_removed, None, operations);
            return Err(consumption_error("backup", error, recovery_errors));
        }
        removed_backups.push(backup.clone());
    }
    if let Err(error) = operations.remove_regular_file(&manifest_path) {
        let recovery_errors = restore_consumed_artifacts(
            &backups,
            &removed_backups,
            Some((&manifest_path, &manifest_bytes)),
            operations,
        );
        return Err(consumption_error("manifest", error, recovery_errors));
    }
    if let Err(error) = operations.sync_directory(&target_dir) {
        let recovery_errors = restore_consumed_artifacts(
            &backups,
            &removed_backups,
            Some((&manifest_path, &manifest_bytes)),
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
    let mut entries = selected_components(&groups)
        .into_iter()
        .map(|(group, component)| {
            let staging_path = staging_dir.join(component);
            let staging_bytes = read_required_staged_component(&staging_path)?;
            validate_cemu_external_component_named(&staging_bytes, component)?;

            let target = target_dir.join(component);
            let previous = read_optional_regular_file(&target, "reading target ExtData component")?;
            if let Some(previous) = previous.as_deref() {
                validate_cemu_external_component_named(previous, component)?;
                if files_alias(&staging_path, &target)? {
                    return Err(ConversionError::InvalidSave(format!(
                        "staged and target ExtData component paths alias: {}",
                        component
                    )));
                }
            }
            let backup = previous
                .as_deref()
                .map(|bytes| controlled_backup_path(&target_dir, component, &sha256_hex(bytes)))
                .transpose()?;
            Ok(PreparedEntry {
                group,
                component,
                staging_bytes,
                target,
                previous,
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
    for entry in &plan.entries {
        if let Some(backup) = entry.backup.as_deref() {
            reject_existing_path(backup, "ExtData backup")?;
        }
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
    temporary_paths: &[Option<PathBuf>],
    replacement_attempts: &[usize],
    backups_created: &[PathBuf],
    manifest_publish_attempted: bool,
    manifest_created: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    for temporary in temporary_paths.iter().flatten() {
        record_cleanup(
            &mut errors,
            "remove staged ExtData component",
            operations.remove_regular_file(temporary),
        );
    }

    let mut targets_restored = true;
    for index in replacement_attempts.iter().rev().copied() {
        let entry = &plan.entries[index];
        let result = match entry.previous.as_deref() {
            Some(previous) => operations.restore_target(&entry.target, previous),
            None => operations.remove_regular_file(&entry.target),
        };
        if result.is_err() {
            targets_restored = false;
        }
        record_cleanup(&mut errors, "restore prior ExtData component", result);
    }

    if targets_restored {
        if manifest_publish_attempted {
            record_cleanup(
                &mut errors,
                "remove new ExtData manifest",
                operations.remove_regular_file(&plan.manifest_path),
            );
        }
        for backup in backups_created {
            record_cleanup(
                &mut errors,
                "remove new ExtData backup",
                operations.remove_regular_file(backup),
            );
        }
    } else if !manifest_created && manifest_publish_attempted {
        record_cleanup(
            &mut errors,
            "remove incomplete ExtData manifest",
            operations.remove_regular_file(&plan.manifest_path),
        );
    }
    record_cleanup(
        &mut errors,
        "sync ExtData transaction directory",
        operations.sync_directory(&plan.target_dir),
    );
    errors
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
    let metadata = io_at_path(fs::symlink_metadata(path), operation, path)?;
    if !metadata.file_type().is_file() {
        return Err(ConversionError::InvalidSave(format!(
            "ExtData component must be a regular non-symlink file: {}",
            path.display()
        )));
    }
    io_at_path(fs::read(path), operation, path)
}

fn read_required_staged_component(path: &Path) -> Result<Vec<u8>, ConversionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            io_at_path(fs::read(path), "reading staged ExtData component", path)
        }
        Ok(_) => Err(ConversionError::InvalidSave(format!(
            "staged ExtData component must be a regular non-symlink file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ConversionError::InvalidSave(format!(
                "selected ExtData group is incomplete; missing staged component: {}",
                path.display()
            )))
        }
        Err(error) => io_at_path(Err(error), "reading staged ExtData component", path),
    }
}

fn read_optional_regular_file(
    path: &Path,
    operation: &'static str,
) -> Result<Option<Vec<u8>>, ConversionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            io_at_path(fs::read(path), operation, path).map(Some)
        }
        Ok(_) => Err(ConversionError::InvalidSave(format!(
            "ExtData component must be a regular non-symlink file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => io_at_path(Err(error), operation, path),
    }
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

fn normalize_manifest_path(manifest_path: &Path) -> Result<(PathBuf, PathBuf), ConversionError> {
    if manifest_path.file_name().and_then(|name| name.to_str()) != Some(EXTRA_MANIFEST_NAME) {
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
    Ok((target_dir.join(EXTRA_MANIFEST_NAME), target_dir))
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
        || manifest_path != target_dir.join(EXTRA_MANIFEST_NAME)
    {
        return Err(ConversionError::InvalidSave(
            "ExtData manifest paths must be normalized absolute controlled paths".to_owned(),
        ));
    }
    validate_sha256(&manifest.staging_set_sha256, "manifest staging set")?;
    validate_sha256(&manifest.target_set_sha256, "manifest target set")?;
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
            (false, None, None) => {}
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

fn compensate_rollback(
    states: &[RollbackState],
    restored: &[usize],
    operations: &dyn ExtraFileOperations,
) -> Vec<String> {
    let mut errors = Vec::new();
    for index in restored.iter().rev().copied() {
        let state = &states[index];
        let Some(current) = state.current.as_deref() else {
            continue;
        };
        record_cleanup(
            &mut errors,
            "restore pre-rollback ExtData component",
            operations.restore_target(&state.entry.entry.target, current),
        );
    }
    errors
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

fn restore_consumed_artifacts(
    backups: &[(PathBuf, Vec<u8>)],
    removed_backups: &[PathBuf],
    manifest: Option<(&Path, &[u8])>,
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
    if let Some((manifest_path, manifest_bytes)) = manifest {
        record_cleanup(
            &mut errors,
            "recreate consumed ExtData manifest",
            restore_artifact_file(manifest_path, manifest_bytes, operations),
        );
    }
    errors
}

fn restore_artifact_file(
    path: &Path,
    expected: &[u8],
    operations: &dyn ExtraFileOperations,
) -> Result<(), ConversionError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            operations.write_new_file(path, expected)
        }
        Ok(metadata) if metadata.file_type().is_file() => {
            let actual = io_at_path(fs::read(path), "reading ExtData recovery artifact", path)?;
            if actual == expected {
                Ok(())
            } else {
                Err(ConversionError::UnsafeInstall(format!(
                    "ExtData recovery artifact has unexpected bytes: {}",
                    path.display()
                )))
            }
        }
        Ok(_) => Err(ConversionError::UnsafeInstall(format!(
            "ExtData recovery artifact is not a regular file: {}",
            path.display()
        ))),
        Err(error) => io_at_path(Err(error), "reading ExtData recovery artifact", path),
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
