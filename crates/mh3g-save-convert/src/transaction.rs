use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    ConversionError,
    converter::convert_source_to_cemu,
    profile::{SaveProfile, inspect_bytes, validate_save_component_path},
};

pub const INSTALL_MANIFEST_VERSION: u32 = 1;
#[cfg(any(target_os = "macos", test))]
const GUARDED_PROCESS_NAMES: [&str; 8] = [
    "Nemessix",
    "nemessix",
    "Azahar",
    "azahar",
    "Cemu",
    "cemu",
    "Cemu_release",
    "cemu_release",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallManifest {
    pub version: u32,
    pub source_sha256: String,
    pub installed_sha256: String,
    pub previous_sha256: Option<String>,
    pub target: PathBuf,
    pub backup: Option<PathBuf>,
    pub target_previously_existed: bool,
}

#[derive(Debug)]
struct ExistingInstallManifest {
    bytes: Vec<u8>,
    history_path: PathBuf,
}

pub trait ProcessProbe {
    fn matching_process(&self) -> Result<Option<String>, ConversionError>;
}

pub trait InstallValidator {
    fn validate(&self, bytes: &[u8]) -> Result<(), ConversionError>;
}

pub trait ManifestPublisher {
    fn publish(&self, path: &Path, manifest: &InstallManifest) -> Result<(), ConversionError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MacOsProcessProbe;

impl ProcessProbe for MacOsProcessProbe {
    fn matching_process(&self) -> Result<Option<String>, ConversionError> {
        #[cfg(not(target_os = "macos"))]
        {
            return Ok(None);
        }

        #[cfg(target_os = "macos")]
        {
            for name in GUARDED_PROCESS_NAMES {
                let status = Command::new("pgrep")
                    .args(["-x", name])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()?;
                match status.code() {
                    Some(0) => return Ok(Some(name.to_owned())),
                    Some(1) => {}
                    Some(code) => {
                        return Err(ConversionError::Io(std::io::Error::other(format!(
                            "pgrep -x {name} exited with status {code}"
                        ))));
                    }
                    None => {
                        return Err(ConversionError::Io(std::io::Error::other(format!(
                            "pgrep -x {name} terminated by signal"
                        ))));
                    }
                }
            }
            Ok(None)
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CemuSaveValidator;

impl InstallValidator for CemuSaveValidator {
    fn validate(&self, bytes: &[u8]) -> Result<(), ConversionError> {
        let inspection = inspect_bytes(bytes)?;
        if !matches!(
            inspection.profile,
            SaveProfile::JpCemu | SaveProfile::JpCemuSystem
        ) {
            return Err(ConversionError::InvalidSave(
                "installed bytes are not a Japanese MH3G Cemu save component".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct JsonManifestPublisher;

impl ManifestPublisher for JsonManifestPublisher {
    fn publish(&self, path: &Path, manifest: &InstallManifest) -> Result<(), ConversionError> {
        write_manifest(path, manifest)
    }
}

struct SlotInstallLock {
    path: PathBuf,
    _file: File,
}

impl SlotInstallLock {
    fn acquire(target: &Path) -> Result<Self, ConversionError> {
        let path = install_lock_path_for_normalized_target(target)?;
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(ConversionError::UnsafeInstall(format!(
                    "save slot installation is already locked: {}",
                    path.display()
                )));
            }
            Err(error) => return Err(error.into()),
        };
        if let Err(error) =
            writeln!(file, "pid={}", std::process::id()).and_then(|_| file.sync_all())
        {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(error.into());
        }
        Ok(Self { path, _file: file })
    }
}

impl Drop for SlotInstallLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn manifest_path_for_target(target: impl AsRef<Path>) -> Result<PathBuf, ConversionError> {
    let target = normalize_path(target.as_ref())?;
    validate_save_component_path(&target)?;
    manifest_path_for_normalized_target(&target)
}

pub fn install(
    source: &[u8],
    installed: &[u8],
    target: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
) -> Result<InstallManifest, ConversionError> {
    install_with_publisher(
        source,
        installed,
        target,
        manifest_path,
        &MacOsProcessProbe,
        &CemuSaveValidator,
        &JsonManifestPublisher,
    )
}

pub fn install_with(
    source: &[u8],
    installed: &[u8],
    target: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
    probe: &dyn ProcessProbe,
    validator: &dyn InstallValidator,
) -> Result<InstallManifest, ConversionError> {
    install_with_publisher(
        source,
        installed,
        target,
        manifest_path,
        probe,
        validator,
        &JsonManifestPublisher,
    )
}

pub fn install_with_publisher(
    source: &[u8],
    installed: &[u8],
    target: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
    probe: &dyn ProcessProbe,
    validator: &dyn InstallValidator,
    publisher: &dyn ManifestPublisher,
) -> Result<InstallManifest, ConversionError> {
    let (target, manifest_path) = normalize_bound_paths(target.as_ref(), manifest_path.as_ref())?;
    let _slot_lock = SlotInstallLock::acquire(&target)?;
    let (target, manifest_path) = validate_install_paths(&target, &manifest_path)?;
    let existing_manifest = existing_manifest_for_reinstall(&target, &manifest_path)?;

    if let Some(name) = probe.matching_process()? {
        return Err(ConversionError::UnsafeInstall(format!(
            "emulator process is running: {name}"
        )));
    }

    let filename = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ConversionError::InvalidSave("target filename is invalid".to_owned()))?;
    let expected = convert_source_to_cemu(source, filename)?;
    if expected != installed {
        return Err(ConversionError::InvalidSave(
            "installed bytes do not match the converted source save".to_owned(),
        ));
    }

    let source_sha256 = sha256_hex(source);
    let installed_sha256 = sha256_hex(installed);
    let target_previously_existed = target.exists();
    let previous = if target_previously_existed {
        Some(fs::read(&target)?)
    } else {
        None
    };
    let previous_sha256 = previous.as_deref().map(sha256_hex);
    let backup = previous_sha256
        .as_deref()
        .map(|hash| backup_path_for(&target, hash))
        .transpose()?;
    let temporary = unique_path(&target, "tmp");
    let mut backup_created = false;
    let mut target_installed = false;
    let mut manifest_publish_attempted = false;
    let mut history_created = false;

    let result = (|| {
        if let Some(existing) = existing_manifest.as_ref() {
            history_created = archive_existing_manifest(existing)?;
        }

        if let (Some(backup_path), Some(previous)) = (&backup, previous.as_deref()) {
            write_new_file(backup_path, previous)?;
            backup_created = true;
        }

        write_new_file(&temporary, installed)?;
        let staged = fs::read(&temporary)?;
        if staged != installed {
            return Err(ConversionError::InvalidSave(
                "staged save bytes do not match the requested installation".to_owned(),
            ));
        }
        validator.validate(&staged)?;
        fs::rename(&temporary, &target)?;
        target_installed = true;
        sync_directory(parent_dir(&target))?;

        let manifest = InstallManifest {
            version: INSTALL_MANIFEST_VERSION,
            source_sha256,
            installed_sha256,
            previous_sha256,
            target: target.clone(),
            backup: backup.clone(),
            target_previously_existed,
        };
        manifest_publish_attempted = true;
        publisher.publish(&manifest_path, &manifest)?;
        sync_directory(parent_dir(&manifest_path))?;
        Ok(manifest)
    })();

    match result {
        Ok(manifest) => Ok(manifest),
        Err(install_error) => {
            let mut cleanup_errors = Vec::new();
            let mut record_cleanup = |step: &str, result: Result<(), ConversionError>| {
                if let Err(error) = result {
                    cleanup_errors.push(format!("{step}: {error}"));
                }
            };

            record_cleanup("remove staged save", remove_if_regular_file(&temporary));
            if manifest_publish_attempted {
                record_cleanup(
                    "restore prior manifest",
                    restore_existing_manifest(&manifest_path, existing_manifest.as_ref()),
                );
            }
            if target_installed {
                let restore = if let Some(previous) = previous.as_deref() {
                    atomic_replace(&target, previous)
                } else {
                    remove_if_regular_file(&target)
                };
                record_cleanup("restore prior save", restore);
            }
            if backup_created {
                record_cleanup(
                    "remove new backup",
                    remove_if_regular_file(backup.as_ref().expect("backup was created")),
                );
            }
            if history_created {
                let history_path = &existing_manifest
                    .as_ref()
                    .expect("history requires an existing manifest")
                    .history_path;
                record_cleanup(
                    "remove new manifest history",
                    remove_if_regular_file(history_path),
                );
                record_cleanup(
                    "sync manifest history directory",
                    sync_directory(parent_dir(history_path)),
                );
            }

            if cleanup_errors.is_empty() {
                Err(install_error)
            } else {
                Err(ConversionError::UnsafeInstall(format!(
                    "installation failed: {install_error}; cleanup also failed: {}",
                    cleanup_errors.join("; ")
                )))
            }
        }
    }
}

pub fn rollback(manifest_path: impl AsRef<Path>) -> Result<(), ConversionError> {
    rollback_with(manifest_path, &MacOsProcessProbe)
}

pub fn rollback_with(
    manifest_path: impl AsRef<Path>,
    probe: &dyn ProcessProbe,
) -> Result<(), ConversionError> {
    if let Some(name) = probe.matching_process()? {
        return Err(ConversionError::UnsafeInstall(format!(
            "emulator process is running: {name}"
        )));
    }

    let manifest_path = normalize_path(manifest_path.as_ref())?;
    let manifest_metadata = fs::symlink_metadata(&manifest_path)?;
    if !manifest_metadata.file_type().is_file() {
        return Err(ConversionError::InvalidSave(
            "rollback manifest must be a regular file".to_owned(),
        ));
    }
    let manifest: InstallManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    if manifest.version != INSTALL_MANIFEST_VERSION {
        return Err(ConversionError::InvalidSave(format!(
            "unsupported install manifest version: {}",
            manifest.version
        )));
    }
    validate_manifest_hash(&manifest.source_sha256, "source")?;
    validate_manifest_hash(&manifest.installed_sha256, "installed")?;
    let (target, normalized_manifest_path) =
        validate_transaction_paths(&manifest.target, &manifest_path)?;
    if target != manifest.target || normalized_manifest_path != manifest_path {
        return Err(ConversionError::InvalidSave(
            "manifest paths must be normalized absolute paths".to_owned(),
        ));
    }
    let current = match fs::read(&target) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(ConversionError::InvalidSave(format!(
                "cannot read rollback target: {error}"
            )));
        }
    };
    let current_sha256 = current.as_deref().map(sha256_hex);

    match (
        &manifest.backup,
        &manifest.previous_sha256,
        manifest.target_previously_existed,
    ) {
        (Some(backup), Some(previous_sha256), true) => {
            validate_manifest_hash(previous_sha256, "previous")?;
            let expected_backup = backup_path_for(&target, previous_sha256)?;
            if backup != &expected_backup
                || backup == &target
                || backup == &manifest_path
                || is_managed_save_component_name(backup)
            {
                return Err(ConversionError::InvalidSave(
                    "rollback backup path is not the controlled backup path".to_owned(),
                ));
            }
            match current_sha256.as_deref() {
                Some(hash) if hash == manifest.installed_sha256 => {
                    let backup_metadata = fs::symlink_metadata(backup).map_err(|error| {
                        ConversionError::InvalidSave(format!(
                            "cannot read rollback backup metadata: {error}"
                        ))
                    })?;
                    if !backup_metadata.file_type().is_file() {
                        return Err(ConversionError::InvalidSave(
                            "rollback backup must be a regular non-symlink file".to_owned(),
                        ));
                    }
                    let previous = fs::read(backup).map_err(|error| {
                        ConversionError::InvalidSave(format!(
                            "cannot read rollback backup: {error}"
                        ))
                    })?;
                    if sha256_hex(&previous) != *previous_sha256 {
                        return Err(ConversionError::InvalidSave(
                            "rollback backup hash does not match manifest".to_owned(),
                        ));
                    }
                    atomic_replace(&target, &previous)?;
                    fs::remove_file(backup)?;
                }
                Some(hash) if hash == previous_sha256 => match fs::symlink_metadata(backup) {
                    Ok(metadata) if metadata.file_type().is_file() => {
                        let previous = fs::read(backup)?;
                        if sha256_hex(&previous) != *previous_sha256 {
                            return Err(ConversionError::InvalidSave(
                                "rollback backup hash does not match manifest".to_owned(),
                            ));
                        }
                        fs::remove_file(backup)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Ok(_) => {
                        return Err(ConversionError::InvalidSave(
                            "rollback backup must be a regular non-symlink file".to_owned(),
                        ));
                    }
                    Err(error) => return Err(error.into()),
                },
                _ => {
                    return Err(ConversionError::InvalidSave(
                        "rollback target hash does not match the manifest".to_owned(),
                    ));
                }
            }
        }
        (None, None, false) => match current_sha256.as_deref() {
            Some(hash) if hash == manifest.installed_sha256 => remove_if_regular_file(&target)?,
            None => {}
            _ => {
                return Err(ConversionError::InvalidSave(
                    "rollback target hash does not match the manifest".to_owned(),
                ));
            }
        },
        _ => {
            return Err(ConversionError::InvalidSave(
                "manifest backup fields are inconsistent".to_owned(),
            ));
        }
    }

    fs::remove_file(&manifest_path)?;
    sync_directory(parent_dir(&manifest_path))?;
    Ok(())
}

fn validate_install_paths(
    target: &Path,
    manifest_path: &Path,
) -> Result<(PathBuf, PathBuf), ConversionError> {
    let (target, manifest_path) = validate_transaction_paths(target, manifest_path)?;
    if is_managed_save_component_name(&manifest_path) {
        return Err(ConversionError::InvalidSave(
            "manifest path cannot use a save component basename".to_owned(),
        ));
    }
    Ok((target, manifest_path))
}

fn existing_manifest_for_reinstall(
    target: &Path,
    manifest_path: &Path,
) -> Result<Option<ExistingInstallManifest>, ConversionError> {
    let metadata = match fs::symlink_metadata(manifest_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Err(ConversionError::InvalidSave(
            "existing install manifest must be a regular non-symlink file".to_owned(),
        ));
    }

    let bytes = fs::read(manifest_path)?;
    let manifest: InstallManifest = serde_json::from_slice(&bytes).map_err(|error| {
        ConversionError::InvalidSave(format!("existing install manifest is invalid: {error}"))
    })?;
    validate_existing_manifest(target, manifest_path, &manifest)?;

    let history_path = install_history_path_for(target, &bytes)?;
    match fs::symlink_metadata(&history_path) {
        Ok(history_metadata) if history_metadata.file_type().is_file() => {
            if fs::read(&history_path)? != bytes {
                return Err(ConversionError::InvalidSave(
                    "existing install history would be overwritten".to_owned(),
                ));
            }
        }
        Ok(_) => {
            return Err(ConversionError::InvalidSave(
                "existing install history must be a regular non-symlink file".to_owned(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    Ok(Some(ExistingInstallManifest {
        bytes,
        history_path,
    }))
}

fn validate_existing_manifest(
    target: &Path,
    manifest_path: &Path,
    manifest: &InstallManifest,
) -> Result<(), ConversionError> {
    if manifest.version != INSTALL_MANIFEST_VERSION {
        return Err(ConversionError::InvalidSave(format!(
            "unsupported existing install manifest version: {}",
            manifest.version
        )));
    }
    validate_manifest_hash(&manifest.source_sha256, "source")?;
    validate_manifest_hash(&manifest.installed_sha256, "installed")?;
    if manifest.target != target {
        return Err(ConversionError::InvalidSave(
            "existing install manifest target is not bound to this save slot".to_owned(),
        ));
    }

    match (
        &manifest.backup,
        &manifest.previous_sha256,
        manifest.target_previously_existed,
    ) {
        (Some(backup), Some(previous_sha256), true) => {
            validate_manifest_hash(previous_sha256, "previous")?;
            let expected_backup = backup_path_for(target, previous_sha256)?;
            if backup != &expected_backup
                || backup == target
                || backup == manifest_path
                || is_managed_save_component_name(backup)
            {
                return Err(ConversionError::InvalidSave(
                    "existing install manifest backup is not the controlled backup path".to_owned(),
                ));
            }
            let backup_metadata = fs::symlink_metadata(backup).map_err(|error| {
                ConversionError::InvalidSave(format!(
                    "cannot read existing install backup metadata: {error}"
                ))
            })?;
            if !backup_metadata.file_type().is_file() {
                return Err(ConversionError::InvalidSave(
                    "existing install backup must be a regular non-symlink file".to_owned(),
                ));
            }
            if sha256_hex(&fs::read(backup)?) != *previous_sha256 {
                return Err(ConversionError::InvalidSave(
                    "existing install backup hash does not match its manifest".to_owned(),
                ));
            }
        }
        (None, None, false) => {}
        _ => {
            return Err(ConversionError::InvalidSave(
                "existing install manifest backup fields are inconsistent".to_owned(),
            ));
        }
    }
    Ok(())
}

fn archive_existing_manifest(existing: &ExistingInstallManifest) -> Result<bool, ConversionError> {
    match fs::symlink_metadata(&existing.history_path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            if fs::read(&existing.history_path)? == existing.bytes {
                Ok(false)
            } else {
                Err(ConversionError::InvalidSave(
                    "existing install history would be overwritten".to_owned(),
                ))
            }
        }
        Ok(_) => Err(ConversionError::InvalidSave(
            "existing install history must be a regular non-symlink file".to_owned(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_new_file(&existing.history_path, &existing.bytes)?;
            sync_directory(parent_dir(&existing.history_path))?;
            Ok(true)
        }
        Err(error) => Err(error.into()),
    }
}

fn restore_existing_manifest(
    manifest_path: &Path,
    existing_manifest: Option<&ExistingInstallManifest>,
) -> Result<(), ConversionError> {
    match existing_manifest {
        Some(existing) => atomic_replace(manifest_path, &existing.bytes),
        None => remove_if_regular_file(manifest_path),
    }
}

fn validate_transaction_paths(
    target: &Path,
    manifest_path: &Path,
) -> Result<(PathBuf, PathBuf), ConversionError> {
    let (target, manifest_path) = normalize_bound_paths(target, manifest_path)?;
    validate_save_component_path(&target)?;
    if target
        .symlink_metadata()
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(ConversionError::InvalidSave(
            "save slot path cannot be a symlink".to_owned(),
        ));
    }
    Ok((target, manifest_path))
}

fn normalize_bound_paths(
    target: &Path,
    manifest_path: &Path,
) -> Result<(PathBuf, PathBuf), ConversionError> {
    let target = normalize_path(target)?;
    let manifest_path = normalize_path(manifest_path)?;
    if !is_managed_save_component_name(&target) {
        return Err(ConversionError::InvalidSave(format!(
            "save component basename must be user1, user2, user3, or system: {}",
            target.display()
        )));
    }
    if target == manifest_path {
        return Err(ConversionError::InvalidSave(
            "manifest path cannot be the save slot".to_owned(),
        ));
    }
    if parent_dir(&target) != parent_dir(&manifest_path) {
        return Err(ConversionError::InvalidSave(
            "target and manifest must be in the same directory".to_owned(),
        ));
    }
    if manifest_path != manifest_path_for_normalized_target(&target)? {
        return Err(ConversionError::InvalidSave(
            "manifest path is not bound to the target save slot".to_owned(),
        ));
    }
    Ok((target, manifest_path))
}

fn normalize_path(path: &Path) -> Result<PathBuf, ConversionError> {
    let basename = path.file_name().ok_or_else(|| {
        ConversionError::InvalidSave(format!("path must name a file: {}", path.display()))
    })?;
    Ok(fs::canonicalize(parent_dir(path))?.join(basename))
}

fn manifest_path_for_normalized_target(target: &Path) -> Result<PathBuf, ConversionError> {
    let target_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ConversionError::InvalidSave("target must have a UTF-8 slot basename".to_owned())
        })?;
    Ok(parent_dir(target).join(format!(".{target_name}.mh3g-install.json")))
}

fn install_lock_path_for_normalized_target(target: &Path) -> Result<PathBuf, ConversionError> {
    let target_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ConversionError::InvalidSave("target must have a UTF-8 slot basename".to_owned())
        })?;
    Ok(parent_dir(target).join(format!(".{target_name}.mh3g-install.lock")))
}

fn is_managed_save_component_name(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("user1" | "user2" | "user3" | "system")
    )
}

fn validate_manifest_hash(value: &str, label: &str) -> Result<(), ConversionError> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ConversionError::InvalidSave(format!(
            "manifest {label} hash is not a SHA-256 hex digest"
        )))
    }
}

fn backup_path_for(target: &Path, previous_sha256: &str) -> Result<PathBuf, ConversionError> {
    validate_manifest_hash(previous_sha256, "previous")?;
    let target_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ConversionError::InvalidSave("target must have a UTF-8 slot basename".to_owned())
        })?;
    Ok(parent_dir(target).join(format!(".{target_name}.mh3g-backup-{previous_sha256}")))
}

fn install_history_path_for(
    target: &Path,
    manifest_bytes: &[u8],
) -> Result<PathBuf, ConversionError> {
    let target_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ConversionError::InvalidSave("target must have a UTF-8 slot basename".to_owned())
        })?;
    Ok(parent_dir(target).join(format!(
        ".{target_name}.mh3g-install-history-{}.json",
        sha256_hex(manifest_bytes)
    )))
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

