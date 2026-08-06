//! Atomic installation and rollback for complete MH3G ExtData component groups.
//!
//! The Cemu-side components are independently stored files, but game state
//! treats each group as one logical unit.  This module therefore never offers
//! a per-file install entry point.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[cfg(unix)]
use std::{
    cell::RefCell,
    ffi::CString,
    marker::PhantomData,
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{ffi::OsStrExt, fs::MetadataExt},
    },
    rc::Rc,
};

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
    transaction::{sha256_hex, sync_directory},
};

pub const EXTRA_INSTALL_MANIFEST_VERSION: u32 = 5;
const LEGACY_EXTRA_INSTALL_MANIFEST_VERSION: u32 = 3;
const PREVIOUS_EXTRA_INSTALL_MANIFEST_VERSION: u32 = 4;
const EXTRA_MANIFEST_NAME: &str = ".mh3g-extra-install.json";
const EXTRA_RECOVERY_JOURNAL_NAME: &str = ".mh3g-extra-recovery.json";
const EXTRA_LOCK_NAME: &str = ".mh3g-extra-install.lock";
const EXTRA_TRANSACTION_DIRECTORY_PREFIX: &str = ".mh3g-extra-transaction-";

type ManifestDirectoryIdentity = (u64, u64);
type ManifestDirectoryIdentities = (
    Option<ManifestDirectoryIdentity>,
    Option<ManifestDirectoryIdentity>,
);

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
    #[serde(default)]
    pub transaction_dir: Option<PathBuf>,
    /// POSIX device/inode identity of the target directory that owned this
    /// transaction when its recovery journal was written.
    #[serde(default)]
    pub target_dir_identity: Option<(u64, u64)>,
    /// POSIX device/inode identity of the transaction directory that contains
    /// this recovery journal and its append-only artifacts.
    #[serde(default)]
    pub transaction_dir_identity: Option<(u64, u64)>,
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
    /// Restore a target that is proven absent after an interrupted platform
    /// replacement. The implementation must fail instead of overwriting if a
    /// competing writer recreates the target.
    fn restore_missing_target(
        &self,
        staged: &Path,
        target: &Path,
        expected_staged: &[u8],
    ) -> Result<(), ConversionError> {
        validate_regular_file_bytes(
            staged,
            expected_staged,
            None,
            "restoring missing ExtData target",
        )?;
        if target.exists() {
            return Err(ConversionError::UnsafeInstall(format!(
                "missing ExtData rollback target reappeared before restore: {}",
                target.display()
            )));
        }
        io_at_path(
            fs::rename(staged, target),
            "restoring missing ExtData target",
            target,
        )?;
        validate_regular_file_bytes(
            target,
            expected_staged,
            None,
            "verifying restored missing ExtData target",
        )
    }
    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError>;
    /// Windows `ReplaceFileW` moves the former target directly to the
    /// transaction backup path instead of retaining it at `staged`.
    ///
    /// The default deliberately follows the platform primitive.  Synthetic
    /// operation wrappers normally delegate `replace_staged`/`restore_target`
    /// to [`StdExtraFileOperations`], so they must inherit the same recovery
    /// artifact layout unless they explicitly provide a different primitive.
    fn previous_value_moves_to_backup(&self) -> bool {
        cfg!(windows)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdExtraFileOperations;

impl ExtraFileOperations for StdExtraFileOperations {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        write_new_extra_file(path, bytes)
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

    fn sync_directory(&self, path: &Path) -> Result<(), ConversionError> {
        sync_extra_directory(path)
    }

    fn previous_value_moves_to_backup(&self) -> bool {
        cfg!(windows)
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

    #[cfg(windows)]
    {
        conditional_replace_windows(staged, target, expected_staged, expected_target, operation)
    }

    #[cfg(not(windows))]
    {
        atomic_exchange_paths(staged, target, operation)?;

        let target_after = read_regular_file(target, operation);
        let staged_after = read_regular_file(staged, operation);
        match (target_after, staged_after) {
            (Ok(target_after), Ok(staged_after))
                if target_after == expected_staged && staged_after == expected_target =>
            {
                sync_exchange_parents(staged, target, operation)
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
                    .and_then(|_| sync_exchange_parents(staged, target, operation));
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
}

#[cfg(windows)]
fn conditional_replace_windows(
    staged: &Path,
    target: &Path,
    expected_staged: &[u8],
    expected_target: &[u8],
    operation: &'static str,
) -> Result<(), ConversionError> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{Foundation::GetLastError, Storage::FileSystem::ReplaceFileW};

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let is_install = operation == "replacing staged ExtData component";
    let backup = if is_install {
        let transaction_dir = staged.parent().ok_or_else(|| {
            ConversionError::UnsafeInstall(format!(
                "Windows ExtData replacement has no transaction directory: {}",
                staged.display()
            ))
        })?;
        let component = target
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                ConversionError::InvalidSave(format!(
                    "Windows ExtData target has an invalid component name: {}",
                    target.display()
                ))
            })?;
        let path =
            controlled_backup_path(transaction_dir, component, &sha256_hex(expected_target))?;
        reject_existing_path(&path, "Windows ExtData replacement backup")?;
        Some(path)
    } else {
        None
    };

    let target_wide = wide(target);
    let staged_wide = wide(staged);
    let backup_wide = backup.as_deref().map(wide);
    let backup_ptr = backup_wide
        .as_ref()
        .map_or(ptr::null(), |value| value.as_ptr());
    let replaced = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            staged_wide.as_ptr(),
            backup_ptr,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if replaced == 0 {
        let code = unsafe { GetLastError() };
        return Err(ConversionError::UnsafeInstall(format!(
            "Windows ExtData replacement failed with Win32 error {code}; retain the recovery journal and inspect target={}, staged={}, backup={}",
            target.display(),
            staged.display(),
            backup
                .as_deref()
                .map_or_else(|| "<none>".to_owned(), |path| path.display().to_string())
        )));
    }

    validate_regular_file_bytes(target, expected_staged, None, operation)?;
    if let Some(backup) = backup.as_deref() {
        validate_regular_file_bytes(
            backup,
            expected_target,
            None,
            "verifying Windows ExtData replacement backup",
        )?;
    } else if read_optional_regular_file(staged, operation)?.is_some() {
        return Err(ConversionError::UnsafeInstall(format!(
            "Windows ExtData rollback left an unexpected staged file: {}",
            staged.display()
        )));
    }
    sync_exchange_parents(staged, target, operation)
}

fn sync_exchange_parents(
    staged: &Path,
    target: &Path,
    operation: &'static str,
) -> Result<(), ConversionError> {
    let staged_parent = staged.parent().ok_or_else(|| {
        ConversionError::UnsafeInstall(format!(
            "ExtData exchange staged path has no parent ({operation}): {}",
            staged.display()
        ))
    })?;
    let target_parent = target.parent().ok_or_else(|| {
        ConversionError::UnsafeInstall(format!(
            "ExtData exchange target has no parent ({operation}): {}",
            target.display()
        ))
    })?;
    sync_extra_directory(staged_parent)?;
    if staged_parent != target_parent {
        sync_extra_directory(target_parent)?;
    }
    Ok(())
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

#[cfg(unix)]
#[derive(Debug)]
struct ExtraDirectoryAnchor {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
}

#[cfg(unix)]
impl ExtraDirectoryAnchor {
    fn open(path: &Path, operation: &'static str) -> Result<Self, ConversionError> {
        use std::os::unix::fs::OpenOptionsExt;

        let mut options = OpenOptions::new();
        options
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let file = io_at_path(options.open(path), operation, path)?;
        let opened = io_at_path(file.metadata(), operation, path)?;
        let named = io_at_path(fs::symlink_metadata(path), operation, path)?;
        if !opened.file_type().is_dir()
            || !named.file_type().is_dir()
            || directory_identity(&opened) != directory_identity(&named)
        {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData directory changed while opening it: {}",
                path.display()
            )));
        }
        Ok(Self {
            path: path.to_path_buf(),
            identity: directory_identity(&opened),
            file,
        })
    }

    fn open_child(
        &self,
        name: &CString,
        path: &Path,
        operation: &'static str,
    ) -> Result<Self, ConversionError> {
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return io_at_path(Err(std::io::Error::last_os_error()), operation, path);
        }
        let file = unsafe { File::from_raw_fd(fd) };
        let opened = io_at_path(file.metadata(), operation, path)?;
        let named = stat_at(self.file.as_raw_fd(), name, operation, path)?;
        if !opened.file_type().is_dir()
            || !stat_is_directory(&named)
            || directory_identity(&opened) != stat_identity(&named)
        {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData directory changed while opening it: {}",
                path.display()
            )));
        }
        Ok(Self {
            path: path.to_path_buf(),
            identity: directory_identity(&opened),
            file,
        })
    }

    fn ensure_named_binding(&self, operation: &'static str) -> Result<(), ConversionError> {
        let metadata = io_at_path(fs::symlink_metadata(&self.path), operation, &self.path)?;
        if !metadata.file_type().is_dir() || directory_identity(&metadata) != self.identity {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData directory path changed while transaction was active: {}",
                self.path.display()
            )));
        }
        Ok(())
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct ExtraTransactionAnchors {
    target: ExtraDirectoryAnchor,
    transaction: Option<ExtraDirectoryAnchor>,
}

