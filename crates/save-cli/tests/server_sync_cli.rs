use save_server::{AppState, router};
use serde_json::Value;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::process::{Command, Output};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

fn mh_save() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mh-save"))
}

async fn run_mh_save(args: &[String]) -> Output {
    let args = args.to_vec();
    tokio::task::spawn_blocking(move || mh_save().args(args).output().unwrap())
        .await
        .unwrap()
}

async fn spawn_memory_server() -> Option<(String, JoinHandle<()>)> {
    let listener = match TcpListener::bind("127.0.0.1:0").await {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            if std::env::var("MH_SAVE_SYNC_REQUIRE_NETWORK_E2E").as_deref() == Ok("1") {
                panic!("loopback bind is required for this test: {error}");
            }
            eprintln!(
                "skipping live server CLI test because this sandbox denies loopback bind: {error}"
            );
            return None;
        }
        Err(error) => panic!("failed to bind test server: {error}"),
    };
    let addr: SocketAddr = listener.local_addr().unwrap();
    let server = axum::serve(listener, router(AppState::default()));
    let handle = tokio::spawn(async move {
        server.await.unwrap();
    });
    Some((format!("http://{addr}"), handle))
}

#[tokio::test]
async fn cli_uploads_snapshot_to_server_and_prints_visible_sync_target() {
    let Some((server_url, handle)) = spawn_memory_server().await else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir_all(source.join("slot1")).unwrap();
    fs::write(source.join("slot1/main.bin"), b"server-visible-save").unwrap();

    let first = run_mh_save(&[
        "server-upload".into(),
        "--server-url".into(),
        server_url.clone(),
        "--root".into(),
        source.to_string_lossy().into_owned(),
        "--secret-hex".into(),
        "3333333333333333333333333333333333333333333333333333333333333333".into(),
        "--device-id".into(),
        "office-mac".into(),
    ])
    .await;
    assert!(
        first.status.success(),
        "server upload failed: status={:?}\nstdout={}\nstderr={}",
        first.status.code(),
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr),
    );
    let first_json: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first_json["server_url"], server_url);
    assert_eq!(first_json["device_id"], "office-mac");
    assert_eq!(first_json["outcome"], "first-snapshot");
    assert_eq!(first_json["cloud_head"], first_json["snapshot_id"]);
    assert_eq!(first_json["conflict_snapshot"], Value::Null);
    assert_eq!(first_json["file_count"], 1);
    assert_eq!(first_json["chunk_count"], 1);
    assert!(
        first_json["message_zh"]
            .as_str()
            .unwrap()
            .contains("已上传到服务器"),
        "Chinese UX must explain where sync went: {first_json}",
    );

    let repeated_same_content = run_mh_save(&[
        "server-upload".into(),
        "--server-url".into(),
        server_url.clone(),
        "--root".into(),
        source.to_string_lossy().into_owned(),
        "--secret-hex".into(),
        "3333333333333333333333333333333333333333333333333333333333333333".into(),
        "--device-id".into(),
        "office-mac".into(),
    ])
    .await;
    assert!(
        repeated_same_content.status.success(),
        "same-content upload should be a no-op, not a conflict: status={:?}\nstdout={}\nstderr={}",
        repeated_same_content.status.code(),
        String::from_utf8_lossy(&repeated_same_content.stdout),
        String::from_utf8_lossy(&repeated_same_content.stderr),
    );
    let repeated_json: Value = serde_json::from_slice(&repeated_same_content.stdout).unwrap();
    assert_eq!(repeated_json["outcome"], "up-to-date");
    assert_eq!(repeated_json["cloud_head"], first_json["cloud_head"]);
    assert_eq!(repeated_json["conflict_snapshot"], Value::Null);
    assert_eq!(repeated_json["missing_chunks_uploaded"], 0);
    assert_eq!(repeated_json["manifest_uploaded"], false);
    assert!(
        repeated_json["message_zh"]
            .as_str()
            .unwrap()
            .contains("没有重复上传"),
        "same-content no-op must be visible in Chinese: {repeated_json}",
    );

    fs::write(source.join("slot1/main.bin"), b"server-visible-save-v2").unwrap();
    let second = run_mh_save(&[
        "server-upload".into(),
        "--server-url".into(),
        server_url.clone(),
        "--root".into(),
        source.to_string_lossy().into_owned(),
        "--secret-hex".into(),
        "3333333333333333333333333333333333333333333333333333333333333333".into(),
        "--device-id".into(),
        "home-android".into(),
        "--base-head".into(),
        first_json["cloud_head"].as_str().unwrap().into(),
    ])
    .await;
    assert!(
        second.status.success(),
        "fast-forward upload failed: status={:?}\nstdout={}\nstderr={}",
        second.status.code(),
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr),
    );
    let second_json: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second_json["outcome"], "fast-forward");
    assert_eq!(second_json["cloud_head"], second_json["snapshot_id"]);

    let status = run_mh_save(&[
        "server-status".into(),
        "--server-url".into(),
        server_url.clone(),
        "--secret-hex".into(),
        "3333333333333333333333333333333333333333333333333333333333333333".into(),
    ])
    .await;
    assert!(
        status.status.success(),
        "server status failed: status={:?}\nstdout={}\nstderr={}",
        status.status.code(),
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr),
    );
    let status_json: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status_json["server_url"], server_url);
    assert_eq!(status_json["cloud_head"], second_json["snapshot_id"]);
    assert_eq!(status_json["history_count"], 2);
    assert_eq!(status_json["conflict_count"], 0);
    assert!(
        status_json["message_zh"]
            .as_str()
            .unwrap()
            .contains("云端当前 HEAD"),
        "status must be understandable in Chinese: {status_json}",
    );

    let restored = tmp.path().join("restored-fast-forward");
    let restore = run_mh_save(&[
        "server-restore".into(),
        "--server-url".into(),
        server_url.clone(),
        "--secret-hex".into(),
        "3333333333333333333333333333333333333333333333333333333333333333".into(),
        "--target".into(),
        restored.to_string_lossy().into_owned(),
        "--emulator-state".into(),
        "stopped".into(),
    ])
    .await;
    assert!(
        restore.status.success(),
        "server restore failed: status={:?}
stdout={}
stderr={}",
        restore.status.code(),
        String::from_utf8_lossy(&restore.stdout),
        String::from_utf8_lossy(&restore.stderr),
    );
    let restore_json: Value = serde_json::from_slice(&restore.stdout).unwrap();
    assert_eq!(restore_json["server_url"], server_url);
    assert_eq!(restore_json["snapshot_id"], second_json["snapshot_id"]);
    assert!(
        restore_json["message_zh"]
            .as_str()
            .unwrap()
            .contains("已从服务器下载并恢复"),
        "restore UX must be Chinese and explicit: {restore_json}",
    );
    assert_eq!(
        fs::read(restored.join("slot1/main.bin")).unwrap(),
        b"server-visible-save-v2",
    );

    handle.abort();
}

