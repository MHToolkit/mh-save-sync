use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;
use sha2::Digest;
use tempfile::TempDir;

use mh3g_save_convert::{
    cec::{CEMU_HEADER_SIZE, CEMU_RECORD_AREA_OFFSET, empty_cemu_cec},
    converter::{
        convert_3ds_system_to_cemu_named, convert_3ds_to_cemu_named,
        convert_external_component_to_cemu_named, merge_3ds_system_gallery_into_cemu_named,
    },
    profile::{JP_3DS_HEADER, JP_CEMU_HEADER, build_jp_cemu_header},
};

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

fn stderr_mentions_path(stderr: &str, path: &Path) -> bool {
    stderr_mentions_path_for_platform(stderr, path, cfg!(windows))
}

fn stderr_mentions_path_for_platform(stderr: &str, path: &Path, windows: bool) -> bool {
    let displayed = path.to_string_lossy();
    if stderr.contains(displayed.as_ref()) {
        return true;
    }

    // `std::fs::canonicalize` can return a verbatim path (\\?\C:\\...) on
    // Windows. Its exact prefix rendering is not stable across the standard
    // library and error formatter, so require the final two caller-provided
    // path components rather than comparing a formatter-owned absolute prefix.
    windows
        && path
            .file_name()
            .is_some_and(|file_name| stderr.contains(file_name.to_string_lossy().as_ref()))
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|parent| stderr.contains(parent.to_string_lossy().as_ref()))
}

#[test]
fn diagnostic_path_matcher_accepts_a_windows_verbatim_path_without_losing_resource_identity() {
    let path = Path::new(r"C:\Temp\slot\.user2.mh3g-install.lock");
    assert!(stderr_mentions_path_for_platform(
        r"I/O error `\\?\C:\Temp\slot\.user2.mh3g-install.lock`",
        path,
        true,
    ));
    assert!(!stderr_mentions_path_for_platform(
        r"I/O error `\\?\C:\Temp\other\.user2.mh3g-install.lock`",
        path,
        true,
    ));
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

fn cemu_system_fixture(temp: &TempDir) -> PathBuf {
    let path = target_slot(temp, "system");
    let mut bytes = build_jp_cemu_header("system", THREE_DS_SYSTEM_SIZE - JP_3DS_HEADER.len())
        .unwrap()
        .to_vec();
    bytes.resize(CEMU_SYSTEM_SIZE, 0);
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
fn repair_converted_dry_run_then_write_repairs_only_an_old_lamp_field() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("3ds").join("user2");
    let current_path = temp.path().join("cemu").join("user2");
    fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    fs::create_dir_all(current_path.parent().unwrap()).unwrap();

    let mut source = (0..THREE_DS_SIZE)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect::<Vec<_>>();
    source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
    let source_lamp = JP_3DS_HEADER.len() + 0x6F44 + 0xE4;
    source[source_lamp..source_lamp + 2].copy_from_slice(&[0x1E, 0x00]);
    fs::write(&source_path, &source).unwrap();

    let mut current = convert_3ds_to_cemu_named(&source, "user2").unwrap();
    let lamp = JP_CEMU_HEADER.len() + 0x6F44 + 0xE4;
    current[lamp..lamp + 2].copy_from_slice(&source[source_lamp..source_lamp + 2]);
    let unrelated = JP_CEMU_HEADER.len() + 0x240;
    current[unrelated] ^= 0x5A;
    let unrelated_after = current[unrelated];
    fs::write(&current_path, &current).unwrap();
    let before_repair = current.clone();

    let dry = run_json(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.5".into(),
        "--dry-run".into(),
    ]);
    assert_eq!(dry["status"], "dry-run");
    assert_eq!(dry["components"][0]["merge"]["repaired_fields"], 1);
    assert_eq!(dry["components"][0]["merge"]["preserved_conflicts"], 0);

    let written = run_json_with_stopped_emulators(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.5".into(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert_eq!(written["status"], "written");
    let installed = fs::read(&current_path).unwrap();
    assert_eq!(&installed[lamp..lamp + 2], &[0x00, 0x1E]);
    assert_eq!(installed[unrelated], unrelated_after);
    assert!(written["manifests"].as_array().unwrap().len() == 1);
    let compatibility_manifest = written["compatibility_manifest"]
        .as_str()
        .expect("written compatibility repair has a coordinator manifest");
    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback-repair".into(),
        "--manifest".into(),
        compatibility_manifest.to_owned(),
    ]);
    assert_eq!(rolled_back["status"], "rolled-back");
    assert_eq!(fs::read(&current_path).unwrap(), before_repair);
}