#[cfg(unix)]
thread_local! {
    static EXTRA_TRANSACTION_ANCHORS: RefCell<Option<ExtraTransactionAnchors>> = const { RefCell::new(None) };
}

/// Holds no file descriptor itself. The thread-local context owns the
/// descriptors so every regular filesystem helper can resolve a planned path
/// through `openat`/`renameat*` instead of reparsing a replaceable parent name.
#[cfg(unix)]
struct ExtraTransactionScope {
    previous: Option<ExtraTransactionAnchors>,
    // The anchors live in thread-local storage and must be dropped from the
    // same thread that installed them.
    _not_send_or_sync: PhantomData<Rc<()>>,
}

#[cfg(unix)]
impl ExtraTransactionScope {
    fn open(target_dir: &Path) -> Result<Self, ConversionError> {
        let target =
            ExtraDirectoryAnchor::open(target_dir, "opening anchored ExtData target directory")?;
        let previous = EXTRA_TRANSACTION_ANCHORS.with(|cell| {
            let mut anchors = cell.borrow_mut();
            if anchors.is_some() {
                return Err(ConversionError::UnsafeInstall(
                    "nested ExtData transaction directory anchors are not supported".to_owned(),
                ));
            }
            Ok(anchors.replace(ExtraTransactionAnchors {
                target,
                transaction: None,
            }))
        })?;
        Ok(Self {
            previous,
            _not_send_or_sync: PhantomData,
        })
    }
}

#[cfg(unix)]
impl Drop for ExtraTransactionScope {
    fn drop(&mut self) {
        EXTRA_TRANSACTION_ANCHORS.with(|cell| {
            let mut anchors = cell.borrow_mut();
            *anchors = self.previous.take();
        });
    }
}

#[cfg(not(unix))]
struct ExtraTransactionScope;

#[cfg(not(unix))]
impl ExtraTransactionScope {
    fn open(_target_dir: &Path) -> Result<Self, ConversionError> {
        Ok(Self)
    }
}

#[cfg(unix)]
fn directory_identity(metadata: &fs::Metadata) -> FileIdentity {
    (metadata.dev(), metadata.ino())
}

#[cfg(unix)]
fn anchored_child_name(
    parent: &Path,
    path: &Path,
    operation: &'static str,
) -> Result<Option<CString>, ConversionError> {
    if path.parent() != Some(parent) {
        return Ok(None);
    }
    let name = path.file_name().ok_or_else(|| {
        ConversionError::InvalidSave(format!(
            "ExtData transaction path has no file name ({operation}): {}",
            path.display()
        ))
    })?;
    CString::new(name.as_bytes()).map(Some).map_err(|_| {
        ConversionError::InvalidSave(format!(
            "ExtData transaction path contains an embedded NUL ({operation}): {}",
            path.display()
        ))
    })
}

#[cfg(unix)]
fn anchored_file_location(
    path: &Path,
    operation: &'static str,
) -> Result<Option<(RawFd, CString)>, ConversionError> {
    EXTRA_TRANSACTION_ANCHORS.with(|cell| {
        let anchors = cell.borrow();
        let Some(anchors) = anchors.as_ref() else {
            return Ok(None);
        };
        if let Some(name) = anchored_child_name(&anchors.target.path, path, operation)? {
            return Ok(Some((anchors.target.file.as_raw_fd(), name)));
        }
        if let Some(transaction) = anchors.transaction.as_ref()
            && let Some(name) = anchored_child_name(&transaction.path, path, operation)?
        {
            return Ok(Some((transaction.file.as_raw_fd(), name)));
        }
        Ok(None)
    })
}

#[cfg(unix)]
fn extra_transaction_scope_is_active() -> bool {
    EXTRA_TRANSACTION_ANCHORS.with(|cell| cell.borrow().is_some())
}

#[cfg(not(unix))]
fn extra_transaction_scope_is_active() -> bool {
    false
}

