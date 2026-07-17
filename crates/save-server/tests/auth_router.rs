use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use base64::Engine;
use ed25519_dalek::SigningKey;
use save_crypto::{
    account_handle, account_root_signing_key, derive_account_keys,
    issue_device_certificate_with_id, sign_http_request,
};
use save_domain::SnapshotId;
use save_server::{
    AccountBootstrapRequest, AppState, BeginSnapshotRequest, BeginSnapshotResponse,
    ChallengeRequest, ChallengeResponse, CommitSnapshotRequest, DeviceRegisterRequest,
    PutManifestRequest, router,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const BEGIN_PATH: &str = "/v1/snapshots/begin";

struct Identity {
    account: String,
    cert_id: String,
    signing: SigningKey,
}

fn json_request<T: Serialize>(path: &str, value: &T) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).unwrap()))
        .unwrap()
}

async fn response_json<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn register_identity(app: &axum::Router) -> Identity {
    register_distinct_identity(app, 0x42, 0x24, 0x33).await
}

async fn register_distinct_identity(
    app: &axum::Router,
    recovery_byte: u8,
    signing_byte: u8,
    cert_byte: u8,
) -> Identity {
    let keys = derive_account_keys(&[recovery_byte; 32]).unwrap();
    let root = account_root_signing_key(&keys);
    let account = account_handle(&keys);
    let signing = SigningKey::from_bytes(&[signing_byte; 32]);
    let cert_id_bytes = [cert_byte; 16];
    let cert_id = hex::encode(cert_id_bytes);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let cert = issue_device_certificate_with_id(
        &root,
        &signing.verifying_key(),
        cert_id_bytes,
        now.saturating_sub(1),
        now + 3600,
        1,
    )
    .unwrap();
    let mut certificate = Vec::new();
    ciborium::ser::into_writer(&cert, &mut certificate).unwrap();

    let bootstrap = app
        .clone()
        .oneshot(json_request(
            "/v1/accounts/bootstrap",
            &AccountBootstrapRequest {
                account_handle: account.clone(),
                root_public_key_b64: base64::engine::general_purpose::STANDARD
                    .encode(root.verifying_key().to_bytes()),
            },
        ))
        .await
        .unwrap();
    assert_eq!(bootstrap.status(), StatusCode::CREATED);

    let register = app
        .clone()
        .oneshot(json_request(
            "/v1/devices/register",
            &DeviceRegisterRequest {
                account_handle: account.clone(),
                cert_id: cert_id.clone(),
                device_public_key_b64: base64::engine::general_purpose::STANDARD
                    .encode(signing.verifying_key().to_bytes()),
                certificate_b64: base64::engine::general_purpose::STANDARD.encode(certificate),
            },
        ))
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::CREATED);

    Identity {
        account,
        cert_id,
        signing,
    }
}

