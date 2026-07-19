#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use crate::{
        ConversionError,
        converter::convert_3ds_to_cemu,
        profile::{JP_3DS_HEADER, THREE_DS_SIZE, inspect_bytes},
        transaction::{InstallValidator, ProcessProbe, install_with, rollback},
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
            Err(ConversionError::InvalidSave("simulated validation failure".to_owned()))
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

    fn paths(temp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        (temp.path().join("user2"), temp.path().join("install.json"))
    }

    fn install(
        target: &Path,
        manifest: &Path,
        validator: &dyn InstallValidator,
    ) -> crate::transaction::InstallManifest {
        install_with(&source(), &converted(), target, manifest, &Stopped, validator).unwrap()
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
        assert_eq!(manifest.source_sha256, inspect_bytes(&source()).unwrap().sha256);
        assert_eq!(manifest.installed_sha256, inspect_bytes(&new_save).unwrap().sha256);
        let from_disk: crate::transaction::InstallManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(from_disk.target, target);
    }

    #[test]
    fn installs_over_an_existing_slot_after_creating_a_same_directory_backup() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let old_save = b"preexisting cemu target".to_vec();
        fs::write(&target, &old_save).unwrap();

        let manifest = install(&target, &manifest_path, &AcceptCemu);

        assert!(manifest.target_previously_existed);
        assert_eq!(manifest.previous_sha256, Some(crate::transaction::sha256_hex(&old_save)));
        let backup = manifest.backup.unwrap();
        assert_eq!(backup.parent(), target.parent());
        assert_eq!(fs::read(backup).unwrap(), old_save);
        assert_eq!(fs::read(target).unwrap(), converted());
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

        assert!(matches!(error, ConversionError::UnsafeInstall(message) if message.contains("Cemu")));
        assert!(!target.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn rollback_restores_the_verified_backup_and_removes_transaction_files() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        let old_save = b"original cemu save".to_vec();
        fs::write(&target, &old_save).unwrap();
        let manifest = install(&target, &manifest_path, &AcceptCemu);
        let backup = manifest.backup.clone().unwrap();

        rollback(&manifest_path).unwrap();

        assert_eq!(fs::read(target).unwrap(), old_save);
        assert!(!backup.exists());
        assert!(!manifest_path.exists());
    }

    #[test]
    fn rollback_returns_an_originally_absent_slot_to_absent() {
        let temp = tempfile::tempdir().unwrap();
        let (target, manifest_path) = paths(&temp);
        install(&target, &manifest_path, &AcceptCemu);

        rollback(&manifest_path).unwrap();

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

        let error = rollback(&manifest_path).unwrap_err();

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

        let error = rollback(&manifest_path).unwrap_err();

        assert!(matches!(error, ConversionError::InvalidSave(_)));
        assert_eq!(fs::read(target).unwrap(), installed);
        assert!(manifest_path.exists());
        assert!(backup.exists());
    }
}