#[cfg(unix)]
fn anchored_directory_fd(path: &Path) -> Option<RawFd> {
    EXTRA_TRANSACTION_ANCHORS.with(|cell| {
        let anchors = cell.borrow();
        let anchors = anchors.as_ref()?;
        if path == anchors.target.path {
            Some(anchors.target.file.as_raw_fd())
        } else if anchors
            .transaction
            .as_ref()
            .is_some_and(|transaction| path == transaction.path)
        {
            anchors
                .transaction
                .as_ref()
                .map(|transaction| transaction.file.as_raw_fd())
        } else {
            None
        }
    })
}

#[cfg(unix)]
fn anchored_directory_identity(path: &Path) -> Option<ManifestDirectoryIdentity> {
    EXTRA_TRANSACTION_ANCHORS.with(|cell| {
        let anchors = cell.borrow();
        let anchors = anchors.as_ref()?;
        if path == anchors.target.path {
            Some(anchors.target.identity)
        } else if anchors
            .transaction
            .as_ref()
            .is_some_and(|transaction| path == transaction.path)
        {
            anchors
                .transaction
                .as_ref()
                .map(|transaction| transaction.identity)
        } else {
            None
        }
    })
}

#[cfg(unix)]
fn is_within_anchored_transaction_namespace(path: &Path) -> bool {
    EXTRA_TRANSACTION_ANCHORS.with(|cell| {
        let anchors = cell.borrow();
        let Some(anchors) = anchors.as_ref() else {
            return false;
        };
        path.starts_with(&anchors.target.path)
            || anchors
                .transaction
                .as_ref()
                .is_some_and(|transaction| path.starts_with(&transaction.path))
    })
}

#[cfg(not(unix))]
fn is_within_anchored_transaction_namespace(_path: &Path) -> bool {
    false
}

#[cfg(unix)]
fn ensure_extra_transaction_directory_bindings(
    operation: &'static str,
) -> Result<(), ConversionError> {
    EXTRA_TRANSACTION_ANCHORS.with(|cell| {
        let anchors = cell.borrow();
        let Some(anchors) = anchors.as_ref() else {
            return Ok(());
        };
        anchors.target.ensure_named_binding(operation)?;
        if let Some(transaction) = anchors.transaction.as_ref() {
            transaction.ensure_named_binding(operation)?;
        }
        Ok(())
    })
}

#[cfg(not(unix))]
fn ensure_extra_transaction_directory_bindings(
    _operation: &'static str,
) -> Result<(), ConversionError> {
    Ok(())
}

fn manifest_directory_identities(
    target_dir: &Path,
    transaction_dir: &Path,
) -> Result<ManifestDirectoryIdentities, ConversionError> {
    #[cfg(unix)]
    {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before recording manifest identities",
        )?;
        let target_identity = anchored_directory_identity(target_dir).ok_or_else(|| {
            ConversionError::UnsafeInstall(format!(
                "ExtData target directory is outside the anchored transaction scope: {}",
                target_dir.display()
            ))
        })?;
        let transaction_identity =
            anchored_directory_identity(transaction_dir).ok_or_else(|| {
                ConversionError::UnsafeInstall(format!(
                    "ExtData transaction directory is outside the anchored transaction scope: {}",
                    transaction_dir.display()
                ))
            })?;
        Ok((Some(target_identity), Some(transaction_identity)))
    }

    #[cfg(windows)]
    {
        Ok((
            Some(windows_directory_identity(
                target_dir,
                "recording Windows ExtData target directory identity",
            )?),
            Some(windows_directory_identity(
                transaction_dir,
                "recording Windows ExtData transaction directory identity",
            )?),
        ))
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target_dir, transaction_dir);
        Err(ConversionError::UnsafeInstall(
            "ExtData transactions require POSIX directory identities".to_owned(),
        ))
    }
}

#[cfg(windows)]
fn windows_directory_identity(
    path: &Path,
    operation: &'static str,
) -> Result<ManifestDirectoryIdentity, ConversionError> {
    use std::os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    let metadata = io_at_path(fs::symlink_metadata(path), operation, path)?;
    if !metadata.file_type().is_dir()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(ConversionError::InvalidSave(format!(
            "Windows ExtData transaction directory must not be a reparse point: {}",
            path.display()
        )));
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let directory = io_at_path(options.open(path), operation, path)?;
    let mut information = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let result =
        unsafe { GetFileInformationByHandle(directory.as_raw_handle() as _, &mut information) };
    if result == 0 {
        return io_at_path(
            Err(std::io::Error::last_os_error()),
            "reading Windows ExtData directory identity",
            path,
        );
    }
    let index =
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
    Ok((u64::from(information.dwVolumeSerialNumber), index))
}

#[cfg(unix)]
fn stat_at(
    directory_fd: RawFd,
    name: &CString,
    operation: &'static str,
    display_path: &Path,
) -> Result<libc::stat, ConversionError> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            directory_fd,
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        Ok(unsafe { stat.assume_init() })
    } else {
        io_at_path(
            Err(std::io::Error::last_os_error()),
            operation,
            display_path,
        )
    }
}

#[cfg(unix)]
fn stat_is_regular(stat: &libc::stat) -> bool {
    (stat.st_mode & libc::S_IFMT) == libc::S_IFREG
}

#[cfg(unix)]
fn stat_is_directory(stat: &libc::stat) -> bool {
    (stat.st_mode & libc::S_IFMT) == libc::S_IFDIR
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)] // macOS dev_t is signed; Linux already uses u64.
fn stat_identity(stat: &libc::stat) -> FileIdentity {
    (stat.st_dev as u64, stat.st_ino)
}

#[cfg(unix)]
fn create_anchored_transaction_directory(
    path: &Path,
    operation: &'static str,
) -> Result<bool, ConversionError> {
    let Some((target_fd, name)) = anchored_file_location(path, operation)? else {
        return Ok(false);
    };
    let result = unsafe { libc::mkdirat(target_fd, name.as_ptr(), 0o700) };
    if result != 0 {
        return Err(
            io_at_path::<()>(Err(std::io::Error::last_os_error()), operation, path)
                .expect_err("an explicit I/O error cannot succeed"),
        );
    }
    let created = stat_at(target_fd, &name, operation, path)?;
    if !stat_is_directory(&created) {
        return Err(ConversionError::UnsafeInstall(format!(
            "created ExtData transaction path is not a directory: {}",
            path.display()
        )));
    }
    EXTRA_TRANSACTION_ANCHORS.with(|cell| {
        let mut anchors = cell.borrow_mut();
        let anchors = anchors.as_mut().ok_or_else(|| {
            ConversionError::UnsafeInstall(
                "ExtData transaction directory anchor disappeared during creation".to_owned(),
            )
        })?;
        if anchors.transaction.is_some() {
            return Err(ConversionError::UnsafeInstall(
                "ExtData transaction directory anchor already exists".to_owned(),
            ));
        }
        let opened = anchors.target.open_child(&name, path, operation)?;
        if opened.identity != stat_identity(&created) {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData transaction directory changed while anchoring it: {}",
                path.display()
            )));
        }
        anchors.transaction = Some(opened);
        Ok(())
    })?;
    Ok(true)
}

