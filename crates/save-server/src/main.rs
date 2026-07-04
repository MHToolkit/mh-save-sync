use save_server::{AppState, router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--healthcheck") {
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
        anyhow::bail!("healthcheck failed");
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
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
