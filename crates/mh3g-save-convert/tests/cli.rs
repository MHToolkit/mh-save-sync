use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;
use sha2::Digest;
use tempfile::TempDir;

use mh3g_save_convert::profile::build_jp_cemu_header;

#[cfg(target_os = "macos")]
use std::{
    os::unix::fs::PermissionsExt,
    process::{Child, Output, Stdio},
    sync::Mutex,
};

#[cfg(not(target_os = "macos"))]
use std::process::Output;

const THREE_DS_SIZE: usize = 0x8A00;
const CEMU_SIZE: usize = 0x8A24;
const THREE_DS_SYSTEM_SIZE: usize = 0x3000;
const CEMU_SYSTEM_SIZE: usize = 0x3024;
const CEMU_CEC_PAYLOAD_SIZE: usize = 0x835FC;
#[cfg(target_os = "macos")]
static PROCESS_GUARD: Mutex<()> = Mutex::new(());

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mh3g-save-convert"))
}

fn source_fixture(temp: &TempDir) -> PathBuf {
    write_source(temp.path().join("source.bin"))
}

fn slot_fixture(temp: &TempDir, slot: &str) -> PathBuf {
    let directory = temp.path().join("3ds");
    fs::create_dir_all(&directory).unwrap();
    write_source(directory.join(slot))
}

fn system_fixture(temp: &TempDir) -> PathBuf {
    let directory = temp.path().join("3ds");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("system");
    let mut bytes = vec![0_u8; THREE_DS_SYSTEM_SIZE];
    bytes[..4].copy_from_slice(&[0x2B, 0, 0, 0]);
    fs::write(&path, bytes).unwrap();
    path
}

fn target_slot(temp: &TempDir, slot: &str) -> PathBuf {
    let directory = temp.path().join("cemu");
    fs::create_dir_all(&directory).unwrap();
    directory.join(slot)
}

fn extras_fixture(temp: &TempDir) -> PathBuf {
    let source_dir = temp.path().join("3ds-extdata");
    fs::create_dir_all(&source_dir).unwrap();
    for (component, size) in [
        ("card1", 0x58_000),
        ("card2", 0x58_000),
        ("card3", 0x58_000),
        ("cardbox", 0x30_000),
        ("quest1", 0x29_000),
        ("quest2", 0x29_000),
        ("quest3", 0x29_000),
        ("quest4", 0x29_000),
    ] {
        let mut bytes = vec![0_u8; size];
        bytes[..4].copy_from_slice(&[0x2B, 0, 0, 0]);
        fs::write(source_dir.join(component), bytes).unwrap();
    }
    source_dir
}

fn staged_extras_fixture(temp: &TempDir) -> PathBuf {
    let source_dir = extras_fixture(temp);
    let staging_dir = temp.path().join("staged-extras");
    let report = run_json(&[
        "convert-extras".into(),
        "--source-dir".into(),
        source_dir.to_string_lossy().into_owned(),
        "--output-dir".into(),
        staging_dir.to_string_lossy().into_owned(),
        "--write".into(),
    ]);
    assert_eq!(report["status"], "written");
    staging_dir
}

fn initialized_extra_target(temp: &TempDir, staging_dir: &Path) -> PathBuf {
    let target_dir = temp.path().join("cemu-extras");
    fs::create_dir_all(&target_dir).unwrap();
    for component in [
        "card1", "card2", "card3", "cardbox", "quest1", "quest2", "quest3", "quest4",
    ] {
        fs::copy(staging_dir.join(component), target_dir.join(component)).unwrap();
    }

    // Keep a valid Cemu header while ensuring the target set is observably
    // different from the staged set before installation.
    let card1 = target_dir.join("card1");
    let mut bytes = fs::read(&card1).unwrap();
    bytes[0x28] = 0xA5;
    fs::write(card1, bytes).unwrap();
    target_dir
}

fn write_source(path: PathBuf) -> PathBuf {
    let mut bytes = vec![0_u8; THREE_DS_SIZE];
    bytes[..4].copy_from_slice(&[0x2B, 0, 0, 0]);
    fs::write(&path, bytes).unwrap();
    path
}

fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn cec_box_info(message_count: u32, box_size: u32) -> Vec<u8> {
    let mut bytes = vec![0_u8; 0x20];
    bytes[..2].copy_from_slice(&0x6262_u16.to_le_bytes());
    write_u32_le(&mut bytes, 4, 0x20);
    write_u32_le(&mut bytes, 8, 0x9000);
    write_u32_le(&mut bytes, 12, box_size);
    write_u32_le(&mut bytes, 16, 8);
    write_u32_le(&mut bytes, 20, message_count);
    write_u32_le(&mut bytes, 24, 8);
    write_u32_le(&mut bytes, 28, 0x4000);
    bytes
}