#[cfg(unix)]
fn anchor_existing_transaction_directory(
    path: &Path,
    operation: &'static str,
) -> Result<bool, ConversionError> {
    let Some((_, name)) = anchored_file_location(path, operation)? else {
        return Ok(false);
    };
    EXTRA_TRANSACTION_ANCHORS.with(|cell| {
        let mut anchors = cell.borrow_mut();
        let anchors = anchors.as_mut().ok_or_else(|| {
            ConversionError::UnsafeInstall(
                "ExtData target directory anchor disappeared while reading a recovery journal"
                    .to_owned(),
            )
        })?;
        if anchors.transaction.is_some() {
            return Err(ConversionError::UnsafeInstall(
                "ExtData recovery journal has more than one transaction directory anchor"
                    .to_owned(),
            ));
        }
        anchors.transaction = Some(anchors.target.open_child(&name, path, operation)?);
        Ok(())
    })?;
    Ok(true)
}

/// Write an ExtData transaction artifact with create-new semantics.
///
/// This deliberately differs from the single-save transaction helper: if the
/// write or `sync_all` fails, the partially written pathname is retained. A
/// later writer might otherwise replace the name between the error and an
/// attempted cleanup, making an unlink delete somebody else's recovery
/// material. The enclosing per-transaction directory is retained for the
/// same reason.
fn write_new_extra_file(path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
    #[cfg(unix)]
    if let Some((directory_fd, name)) =
        anchored_file_location(path, "creating append-only ExtData transaction artifact")?
    {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before creating transaction artifact",
        )?;
        let fd = unsafe {
            libc::openat(
                directory_fd,
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return io_at_path(
                Err(std::io::Error::last_os_error()),
                "creating append-only ExtData transaction artifact",
                path,
            );
        }
        return write_new_extra_file_to_open_file(unsafe { File::from_raw_fd(fd) }, bytes, path);
    }

    if extra_transaction_scope_is_active() {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction artifact is outside the anchored directories: {}",
            path.display()
        )));
    }

    let file = io_at_path(
        OpenOptions::new().write(true).create_new(true).open(path),
        "creating append-only ExtData transaction artifact",
        path,
    )?;
    write_new_extra_file_to_open_file(file, bytes, path)
}

fn write_new_extra_file_to_open_file(
    mut file: File,
    bytes: &[u8],
    path: &Path,
) -> Result<(), ConversionError> {
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        return io_at_path(
            Err(error),
            "writing append-only ExtData transaction artifact",
            path,
        );
    }
    Ok(())
}

fn sync_extra_directory(path: &Path) -> Result<(), ConversionError> {
    #[cfg(unix)]
    if let Some(directory_fd) = anchored_directory_fd(path) {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before syncing transaction metadata",
        )?;
        let result = unsafe { libc::fsync(directory_fd) };
        if result != 0 {
            return io_at_path(
                Err(std::io::Error::last_os_error()),
                "syncing ExtData transaction directory",
                path,
            );
        }
        return Ok(());
    }
    if extra_transaction_scope_is_active() {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData directory is outside the anchored transaction directories: {}",
            path.display()
        )));
    }
    sync_directory(path)
}

#[cfg(target_os = "macos")]
fn atomic_exchange_paths(
    staged: &Path,
    target: &Path,
    operation: &'static str,
) -> Result<(), ConversionError> {
    let staged_at = anchored_file_location(staged, operation)?;
    let target_at = anchored_file_location(target, operation)?;
    match (staged_at, target_at) {
        (Some((staged_fd, staged_name)), Some((target_fd, target_name))) => {
            ensure_extra_transaction_directory_bindings(
                "rechecking ExtData directories before atomic exchange",
            )?;
            const RENAME_NOFOLLOW_ANY: libc::c_uint = 0x0000_0010;
            let result = unsafe {
                libc::renameatx_np(
                    staged_fd,
                    staged_name.as_ptr(),
                    target_fd,
                    target_name.as_ptr(),
                    libc::RENAME_SWAP | RENAME_NOFOLLOW_ANY,
                )
            };
            if result == 0 {
                return Ok(());
            }
            return io_at_path(Err(std::io::Error::last_os_error()), operation, target);
        }
        (None, None) if !extra_transaction_scope_is_active() => {}
        _ => {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData atomic exchange path is outside the anchored transaction directories ({operation}; staged={}, target={})",
                staged.display(),
                target.display()
            )));
        }
    }

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
    let staged_at = anchored_file_location(staged, operation)?;
    let target_at = anchored_file_location(target, operation)?;
    match (staged_at, target_at) {
        (Some((staged_fd, staged_name)), Some((target_fd, target_name))) => {
            ensure_extra_transaction_directory_bindings(
                "rechecking ExtData directories before atomic exchange",
            )?;
            let result = unsafe {
                libc::syscall(
                    libc::SYS_renameat2 as libc::c_long,
                    staged_fd,
                    staged_name.as_ptr(),
                    target_fd,
                    target_name.as_ptr(),
                    libc::RENAME_EXCHANGE,
                )
            };
            if result == 0 {
                return Ok(());
            }
            return io_at_path(Err(std::io::Error::last_os_error()), operation, target);
        }
        (None, None) if !extra_transaction_scope_is_active() => {}
        _ => {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData atomic exchange path is outside the anchored transaction directories ({operation}; staged={}, target={})",
                staged.display(),
                target.display()
            )));
        }
    }

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

