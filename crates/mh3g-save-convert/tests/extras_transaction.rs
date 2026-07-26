use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use mh3g_save_convert::{
    ConversionError,
    extras_transaction::{
        ExtraFileOperations, ExtraGroup, ExtraInstallManifest, StdExtraFileOperations,
        dry_run_extra_groups_with, install_extra_groups_with, rollback_extra_groups_with,
    },
    process_probe::ProcessProbe,
    profile::build_jp_cemu_header,
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
        .filter(|name| name.contains("mh3g-extra-"))
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "unexpected transaction files: {leftovers:?}"
    );
}

struct FailingOperations {
    replacements: AtomicUsize,
    fail_second_replace: bool,
    fail_manifest_publish: bool,
}

impl FailingOperations {
    fn second_replace() -> Self {
        Self {
            replacements: AtomicUsize::new(0),
            fail_second_replace: true,
            fail_manifest_publish: false,
        }
    }

    fn manifest_publish() -> Self {
        Self {
            replacements: AtomicUsize::new(0),
            fail_second_replace: false,
            fail_manifest_publish: true,
        }
    }
}

impl ExtraFileOperations for FailingOperations {
    fn write_new_file(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.write_new_file(path, bytes)
    }

    fn replace_staged(&self, staged: &Path, target: &Path) -> Result<(), ConversionError> {
        let replacement = self.replacements.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_second_replace && replacement == 2 {
            return Err(ConversionError::UnsafeInstall(
                "injected second replacement failure".to_owned(),
            ));
        }
        StdExtraFileOperations.replace_staged(staged, target)
    }

    fn restore_target(&self, target: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        StdExtraFileOperations.restore_target(target, bytes)
    }

    fn remove_regular_file(&self, path: &Path) -> Result<(), ConversionError> {
        StdExtraFileOperations.remove_regular_file(path)
    }

    fn publish_manifest(&self, path: &Path, bytes: &[u8]) -> Result<(), ConversionError> {
        if self.fail_manifest_publish {
            return Err(ConversionError::UnsafeInstall(
                "injected manifest publication failure".to_owned(),
            ));
        }
        StdExtraFileOperations.publish_manifest(path, bytes)
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
    assert_no_transaction_artifacts(&staging);
    assert_no_transaction_artifacts(&target);
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
    for component in ExtraGroup::GuildCards.components() {
        assert_eq!(
            fs::read(target.join(component)).unwrap(),
            fs::read(staging.join(component)).unwrap(),
            "component {component}"
        );
    }
}

#[test]
fn second_replacement_failure_restores_every_target_and_cleans_artifacts() {
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
    assert_target_bytes(&target, &before);
    assert_no_transaction_artifacts(&target);
}

#[test]
fn rollback_restores_existing_targets_and_removes_originally_absent_targets() {
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
    assert_no_transaction_artifacts(&target);
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
fn manifest_publication_failure_restores_targets_and_cleans_artifacts() {
    let temp = tempdir().unwrap();
    let staging = temp.path().join("staging");
    let target = temp.path().join("target");
    write_group(&staging, ExtraGroup::GuildCards, 0x82);
    write_group(&target, ExtraGroup::GuildCards, 0xE2);
    let before = target_bytes(&target, ExtraGroup::GuildCards);
    let operations = FailingOperations::manifest_publish();

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
        matches!(error, ConversionError::UnsafeInstall(message) if message.contains("manifest publication"))
    );
    assert_target_bytes(&target, &before);
    assert_no_transaction_artifacts(&target);
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
    fs::create_dir_all(&target).unwrap();

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