async fn signed_get(app: &axum::Router, identity: &Identity, path: &str) -> Request<Body> {
    let challenge_response = app
        .clone()
        .oneshot(json_request(
            "/v1/accounts/challenge",
            &ChallengeRequest {
                account_handle: identity.account.clone(),
                device_cert_id: identity.cert_id.clone(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(challenge_response.status(), StatusCode::OK);
    let challenge: ChallengeResponse = response_json(challenge_response).await;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let signature = sign_http_request(
        &identity.signing,
        "GET",
        path,
        &[],
        &challenge.challenge_id.to_string(),
        &challenge.nonce_b64,
        timestamp,
    );
    Request::builder()
        .method("GET")
        .uri(path)
        .header("x-mh-account", &identity.account)
        .header("x-mh-device-cert", &identity.cert_id)
        .header("x-mh-challenge-id", challenge.challenge_id.to_string())
        .header("x-mh-nonce", challenge.nonce_b64)
        .header("x-mh-timestamp", timestamp.to_string())
        .header(
            "x-mh-signature",
            base64::engine::general_purpose::STANDARD.encode(signature),
        )
        .body(Body::empty())
        .unwrap()
}

async fn upload_empty_snapshot(
    app: &axum::Router,
    identity: &Identity,
    logical_save_id: &str,
    snapshot_byte: u8,
) -> SnapshotId {
    let manifest = b"encrypted-manifest";
    let manifest_id = hex::encode(Sha256::digest(manifest));
    let begin = serde_json::to_vec(&BeginSnapshotRequest {
        account_handle: Some(identity.account.clone()),
        device_cert_id: Some(identity.cert_id.clone()),
        logical_save_id: logical_save_id.to_owned(),
        base_head: None,
        parents: Vec::new(),
        encrypted_manifest_id: manifest_id.clone(),
        chunk_ids: Vec::new(),
    })
    .unwrap();
    let request = signed_request(app, identity, BEGIN_PATH, BEGIN_PATH, &begin, &begin).await;
    let response = app.clone().oneshot(request).await.unwrap();
    if response.status() != StatusCode::OK {
        let status = response.status();
        let detail =
            String::from_utf8_lossy(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .into_owned();
        panic!("signed begin failed with {status}: {detail}");
    }
    let begin: BeginSnapshotResponse = response_json(response).await;

    let manifest_path = format!("/v1/snapshots/{}/manifest", begin.upload_id);
    let manifest_body = serde_json::to_vec(&PutManifestRequest {
        manifest_id,
        sha256: hex::encode(Sha256::digest(manifest)),
        bytes_b64: base64::engine::general_purpose::STANDARD.encode(manifest),
    })
    .unwrap();
    let request = signed_request(
        app,
        identity,
        &manifest_path,
        &manifest_path,
        &manifest_body,
        &manifest_body,
    )
    .await;
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let snapshot = SnapshotId(hex::encode([snapshot_byte; 32]));
    let commit_path = format!("/v1/snapshots/{}/commit", begin.upload_id);
    let commit_body = serde_json::to_vec(&CommitSnapshotRequest {
        snapshot_id: snapshot.clone(),
    })
    .unwrap();
    let request = signed_request(
        app,
        identity,
        &commit_path,
        &commit_path,
        &commit_body,
        &commit_body,
    )
    .await;
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::OK
    );
    snapshot
}

async fn signed_request(
    app: &axum::Router,
    identity: &Identity,
    signed_path: &str,
    request_path: &str,
    signed_body: &[u8],
    request_body: &[u8],
) -> Request<Body> {
    let challenge_response = app
        .clone()
        .oneshot(json_request(
            "/v1/accounts/challenge",
            &ChallengeRequest {
                account_handle: identity.account.clone(),
                device_cert_id: identity.cert_id.clone(),
            },
        ))
        .await
        .unwrap();
    assert_eq!(challenge_response.status(), StatusCode::OK);
    let challenge: ChallengeResponse = response_json(challenge_response).await;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let signature = sign_http_request(
        &identity.signing,
        "POST",
        signed_path,
        signed_body,
        &challenge.challenge_id.to_string(),
        &challenge.nonce_b64,
        timestamp,
    );
    Request::builder()
        .method("POST")
        .uri(request_path)
        .header("content-type", "application/json")
        .header("x-mh-account", &identity.account)
        .header("x-mh-device-cert", &identity.cert_id)
        .header("x-mh-challenge-id", challenge.challenge_id.to_string())
        .header("x-mh-nonce", challenge.nonce_b64)
        .header("x-mh-timestamp", timestamp.to_string())
        .header(
            "x-mh-signature",
            base64::engine::general_purpose::STANDARD.encode(signature),
        )
        .body(Body::from(request_body.to_vec()))
        .unwrap()
}

fn begin_body(account: &str, cert_id: &str, logical_save_id: &str) -> Vec<u8> {
    serde_json::to_vec(&BeginSnapshotRequest {
        account_handle: Some(account.to_owned()),
        device_cert_id: Some(cert_id.to_owned()),
        logical_save_id: logical_save_id.to_owned(),
        base_head: None,
        parents: Vec::new(),
        encrypted_manifest_id: "11".repeat(32),
        chunk_ids: Vec::new(),
    })
    .unwrap()
}

#[tokio::test]
async fn begin_rejects_anonymous_request() {
    let app = router(AppState::default());
    let response = app
        .oneshot(json_request(
            BEGIN_PATH,
            &serde_json::json!({
                "logical_save_id": "anonymous",
                "base_head": null,
                "parents": [],
                "encrypted_manifest_id": "11".repeat(32),
                "chunk_ids": []
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signed_begin_succeeds_and_exact_replay_fails() {
    let app = router(AppState::default());
    let identity = register_identity(&app).await;
    let body = begin_body(&identity.account, &identity.cert_id, "signed-begin");
    let request = signed_request(&app, &identity, BEGIN_PATH, BEGIN_PATH, &body, &body).await;
    let (parts, _) = request.into_parts();
    let replay_headers = parts.headers.clone();
    let first = app
        .clone()
        .oneshot(Request::from_parts(parts, Body::from(body.clone())))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let mut replay = Request::builder().method("POST").uri(BEGIN_PATH);
    *replay.headers_mut().unwrap() = replay_headers;
    let replay = replay.body(Body::from(body)).unwrap();
    let second = app.oneshot(replay).await.unwrap();
    assert_eq!(second.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signed_begin_rejects_body_tampering() {
    let app = router(AppState::default());
    let identity = register_identity(&app).await;
    let signed = begin_body(&identity.account, &identity.cert_id, "original");
    let tampered = begin_body(&identity.account, &identity.cert_id, "tampered");
    let request = signed_request(&app, &identity, BEGIN_PATH, BEGIN_PATH, &signed, &tampered).await;
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signed_begin_rejects_path_tampering() {
    let app = router(AppState::default());
    let identity = register_identity(&app).await;
    let body = begin_body(&identity.account, &identity.cert_id, "path-bound");
    let request = signed_request(
        &app,
        &identity,
        BEGIN_PATH,
        "/v1/snapshots/00000000-0000-0000-0000-000000000000/commit",
        &body,
        &body,
    )
    .await;
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn heads_require_authentication_and_are_account_isolated() {
    let app = router(AppState::default());
    let account_a = register_distinct_identity(&app, 0x41, 0x21, 0x31).await;
    let account_b = register_distinct_identity(&app, 0x42, 0x22, 0x32).await;
    let logical_save_id = "same-logical-save-id";
    let snapshot_a = upload_empty_snapshot(&app, &account_a, logical_save_id, 0xa1).await;

    let path = format!("/v1/heads/{logical_save_id}");
    let anonymous = app
        .clone()
        .oneshot(Request::builder().uri(&path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let own = app
        .clone()
        .oneshot(signed_get(&app, &account_a, &path).await)
        .await
        .unwrap();
    assert_eq!(own.status(), StatusCode::OK);
    assert_eq!(response_json::<SnapshotId>(own).await, snapshot_a);

    let other = app
        .clone()
        .oneshot(signed_get(&app, &account_b, &path).await)
        .await
        .unwrap();
    assert_eq!(other.status(), StatusCode::NOT_FOUND);

    let snapshot_b = upload_empty_snapshot(&app, &account_b, logical_save_id, 0xb2).await;
    let own_b = app
        .clone()
        .oneshot(signed_get(&app, &account_b, &path).await)
        .await
        .unwrap();
    assert_eq!(own_b.status(), StatusCode::OK);
    assert_eq!(response_json::<SnapshotId>(own_b).await, snapshot_b);

    let own_a_again = app
        .clone()
        .oneshot(signed_get(&app, &account_a, &path).await)
        .await
        .unwrap();
    assert_eq!(response_json::<SnapshotId>(own_a_again).await, snapshot_a);
}