#[cfg(not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "macos",
    windows
)))]
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
        #[cfg(unix)]
        if let Some(target_fd) = anchored_directory_fd(target_dir) {
            return Self::acquire_anchored(target_fd, &path);
        }
        if extra_transaction_scope_is_active() {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData install lock is outside the anchored target directory: {}",
                path.display()
            )));
        }

        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
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

    #[cfg(unix)]
    fn acquire_anchored(target_fd: RawFd, path: &Path) -> Result<Self, ConversionError> {
        let name = CString::new(EXTRA_LOCK_NAME).expect("lock name is a fixed NUL-free literal");
        for _ in 0..2 {
            ensure_extra_transaction_directory_bindings(
                "rechecking ExtData target directory before opening install lock",
            )?;
            let fd = unsafe {
                libc::openat(
                    target_fd,
                    name.as_ptr(),
                    libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return io_at_path(
                    Err(std::io::Error::last_os_error()),
                    "opening anchored ExtData install lock",
                    path,
                );
            }
            let file = unsafe { File::from_raw_fd(fd) };
            let file_metadata = io_at_path(
                file.metadata(),
                "reading anchored ExtData install lock",
                path,
            )?;
            let named = stat_at(
                target_fd,
                &name,
                "reading anchored ExtData install lock metadata",
                path,
            )?;
            if !file_metadata.file_type().is_file()
                || !stat_is_regular(&named)
                || file_identity(&file_metadata) != stat_identity(&named)
            {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData install lock is not a stable regular file: {}",
                    path.display()
                )));
            }
            match file.try_lock_exclusive() {
                Ok(()) => {
                    ensure_extra_transaction_directory_bindings(
                        "rechecking ExtData target directory after locking",
                    )?;
                    let final_named = stat_at(
                        target_fd,
                        &name,
                        "rechecking anchored ExtData install lock metadata",
                        path,
                    )?;
                    if stat_is_regular(&final_named)
                        && file_identity(&file_metadata) == stat_identity(&final_named)
                    {
                        return Ok(Self { _file: file });
                    }
                    // Dropping the handle releases the advisory lock before a
                    // bounded retry against the replacement directory entry.
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "ExtData group installation is already locked: {}",
                        path.display()
                    )));
                }
                Err(error) => {
                    return io_at_path(Err(error), "locking anchored ExtData install lock", path);
                }
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
    transaction_dir: PathBuf,
    groups: Vec<ExtraGroup>,
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

    fn manifest(&self) -> Result<ExtraInstallManifest, ConversionError> {
        let (target_dir_identity, transaction_dir_identity) =
            manifest_directory_identities(&self.target_dir, &self.transaction_dir)?;
        Ok(ExtraInstallManifest {
            version: EXTRA_INSTALL_MANIFEST_VERSION,
            transaction_id: self.transaction_id.clone(),
            transaction_dir: Some(self.transaction_dir.clone()),
            target_dir_identity,
            transaction_dir_identity,
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
        })
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

/// Return the root-level recovery journal location used by pre-v4 transactions.
///
/// New transactions never write here: each v5 transaction has a unique,
/// append-only child directory.  This helper remains available only so callers
/// can identify an already-created legacy journal. Legacy journals lack the
/// persisted directory identities required for a safe rollback and are
/// therefore rejected before mutation.
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
    let target_dir = normalize_directory(target_dir.as_ref(), "target ExtData directory")?;
    let _scope = ExtraTransactionScope::open(&target_dir)?;
    let plan = prepare_extra_install(
        staging_dir.as_ref(),
        &target_dir,
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
    require_durable_extra_transaction_support()?;
    let target_dir = normalize_directory(target_dir.as_ref(), "target ExtData directory")?;
    let _scope = ExtraTransactionScope::open(&target_dir)?;
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

    let mut temporary_paths = vec![None; plan.entries.len()];
    let mut recovery_journal = None::<OwnedArtifact>;
    let mut replacement_started = false;

    let result = (|| {
        create_transaction_directory(&plan)?;

        // Persist a complete, rollback-valid journal before creating any
        // backup or temporary file.  A hard interruption at any later point
        // can therefore recover by inspecting before/after states.
        let recovery_bytes = serde_json::to_vec_pretty(&plan.manifest()?)?;
        // This must have create-new semantics: a second writer that reaches
        // this path after preflight owns its journal, not us.
        operations.write_new_file(&plan.recovery_journal_path, &recovery_bytes)?;
        recovery_journal = Some(capture_owned_regular_file(
            &plan.recovery_journal_path,
            &recovery_bytes,
            "capturing ExtData recovery journal",
        )?);
        operations.sync_directory(&plan.transaction_dir)?;
        operations.sync_directory(&plan.target_dir)?;

        for entry in &plan.entries {
            if !operations.previous_value_moves_to_backup()
                && let (Some(backup), Some(previous)) = (&entry.backup, entry.previous.as_deref())
            {
                operations.write_new_file(backup, previous)?;
                let staged_backup = read_regular_file(backup, "reading staged ExtData backup")?;
                if staged_backup != previous {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "staged ExtData backup does not match its target snapshot: {}",
                        backup.display()
                    )));
                }
                capture_owned_regular_file(backup, previous, "capturing staged ExtData backup")?;
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
        operations.sync_directory(&plan.transaction_dir)?;
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
            // POSIX exchange retains the former target at the controlled
            // temporary path. Windows ReplaceFileW moves it directly to the
            // controlled backup path. Retain the platform-specific artifact
            // identity for compensated failure handling and rollback.
            if operations.previous_value_moves_to_backup() {
                capture_owned_regular_file(
                    entry
                        .backup
                        .as_deref()
                        .expect("initialized ExtData targets have backup paths"),
                    previous,
                    "capturing replaced Windows ExtData target backup",
                )?;
                temporary_paths[index] = None;
            } else {
                temporary_paths[index] = Some(capture_owned_regular_file(
                    &entry.temporary,
                    previous,
                    "capturing exchanged ExtData target snapshot",
                )?);
            }
            operations.sync_directory(&plan.transaction_dir)?;
            operations.sync_directory(&plan.target_dir)?;
        }

        revalidate_fully_installed_plan(&plan, probe)?;
        validate_owned_regular_file(
            recovery_journal
                .as_ref()
                .expect("journal is captured before any transaction artifact"),
            "rechecking retained ExtData recovery journal after installation",
        )?;
        operations.sync_directory(&plan.transaction_dir)?;
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
            let retained_state = if let Some(journal) = recovery_journal.as_ref() {
                format!(
                    "retained recovery journal for audit and explicit rollback: {}",
                    journal.path.display()
                )
            } else {
                format!(
                    "no recovery journal was written and no target exchange began; retained transaction directory for audit: {}",
                    plan.transaction_dir.display()
                )
            };
            Err(ConversionError::UnsafeInstall(format!(
                "ExtData group installation failed before an atomic target exchange: {install_error}; {retained_state}"
            )))
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

/// Roll back a complete ExtData manifest. Every entry is validated before a
/// target changes. The journal, backups, and transaction directory are
/// deliberately retained after a verified rollback for audit and recovery.
pub fn rollback_extra_groups_with(
    manifest_path: impl AsRef<Path>,
    probe: &dyn ProcessProbe,
    operations: &dyn ExtraFileOperations,
) -> Result<(), ConversionError> {
    reject_running_emulator(probe)?;
    require_durable_extra_transaction_support()?;
    let (manifest_path, target_dir) = normalize_manifest_path(manifest_path.as_ref())?;
    let _scope = ExtraTransactionScope::open(&target_dir)?;
    #[cfg(unix)]
    if manifest_path.parent() != Some(target_dir.as_path())
        && !anchor_existing_transaction_directory(
            manifest_path
                .parent()
                .expect("normalized manifest has a parent"),
            "opening anchored ExtData transaction directory for rollback",
        )?
    {
        return Err(ConversionError::UnsafeInstall(
            "ExtData rollback transaction directory is not anchored to its target".to_owned(),
        ));
    }
    ensure_extra_transaction_directory_bindings("checking ExtData rollback directory bindings")?;
    reject_running_emulator(probe)?;
    let manifest_bytes = read_regular_file(&manifest_path, "reading ExtData rollback manifest")?;
    let manifest: ExtraInstallManifest = serde_json::from_slice(&manifest_bytes)?;
    let entries = validate_rollback_manifest(&manifest, &manifest_path, &target_dir)?;
    // A v5 journal binds both directories by inode.  Validate it before
    // creating the advisory lock or reading a target component, so a
    // same-path replacement cannot acquire transaction material from the
    // former directory.
    ensure_extra_transaction_directory_bindings(
        "rechecking ExtData rollback directory bindings after manifest validation",
    )?;
    let _lock = ExtraInstallLock::acquire(&target_dir)?;
    reject_running_emulator(probe)?;
    let rollback_states = prepare_rollback_states(&entries)?;
    for state in &rollback_states {
        if !state.needs_restore {
            continue;
        }
        reject_running_emulator(probe)?;
        let current = read_optional_regular_file(
            &state.entry.entry.target,
            "rechecking ExtData rollback target hash before restore",
        )?;
        match current.as_deref() {
            Some(current) => {
                validate_cemu_external_component_named(current, &state.entry.entry.component)?;
                if sha256_hex(current) != state.entry.entry.after_sha256 {
                    return Err(ConversionError::UnsafeInstall(format!(
                        "ExtData rollback target changed after preflight: {}",
                        state.entry.entry.target.display()
                    )));
                }
            }
            None if state.current.is_none() => {}
            None => {
                return Err(ConversionError::UnsafeInstall(format!(
                    "ExtData rollback target disappeared after preflight: {}",
                    state.entry.entry.target.display()
                )));
            }
        }
        let previous = state
            .entry
            .previous
            .as_deref()
            .expect("validated ExtData rollback manifest requires initialized targets");
        if operations.previous_value_moves_to_backup() {
            if state.entry.entry.temporary.exists() {
                return Err(ConversionError::UnsafeInstall(format!(
                    "Windows ExtData rollback temporary already exists: {}",
                    state.entry.entry.temporary.display()
                )));
            }
            operations.write_new_file(&state.entry.entry.temporary, previous)?;
        } else {
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
        }
        let result = if let Some(installed) = state.current.as_deref() {
            operations.restore_target(
                &state.entry.entry.temporary,
                &state.entry.entry.target,
                previous,
                installed,
            )
        } else {
            operations.restore_missing_target(
                &state.entry.entry.temporary,
                &state.entry.entry.target,
                previous,
            )
        };
        if let Err(error) = result {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData rollback reached an atomic target exchange but did not finish cleanly at {}: {error}; retain the recovery journal and run rollback again only after resolving the conflict",
                state.entry.entry.target.display(),
            )));
        }
    }
    operations.sync_directory(&target_dir)?;
    if let Some(transaction_dir) = manifest.transaction_dir.as_deref() {
        // A rollback swaps a target name with a staging name in this distinct
        // transaction directory.  Persist both namespaces before reporting a
        // restored target state.
        operations.sync_directory(transaction_dir)?;
        operations.sync_directory(&target_dir)?;
    }
    verify_restored_targets(&entries)?;
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
    let target_dir = target_dir.to_path_buf();
    if staging_dir == target_dir {
        return Err(ConversionError::InvalidSave(
            "staged and target ExtData directories alias the same directory".to_owned(),
        ));
    }
    reject_running_emulator(probe)?;
    let transaction_id = Uuid::new_v4().hyphenated().to_string();
    let transaction_dir = controlled_transaction_directory(&target_dir, &transaction_id)?;
    let recovery_journal_path = transaction_dir.join(EXTRA_RECOVERY_JOURNAL_NAME);
    let groups = normalize_groups(groups)?;
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
                &transaction_dir,
                component,
                &sha256_hex(&previous),
            )?);
            let temporary = controlled_temporary_path(&transaction_dir, component, &transaction_id)?;
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
        transaction_dir,
        groups,
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
    reject_existing_path(&plan.transaction_dir, "ExtData transaction directory")
}