#[tokio::test]
async fn cli_preserves_cloud_head_and_reports_conflict_branch() {
    let Some((server_url, handle)) = spawn_memory_server().await else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("save.bin"), b"office").unwrap();

    let office = run_mh_save(&[
        "server-upload".into(),
        "--server-url".into(),
        server_url.clone(),
        "--root".into(),
        source.to_string_lossy().into_owned(),
        "--secret-hex".into(),
        "4444444444444444444444444444444444444444444444444444444444444444".into(),
        "--device-id".into(),
        "office-mac".into(),
    ])
    .await;
    assert!(office.status.success());
    let office_json: Value = serde_json::from_slice(&office.stdout).unwrap();

    fs::write(source.join("save.bin"), b"home-offline-branch").unwrap();
    let conflict = run_mh_save(&[
        "server-upload".into(),
        "--server-url".into(),
        server_url.clone(),
        "--root".into(),
        source.to_string_lossy().into_owned(),
        "--secret-hex".into(),
        "4444444444444444444444444444444444444444444444444444444444444444".into(),
        "--device-id".into(),
        "home-android".into(),
    ])
    .await;
    assert!(
        conflict.status.success(),
        "conflict upload itself should be safely committed as a branch: status={:?}\nstdout={}\nstderr={}",
        conflict.status.code(),
        String::from_utf8_lossy(&conflict.stdout),
        String::from_utf8_lossy(&conflict.stderr),
    );
    let conflict_json: Value = serde_json::from_slice(&conflict.stdout).unwrap();
    assert_eq!(conflict_json["outcome"], "conflict");
    assert_eq!(conflict_json["cloud_head"], office_json["snapshot_id"]);
    assert_eq!(
        conflict_json["conflict_snapshot"],
        conflict_json["snapshot_id"]
    );
    assert!(
        conflict_json["message_zh"]
            .as_str()
            .unwrap()
            .contains("不会覆盖云端 HEAD"),
        "conflict UX must reject silent overwrite: {conflict_json}",
    );

    let status = run_mh_save(&[
        "server-status".into(),
        "--server-url".into(),
        server_url.clone(),
        "--secret-hex".into(),
        "4444444444444444444444444444444444444444444444444444444444444444".into(),
        "--game-profile".into(),
        "mh3g-3ds".into(),
    ])
    .await;
    assert!(status.status.success());
    let status_json: Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(status_json["cloud_head"], office_json["snapshot_id"]);
    assert_eq!(status_json["history_count"], 2);
    assert_eq!(status_json["conflict_count"], 1);
    assert_eq!(status_json["game_profile"], "mh3g-3ds");
    assert_eq!(status_json["conflict_diffs"].as_array().unwrap().len(), 1);
    assert_eq!(
        status_json["conflict_diffs"][0]["diff"]["game_profile"],
        "mh3g-3ds"
    );
    assert_eq!(status_json["conflict_diffs"][0]["diff"]["changed_files"], 1);
    assert_eq!(
        status_json["conflict_diffs"][0]["diff"]["semantic_available"],
        false
    );
    assert!(
        status_json["conflict_diffs"][0]["message_zh"]
            .as_str()
            .unwrap()
            .contains("文件/字节级差异"),
        "conflict status must expose user-readable diff: {status_json}",
    );

    let resolve = run_mh_save(&[
        "server-resolve-conflict".into(),
        "--server-url".into(),
        server_url.clone(),
        "--secret-hex".into(),
        "4444444444444444444444444444444444444444444444444444444444444444".into(),
        "--conflict-snapshot-id".into(),
        conflict_json["snapshot_id"].as_str().unwrap().into(),
        "--chosen-snapshot-id".into(),
        office_json["snapshot_id"].as_str().unwrap().into(),
        "--resolution".into(),
        "keep-cloud-head".into(),
    ])
    .await;
    assert!(
        resolve.status.success(),
        "explicit conflict resolution failed: {}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let resolve_json: Value = serde_json::from_slice(&resolve.stdout).unwrap();
    assert_eq!(resolve_json["resolved"], true);
    assert_eq!(
        resolve_json["chosen_snapshot_id"],
        office_json["snapshot_id"]
    );

    let resolved_status = run_mh_save(&[
        "server-status".into(),
        "--server-url".into(),
        server_url.clone(),
        "--secret-hex".into(),
        "4444444444444444444444444444444444444444444444444444444444444444".into(),
    ])
    .await;
    assert!(resolved_status.status.success());
    let resolved_status_json: Value = serde_json::from_slice(&resolved_status.stdout).unwrap();
    assert_eq!(resolved_status_json["history_count"], 2);
    assert_eq!(resolved_status_json["conflict_count"], 0);

    let restored = tmp.path().join("restored-conflict-head");
    let restore = run_mh_save(&[
        "server-restore".into(),
        "--server-url".into(),
        server_url.clone(),
        "--secret-hex".into(),
        "4444444444444444444444444444444444444444444444444444444444444444".into(),
        "--target".into(),
        restored.to_string_lossy().into_owned(),
        "--emulator-state".into(),
        "stopped".into(),
    ])
    .await;
    assert!(
        restore.status.success(),
        "restore must download cloud HEAD, not conflict branch: status={:?}
stdout={}
stderr={}",
        restore.status.code(),
        String::from_utf8_lossy(&restore.stdout),
        String::from_utf8_lossy(&restore.stderr),
    );
    let restore_json: Value = serde_json::from_slice(&restore.stdout).unwrap();
    assert_eq!(restore_json["snapshot_id"], office_json["snapshot_id"]);
    assert_eq!(fs::read(restored.join("save.bin")).unwrap(), b"office");

    let blocked = run_mh_save(&[
        "server-restore".into(),
        "--server-url".into(),
        server_url.clone(),
        "--secret-hex".into(),
        "4444444444444444444444444444444444444444444444444444444444444444".into(),
        "--target".into(),
        tmp.path()
            .join("blocked-running")
            .to_string_lossy()
            .into_owned(),
        "--emulator-state".into(),
        "running".into(),
    ])
    .await;
    assert!(
        !blocked.status.success(),
        "running restore must fail closed"
    );
    assert!(
        String::from_utf8_lossy(&blocked.stderr)
            .contains("已拒绝恢复：模拟器仍在运行，没有覆盖本地存档"),
        "stderr should explain restore precondition: {}",
        String::from_utf8_lossy(&blocked.stderr),
    );
    let blocked_error: Value = serde_json::from_slice(&blocked.stderr).unwrap();
    assert_eq!(blocked_error["error_code"], "emulator_running");
    assert_eq!(
        blocked_error["message"],
        "restore refused while emulator is running"
    );

    handle.abort();
}