fn cec_message() -> Vec<u8> {
    const HEADER_SIZE: usize = 0xD80;
    const BODY_SIZE: usize = 0x2A08;
    // Native messages carry a 0x20-byte trailer after the declared body.
    let mut bytes = vec![0_u8; HEADER_SIZE + BODY_SIZE + 0x20];
    let message_size = bytes.len() as u32;
    bytes[..2].copy_from_slice(&0x6060_u16.to_le_bytes());
    write_u32_le(&mut bytes, 4, message_size);
    write_u32_le(&mut bytes, 8, HEADER_SIZE as u32);
    write_u32_le(&mut bytes, 12, BODY_SIZE as u32);
    write_u32_le(&mut bytes, 16, 0x0004_8100);
    write_u32_le(&mut bytes, 24, 7);
    bytes[32..40].copy_from_slice(b"CEC-TEST");
    // A 3DS CEC body carries an 8-byte prefix followed by the 0x2A00-byte
    // record-shaped candidate observed in MH3G's outgoing mailbox data.
    bytes[HEADER_SIZE + 8] = 0xA5;
    bytes
}

fn cemu_cec_fixture(temp: &TempDir) -> PathBuf {
    let path = temp.path().join("cemu").join("cec");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut bytes = vec![0_u8; 40 + CEMU_CEC_PAYLOAD_SIZE];
    bytes[..40].copy_from_slice(&build_jp_cemu_header("cec", CEMU_CEC_PAYLOAD_SIZE).unwrap());
    fs::write(&path, bytes).unwrap();
    path
}

fn cec_fixture(temp: &TempDir) -> PathBuf {
    let root = temp.path().join("3ds-cec");
    let inbox = root.join("InBox___");
    let outbox = root.join("OutBox__");
    fs::create_dir_all(&inbox).unwrap();
    fs::create_dir_all(&outbox).unwrap();
    fs::write(inbox.join("BoxInfo_____"), cec_box_info(1, 0x2A08)).unwrap();
    fs::write(inbox.join("_CEC-TEST"), cec_message()).unwrap();
    fs::write(outbox.join("BoxInfo_____"), cec_box_info(0, 0)).unwrap();
    root
}

fn outbox_only_cec_fixture(temp: &TempDir) -> PathBuf {
    let root = temp.path().join("3ds-cec-outbox-only");
    let inbox = root.join("InBox___");
    let outbox = root.join("OutBox__");
    fs::create_dir_all(&inbox).unwrap();
    fs::create_dir_all(&outbox).unwrap();
    fs::write(inbox.join("BoxInfo_____"), cec_box_info(0, 0)).unwrap();
    fs::write(outbox.join("BoxInfo_____"), cec_box_info(1, 0x2A08)).unwrap();
    fs::write(outbox.join("_CEC-TEST"), cec_message()).unwrap();
    root
}

fn run_json(args: &[String]) -> Value {
    run_json_command(binary(), args)
}