fn reject_existing_path(path: &Path, label: &str) -> Result<(), ConversionError> {
    #[cfg(unix)]
    if let Some((directory_fd, name)) =
        anchored_file_location(path, "reading anchored ExtData transaction artifact")?
    {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before validating transaction namespace",
        )?;
        return match stat_at(
            directory_fd,
            &name,
            "reading anchored ExtData transaction artifact",
            path,
        ) {
            Ok(_) => Err(ConversionError::UnsafeInstall(format!(
                "{label} already exists: {}",
                path.display()
            ))),
            Err(error) if is_not_found_error(&error) => Ok(()),
            Err(error) => Err(error),
        };
    }
    if extra_transaction_scope_is_active() {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction artifact is outside the anchored directories: {}",
            path.display()
        )));
    }

    match fs::symlink_metadata(path) {
        Ok(_) => Err(ConversionError::UnsafeInstall(format!(
            "{label} already exists: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => io_at_path(Err(error), "reading ExtData transaction artifact", path),
    }
}

/// Create the single namespace owned by this transaction.  We intentionally
/// never remove this directory or its contents: POSIX does not offer an
/// unlink-if-this-inode-and-this-content-still-match primitive, so any cleanup
/// after a pathname check could delete a file created by another writer.
fn create_transaction_directory(plan: &ExtraInstallPlan) -> Result<(), ConversionError> {
    #[cfg(unix)]
    if create_anchored_transaction_directory(
        &plan.transaction_dir,
        "creating ExtData transaction directory",
    )? {
        ensure_extra_transaction_directory_bindings(
            "validating created ExtData transaction directory",
        )?;
        // This commits the new child name before the recovery journal and
        // staged values are written inside it.
        return sync_extra_directory(&plan.target_dir);
    }

    if extra_transaction_scope_is_active() {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction directory is outside the anchored target directory: {}",
            plan.transaction_dir.display()
        )));
    }

    io_at_path(
        fs::create_dir(&plan.transaction_dir),
        "creating ExtData transaction directory",
        &plan.transaction_dir,
    )?;
    validate_controlled_transaction_directory(
        &plan.transaction_dir,
        &plan.target_dir,
        &plan.transaction_id,
        "validating created ExtData transaction directory",
    )?;
    // This commits the new child name before the recovery journal and staged
    // values are written inside it.
    sync_extra_directory(&plan.target_dir)
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
    #[cfg(unix)]
    if let Some((directory_fd, name)) = anchored_file_location(path, operation)? {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before reading transaction file",
        )?;
        return read_anchored_regular_file(directory_fd, &name, path, operation);
    }
    if extra_transaction_scope_is_active() && is_within_anchored_transaction_namespace(path) {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData file is inside the anchored transaction directories but has no FD-relative mapping ({operation}): {}",
            path.display()
        )));
    }

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

