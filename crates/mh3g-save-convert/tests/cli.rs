use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
};

use serde_json::Value;
use tempfile::TempDir;

const THREE_DS_SIZE: usize = 0x8A00;
const CEMU_SIZE: usize = 0x8A24;
static PROCESS_GUARD: Mutex<()> = Mutex::new(());

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mh3g-save-convert"))
}

fn source_fixture(temp: &TempDir) -> PathBuf {
    let path = temp.path().join("source.bin");
    let mut bytes = vec![0_u8; THREE_DS_SIZE];
    bytes[..4].copy_from_slice(&[0x2B, 0, 0, 0]);
    fs::write(&path, bytes).unwrap();
    path
}

fn run_json(args: &[String]) -> Value {
    let output = binary().args(args).output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
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
fn convert_defaults_to_dry_run_and_creates_no_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = source_fixture(&temp);
    let target = temp.path().join("user2");
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
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn write_then_rollback_restores_previous_slot() {
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = source_fixture(&temp);
    let target = temp.path().join("user2");
    let previous = vec![0xA5; CEMU_SIZE];
    fs::write(&target, &previous).unwrap();

    let converted = run_json(&[
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

    let rollback = run_json(&[
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
    let source = source_fixture(&temp);
    let target = temp.path().join("user2");
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
fn write_refuses_when_a_supported_emulator_process_is_running() {
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = source_fixture(&temp);
    let target = temp.path().join("user2");
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