fn run_json_command(mut command: Command, args: &[String]) -> Value {
    let output = command.args(args).output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[cfg(target_os = "macos")]
fn run_json_with_stopped_emulators(args: &[String]) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let pgrep = directory.path().join("pgrep");
    fs::write(&pgrep, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&pgrep, fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = binary();
    command.env("PATH", directory.path());
    run_json_command(command, args)
}

#[cfg(not(target_os = "macos"))]
fn run_json_with_stopped_emulators(args: &[String]) -> Value {
    run_json(args)
}

#[cfg(target_os = "macos")]
fn run_output_with_stopped_emulators(args: &[String]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let pgrep = directory.path().join("pgrep");
    fs::write(&pgrep, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&pgrep, fs::Permissions::from_mode(0o755)).unwrap();
    binary()
        .env("PATH", directory.path())
        .args(args)
        .output()
        .unwrap()
}

#[cfg(not(target_os = "macos"))]
fn run_output_with_stopped_emulators(args: &[String]) -> Output {
    binary().args(args).output().unwrap()
}

fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn inspect_reports_metadata_without_decoded_player_data() {
    let temp = tempfile::tempdir().unwrap();
    let source = source_fixture(&temp);
    let value = run_json(&["inspect".into(), source.to_string_lossy().into_owned()]);

    assert_eq!(
        keys(&value),
        BTreeSet::from_iter(
            [
                "profile", "size", "hashes", "output", "backup", "manifest", "status"
            ]
            .map(str::to_owned)
        )
    );
    assert_eq!(value["profile"], "JpThreeDs");
    assert_eq!(value["size"], THREE_DS_SIZE);
    assert_eq!(value["status"], "inspected");
    assert!(value["hashes"]["source"].as_str().unwrap().len() == 64);
    assert!(value.get("player").is_none());
}

#[test]
fn inspect_cec_reports_3ds_inbox_and_empty_cemu_cache_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);

    let value = run_json(&[
        "inspect-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
    ]);

    assert_eq!(value["status"], "inspected-cec");
    assert_eq!(value["source"]["inbox"]["declared_message_count"], 1);
    assert_eq!(value["source"]["inbox"]["actual_message_count"], 1);
    assert_eq!(value["source"]["outbox"]["declared_message_count"], 0);
    assert_eq!(value["source"]["messages"][0]["title_id"], "0x00048100");
    assert_eq!(
        value["source"]["messages"][0]["record_candidate_offset"],
        0xD88
    );
    assert_eq!(
        value["source"]["messages"][0]["record_candidate_size"],
        0x2A00
    );
    assert_eq!(
        value["source"]["messages"][0]["record_candidate_nonzero_bytes"],
        1
    );
    assert_eq!(value["target"]["payload_size"], CEMU_CEC_PAYLOAD_SIZE);
    assert_eq!(value["target"]["logical_source_size"], 0x83600);
    assert_eq!(value["target"]["nonzero_payload_bytes"], 0);
    assert_eq!(value["target"]["record_area_offset"], 0x1FC);
    assert_eq!(value["target"]["record_slot_size"], 0x2A00);
    assert_eq!(value["target"]["record_slot_count"], 50);
    assert!(value["target"]["expected_layout"] == true);
    assert!(value["target"]["is_empty"] == true);
    assert!(!target.with_extension("bak").exists());
}

#[test]
fn convert_cec_dry_run_plans_the_first_empty_cemu_slot_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let before = fs::read(&target).unwrap();

    let value = run_json(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
    ]);

    assert_eq!(value["status"], "dry-run");
    assert_eq!(value["imported_messages"], 1);
    assert_eq!(value["slots"][0], 0);
    assert_eq!(
        value["source_record_set_sha256"].as_str().unwrap().len(),
        64
    );
    assert_eq!(value["target_sha256_before"].as_str().unwrap().len(), 64);
    assert!(value["backup"].is_null());
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[test]
fn convert_cec_refuses_outbox_only_records_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let source = outbox_only_cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let before = fs::read(&target).unwrap();

    let output = binary()
        .args([
            "convert-cec",
            "--source-dir",
            &source.display().to_string(),
            "--target",
            &target.display().to_string(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("InBox___"));
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[test]
fn convert_cec_write_requires_experimental_acknowledgement() {
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let before = fs::read(&target).unwrap();

    let output = binary()
        .args([
            "convert-cec",
            "--source-dir",
            &source.display().to_string(),
            "--target",
            &target.display().to_string(),
            "--write",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--experimental"));
    assert_eq!(fs::read(&target).unwrap(), before);
}

#[test]
fn convert_cec_write_requires_dry_run_hash_preconditions() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let before = fs::read(&target).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
        "--write".into(),
        "--experimental".into(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--expected-source-record-set-sha256")
    );
    assert_eq!(fs::read(&target).unwrap(), before);
    assert!(
        !target
            .parent()
            .unwrap()
            .join(".cec.mh3g-install.json")
            .exists()
    );
}

#[test]
fn convert_cec_write_keeps_a_hash_addressed_backup_and_manifest() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let before = fs::read(&target).unwrap();
    let before_hash = sha2::Sha256::digest(&before);
    let before_hash = hex::encode(before_hash);
    let dry_run = run_json(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
    ]);
    let expected_source_record_set = dry_run["source_record_set_sha256"]
        .as_str()
        .unwrap()
        .to_owned();
    let expected_target = dry_run["target_sha256_before"].as_str().unwrap().to_owned();

    let value = run_json_with_stopped_emulators(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
        "--write".into(),
        "--experimental".into(),
        "--expected-source-record-set-sha256".into(),
        expected_source_record_set,
        "--expected-target-sha256".into(),
        expected_target,
    ]);

    assert_eq!(value["status"], "written");
    let backup = PathBuf::from(value["backup"].as_str().unwrap());
    let manifest = PathBuf::from(value["manifest"].as_str().unwrap());
    assert_eq!(fs::read(&backup).unwrap(), before);
    assert!(manifest.exists());
    assert_eq!(
        backup.file_name().unwrap().to_string_lossy(),
        format!(".cec.mh3g-backup-{before_hash}")
    );
    assert_ne!(fs::read(&target).unwrap(), before);

    let rollback = run_json_with_stopped_emulators(&[
        "rollback-cec".into(),
        "--manifest".into(),
        manifest.display().to_string(),
    ]);
    assert_eq!(rollback["status"], "rolled-back");
    assert_eq!(fs::read(&target).unwrap(), before);
    assert!(!backup.exists());
    assert!(!manifest.exists());
}