#[cfg(unix)]
fn read_anchored_regular_file(
    directory_fd: RawFd,
    name: &CString,
    path: &Path,
    operation: &'static str,
) -> Result<Vec<u8>, ConversionError> {
    let initial = stat_at(directory_fd, name, operation, path)?;
    if !stat_is_regular(&initial) {
        return Err(ConversionError::InvalidSave(format!(
            "ExtData component must be a regular non-symlink file: {}",
            path.display()
        )));
    }
    let fd = unsafe {
        libc::openat(
            directory_fd,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return io_at_path(Err(std::io::Error::last_os_error()), operation, path);
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let opened = io_at_path(file.metadata(), operation, path)?;
    if !opened.file_type().is_file() || file_identity(&opened) != stat_identity(&initial) {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData component changed while opening it: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(opened.len().try_into().unwrap_or(0));
    io_at_path(file.read_to_end(&mut bytes), operation, path)?;
    let final_stat = stat_at(directory_fd, name, operation, path)?;
    if !stat_is_regular(&final_stat) || stat_identity(&final_stat) != file_identity(&opened) {
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
    if extra_transaction_scope_is_active() && is_within_anchored_transaction_namespace(path) {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData file is inside the anchored transaction directories but has no FD-relative mapping ({operation}): {}",
            path.display()
        )));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
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
    #[cfg(unix)]
    if let Some((directory_fd, name)) = anchored_file_location(path, operation)? {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before reading transaction identity",
        )?;
        let metadata = stat_at(directory_fd, &name, operation, path)?;
        if !stat_is_regular(&metadata) {
            return Err(ConversionError::InvalidSave(format!(
                "ExtData transaction artifact must be a regular non-symlink file: {}",
                path.display()
            )));
        }
        return Ok(stat_identity(&metadata));
    }
    if extra_transaction_scope_is_active() && is_within_anchored_transaction_namespace(path) {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData file identity is inside the anchored transaction directories but has no FD-relative mapping ({operation}): {}",
            path.display()
        )));
    }

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
    #[cfg(unix)]
    if extra_transaction_scope_is_active() {
        let target_identity =
            regular_file_identity(target, "reading anchored target ExtData component identity")?;
        let staging_metadata = io_at_path(
            fs::metadata(staging),
            "reading staged ExtData component identity",
            staging,
        )?;
        return Ok(file_identity(&staging_metadata) == target_identity);
    }

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

fn controlled_transaction_directory(
    target_dir: &Path,
    transaction_id: &str,
) -> Result<PathBuf, ConversionError> {
    validate_transaction_id(transaction_id)?;
    Ok(target_dir.join(format!(
        "{EXTRA_TRANSACTION_DIRECTORY_PREFIX}{transaction_id}"
    )))
}

fn controlled_backup_path(
    transaction_dir: &Path,
    component: &str,
    previous_sha256: &str,
) -> Result<PathBuf, ConversionError> {
    validate_sha256(previous_sha256, "previous")?;
    Ok(transaction_dir.join(format!(".{component}.mh3g-extra-backup-{previous_sha256}")))
}

fn controlled_temporary_path(
    transaction_dir: &Path,
    component: &str,
    transaction_id: &str,
) -> Result<PathBuf, ConversionError> {
    validate_transaction_id(transaction_id)?;
    Ok(transaction_dir.join(format!(".{component}.mh3g-extra-tmp-{transaction_id}")))
}

fn validate_controlled_transaction_directory(
    transaction_dir: &Path,
    target_dir: &Path,
    transaction_id: &str,
    operation: &'static str,
) -> Result<(), ConversionError> {
    let expected = controlled_transaction_directory(target_dir, transaction_id)?;
    if !is_normalized_absolute(transaction_dir) || transaction_dir != expected {
        return Err(ConversionError::InvalidSave(
            "ExtData transaction directory is not bound to its target directory".to_owned(),
        ));
    }

    #[cfg(unix)]
    if let Some((directory_fd, name)) = anchored_file_location(transaction_dir, operation)? {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before validating the transaction directory",
        )?;
        let metadata = stat_at(directory_fd, &name, operation, transaction_dir)?;
        if !stat_is_directory(&metadata) {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData transaction directory is not a real directory: {}",
                transaction_dir.display()
            )));
        }
        return Ok(());
    }
    if extra_transaction_scope_is_active() {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction directory is outside the anchored transaction scope: {}",
            transaction_dir.display()
        )));
    }

    let metadata = io_at_path(
        fs::symlink_metadata(transaction_dir),
        operation,
        transaction_dir,
    )?;
    if !metadata.file_type().is_dir() {
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData transaction directory is not a real directory: {}",
            transaction_dir.display()
        )));
    }
    Ok(())
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
        hasher.update(b"\n");
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
    // Every transaction artifact is flushed before replacement. ReplaceFileW
    // then moves the former target into the manifest-bound backup pathname as
    // part of the same filesystem operation. The recovery journal and stable
    // Windows volume/file identities make an interrupted group recoverable.
    Ok(())
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
    let parent = normalize_directory(parent, "ExtData rollback manifest directory")?;
    let target_dir = if parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(EXTRA_TRANSACTION_DIRECTORY_PREFIX))
    {
        let target_dir = parent.parent().ok_or_else(|| {
            ConversionError::InvalidSave(format!(
                "ExtData transaction directory has no target parent: {}",
                parent.display()
            ))
        })?;
        normalize_directory(target_dir, "ExtData rollback target directory")?
    } else {
        parent.clone()
    };
    Ok((
        parent.join(filename.expect("filename was validated above")),
        target_dir,
    ))
}

