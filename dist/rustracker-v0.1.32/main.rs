use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use rustracker::server::{router, AppState, DEFAULT_TRACKER_SHARDS};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    #[arg(long, env = "RUSTRACKER_LISTEN", default_value = "0.0.0.0:8080")]
    listen: SocketAddr,

    #[arg(long, env = "RUSTRACKER_INTERVAL_SECS", default_value_t = 1800)]
    interval_secs: u64,

    #[arg(long, env = "RUSTRACKER_PEER_TIMEOUT_SECS", default_value_t = 3000)]
    peer_timeout_secs: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let app = router(AppState::sharded(
        Duration::from_secs(args.interval_secs),
        Duration::from_secs(args.peer_timeout_secs),
        DEFAULT_TRACKER_SHARDS,
    ));
    let listener = TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("failed to bind {}", args.listen))?;

    info!(listen = %args.listen, "rustracker listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