#[test]
fn convert_cec_write_rejects_a_stale_expected_source_record_set_without_replacing_target() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let before = fs::read(&target).unwrap();
    let dry_run = run_json(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
    ]);

    let message = source.join("InBox___").join("_CEC-TEST");
    let mut changed_message = fs::read(&message).unwrap();
    changed_message[0xD80 + 8] ^= 0x01;
    fs::write(&message, changed_message).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
        "--write".into(),
        "--experimental".into(),
        "--expected-source-record-set-sha256".into(),
        dry_run["source_record_set_sha256"]
            .as_str()
            .unwrap()
            .to_owned(),
        "--expected-target-sha256".into(),
        dry_run["target_sha256_before"].as_str().unwrap().to_owned(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("source record set SHA-256 does not match the expected dry-run value")
    );
    assert_eq!(fs::read(&target).unwrap(), before);
    assert!(
        !target
            .parent()
            .unwrap()
            .join(".cec.mh3g-install.json")
            .exists()
    );
}

#[test]
fn convert_cec_write_rejects_a_stale_expected_target_hash_without_replacing_target() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let dry_run = run_json(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
    ]);
    let mut changed_target = fs::read(&target).unwrap();
    changed_target[40 + 0x1FC] = 0x5A;
    fs::write(&target, &changed_target).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
        "--write".into(),
        "--experimental".into(),
        "--expected-source-record-set-sha256".into(),
        dry_run["source_record_set_sha256"]
            .as_str()
            .unwrap()
            .to_owned(),
        "--expected-target-sha256".into(),
        dry_run["target_sha256_before"].as_str().unwrap().to_owned(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("target SHA-256 does not match the expected dry-run value")
    );
    assert_eq!(fs::read(&target).unwrap(), changed_target);
    assert!(
        !target
            .parent()
            .unwrap()
            .join(".cec.mh3g-install.json")
            .exists()
    );
}

#[test]
fn convert_cec_write_binds_a_missing_target_to_the_empty_cemu_container_hash() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = temp.path().join("cemu").join("cec");
    fs::create_dir_all(target.parent().unwrap()).unwrap();

    let dry_run = run_json(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
    ]);
    let value = run_json_with_stopped_emulators(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
        "--write".into(),
        "--experimental".into(),
        "--expected-source-record-set-sha256".into(),
        dry_run["source_record_set_sha256"]
            .as_str()
            .unwrap()
            .to_owned(),
        "--expected-target-sha256".into(),
        dry_run["target_sha256_before"].as_str().unwrap().to_owned(),
    ]);

    assert_eq!(value["status"], "written");
    assert_eq!(
        value["target_sha256_before"],
        dry_run["target_sha256_before"]
    );
    assert!(target.is_file());
}

#[test]
fn convert_cec_expected_hash_arguments_require_write() {
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);

    for flag in [
        "--expected-source-record-set-sha256",
        "--expected-target-sha256",
    ] {
        let output = binary()
            .args([
                "convert-cec",
                "--source-dir",
                &source.to_string_lossy(),
                "--target",
                &target.to_string_lossy(),
                flag,
                &"0".repeat(64),
            ])
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(2), "flag: {flag}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--write"),
            "flag: {flag}"
        );
    }
}

#[test]
fn convert_defaults_to_dry_run_and_creates_no_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let value = run_json(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
    ]);

    assert_eq!(value["profile"], "JpCemu");
    assert_eq!(value["size"], CEMU_SIZE);
    assert_eq!(value["status"], "dry-run");
    assert_eq!(value["output"], target.to_string_lossy().as_ref());
    assert!(value["manifest"].is_null());
    assert!(!target.exists());
    assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 0);
}

