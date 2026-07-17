use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn cli_save_diff_reports_mh3g_binary_changes_without_semantic_claims() {
    let tmp = tempfile::tempdir().unwrap();
    let left = tmp.path().join("left");
    let right = tmp.path().join("right");
    fs::create_dir_all(left.join("slot1")).unwrap();
    fs::create_dir_all(right.join("slot1")).unwrap();
    fs::write(left.join("slot1/main.bin"), b"hunter-rank-001").unwrap();
    fs::write(right.join("slot1/main.bin"), b"hunter-rank-002").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_mh-save"))
        .args([
            "save-diff",
            "--left",
            left.to_str().unwrap(),
            "--right",
            right.to_str().unwrap(),
            "--game-profile",
            "mh3g-3ds",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "save-diff failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["parser_id"], "mh3g-3ds-binary-v0");
    assert_eq!(json["semantic_available"], false);
    assert_eq!(json["changed_files"], 1);
    assert!(
        json["summary_zh"]
            .as_str()
            .unwrap()
            .contains("文件/字节级差异")
    );
    assert!(
        json["summary_zh"]
            .as_str()
            .unwrap()
            .contains("不解读猎人名")
    );
    assert!(
        json["entries"][0]["notes_zh"][0]
            .as_str()
            .unwrap()
            .contains("不声称能语义解析")
    );
}