fn write_manifest(path: &Path, manifest: &InstallManifest) -> Result<(), ConversionError> {
    let temporary = unique_path(path, "manifest-tmp");
    let bytes = serde_json::to_vec_pretty(manifest)?;
    match write_new_file(&temporary, &bytes)
        .and_then(|_| fs::rename(&temporary, path).map_err(Into::into))
    {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

fn atomic_replace(target: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
    let temporary = unique_path(target, "restore-tmp");
    write_new_file(&temporary, bytes)?;
    if let Err(error) = fs::rename(&temporary, target) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    sync_directory(parent_dir(target))
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn remove_if_regular_file(path: &Path) -> Result<(), ConversionError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(fs::remove_file(path)?),
        Ok(_) => Err(ConversionError::InvalidSave(
            "rollback target is not a regular file".to_owned(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn sync_directory(path: &Path) -> Result<(), ConversionError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn unique_path(base: &Path, kind: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("save");
    base.with_file_name(format!(".{name}.mh3g-{kind}-{stamp}-{counter}"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{Arc, Barrier},
        thread,
    };

    use crate::{
        ConversionError,
        converter::{convert_3ds_system_to_cemu, convert_3ds_to_cemu},
        profile::{JP_3DS_HEADER, THREE_DS_SIZE, THREE_DS_SYSTEM_SIZE, inspect_bytes},
        transaction::{
            InstallManifest, InstallValidator, ManifestPublisher, ProcessProbe, install_with,
            install_with_publisher, manifest_path_for_target, rollback_with,
        },
    };

    struct Stopped;

    impl ProcessProbe for Stopped {
        fn matching_process(&self) -> Result<Option<String>, ConversionError> {
            Ok(None)
        }
    }

    struct Running;

    impl ProcessProbe for Running {
        fn matching_process(&self) -> Result<Option<String>, ConversionError> {
            Ok(Some("Cemu".to_owned()))
        }
    }

    struct AcceptCemu;

    impl InstallValidator for AcceptCemu {
        fn validate(&self, bytes: &[u8]) -> Result<(), ConversionError> {
            inspect_bytes(bytes).map(|_| ())
        }
    }

    struct RejectingValidator;

    impl InstallValidator for RejectingValidator {
        fn validate(&self, _bytes: &[u8]) -> Result<(), ConversionError> {
            Err(ConversionError::InvalidSave(
                "simulated validation failure".to_owned(),
            ))
        }
    }

    struct FailingPublisher;

    impl ManifestPublisher for FailingPublisher {
        fn publish(&self, path: &Path, _manifest: &InstallManifest) -> Result<(), ConversionError> {
            fs::write(path, b"partial manifest")?;
            Err(ConversionError::Io(std::io::Error::other(
                "simulated manifest publish failure",
            )))
        }
    }

    struct BlockingValidator {
        entered: Arc<Barrier>,
        release: Arc<Barrier>,
    }

    impl InstallValidator for BlockingValidator {
        fn validate(&self, bytes: &[u8]) -> Result<(), ConversionError> {
            self.entered.wait();
            self.release.wait();
            inspect_bytes(bytes).map(|_| ())
        }
    }

    fn source() -> Vec<u8> {
        let mut source = vec![0_u8; THREE_DS_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        source
    }

    fn converted() -> Vec<u8> {
        convert_3ds_to_cemu(&source()).unwrap()
    }

    fn system_source() -> Vec<u8> {
        let mut source = vec![0_u8; THREE_DS_SYSTEM_SIZE];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        source
    }

    fn paths(temp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        let target = temp.path().join("user2");
        let manifest = manifest_path_for_target(&target).unwrap();
        (target, manifest)
    }

    fn install(
        target: &Path,
        manifest: &Path,
        validator: &dyn InstallValidator,
    ) -> crate::transaction::InstallManifest {
        install_with(
            &source(),
            &converted(),
            target,
            manifest,
            &Stopped,
            validator,
        )
        .unwrap()
    }

    #[test]
    fn installs_into_an_absent_slot_and_records_a_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let new_save = converted();

        let manifest = install(&target, &manifest_path, &AcceptCemu);

        assert_eq!(fs::read(&target).unwrap(), new_save);
        assert!(!manifest.target_previously_existed);
        assert!(manifest.previous_sha256.is_none());
        assert!(manifest.backup.is_none());
        assert_eq!(
            manifest.source_sha256,
            inspect_bytes(&source()).unwrap().sha256
        );
        assert_eq!(
            manifest.installed_sha256,
            inspect_bytes(&new_save).unwrap().sha256
        );
        let from_disk: crate::transaction::InstallManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(from_disk.target, fs::canonicalize(target).unwrap());
    }

    #[test]
    fn installs_shared_system_data_with_the_same_backup_and_manifest_guards() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("system");
        let manifest_path = manifest_path_for_target(&target).unwrap();
        let source = system_source();
        let converted = convert_3ds_system_to_cemu(&source).unwrap();
        let previous = b"previous shared system data";
        fs::write(&target, previous).unwrap();

        let manifest = install_with(
            &source,
            &converted,
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
        )
        .unwrap();

        assert_eq!(fs::read(&target).unwrap(), converted);
        assert_eq!(fs::read(manifest.backup.unwrap()).unwrap(), previous);
        assert_eq!(
            manifest.installed_sha256,
            crate::transaction::sha256_hex(&converted)
        );
    }

    #[test]
    fn installs_over_an_existing_slot_after_creating_a_same_directory_backup() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let old_save = b"preexisting cemu target".to_vec();
        fs::write(&target, &old_save).unwrap();

        let manifest = install(&target, &manifest_path, &AcceptCemu);

        assert!(manifest.target_previously_existed);
        assert_eq!(
            manifest.previous_sha256,
            Some(crate::transaction::sha256_hex(&old_save))
        );
        let backup = manifest.backup.unwrap();
        assert_eq!(
            backup.parent(),
            Some(
                fs::canonicalize(target.parent().unwrap())
                    .unwrap()
                    .as_path()
            )
        );
        assert_eq!(fs::read(backup).unwrap(), old_save);
        assert_eq!(fs::read(target).unwrap(), converted());
    }

    #[test]
    fn concurrent_installs_to_one_slot_are_serialized_without_silent_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let source_bytes = source();
        let installed_bytes = converted();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let blocking_validator = Arc::new(BlockingValidator {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        });

        let first_target = target.clone();
        let first_manifest = manifest_path.clone();
        let first_source = source_bytes.clone();
        let first_installed = installed_bytes.clone();
        let first = thread::spawn(move || {
            install_with(
                &first_source,
                &first_installed,
                first_target,
                first_manifest,
                &Stopped,
                blocking_validator.as_ref(),
            )
        });
        entered.wait();

        let second_target = target.clone();
        let second_manifest = manifest_path.clone();
        let second_source = source_bytes.clone();
        let second_installed = installed_bytes.clone();
        let second = thread::spawn(move || {
            install_with(
                &second_source,
                &second_installed,
                second_target,
                second_manifest,
                &Stopped,
                &AcceptCemu,
            )
        });
        let second_result = second.join().unwrap();
        release.wait();
        let first_result = first.join().unwrap();

        assert!(first_result.is_ok());
        assert!(matches!(
            second_result,
            Err(ConversionError::UnsafeInstall(_))
        ));
        assert_eq!(fs::read(&target).unwrap(), installed_bytes);
        let manifest: InstallManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest.target, fs::canonicalize(&target).unwrap());
        assert_eq!(
            manifest.installed_sha256,
            crate::transaction::sha256_hex(&installed_bytes)
        );
        assert!(
            !manifest_path
                .parent()
                .unwrap()
                .join(".user2.mh3g-install.lock")
                .exists()
        );
    }

    #[test]
    fn preexisting_slot_lock_fails_closed_without_stealing_it() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let lock = manifest_path
            .parent()
            .unwrap()
            .join(".user2.mh3g-install.lock");
        fs::write(&lock, b"held by another installer").unwrap();

        let error = install_with(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::UnsafeInstall(_)));
        assert!(!target.exists());
        assert!(!manifest_path.exists());
        assert_eq!(fs::read(lock).unwrap(), b"held by another installer");
    }

    #[test]
    fn validation_failure_preserves_an_existing_target_and_leaves_no_transaction_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let old_save = b"existing target remains untouched".to_vec();
        fs::write(&target, &old_save).unwrap();

        let error = install_with(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Stopped,
            &RejectingValidator,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert_eq!(fs::read(&target).unwrap(), old_save);
        assert!(!manifest_path.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn refuses_install_when_a_supported_emulator_is_running() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);

        let error = install_with(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Running,
            &AcceptCemu,
        )
        .unwrap_err();

        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("Cemu"))
        );
        assert!(!target.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn process_guard_uses_the_complete_exact_executable_name_list() {
        assert_eq!(
            super::GUARDED_PROCESS_NAMES,
            [
                "Nemessix",
                "nemessix",
                "Azahar",
                "azahar",
                "Cemu",
                "cemu",
                "Cemu_release",
                "cemu_release",
            ]
        );
    }

    #[test]
    fn rejects_a_manifest_path_that_is_a_real_save_slot_name() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("user2");
        let manifest_path = temp.path().join("user1");

        let error = install_with(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert!(!target.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn rejects_an_invalid_existing_manifest_path_without_touching_the_target() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        fs::write(&manifest_path, b"existing manifest must not be overwritten").unwrap();

        let error = install_with(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert!(!target.exists());
        assert_eq!(
            fs::read(&manifest_path).unwrap(),
            b"existing manifest must not be overwritten"
        );
    }

    #[test]
    fn archives_a_valid_existing_manifest_before_reinstalling_a_slot() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        fs::write(&target, b"original target before first install").unwrap();
        let _first = install(&target, &manifest_path, &AcceptCemu);
        let first_manifest = fs::read(&manifest_path).unwrap();
        let history = target.parent().unwrap().join(format!(
            ".user2.mh3g-install-history-{}.json",
            crate::transaction::sha256_hex(&first_manifest)
        ));

        let cemu_saved_target = b"target after Cemu persisted a save".to_vec();
        fs::write(&target, &cemu_saved_target).unwrap();

        let second = install(&target, &manifest_path, &AcceptCemu);

        assert_eq!(fs::read(&target).unwrap(), converted());
        assert_eq!(fs::read(&history).unwrap(), first_manifest);
        assert_eq!(
            second.previous_sha256,
            Some(crate::transaction::sha256_hex(&cemu_saved_target))
        );
        let second_backup = second.backup.clone().unwrap();
        assert_eq!(fs::read(&second_backup).unwrap(), cemu_saved_target);
        assert_eq!(
            serde_json::from_slice::<InstallManifest>(&fs::read(&manifest_path).unwrap()).unwrap(),
            second
        );

        rollback_with(&manifest_path, &Stopped).unwrap();
        assert_eq!(fs::read(&target).unwrap(), cemu_saved_target);
        assert!(!second_backup.exists());
        assert!(!manifest_path.exists());
        assert!(history.exists());
    }

    #[test]
    fn failed_reinstall_restores_the_existing_manifest_and_removes_new_history() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        fs::write(&target, b"original target before first install").unwrap();
        let _first = install(&target, &manifest_path, &AcceptCemu);
        let first_manifest = fs::read(&manifest_path).unwrap();
        let history = target.parent().unwrap().join(format!(
            ".user2.mh3g-install-history-{}.json",
            crate::transaction::sha256_hex(&first_manifest)
        ));

        let cemu_saved_target = b"target after Cemu persisted a save".to_vec();
        fs::write(&target, &cemu_saved_target).unwrap();

        let error = install_with_publisher(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
            &FailingPublisher,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::Io(_)));
        assert_eq!(fs::read(&target).unwrap(), cemu_saved_target);
        assert_eq!(fs::read(&manifest_path).unwrap(), first_manifest);
        assert!(!history.exists());
        let attempted_backup = target.parent().unwrap().join(format!(
            ".user2.mh3g-backup-{}",
            crate::transaction::sha256_hex(&cemu_saved_target)
        ));
        assert!(!attempted_backup.exists());
    }

    #[test]
    fn archives_repeated_same_output_installs_without_history_collisions() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        fs::write(&target, b"original target before first install").unwrap();
        let first = install(&target, &manifest_path, &AcceptCemu);
        let first_manifest = fs::read(&manifest_path).unwrap();

        fs::write(&target, b"first Cemu save after conversion").unwrap();
        let second = install(&target, &manifest_path, &AcceptCemu);
        let second_manifest = fs::read(&manifest_path).unwrap();

        fs::write(&target, b"second Cemu save after conversion").unwrap();
        let third = install(&target, &manifest_path, &AcceptCemu);

        assert_eq!(first.installed_sha256, second.installed_sha256);
        assert_eq!(second.installed_sha256, third.installed_sha256);
        assert_ne!(first_manifest, second_manifest);
        for manifest in [&first_manifest, &second_manifest] {
            let history = target.parent().unwrap().join(format!(
                ".user2.mh3g-install-history-{}.json",
                crate::transaction::sha256_hex(manifest)
            ));
            assert_eq!(fs::read(history).unwrap(), *manifest);
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_manifest_symlink_without_touching_the_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let symlink_target = temp.path().join("elsewhere.json");
        symlink(&symlink_target, &manifest_path).unwrap();

        let error = install_with(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert!(!target.exists());
        assert!(
            fs::symlink_metadata(manifest_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn rejects_installed_bytes_that_do_not_derive_from_the_source() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let mut unrelated = converted();
        unrelated[100] ^= 0x5A;

        let error = install_with(
            &source(),
            &unrelated,
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert!(!target.exists());
        assert!(!manifest_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn canonicalizes_a_target_reached_through_a_parent_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let alias = temp.path().join("alias");
        fs::create_dir(&real).unwrap();
        symlink(&real, &alias).unwrap();
        let target = alias.join("user2");
        let manifest_path = manifest_path_for_target(&target).unwrap();

        let manifest = install(&target, &manifest_path, &AcceptCemu);

        assert_eq!(manifest.target, real.canonicalize().unwrap().join("user2"));
        assert_eq!(fs::read(real.join("user2")).unwrap(), converted());
    }

    #[test]
    fn manifest_publish_failure_restores_the_target_and_cleans_new_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let old_save = b"original target before failed manifest publish".to_vec();
        fs::write(&target, &old_save).unwrap();

        let error = install_with_publisher(
            &source(),
            &converted(),
            &target,
            &manifest_path,
            &Stopped,
            &AcceptCemu,
            &FailingPublisher,
        )
        .unwrap_err();

        assert!(matches!(error, ConversionError::Io(_)));
        assert_eq!(fs::read(&target).unwrap(), old_save);
        assert!(!manifest_path.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[test]
    fn rollback_restores_the_verified_backup_and_removes_transaction_files() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let old_save = b"original cemu save".to_vec();
        fs::write(&target, &old_save).unwrap();
        let manifest = install(&target, &manifest_path, &AcceptCemu);
        let backup = manifest.backup.clone().unwrap();

        rollback_with(&manifest_path, &Stopped).unwrap();

        assert_eq!(fs::read(target).unwrap(), old_save);
        assert!(!backup.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn rollback_returns_an_originally_absent_slot_to_absent() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        install(&target, &manifest_path, &AcceptCemu);

        rollback_with(&manifest_path, &Stopped).unwrap();

        assert!(!target.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn rollback_refuses_to_mutate_a_slot_while_an_emulator_is_running() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        install(&target, &manifest_path, &AcceptCemu);
        let installed = fs::read(&target).unwrap();

        let error = rollback_with(&manifest_path, &Running).unwrap_err();

        assert!(
            matches!(error, ConversionError::UnsafeInstall(message) if message.contains("Cemu"))
        );
        assert_eq!(fs::read(target).unwrap(), installed);
        assert!(manifest_path.exists());
    }

    #[test]
    fn rollback_rejects_a_backup_path_tampered_to_another_save_slot() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let old_save = b"original cemu save".to_vec();
        fs::write(&target, &old_save).unwrap();
        let manifest = install(&target, &manifest_path, &AcceptCemu);
        let real_backup = manifest.backup.unwrap();
        let installed = fs::read(&target).unwrap();
        let other_slot = temp.path().join("user3");
        fs::write(&other_slot, &old_save).unwrap();
        let mut manifest: InstallManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.backup = Some(other_slot.clone());
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let error = rollback_with(&manifest_path, &Stopped).unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert_eq!(fs::read(target).unwrap(), installed);
        assert_eq!(fs::read(other_slot).unwrap(), old_save);
        assert!(real_backup.exists());
        assert!(manifest_path.exists());
    }

    #[test]
    fn rollback_rejects_a_manifest_target_rebound_to_another_slot() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        install(&target, &manifest_path, &AcceptCemu);
        let installed = fs::read(&target).unwrap();
        let other_slot = temp.path().join("user3");
        fs::write(&other_slot, &installed).unwrap();

        let mut manifest: InstallManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.target = other_slot.clone();
        manifest.installed_sha256 = crate::transaction::sha256_hex(&installed);
        manifest.previous_sha256 = None;
        manifest.backup = None;
        manifest.target_previously_existed = false;
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let error = rollback_with(&manifest_path, &Stopped).unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert_eq!(fs::read(&other_slot).unwrap(), installed);
        assert_eq!(fs::read(target).unwrap(), installed);
        assert!(manifest_path.exists());
    }

    #[test]
    fn rollback_finishes_cleanup_when_the_backup_was_already_restored() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        fs::write(&target, b"original cemu save").unwrap();
        let manifest = install(&target, &manifest_path, &AcceptCemu);
        let backup = manifest.backup.unwrap();
        let old_save = fs::read(&backup).unwrap();
        fs::write(&target, &old_save).unwrap();
        fs::remove_file(&backup).unwrap();

        rollback_with(&manifest_path, &Stopped).unwrap();

        assert_eq!(fs::read(target).unwrap(), old_save);
        assert!(!manifest_path.exists());
    }

    #[test]
    fn rollback_finishes_cleanup_when_an_originally_absent_target_is_already_removed() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        install(&target, &manifest_path, &AcceptCemu);
        fs::remove_file(&target).unwrap();

        rollback_with(&manifest_path, &Stopped).unwrap();

        assert!(!target.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn rollback_rejects_a_tampered_manifest_without_changing_the_installed_slot() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        install(&target, &manifest_path, &AcceptCemu);
        let installed = fs::read(&target).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["version"] = serde_json::json!(999_u32);
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let error = rollback_with(&manifest_path, &Stopped).unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert_eq!(fs::read(target).unwrap(), installed);
        assert!(manifest_path.exists());
    }

    #[test]
    fn rollback_rejects_a_tampered_backup_without_changing_the_installed_slot() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        fs::write(&target, b"original cemu save").unwrap();
        let manifest = install(&target, &manifest_path, &AcceptCemu);
        let backup = manifest.backup.unwrap();
        let installed = fs::read(&target).unwrap();
        fs::write(&backup, b"tampered backup").unwrap();

        let error = rollback_with(&manifest_path, &Stopped).unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert_eq!(fs::read(target).unwrap(), installed);
        assert!(manifest_path.exists());
        assert!(backup.exists());
    }
}
