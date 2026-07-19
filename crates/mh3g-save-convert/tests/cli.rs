use std::{collections::BTreeSet, fs, path::PathBuf, process::Command};

use serde_json::Value;
use tempfile::TempDir;

#[cfg(target_os = "macos")]
use std::{
    os::unix::fs::PermissionsExt,
    process::{Child, Stdio},
    sync::Mutex,
};

const THREE_DS_SIZE: usize = 0x8A00;
const CEMU_SIZE: usize = 0x8A24;
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