#[test]
fn repair_converted_can_read_current_and_write_a_separate_output() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_path = slot_fixture(&temp, "user2");
    let mut source = fs::read(&source_path).unwrap();
    let source_lamp = JP_3DS_HEADER.len() + 0x6F44 + 0xE4;
    source[source_lamp..source_lamp + 2].copy_from_slice(&[0x1E, 0x00]);
    fs::write(&source_path, &source).unwrap();
    let current_path = temp.path().join("played-cemu").join("user2");
    let output_path = temp.path().join("repaired-export").join("user2");
    fs::create_dir_all(current_path.parent().unwrap()).unwrap();
    fs::create_dir_all(output_path.parent().unwrap()).unwrap();

    let mut current = convert_3ds_to_cemu_named(&source, "user2").unwrap();
    let lamp = JP_CEMU_HEADER.len() + 0x6F44 + 0xE4;
    current[lamp..lamp + 2].copy_from_slice(&source[source_lamp..source_lamp + 2]);
    let unrelated = JP_CEMU_HEADER.len() + 0x240;
    current[unrelated] ^= 0x5A;
    let current_before = current.clone();
    fs::write(&current_path, &current).unwrap();

    let dry = run_json(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.5".into(),
        "--dry-run".into(),
    ]);
    assert_eq!(dry["status"], "dry-run");
    assert_eq!(dry["output"], output_path.to_string_lossy().as_ref());
    assert_eq!(dry["components"][0]["write_required"], true);
    assert!(!output_path.exists());
    assert_eq!(fs::read(&current_path).unwrap(), current_before);

    let written = run_json_with_stopped_emulators(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.5".into(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-output-set-sha256".into(),
        dry["output_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert_eq!(written["status"], "written");
    assert_eq!(fs::read(&current_path).unwrap(), current_before);
    let installed = fs::read(&output_path).unwrap();
    assert_eq!(&installed[lamp..lamp + 2], &[0x00, 0x1E]);
    assert_eq!(installed[unrelated], current_before[unrelated]);

    let compatibility_manifest = written["compatibility_manifest"]
        .as_str()
        .expect("separate-output repair has a coordinator manifest");
    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback-repair".into(),
        "--manifest".into(),
        compatibility_manifest.to_owned(),
    ]);
    assert_eq!(rolled_back["status"], "rolled-back");
    assert!(!output_path.exists());
    assert_eq!(fs::read(&current_path).unwrap(), current_before);
}

#[test]
fn repair_converted_rejects_a_separate_output_changed_after_dry_run() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_path = slot_fixture(&temp, "user2");
    let source = fs::read(&source_path).unwrap();
    let current_path = target_slot(&temp, "user2");
    let output_dir = temp.path().join("separate-output");
    let output_path = output_dir.join("user2");
    fs::create_dir_all(&output_dir).unwrap();
    let current = convert_3ds_to_cemu_named(&source, "user2").unwrap();
    fs::write(&current_path, &current).unwrap();

    let dry = run_json(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.6".into(),
        "--dry-run".into(),
    ]);
    fs::write(&output_path, &current).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.6".into(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-output-set-sha256".into(),
        dry["output_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert!(!output.status.success());
    assert_eq!(fs::read(&current_path).unwrap(), current);
    assert_eq!(fs::read(&output_path).unwrap(), current);
}

