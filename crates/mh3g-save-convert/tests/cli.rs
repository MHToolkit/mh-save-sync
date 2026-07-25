use std::{collections::BTreeSet, fs, path::PathBuf, process::Command};

use serde_json::Value;
use sha2::Digest;
use tempfile::TempDir;

use mh3g_save_convert::profile::build_jp_cemu_header;

#[cfg(target_os = "macos")]
use std::{
    os::unix::fs::PermissionsExt,
    process::{Child, Stdio},
    sync::Mutex,
};

const THREE_DS_SIZE: usize = 0x8A00;
const CEMU_SIZE: usize = 0x8A24;
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

fn target_slot(temp: &TempDir, slot: &str) -> PathBuf {
    let directory = temp.path().join("cemu");
    fs::create_dir_all(&directory).unwrap();
    directory.join(slot)
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
fn inspect_cec_reports_3ds_outbox_and_empty_cemu_cache_without_writing() {
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
    assert_eq!(value["source"]["outbox"]["declared_message_count"], 1);
    assert_eq!(value["source"]["outbox"]["actual_message_count"], 1);
    assert_eq!(value["source"]["inbox"]["declared_message_count"], 0);
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
    assert!(value["backup"].is_null());
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
fn convert_cec_write_keeps_a_hash_addressed_backup_and_manifest() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = cec_fixture(&temp);
    let target = cemu_cec_fixture(&temp);
    let before = fs::read(&target).unwrap();
    let before_hash = sha2::Sha256::digest(&before);
    let before_hash = hex::encode(before_hash);

    let value = run_json_with_stopped_emulators(&[
        "convert-cec".into(),
        "--source-dir".into(),
        source.to_string_lossy().into_owned(),
        "--target".into(),
        target.to_string_lossy().into_owned(),
        "--write".into(),
        "--experimental".into(),
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
fn write_then_rollback_restores_previous_slot() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let previous = vec![0xA5; CEMU_SIZE];
    fs::write(&target, &previous).unwrap();

    let converted = run_json_with_stopped_emulators(&[
        "convert".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
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