#[test]
fn convert_write_rejects_a_stale_expected_target_hash_without_replacing_target() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let previous = vec![0xA5; CEMU_SIZE];
    fs::write(&target, &previous).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-target-sha256".into(),
        "0".repeat(64),
        "--write".into(),
    ]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("target SHA-256 does not match the expected dry-run value")
    );
    assert_eq!(fs::read(&target).unwrap(), previous);
    assert!(
        !target
            .parent()
            .unwrap()
            .join(".user2.mh3g-install.json")
            .exists()
    );
}

#[test]
fn convert_write_rejects_an_expected_target_hash_when_the_target_is_missing() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();

    for (command, source, target) in [
        (
            "convert",
            slot_fixture(&temp, "user2"),
            target_slot(&temp, "user2"),
        ),
        (
            "convert-system",
            system_fixture(&temp),
            target_slot(&temp, "system"),
        ),
    ] {
        let output = run_output_with_stopped_emulators(&[
            command.to_owned(),
            source.to_string_lossy().into_owned(),
            "--output".to_owned(),
            target.to_string_lossy().into_owned(),
            "--expected-target-sha256".to_owned(),
            "0".repeat(64),
            "--write".to_owned(),
        ]);

        assert_eq!(output.status.code(), Some(1), "command: {command}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("target is missing but an expected dry-run SHA-256 was supplied"),
            "command: {command}"
        );
        assert!(!target.exists(), "command: {command}");
        assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 0);
    }
}

#[test]
fn convert_write_creates_a_new_export_only_when_the_target_stays_absent() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");

    let dry_run = run_json(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    assert!(dry_run["hashes"].get("target_before").is_none());

    let written = run_json_with_stopped_emulators(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-source-sha256".into(),
        dry_run["hashes"]["source"].as_str().unwrap().into(),
        "--expected-target-absent".into(),
        "--write".into(),
    ]);

    assert_eq!(written["status"], "written");
    assert!(target.is_file());
    assert!(written["backup"].is_null());
}

#[test]
fn convert_write_refuses_an_export_target_that_appeared_after_dry_run() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let dry_run = run_json(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    let appeared = vec![0xA5; CEMU_SIZE];
    fs::write(&target, &appeared).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-source-sha256".into(),
        dry_run["hashes"]["source"].as_str().unwrap().into(),
        "--expected-target-absent".into(),
        "--write".into(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("target appeared after Dry Run and was expected to remain absent")
    );
    assert_eq!(fs::read(&target).unwrap(), appeared);
    assert!(
        !target
            .parent()
            .unwrap()
            .join(".user2.mh3g-install.json")
            .exists()
    );
}

#[test]
fn convert_write_rejects_a_stale_expected_source_hash_without_replacing_target() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let previous = vec![0xA5; CEMU_SIZE];
    fs::write(&target, &previous).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-source-sha256".into(),
        "0".repeat(64),
        "--write".into(),
    ]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("source SHA-256 does not match the expected dry-run value")
    );
    assert_eq!(fs::read(&target).unwrap(), previous);
    assert!(
        !target
            .parent()
            .unwrap()
            .join(".user2.mh3g-install.json")
            .exists()
    );
}

#[test]
fn convert_dry_run_reports_the_existing_target_hash() {
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let previous = vec![0xA5; CEMU_SIZE];
    fs::write(&target, &previous).unwrap();

    let report = run_json(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);

    assert_eq!(report["status"], "dry-run");
    assert_eq!(
        report["hashes"]["target_before"],
        hex::encode(sha2::Sha256::digest(previous))
    );
}

#[test]
fn convert_system_write_rejects_a_stale_expected_target_hash_without_replacing_target() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = system_fixture(&temp);
    let target = target_slot(&temp, "system");
    let previous = vec![0xA5; CEMU_SYSTEM_SIZE];
    fs::write(&target, &previous).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "convert-system".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-target-sha256".into(),
        "0".repeat(64),
        "--write".into(),
    ]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("target SHA-256 does not match the expected dry-run value")
    );
    assert_eq!(fs::read(&target).unwrap(), previous);
    assert!(
        !target
            .parent()
            .unwrap()
            .join(".system.mh3g-install.json")
            .exists()
    );
}