#[test]
fn repair_converted_rejects_an_existing_non_cemu_output_slot() {
    let temp = tempfile::tempdir().unwrap();
    let source_path = slot_fixture(&temp, "user2");
    let source = fs::read(&source_path).unwrap();
    let current_path = target_slot(&temp, "user2");
    let output_dir = temp.path().join("invalid-output");
    let output_path = output_dir.join("user2");
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(
        &current_path,
        convert_3ds_to_cemu_named(&source, "user2").unwrap(),
    )
    .unwrap();
    fs::write(&output_path, b"not a Cemu slot").unwrap();

    let output = run_output_with_stopped_emulators(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.6".into(),
        "--dry-run".into(),
    ]);

    assert!(!output.status.success());
    assert_eq!(fs::read(&output_path).unwrap(), b"not a Cemu slot");
}

#[test]
fn repair_converted_writes_guild_cards_to_the_separate_output_only() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_path = slot_fixture(&temp, "user2");
    let source_slot = fs::read(&source_path).unwrap();
    let current_dir = temp.path().join("played-cemu");
    let output_dir = temp.path().join("repair-output");
    fs::create_dir_all(&current_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();
    let current_path = current_dir.join("user2");
    let output_path = output_dir.join("user2");
    fs::write(
        &current_path,
        convert_3ds_to_cemu_named(&source_slot, "user2").unwrap(),
    )
    .unwrap();

    let extdata = extras_fixture(&temp);
    let card1_path = extdata.join("card1");
    let mut card1_source = fs::read(&card1_path).unwrap();
    let card_row = JP_3DS_HEADER.len() + 0x7C0;
    card1_source[card_row..card_row + 2].copy_from_slice(&[0x01, 0x00]);
    card1_source[card_row + 8] = 0;
    fs::write(&card1_path, &card1_source).unwrap();

    for component in [
        "card1", "card2", "card3", "cardbox", "quest1", "quest2", "quest3", "quest4",
    ] {
        let source_bytes = fs::read(extdata.join(component)).unwrap();
        let mut current_bytes =
            convert_external_component_to_cemu_named(&source_bytes, component).unwrap();
        if component == "card1" {
            current_bytes[JP_CEMU_HEADER.len() + 0x7C0 + 8] = 0;
        }
        fs::write(current_dir.join(component), &current_bytes).unwrap();
        if matches!(component, "card1" | "card2" | "card3" | "cardbox") {
            fs::write(output_dir.join(component), &current_bytes).unwrap();
        }
    }
    let current_card1_before = fs::read(current_dir.join("card1")).unwrap();
    let output_card1_before = fs::read(output_dir.join("card1")).unwrap();

    let dry = run_json(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--source-extdata-dir".into(),
        extdata.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    assert_eq!(dry["status"], "dry-run");

    let written = run_json_with_stopped_emulators(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--source-extdata-dir".into(),
        extdata.to_string_lossy().into_owned(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-output-set-sha256".into(),
        dry["output_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert_eq!(written["status"], "written");
    assert_eq!(
        fs::read(current_dir.join("card1")).unwrap(),
        current_card1_before
    );
    assert_eq!(
        fs::read(output_dir.join("card1")).unwrap()[JP_CEMU_HEADER.len() + 0x7C0 + 8],
        0x80
    );

    let compatibility_manifest = written["compatibility_manifest"].as_str().unwrap();
    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback-repair".into(),
        "--manifest".into(),
        compatibility_manifest.to_owned(),
    ]);
    assert_eq!(rolled_back["status"], "rolled-back");
    assert!(!output_path.exists());
    assert_eq!(
        fs::read(output_dir.join("card1")).unwrap(),
        output_card1_before
    );
    assert_eq!(
        fs::read(current_dir.join("card1")).unwrap(),
        current_card1_before
    );
}

#[test]
fn repair_converted_write_rejects_a_current_save_changed_after_dry_run() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_path = slot_fixture(&temp, "user2");
    let source = fs::read(&source_path).unwrap();
    let current_path = target_slot(&temp, "user2");
    let mut current = convert_3ds_to_cemu_named(&source, "user2").unwrap();
    let lamp = JP_CEMU_HEADER.len() + 0x6F44 + 0xE4;
    let source_lamp = JP_3DS_HEADER.len() + 0x6F44 + 0xE4;
    current[lamp..lamp + 2].copy_from_slice(&source[source_lamp..source_lamp + 2]);
    fs::write(&current_path, &current).unwrap();

    let dry = run_json(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.5".into(),
        "--dry-run".into(),
    ]);
    current[JP_CEMU_HEADER.len() + 0x240] ^= 0x5A;
    fs::write(&current_path, &current).unwrap();

    let output = run_output_with_stopped_emulators(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--from-version".into(),
        "0.0.5".into(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);

    assert!(!output.status.success());
    assert_eq!(fs::read(&current_path).unwrap(), current);
}

#[test]
fn repair_converted_preserves_the_played_directory_and_rolls_back_every_change() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_path = slot_fixture(&temp, "user2");
    let mut source_slot = fs::read(&source_path).unwrap();
    let source_lamp = JP_3DS_HEADER.len() + 0x6F44 + 0xE4;
    source_slot[source_lamp..source_lamp + 2].copy_from_slice(&[0x1E, 0x00]);
    fs::write(&source_path, &source_slot).unwrap();
    let current_dir = temp.path().join("cemu-repair");
    fs::create_dir_all(&current_dir).unwrap();
    let current_path = current_dir.join("user2");
    let mut current_slot = convert_3ds_to_cemu_named(&source_slot, "user2").unwrap();
    let current_lamp = JP_CEMU_HEADER.len() + 0x6F44 + 0xE4;
    current_slot[current_lamp..current_lamp + 2]
        .copy_from_slice(&source_slot[source_lamp..source_lamp + 2]);
    fs::write(&current_path, &current_slot).unwrap();
    let current_slot_before = current_slot.clone();

    let extdata = extras_fixture(&temp);
    let card1_path = extdata.join("card1");
    let mut card1_source = fs::read(&card1_path).unwrap();
    let card_row = JP_3DS_HEADER.len() + 0x7C0;
    card1_source[card_row..card_row + 2].copy_from_slice(&[0x01, 0x00]);
    card1_source[card_row + 8] = 0;
    fs::write(&card1_path, &card1_source).unwrap();

    for component in [
        "card1", "card2", "card3", "cardbox", "quest1", "quest2", "quest3", "quest4",
    ] {
        let source_bytes = fs::read(extdata.join(component)).unwrap();
        let mut current_bytes =
            convert_external_component_to_cemu_named(&source_bytes, component).unwrap();
        if component == "card1" {
            // Recreate the pre-0.0.5 display-state result.
            current_bytes[JP_CEMU_HEADER.len() + 0x7C0 + 8] = 0;
        } else if component == "cardbox" {
            // Model a later Wii U compact-card update outside the repair map.
            current_bytes[JP_CEMU_HEADER.len() + 1976..JP_CEMU_HEADER.len() + 1978]
                .copy_from_slice(&1_u16.to_be_bytes());
        }
        fs::write(current_dir.join(component), current_bytes).unwrap();
    }
    let system_source = fs::read(system_fixture(&temp)).unwrap();
    let mut system = convert_3ds_system_to_cemu_named(&system_source, "system").unwrap();
    system[JP_CEMU_HEADER.len() + 0x40] = 0x5A;
    fs::write(current_dir.join("system"), system).unwrap();

    let mut cec = empty_cemu_cec().unwrap();
    let card1_for_cec = fs::read(current_dir.join("card1")).unwrap();
    let cec_record_start = CEMU_HEADER_SIZE + CEMU_RECORD_AREA_OFFSET;
    cec[cec_record_start..cec_record_start + 0xE00]
        .copy_from_slice(&card1_for_cec[JP_CEMU_HEADER.len()..JP_CEMU_HEADER.len() + 0xE00]);
    fs::write(current_dir.join("cec"), cec).unwrap();

    let card1_before = fs::read(current_dir.join("card1")).unwrap();
    let preserved_before = [
        "system", "cec", "card2", "card3", "cardbox", "quest1", "quest2", "quest3", "quest4",
    ]
    .into_iter()
    .map(|component| (component, fs::read(current_dir.join(component)).unwrap()))
    .collect::<BTreeMap<_, _>>();

    let dry = run_json(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--source-extdata-dir".into(),
        extdata.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    assert!(dry["detection"]["candidates"].is_array());
    let assumed_revisions = dry["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|component| component["merge"]["assumed_revision"].to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        assumed_revisions.len(),
        1,
        "all selected components must use one historical converter revision"
    );
    let card1 = dry["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["component"] == "card1")
        .unwrap();
    assert_eq!(card1["merge"]["repaired_fields"], 1);
    assert_eq!(
        dry["preserved_components"],
        serde_json::json!(["quest1", "quest2", "quest3", "quest4"])
    );

    let written = run_json_with_stopped_emulators(&[
        "repair-converted".into(),
        source_path.to_string_lossy().into_owned(),
        "--current".into(),
        current_path.to_string_lossy().into_owned(),
        "--source-extdata-dir".into(),
        extdata.to_string_lossy().into_owned(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert_eq!(written["status"], "written");
    assert_eq!(
        fs::read(current_dir.join("card1")).unwrap()[JP_CEMU_HEADER.len() + 0x7C0 + 8],
        0x80
    );
    for (component, before) in &preserved_before {
        assert_eq!(
            fs::read(current_dir.join(component)).unwrap(),
            *before,
            "{component} must preserve later Wii U data during repair"
        );
    }
    assert_eq!(written["manifests"].as_array().unwrap().len(), 2);
    let compatibility_manifest = written["compatibility_manifest"]
        .as_str()
        .expect("combined repair has a coordinator manifest");
    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback-repair".into(),
        "--manifest".into(),
        compatibility_manifest.to_owned(),
    ]);
    assert_eq!(rolled_back["status"], "rolled-back");
    assert_eq!(fs::read(&current_path).unwrap(), current_slot_before);
    assert_eq!(fs::read(current_dir.join("card1")).unwrap(), card1_before);
    for (component, before) in &preserved_before {
        assert_eq!(
            fs::read(current_dir.join(component)).unwrap(),
            *before,
            "{component} must remain byte-identical after rollback"
        );
    }
}

#[test]
fn repair_quests_copies_current_wiiu_bytes_to_an_independent_output_and_is_idempotent() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("3ds-extdata");
    let current_dir = temp.path().join("current-cemu");
    let output_dir = temp.path().join("output-cemu");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&current_dir).unwrap();
    fs::create_dir(&output_dir).unwrap();

    let components = ["quest1", "quest2", "quest3", "quest4"];
    let mut output_before = BTreeMap::new();
    for (index, component) in components.iter().enumerate() {
        let mut source = vec![0_u8; 0x29_000];
        source[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
        source[4] = index as u8;
        let baseline = convert_external_component_to_cemu_named(&source, component).unwrap();
        let mut current = baseline.clone();
        current[0x120 + index] ^= 0x5A;
        fs::write(source_dir.join(component), source).unwrap();
        fs::write(current_dir.join(component), current).unwrap();
        fs::write(output_dir.join(component), &baseline).unwrap();
        output_before.insert(*component, baseline);
    }
    let current_before = components
        .iter()
        .map(|component| (*component, fs::read(current_dir.join(component)).unwrap()))
        .collect::<BTreeMap<_, _>>();

    let dry = run_json(&[
        "repair-extras".into(),
        "--source-dir".into(),
        source_dir.to_string_lossy().into_owned(),
        "--current-dir".into(),
        current_dir.to_string_lossy().into_owned(),
        "--output-dir".into(),
        output_dir.to_string_lossy().into_owned(),
        "--group".into(),
        "quests".into(),
        "--dry-run".into(),
    ]);
    assert_eq!(dry["status"], "dry-run");
    assert!(
        dry["components"]
            .as_array()
            .unwrap()
            .iter()
            .all(|component| component["modified"] == false)
    );

    let written = run_json_with_stopped_emulators(&[
        "repair-extras".into(),
        "--source-dir".into(),
        source_dir.to_string_lossy().into_owned(),
        "--current-dir".into(),
        current_dir.to_string_lossy().into_owned(),
        "--output-dir".into(),
        output_dir.to_string_lossy().into_owned(),
        "--group".into(),
        "quests".into(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-output-set-sha256".into(),
        dry["output_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert_eq!(written["status"], "written");
    for component in components {
        assert_eq!(
            fs::read(output_dir.join(component)).unwrap(),
            current_before[component]
        );
        assert_eq!(
            fs::read(current_dir.join(component)).unwrap(),
            current_before[component]
        );
    }

    let second = run_json(&[
        "repair-extras".into(),
        "--source-dir".into(),
        source_dir.to_string_lossy().into_owned(),
        "--current-dir".into(),
        current_dir.to_string_lossy().into_owned(),
        "--output-dir".into(),
        output_dir.to_string_lossy().into_owned(),
        "--group".into(),
        "quests".into(),
        "--dry-run".into(),
    ]);
    assert!(
        second["components"]
            .as_array()
            .unwrap()
            .iter()
            .all(|component| component["write_required"] == false)
    );

    let manifest = written["manifest"].as_str().unwrap();
    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback-extras".into(),
        "--manifest".into(),
        manifest.to_owned(),
    ]);
    assert_eq!(rolled_back["status"], "rolled-back");
    for component in components {
        assert_eq!(
            fs::read(output_dir.join(component)).unwrap(),
            output_before[component]
        );
        assert_eq!(
            fs::read(current_dir.join(component)).unwrap(),
            current_before[component]
        );
    }
}

#[test]
fn repair_system_preserves_current_shared_bytes_and_rolls_back_a_new_output() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let current_dir = temp.path().join("current");
    let output_dir = temp.path().join("output");
    fs::create_dir(&source_dir).unwrap();
    fs::create_dir(&current_dir).unwrap();
    fs::create_dir(&output_dir).unwrap();
    let source = source_dir.join("system");
    let current = current_dir.join("system");
    let output = output_dir.join("system");

    let mut source_bytes = vec![0_u8; THREE_DS_SYSTEM_SIZE];
    source_bytes[..JP_3DS_HEADER.len()].copy_from_slice(&JP_3DS_HEADER);
    source_bytes[0x44..0x48].copy_from_slice(&1_u32.to_le_bytes());
    fs::write(&source, source_bytes).unwrap();
    let mut current_bytes = build_jp_cemu_header("system", CEMU_SYSTEM_SIZE - 40)
        .unwrap()
        .to_vec();
    current_bytes.resize(CEMU_SYSTEM_SIZE, 0);
    current_bytes[0x180] = 0xA5;
    fs::write(&current, &current_bytes).unwrap();

    let dry = run_json(&[
        "repair-system".into(),
        source.to_string_lossy().into_owned(),
        "--current".into(),
        current.to_string_lossy().into_owned(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    assert_eq!(dry["status"], "dry-run");
    assert_eq!(dry["write_required"], true);
    assert!(!output.exists());

    let written = run_json_with_stopped_emulators(&[
        "repair-system".into(),
        source.to_string_lossy().into_owned(),
        "--current".into(),
        current.to_string_lossy().into_owned(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
        "--write".into(),
        "--expected-source-set-sha256".into(),
        dry["source_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-output-set-sha256".into(),
        dry["output_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert_eq!(written["status"], "written");
    let repaired = fs::read(&output).unwrap();
    assert_eq!(repaired[0x180], 0xA5);
    assert_eq!(&repaired[0x68..0x6C], &1_u32.to_be_bytes());
    assert_eq!(fs::read(&current).unwrap(), current_bytes);

    let second = run_json(&[
        "repair-system".into(),
        source.to_string_lossy().into_owned(),
        "--current".into(),
        current.to_string_lossy().into_owned(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    assert_eq!(second["write_required"], false);

    let manifest = written["manifest"].as_str().unwrap();
    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback".into(),
        "--manifest".into(),
        manifest.to_owned(),
    ]);
    assert_eq!(rolled_back["status"], "rolled-back");
    assert!(!output.exists());
    assert_eq!(fs::read(&current).unwrap(), current_bytes);
}

#[test]
fn repair_cec_keeps_current_read_only_and_rolls_back_an_independent_output() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source_dir = cec_fixture(&temp);
    let current = cemu_cec_fixture(&temp);
    let current_before = fs::read(&current).unwrap();
    let output_dir = temp.path().join("cec-output");
    fs::create_dir(&output_dir).unwrap();
    let output = output_dir.join("cec");

    let dry = run_json(&[
        "repair-cec".into(),
        "--source-dir".into(),
        source_dir.to_string_lossy().into_owned(),
        "--current".into(),
        current.to_string_lossy().into_owned(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    assert_eq!(dry["status"], "dry-run");
    assert!(!output.exists());

    let written = run_json_with_stopped_emulators(&[
        "repair-cec".into(),
        "--source-dir".into(),
        source_dir.to_string_lossy().into_owned(),
        "--current".into(),
        current.to_string_lossy().into_owned(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
        "--write".into(),
        "--experimental".into(),
        "--expected-source-record-set-sha256".into(),
        dry["source_record_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-current-set-sha256".into(),
        dry["current_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-output-set-sha256".into(),
        dry["output_set_sha256"].as_str().unwrap().to_owned(),
        "--expected-preview-sha256".into(),
        dry["preview_sha256"].as_str().unwrap().to_owned(),
    ]);
    assert_eq!(written["status"], "written");
    assert_ne!(fs::read(&output).unwrap(), current_before);
    assert_eq!(fs::read(&current).unwrap(), current_before);

    let second = run_json(&[
        "repair-cec".into(),
        "--source-dir".into(),
        source_dir.to_string_lossy().into_owned(),
        "--current".into(),
        current.to_string_lossy().into_owned(),
        "--output".into(),
        output.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);
    assert_eq!(
        second["output_sha256_before"],
        second["output_sha256_after"]
    );

    let manifest = written["manifest"].as_str().unwrap();
    let rolled_back = run_json_with_stopped_emulators(&[
        "rollback-cec".into(),
        "--manifest".into(),
        manifest.to_owned(),
    ]);
    assert_eq!(rolled_back["status"], "rolled-back");
    assert!(!output.exists());
    assert_eq!(fs::read(&current).unwrap(), current_before);
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

    let source = slot_fixture(&temp, "user2");
    let target = target_slot(&temp, "user2");
    let output = run_output_with_stopped_emulators(&[
        "convert".to_owned(),
        source.to_string_lossy().into_owned(),
        "--output".to_owned(),
        target.to_string_lossy().into_owned(),
        "--expected-target-sha256".to_owned(),
        "0".repeat(64),
        "--write".to_owned(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("target is missing but an expected dry-run SHA-256 was supplied")
    );
    assert!(!target.exists());
    assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 0);
}

#[test]
fn convert_system_requires_an_existing_initialized_cemu_baseline() {
    let temp = tempfile::tempdir().unwrap();
    let source = system_fixture(&temp);
    let target = target_slot(&temp, "system");

    let output = binary()
        .args([
            "convert-system",
            &source.to_string_lossy(),
            "--output",
            &target.to_string_lossy(),
            "--dry-run",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("requires an existing initialized Wii U/Cemu system target")
    );
    assert!(!target.exists());
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
fn inspect_recognizes_both_3ds_and_cemu_system_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let source = system_fixture(&temp);
    let target = cemu_system_fixture(&temp);

    let source_report = run_json(&["inspect".into(), source.to_string_lossy().into_owned()]);
    let target_report = run_json(&["inspect".into(), target.to_string_lossy().into_owned()]);

    assert_eq!(source_report["profile"], "JpThreeDsSystem");
    assert_eq!(source_report["size"], THREE_DS_SYSTEM_SIZE);
    assert_eq!(target_report["profile"], "JpCemuSystem");
    assert_eq!(target_report["size"], CEMU_SYSTEM_SIZE);
}

#[test]
fn convert_system_unions_gallery_flags_and_preserves_every_other_cemu_byte() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = system_fixture(&temp);
    let target = cemu_system_fixture(&temp);

    let mut source_bytes = fs::read(&source).unwrap();
    source_bytes[4 + 0x40..4 + 0x44].copy_from_slice(&0x0000_0005_u32.to_le_bytes());
    source_bytes[4 + 0x44..4 + 0x48].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    source_bytes[4 + 0x54] = 0xA5;
    fs::write(&source, &source_bytes).unwrap();

    let mut target_before = fs::read(&target).unwrap();
    target_before[40 + 0x40..40 + 0x44].copy_from_slice(&0x0000_0002_u32.to_be_bytes());
    target_before[40 + 0x48..40 + 0x4C].copy_from_slice(&0x0000_0010_u32.to_be_bytes());
    target_before[40 + 0x54] = 0x5A;
    fs::write(&target, &target_before).unwrap();
    let expected =
        merge_3ds_system_gallery_into_cemu_named(&source_bytes, &target_before, "system").unwrap();

    let dry_run = run_json(&[
        "convert-system".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--dry-run".into(),
    ]);

    assert_eq!(dry_run["profile"], "JpCemuSystem");
    assert_eq!(dry_run["status"], "dry-run");
    assert_eq!(fs::read(&target).unwrap(), target_before);
    assert_eq!(
        dry_run["hashes"]["output"],
        hex::encode(sha2::Sha256::digest(&expected))
    );
    for key in ["source_gallery", "target_gallery_before", "output_gallery"] {
        assert_eq!(dry_run["hashes"][key].as_str().unwrap().len(), 64);
    }

    let written = run_json_with_stopped_emulators(&[
        "convert-system".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-source-sha256".into(),
        dry_run["hashes"]["source"].as_str().unwrap().into(),
        "--expected-target-sha256".into(),
        dry_run["hashes"]["target_before"].as_str().unwrap().into(),
        "--write".into(),
    ]);

    assert_eq!(written["status"], "written");
    assert_eq!(fs::read(&target).unwrap(), expected);
    assert_eq!(&expected[..40 + 0x40], &target_before[..40 + 0x40]);
    assert_eq!(&expected[40 + 0x50..], &target_before[40 + 0x50..]);
}

#[test]
fn convert_system_write_requires_both_dry_run_hashes() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = system_fixture(&temp);
    let target = cemu_system_fixture(&temp);

    for supplied in [None, Some("--expected-source-sha256")] {
        let mut arguments = vec![
            "convert-system".to_owned(),
            source.to_string_lossy().into_owned(),
            "--output".to_owned(),
            target.to_string_lossy().into_owned(),
        ];
        if let Some(flag) = supplied {
            arguments.extend([flag.to_owned(), "0".repeat(64)]);
        }
        arguments.push("--write".to_owned());
        let output = run_output_with_stopped_emulators(&arguments);

        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("requires --expected-source-sha256 and --expected-target-sha256")
        );
    }
}

#[test]
fn convert_system_write_rejects_a_stale_expected_target_hash_without_replacing_target() {
    #[cfg(target_os = "macos")]
    let _guard = PROCESS_GUARD.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let source = system_fixture(&temp);
    let target = cemu_system_fixture(&temp);
    let previous = fs::read(&target).unwrap();
    let source_sha256 = hex::encode(sha2::Sha256::digest(fs::read(&source).unwrap()));

    let output = run_output_with_stopped_emulators(&[
        "convert-system".into(),
        source.to_string_lossy().into_owned(),
        "--output".into(),
        target.to_string_lossy().into_owned(),
        "--expected-source-sha256".into(),
        source_sha256,
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
    let source_before = fs::read(&source).unwrap();
    let source_sha256_before = sha2::Sha256::digest(&source_before);
    let target = target_slot(&temp, "user2");
    let previous = vec![0xA5; CEMU_SIZE];
    let previous_sha256 = sha2::Sha256::digest(&previous);
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
    assert_eq!(fs::read(&source).unwrap(), source_before);
    assert_eq!(
        sha2::Sha256::digest(fs::read(&source).unwrap()),
        source_sha256_before
    );

    let rollback = run_json_with_stopped_emulators(&[
        "rollback".into(),
        "--manifest".into(),
        manifest.display().to_string(),
    ]);
    assert_eq!(rollback["status"], "rolled-back");
    assert_eq!(fs::read(&target).unwrap(), previous);
    assert_eq!(
        sha2::Sha256::digest(fs::read(&target).unwrap()),
        previous_sha256
    );
    assert_eq!(
        sha2::Sha256::digest(fs::read(&source).unwrap()),
        source_sha256_before
    );
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
    assert!(
        stderr_mentions_path(&stderr, &lock_path),
        "stderr: {stderr}"
    );
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
            stderr_mentions_path(&stderr, &manifest),
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
