use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use rustracker::server::{router, AppState, DEFAULT_TRACKER_SHARDS};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    #[arg(long, env = "RUSTRACKER_LISTEN", default_value = "0.0.0.0:8080")]
    listen: SocketAddr,

    #[arg(long, env = "RUSTRACKER_INTERVAL_SECS", default_value_t = 1800)]
    interval_secs: u64,

    #[arg(long, env = "RUSTRACKER_PEER_TIMEOUT_SECS", default_value_t = 3000)]
    peer_timeout_secs: u64,

    /// Path to a torrent blacklist file (one 40-char hex info_hash per line).
    #[arg(long, env = "RUSTRACKER_BLACKLIST")]
    blacklist: Option<PathBuf>,

    /// Path to persist trend data (JSONL). Trends are saved every 10 minutes and
    /// retained for 7 days. A companion `top_clients.jsonl` is created alongside it.
    #[arg(long, env = "RUSTRACKER_TRENDS_FILE")]
    trends_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let app = router(AppState::sharded_with_blacklist_file(
        Duration::from_secs(args.interval_secs),
        Duration::from_secs(args.peer_timeout_secs),
        DEFAULT_TRACKER_SHARDS,
        args.blacklist,
        args.trends_file,
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
