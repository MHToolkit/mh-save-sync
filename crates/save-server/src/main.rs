use save_server::{AppState, router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--healthcheck") {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect("127.0.0.1:8080")?;
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;
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
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mh-save-server listening");
    axum::serve(listener, router(AppState::default())).await?;
    Ok(())
}