fn validate_manifest_directory_identities(
    manifest: &ExtraInstallManifest,
    target_dir: &Path,
    transaction_dir: &Path,
) -> Result<(), ConversionError> {
    let expected_target_identity = manifest.target_dir_identity.ok_or_else(|| {
        ConversionError::InvalidSave(
            "v5 ExtData manifest must record its target directory identity".to_owned(),
        )
    })?;
    let expected_transaction_identity = manifest.transaction_dir_identity.ok_or_else(|| {
        ConversionError::InvalidSave(
            "v5 ExtData manifest must record its transaction directory identity".to_owned(),
        )
    })?;

    #[cfg(unix)]
    {
        ensure_extra_transaction_directory_bindings(
            "rechecking ExtData directories before validating manifest identities",
        )?;
        let observed_target_identity = anchored_directory_identity(target_dir).ok_or_else(|| {
            ConversionError::UnsafeInstall(format!(
                "ExtData rollback target directory is outside the anchored transaction scope: {}",
                target_dir.display()
            ))
        })?;
        let observed_transaction_identity = anchored_directory_identity(transaction_dir).ok_or_else(|| {
            ConversionError::UnsafeInstall(format!(
                "ExtData rollback transaction directory is outside the anchored transaction scope: {}",
                transaction_dir.display()
            ))
        })?;
        if observed_target_identity != expected_target_identity {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData rollback target directory identity does not match its recovery journal: {}",
                target_dir.display()
            )));
        }
        if observed_transaction_identity != expected_transaction_identity {
            return Err(ConversionError::UnsafeInstall(format!(
                "ExtData rollback transaction directory identity does not match its recovery journal: {}",
                transaction_dir.display()
            )));
        }
        Ok(())
    }

    #[cfg(windows)]
    {
        let observed_target_identity = windows_directory_identity(
            target_dir,
            "validating Windows ExtData rollback target directory identity",
        )?;
        let observed_transaction_identity = windows_directory_identity(
            transaction_dir,
            "validating Windows ExtData rollback transaction directory identity",
        )?;
        if observed_target_identity != expected_target_identity {
            return Err(ConversionError::UnsafeInstall(format!(
                "Windows ExtData rollback target directory identity does not match its recovery journal: {}",
                target_dir.display()
            )));
        }
        if observed_transaction_identity != expected_transaction_identity {
            return Err(ConversionError::UnsafeInstall(format!(
                "Windows ExtData rollback transaction directory identity does not match its recovery journal: {}",
                transaction_dir.display()
            )));
        }
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (
            target_dir,
            transaction_dir,
            expected_target_identity,
            expected_transaction_identity,
        );
        Err(ConversionError::UnsafeInstall(
            "v5 ExtData rollback requires POSIX directory identity verification".to_owned(),
        ))
    }
}

fn validate_rollback_manifest(
    manifest: &ExtraInstallManifest,
    manifest_path: &Path,
    target_dir: &Path,
) -> Result<Vec<RollbackEntry>, ConversionError> {
    if !matches!(
        manifest.version,
        EXTRA_INSTALL_MANIFEST_VERSION
            | PREVIOUS_EXTRA_INSTALL_MANIFEST_VERSION
            | LEGACY_EXTRA_INSTALL_MANIFEST_VERSION
    ) {
        return Err(ConversionError::InvalidSave(format!(
            "unsupported ExtData install manifest version: {}",
            manifest.version
        )));
    }
    if !is_normalized_absolute(&manifest.staging_dir)
        || !is_normalized_absolute(&manifest.target_dir)
        || manifest.target_dir != target_dir
    {
        return Err(ConversionError::InvalidSave(
            "ExtData manifest paths must be normalized absolute controlled paths".to_owned(),
        ));
    }
    validate_sha256(&manifest.staging_set_sha256, "manifest staging set")?;
    validate_sha256(&manifest.target_set_sha256, "manifest target set")?;
    validate_transaction_id(&manifest.transaction_id)?;
    validate_manifest_groups(&manifest.groups)?;

    let transaction_artifact_dir = if matches!(
        manifest.version,
        EXTRA_INSTALL_MANIFEST_VERSION | PREVIOUS_EXTRA_INSTALL_MANIFEST_VERSION
    ) {
        let transaction_dir = manifest.transaction_dir.as_deref().ok_or_else(|| {
            ConversionError::InvalidSave(
                "transactional ExtData manifest must record its transaction directory".to_owned(),
            )
        })?;
        validate_controlled_transaction_directory(
            transaction_dir,
            target_dir,
            &manifest.transaction_id,
            "validating ExtData rollback transaction directory",
        )?;
        if manifest_path != transaction_dir.join(EXTRA_RECOVERY_JOURNAL_NAME) {
            return Err(ConversionError::InvalidSave(
                "transactional ExtData rollback manifest is not the transaction recovery journal"
                    .to_owned(),
            ));
        }
        transaction_dir.to_path_buf()
    } else {
        if manifest.transaction_dir.is_some()
            || manifest_path.parent() != Some(target_dir)
            || !matches!(
                manifest_path.file_name().and_then(|name| name.to_str()),
                Some(EXTRA_MANIFEST_NAME | EXTRA_RECOVERY_JOURNAL_NAME)
            )
        {
            return Err(ConversionError::InvalidSave(
                "legacy ExtData manifest is not in its controlled target directory".to_owned(),
            ));
        }
        target_dir.to_path_buf()
    };

    if manifest.version == EXTRA_INSTALL_MANIFEST_VERSION {
        validate_manifest_directory_identities(manifest, target_dir, &transaction_artifact_dir)?;
    } else {
        // v3/v4 journals predate persisted directory identities.  Their
        // component hashes and controlled filenames cannot distinguish an
        // original ExtData directory from a replacement at the same path,
        // especially after a crash or process restart.  Recognize the legacy
        // layout, but never mutate it without that proof.
        return Err(ConversionError::UnsafeInstall(format!(
            "ExtData v{} rollback lacks persisted target and transaction directory identities; refuse to mutate a legacy journal",
            manifest.version
        )));
    }

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
        let expected_temporary = controlled_temporary_path(
            &transaction_artifact_dir,
            &entry.component,
            &manifest.transaction_id,
        )?;
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
                let expected_backup = controlled_backup_path(
                    &transaction_artifact_dir,
                    &entry.component,
                    before_sha256,
                )?;
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
                (Some(before_sha256), Some(backup), None) => (
                    true,
                    Some(read_and_validate_rollback_backup(
                        backup,
                        before_sha256,
                        &entry.component,
                    )?),
                ),
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