#[test]
fn write_then_rollback_restores_previous_slot() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let previous = vec![0xA5; CEMU_SIZE];
    fs::write(&target, &previous).unwrap();

    let dry_run = run_json(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);

    let converted = run_json_with_stopped_emulators(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-source-sha256".into(),
        dry_run["hashes"]["source"].as_str().unwrap().into(),
        "--expected-target-sha256".into(),
        dry_run["hashes"]["target_before"].as_str().unwrap().into(),
        "--write".into(),
    ]);
    assert_eq!(converted["status"], "written");
    let manifest = PathBuf::from(converted["manifest"].as_str().unwrap());
    let backup = PathBuf::from(converted["backup"].as_str().unwrap());
    assert!(target.exists());
    assert!(manifest.exists());
    assert!(backup.exists());

    let rollback = run_json_with_stopped_emulators(&[
        "rollback".into(),
        "--manifest".into(),
        manifest.display().to_string(),
    ]);
    assert_eq!(rollback["status"], "rolled-back");
    assert_eq!(fs::read(&target).unwrap(), previous);
    assert!(!manifest.exists());
    assert!(!backup.exists());
}

#[test]
fn install_extras_dry_run_reports_one_complete_group_without_writing() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let staging_dir = staged_extras_fixture(&temp);
    let target_dir = initialized_extra_target(&temp, &staging_dir);
    let card1_before = fs::read(target_dir.join("card1")).unwrap();

    let report = run_json_with_stopped_emulators(&[
        "install-extras".into(),
        "--staging-dir".into(),
        staging_dir.to_string_lossy().into_owned(),
        "--target-dir".into(),
        target_dir.to_string_lossy().into_owned(),
        "--groups".into(),
        "guild-cards".into(),
        "--dry-run".into(),
    ]);

    assert_eq!(report["operation"], "install-extras");
    assert_eq!(report["status"], "dry-run");
    assert_eq!(report["groups"][0], "guild-cards");
    assert_eq!(report["groups"].as_array().unwrap().len(), 1);
    let entries = report["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    assert!(entries.iter().all(|entry| entry["group"] == "guild-cards"));
    let manifest = PathBuf::from(report["manifest"].as_str().unwrap());
    // v5 reserves an append-only per-transaction directory during planning.
    // Dry-run reports the exact journal location it would use, but creates
    // neither the directory nor the journal.
    let transaction_dir = manifest.parent().unwrap();
    assert_eq!(manifest.file_name().unwrap(), ".mh3g-extra-recovery.json");
    assert_eq!(
        transaction_dir.parent().unwrap(),
        target_dir.canonicalize().unwrap()
    );
    assert!(
        transaction_dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".mh3g-extra-transaction-")
    );
    assert!(!manifest.exists());
    assert!(!transaction_dir.exists());
    assert_eq!(fs::read(target_dir.join("card1")).unwrap(), card1_before);
}

#[cfg(not(windows))]
#[test]
fn install_extras_write_with_dry_run_hashes_then_rolls_back_the_complete_group() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let staging_dir = staged_extras_fixture(&temp);
    let target_dir = initialized_extra_target(&temp, &staging_dir);
    let card1_before = fs::read(target_dir.join("card1")).unwrap();

    let dry_run = run_json_with_stopped_emulators(&[
        "install-extras".into(),
        "--staging-dir".into(),
        staging_dir.to_string_lossy().into_owned(),
        "--target-dir".into(),
        target_dir.to_string_lossy().into_owned(),
        "--groups".into(),
        "guild-cards".into(),
        "--dry-run".into(),
    ]);
    let expected_staging = dry_run["staging_set_sha256"].as_str().unwrap();
    let expected_target = dry_run["target_set_sha256_before"].as_str().unwrap();

    let written = run_json_with_stopped_emulators(&[
        "install-extras".into(),
        "--staging-dir".into(),
        staging_dir.to_string_lossy().into_owned(),
        "--target-dir".into(),
        target_dir.to_string_lossy().into_owned(),
        "--groups".into(),
        "guild-cards".into(),
        "--expected-staging-set-sha256".into(),
        expected_staging.into(),
        "--expected-target-set-sha256".into(),
        expected_target.into(),
        "--write".into(),
    ]);

    assert_eq!(written["operation"], "install-extras");
    assert_eq!(written["status"], "written");
    assert_eq!(written["groups"][0], "guild-cards");
    assert_eq!(written["entries"].as_array().unwrap().len(), 4);
    let manifest = PathBuf::from(written["manifest"].as_str().unwrap());
    assert!(manifest.exists());
    assert_eq!(
        fs::read(target_dir.join("card1")).unwrap(),
        fs::read(staging_dir.join("card1")).unwrap()
    );

    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback-extras".into(),
        "--manifest".into(),
        manifest.to_string_lossy().into_owned(),
    ]);

    assert_eq!(rolled_back["operation"], "rollback-extras");
    assert_eq!(rolled_back["status"], "rolled-back");
    assert_eq!(rolled_back["groups"][0], "guild-cards");
    assert_eq!(rolled_back["entries"].as_array().unwrap().len(), 4);
    assert_eq!(
        PathBuf::from(rolled_back["manifest"].as_str().unwrap()),
        manifest
    );
    assert!(manifest.exists());
    let transaction_dir = manifest.parent().unwrap();
    assert!(transaction_dir.is_dir());
    assert!(fs::read_dir(transaction_dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("mh3g-extra-backup-")
    }));
    assert_eq!(fs::read(target_dir.join("card1")).unwrap(), card1_before);
}

