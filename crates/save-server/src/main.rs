use save_server::{AppState, router};
use std::net::SocketAddr;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--healthcheck") {
        return healthcheck();
    }
    if args.iter().any(|a| a == "--runtime-identity-check") {
        return runtime_identity_check();
    }

    preload_secret_envs()?;
    drop_runtime_privileges()?;
    write_runtime_identity_marker()?;
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_server())
}

async fn run_server() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let (runtime_uid, runtime_gid) = effective_runtime_identity();
    tracing::info!(runtime_uid, runtime_gid, "mh-save-server runtime identity");
    let bind = std::env::var("MH_SAVE_SYNC_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let addr: SocketAddr = bind.parse()?;
    let state = if std::env::var_os("DATABASE_URL").is_some()
        || std::env::var_os("DATABASE_PASSWORD_FILE").is_some()
    {
        AppState::persistent_from_env().await?
    } else {
        AppState::default()
    };
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mh-save-server listening");
    axum::serve(listener, router(state)).await?;
    Ok(())
}

fn healthcheck() -> anyhow::Result<()> {
    use std::io::{Read, Write};
    let bind =
        std::env::var("MH_SAVE_SYNC_HEALTH_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let mut stream = std::net::TcpStream::connect(bind)?;
    stream.write_all(b"GET /ready HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;
    let mut buf = String::new();
    stream.read_to_string(&mut buf)?;
    if buf.starts_with("HTTP/1.1 200") {
        return Ok(());
    }
    anyhow::bail!("healthcheck failed")
}

fn runtime_identity_check() -> anyhow::Result<()> {
    let marker = std::env::var("MH_SAVE_SYNC_RUNTIME_IDENTITY_FILE")?;
    let uid = std::env::var("MH_SAVE_SYNC_RUNTIME_UID").ok();
    let gid = std::env::var("MH_SAVE_SYNC_RUNTIME_GID").ok();
    let Some(expected) = parse_runtime_identity(uid.as_deref(), gid.as_deref())? else {
        anyhow::bail!("runtime identity is not configured");
    };
    let actual = std::fs::read_to_string(marker)?;
    if actual.trim() != runtime_identity_marker(expected) {
        anyhow::bail!("runtime identity marker mismatch");
    }
    Ok(())
}

fn preload_secret_envs() -> anyhow::Result<()> {
    for (value_name, file_name) in [
        ("DATABASE_PASSWORD", "DATABASE_PASSWORD_FILE"),
        ("S3_ACCESS_KEY", "S3_ACCESS_KEY_FILE"),
        ("S3_SECRET_KEY", "S3_SECRET_KEY_FILE"),
    ] {
        let value = std::env::var(value_name).ok();
        let file = std::env::var_os(file_name).map(PathBuf::from);
        if let Some(secret) = resolve_secret(value, file)? {
            // This runs before the Tokio runtime and before any application
            // threads exist, so no concurrent environment readers can race it.
            unsafe { std::env::set_var(value_name, secret) };
        }
    }
    Ok(())
}

fn resolve_secret(value: Option<String>, file: Option<PathBuf>) -> anyhow::Result<Option<String>> {
    if value.is_some() {
        return Ok(value);
    }
    file.map(|path| {
        Ok(std::fs::read_to_string(path)?
            .trim_end_matches(['\r', '\n'])
            .to_string())
    })
    .transpose()
}

fn parse_runtime_identity(
    uid: Option<&str>,
    gid: Option<&str>,
) -> anyhow::Result<Option<(u32, u32)>> {
    match (uid, gid) {
        (None, None) => Ok(None),
        (Some(uid), Some(gid)) => {
            let uid = uid.parse()?;
            let gid = gid.parse()?;
            if uid == 0 || gid == 0 {
                anyhow::bail!("runtime uid/gid must be non-zero");
            }
            Ok(Some((uid, gid)))
        }
        _ => anyhow::bail!(
            "MH_SAVE_SYNC_RUNTIME_UID and MH_SAVE_SYNC_RUNTIME_GID must be set together"
        ),
    }
}

#[cfg(unix)]
fn effective_runtime_identity() -> (u32, u32) {
    // SAFETY: these libc getters have no arguments and no side effects.
    unsafe { (libc::geteuid(), libc::getegid()) }
}

fn runtime_identity_marker((uid, gid): (u32, u32)) -> String {
    format!("{uid}:{gid}")
}

fn write_runtime_identity_marker() -> anyhow::Result<()> {
    let Ok(path) = std::env::var("MH_SAVE_SYNC_RUNTIME_IDENTITY_FILE") else {
        return Ok(());
    };
    std::fs::write(
        path,
        format!(
            "{}\n",
            runtime_identity_marker(effective_runtime_identity())
        ),
    )?;
    Ok(())
}

#[cfg(not(unix))]
fn effective_runtime_identity() -> (u32, u32) {
    (0, 0)
}

#[cfg(unix)]
fn drop_runtime_privileges() -> anyhow::Result<()> {
    let uid = std::env::var("MH_SAVE_SYNC_RUNTIME_UID").ok();
    let gid = std::env::var("MH_SAVE_SYNC_RUNTIME_GID").ok();
    let Some((uid, gid)) = parse_runtime_identity(uid.as_deref(), gid.as_deref())? else {
        return Ok(());
    };

    // SAFETY: these libc calls have no pointer arguments. They run before the
    // Tokio runtime is created, and setgid must precede setuid permanently.
    let current_uid = unsafe { libc::geteuid() };
    let current_gid = unsafe { libc::getegid() };
    if current_uid != 0 {
        if current_uid == uid && current_gid == gid {
            return Ok(());
        }
        anyhow::bail!(
            "cannot change runtime identity from non-root {current_uid}:{current_gid} to {uid}:{gid}"
        );
    }
    if unsafe { libc::setgroups(0, std::ptr::null()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if unsafe { libc::setgid(gid) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if unsafe { libc::setuid(uid) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn drop_runtime_privileges() -> anyhow::Result<()> {
    let uid = std::env::var("MH_SAVE_SYNC_RUNTIME_UID").ok();
    let gid = std::env::var("MH_SAVE_SYNC_RUNTIME_GID").ok();
    if parse_runtime_identity(uid.as_deref(), gid.as_deref())?.is_some() {
        anyhow::bail!("runtime uid/gid drop is only supported on Unix");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_identity_requires_a_complete_numeric_pair() {
        assert_eq!(parse_runtime_identity(None, None).unwrap(), None);
        assert_eq!(
            parse_runtime_identity(Some("65532"), Some("65532")).unwrap(),
            Some((65532, 65532))
        );
        assert!(parse_runtime_identity(Some("65532"), None).is_err());
        assert!(parse_runtime_identity(Some("not-a-uid"), Some("65532")).is_err());
        assert!(parse_runtime_identity(Some("0"), Some("65532")).is_err());
        assert!(parse_runtime_identity(Some("65532"), Some("0")).is_err());
        assert_eq!(runtime_identity_marker((65532, 65532)), "65532:65532");
    }

    #[test]
    fn direct_secret_wins_and_file_secret_is_trimmed() {
        let root =
            std::env::temp_dir().join(format!("mh-save-sync-secret-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("secret.txt");
        std::fs::write(&file, "from-file\n").unwrap();

        assert_eq!(
            resolve_secret(Some("from-env".into()), Some(file.clone())).unwrap(),
            Some("from-env".into())
        );
        assert_eq!(
            resolve_secret(None, Some(file.clone())).unwrap(),
            Some("from-file".into())
        );
        assert_eq!(resolve_secret(None, None).unwrap(), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
