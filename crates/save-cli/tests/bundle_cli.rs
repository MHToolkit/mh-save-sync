use std::fs;
use std::process::Command;

fn mh_save() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mh-save"))
}

#[test]
fn cli_exports_bundle_and_restores_without_server() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    let target = tmp.path().join("restored");
    let bundle = tmp.path().join("offline.mhsavebundle");
    fs::create_dir_all(source.join("slot1")).unwrap();
    fs::write(source.join("slot1/main.bin"), b"offline-portable-save").unwrap();

    let export = mh_save()
        .args([
            "snapshot-export",
            "--root",
            source.to_str().unwrap(),
            "--bundle",
            bundle.to_str().unwrap(),
            "--secret-hex",
            "1111111111111111111111111111111111111111111111111111111111111111",
        ])
        .output()
        .unwrap();
    assert!(
        export.status.success(),
        "export failed: status={:?}\nstdout={}\nstderr={}",
        export.status.code(),
        String::from_utf8_lossy(&export.stdout),
        String::from_utf8_lossy(&export.stderr),
    );
    assert!(
        bundle.exists(),
        "bundle must be written for no-server recovery"
    );

    let restore = mh_save()
        .args([
            "bundle-restore",
            "--bundle",
            bundle.to_str().unwrap(),
            "--target",
            target.to_str().unwrap(),
            "--secret-hex",
            "1111111111111111111111111111111111111111111111111111111111111111",
            "--emulator-state",
            "stopped",
        ])
        .output()
        .unwrap();
    assert!(
        restore.status.success(),
        "restore failed: status={:?}\nstdout={}\nstderr={}",
        restore.status.code(),
        String::from_utf8_lossy(&restore.stdout),
        String::from_utf8_lossy(&restore.stderr),
    );
    assert_eq!(
        fs::read(target.join("slot1/main.bin")).unwrap(),
        b"offline-portable-save",
    );
}

#[test]
fn cli_refuses_bundle_restore_while_emulator_running() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    let target = tmp.path().join("restored");
    let bundle = tmp.path().join("offline.mhsavebundle");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("save.bin"), b"safe").unwrap();

    let export = mh_save()
        .args([
            "snapshot-export",
            "--root",
            source.to_str().unwrap(),
            "--bundle",
            bundle.to_str().unwrap(),
            "--secret-hex",
            "2222222222222222222222222222222222222222222222222222222222222222",
        ])
        .output()
        .unwrap();
    assert!(export.status.success());

    let restore = mh_save()
        .args([
            "bundle-restore",
            "--bundle",
            bundle.to_str().unwrap(),
            "--target",
            target.to_str().unwrap(),
            "--secret-hex",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "--emulator-state",
            "running",
        ])
        .output()
        .unwrap();
    assert!(
        !restore.status.success(),
        "running restore must fail closed"
    );
    assert!(
        String::from_utf8_lossy(&restore.stderr)
            .contains("已拒绝恢复：模拟器仍在运行，没有覆盖本地存档"),
        "stderr should explain restore precondition: {}",
        String::from_utf8_lossy(&restore.stderr),
    );
    let stderr_json: serde_json::Value = serde_json::from_slice(&restore.stderr).unwrap();
    assert_eq!(stderr_json["error_code"], "emulator_running");
    assert_eq!(
        stderr_json["message"],
        "restore refused while emulator is running"
    );
    assert!(
        !target.exists(),
        "running restore must not write target directory"
    );
}