#[cfg(windows)]
#[test]
fn install_extras_write_is_refused_before_mutating_the_complete_group() {
    let temp = tempfile::tempdir().unwrap();
    let staging_dir = staged_extras_fixture(&temp);
    let target_dir = initialized_extra_target(&temp, &staging_dir);
    let guild_card_before = ["card1", "card2", "card3", "cardbox"]
        .into_iter()
        .map(|component| {
            (
                component.to_owned(),
                fs::read(target_dir.join(component)).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let target_entries_before = fs::read_dir(&target_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();

    let dry_run = run_json(&[
        "install-extras".into(),
        "--staging-dir".into(),
        staging_dir.to_string_lossy().into_owned(),
        "--target-dir".into(),
        target_dir.to_string_lossy().into_owned(),
        "--groups".into(),
        "guild-cards".into(),
        "--dry-run".into(),
    ]);
    assert_eq!(dry_run["status"], "dry-run");

    let output = run_output_with_stopped_emulators(&[
        "install-extras".into(),
        "--staging-dir".into(),
        staging_dir.to_string_lossy().into_owned(),
        "--target-dir".into(),
        target_dir.to_string_lossy().into_owned(),
        "--groups".into(),
        "guild-cards".into(),
        "--expected-staging-set-sha256".into(),
        dry_run["staging_set_sha256"].as_str().unwrap().into(),
        "--expected-target-set-sha256".into(),
        dry_run["target_set_sha256_before"].as_str().unwrap().into(),
        "--write".into(),
    ]);

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("multi-file ExtData installation is unavailable on Windows"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (component, before) in guild_card_before {
        assert_eq!(fs::read(target_dir.join(component)).unwrap(), before);
    }
    let target_entries_after = fs::read_dir(&target_dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(target_entries_after, target_entries_before);
}

#[test]
fn dry_run_and_write_are_mutually_exclusive() {
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let output = binary()
        .args([
            "convert",
            source.to_str().unwrap(),
            "--output",
            target.to_str().unwrap(),
            "--dry-run",
            "--write",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn expected_hash_arguments_require_write() {
    let temp = tempfile::tempdir().unwrap();

    for (command, source, target) in [
        (
            "convert",
            slot_fixture(&temp, "user2"),
            target_slot(&temp, "user2"),
        ),
        (
            "convert-system",
            system_fixture(&temp),
            target_slot(&temp, "system"),
        ),
    ] {
        for flag in ["--expected-source-sha256", "--expected-target-sha256"] {
            let output = binary()
                .args([
                    command,
                    &source.to_string_lossy(),
                    "--output",
                    &target.to_string_lossy(),
                    flag,
                    &"0".repeat(64),
                ])
                .output()
                .unwrap();

            assert_eq!(
                output.status.code(),
                Some(2),
                "command: {command}; flag: {flag}"
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("--write"),
                "command: {command}; flag: {flag}"
            );
            assert!(!target.exists(), "command: {command}; flag: {flag}");
        }
    }
}

#[test]
fn invalid_paths_fail_with_one_concise_error_line() {
    let output = binary()
        .args(["inspect", "/does/not/exist"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stderr.split(|byte| *byte == b'\n').count(), 2);
    assert!(output.stdout.is_empty());
}

#[test]
fn inspect_read_failure_identifies_the_source_path_and_operation() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("missing-user2");
    let output = binary()
        .args(["inspect", source.to_str().unwrap()])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("I/O error while reading source save"));
    assert!(stderr.contains(source.to_str().unwrap()));
}

#[test]
fn convert_write_failure_identifies_the_output_path_and_operation() {
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let occupied_parent = temp.path().join("not-a-directory");
    fs::write(&occupied_parent, b"not a directory").unwrap();
    let output_path = occupied_parent.join("user2");
    let output = binary()
        .args([
            "convert",
            source.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
            "--write",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lock_path = occupied_parent.join(".user2.mh3g-install.lock");
    assert!(stderr.contains("I/O error while creating save install lock"));
    assert!(stderr.contains(lock_path.to_str().unwrap()));
}

#[test]
fn progress_and_event_read_failures_identify_the_source_path() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("user2");

    for command in ["inspect-progress", "inspect-events"] {
        let output = binary()
            .args([command, source.to_str().unwrap()])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "command: {command}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("I/O error while reading source save"),
            "command: {command}; stderr: {stderr}"
        );
        assert!(
            stderr.contains(source.to_str().unwrap()),
            "command: {command}; stderr: {stderr}"
        );
    }
}

#[test]
fn cec_commands_identify_the_actual_failed_path() {
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = temp.path().join("cec");
    fs::create_dir(&target).unwrap();

    for command in ["inspect-cec", "convert-cec"] {
        let output = binary()
            .args([
                command,
                "--source-dir",
                source.to_str().unwrap(),
                "--target",
                target.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "command: {command}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("I/O error while reading Cemu CEC target"),
            "command: {command}; stderr: {stderr}"
        );
        assert!(
            stderr.contains(target.to_str().unwrap()),
            "command: {command}; stderr: {stderr}"
        );
    }
}

#[test]
fn rollback_commands_identify_the_manifest_path() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    for (command, manifest_name, operation) in [
        (
            "rollback",
            ".user2.mh3g-install.json",
            "reading rollback manifest metadata",
        ),
        (
            "rollback-cec",
            ".cec.mh3g-install.json",
            "reading CEC rollback manifest",
        ),
    ] {
        let manifest = temp.path().join(manifest_name);
        let output = run_output_with_stopped_emulators(&[
            command.to_owned(),
            "--manifest".to_owned(),
            manifest.to_string_lossy().into_owned(),
        ]);
        assert_eq!(output.status.code(), Some(1), "command: {command}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("I/O error while {operation}")),
            "command: {command}; stderr: {stderr}"
        );
        assert!(
            stderr.contains(manifest.to_str().unwrap()),
            "command: {command}; stderr: {stderr}"
        );
    }
}

#[test]
fn convert_extras_write_failure_identifies_the_output_directory() {
    let temp = tempfile::tempdir().unwrap();
    let source = extras_fixture(&temp);
    let occupied_parent = temp.path().join("not-a-directory");
    fs::write(&occupied_parent, b"not a directory").unwrap();
    let output_dir = occupied_parent.join("extras");
    let output = binary()
        .args([
            "convert-extras",
            "--source-dir",
            source.to_str().unwrap(),
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--write",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("I/O error while creating extra-data output directory"));
    assert!(stderr.contains(output_dir.to_str().unwrap()));
}

#[test]
fn convert_rejects_cross_slot_and_non_slot_sources_without_artifacts() {
    for source_name in ["user1", "source.bin"] {
        let temp = tempfile::tempdir().unwrap();
        let source = if source_name == "source.bin" {
            source_fixture(&temp)
        } else {
            slot_fixture(&temp, source_name)
        };
        let target = target_slot(&temp, "user2");
        let output = binary()
            .args([
                "convert",
                source.to_str().unwrap(),
                "--output",
                target.to_str().unwrap(),
                "--write",
            ])
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(1), "source: {source_name}");
        assert_eq!(
            output.stderr.split(|byte| *byte == b'\n').count(),
            2,
            "source: {source_name}"
        );
        assert!(output.stdout.is_empty());
        assert!(!target.exists());
        assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 0);
    }
}

#[test]
#[cfg(target_os = "macos")]
fn write_refuses_when_a_supported_emulator_process_is_running() {
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let (process_directory, mut process) = fake_cemu_process();
    let output = binary()
        .args([
            "convert",
            source.to_str().unwrap(),
            "--output",
            target.to_str().unwrap(),
            "--write",
        ])
        .output()
        .unwrap();
    process.kill().unwrap();
    process.wait().unwrap();
    drop(process_directory);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("emulator process is running"));
    assert!(!target.exists());
}

#[cfg(target_os = "macos")]
fn fake_cemu_process() -> (TempDir, Child) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Cemu");
    fs::copy("/bin/sleep", &path).unwrap();
    let child = Command::new(&path)
        .arg("30")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // macOS pgrep uses the executable basename for its exact-name match.
    std::thread::sleep(std::time::Duration::from_millis(50));
    (directory, child)
}