#[tokio::test]
async fn cli_explicit_replace_cloud_head_uses_observed_head_as_cas_base() {
    let Some((server_url, handle)) = spawn_memory_server().await else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("save.bin"), b"cloud-old").unwrap();
    let common = [
        "--server-url".to_string(),
        server_url.clone(),
        "--root".into(),
        source.to_string_lossy().into_owned(),
        "--secret-hex".into(),
        "5555555555555555555555555555555555555555555555555555555555555555".into(),
    ];
    let first = run_mh_save(
        &[
            vec!["server-upload".into()],
            common.to_vec(),
            vec!["--device-id".into(), "office-mac".into()],
        ]
        .concat(),
    )
    .await;
    assert!(first.status.success());
    let first_json: Value = serde_json::from_slice(&first.stdout).unwrap();

    fs::write(source.join("save.bin"), b"android-authoritative").unwrap();
    let replace = run_mh_save(
        &[
            vec!["server-upload".into()],
            common.to_vec(),
            vec![
                "--device-id".into(),
                "home-android".into(),
                "--replace-cloud-head".into(),
            ],
        ]
        .concat(),
    )
    .await;
    assert!(
        replace.status.success(),
        "explicit replace failed: {}",
        String::from_utf8_lossy(&replace.stderr)
    );
    let replace_json: Value = serde_json::from_slice(&replace.stdout).unwrap();
    assert_eq!(replace_json["outcome"], "fast-forward");
    assert_eq!(replace_json["cloud_head_before"], first_json["snapshot_id"]);
    assert_eq!(replace_json["cloud_head"], replace_json["snapshot_id"]);
    assert_eq!(replace_json["conflict_snapshot"], Value::Null);

    handle.abort();
}
