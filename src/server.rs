#![allow(clippy::type_complexity)]

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;

use crate::core::tracker::Tracker;

mod admin;
mod audit;
mod blacklist;
pub(crate) mod handlers;
mod mcp;
mod pool;
mod rps;
mod trends;

use audit::AuditLog;
use blacklist::BlacklistStore;
use pool::TrackerPool;
pub use pool::DEFAULT_TRACKER_SHARDS;
use rps::RpsMeter;
use trends::TrendsState;

#[derive(Clone)]
pub struct AppState {
    pub(crate) tracker: Arc<TrackerPool>,
    pub(crate) trends: TrendsState,
    pub(crate) blacklist: Arc<BlacklistStore>,
    pub(crate) audit: Arc<AuditLog>,
    pub(crate) admin_token: Option<String>,
    pub(crate) trust_proxy_headers: bool,
    pub(crate) mcp_enabled: bool,
    pub(crate) started_at: Instant,
    pub(crate) rps: RpsMeter,
    #[cfg(feature = "dashboard")]
    pub(crate) versioned_index: axum::body::Bytes,
}

impl AppState {
    pub fn new(tracker: Tracker, trends_file: Option<PathBuf>) -> Self {
        Self {
            tracker: Arc::new(TrackerPool::single(tracker)),
            trends: TrendsState::new(trends_file),
            blacklist: Arc::new(BlacklistStore::new(None)),
            audit: Arc::new(AuditLog::new(None)),
            admin_token: None,
            trust_proxy_headers: false,
            mcp_enabled: true,
            started_at: Instant::now(),
            rps: RpsMeter::new(),
            #[cfg(feature = "dashboard")]
            versioned_index: handlers::make_versioned_index(),
        }
    }

    pub fn sharded(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        Self::sharded_with_blacklist_file(interval, peer_timeout, shards, None, None, None, false)
    }

    pub fn sharded_with_blacklist_file(
        interval: Duration,
        peer_timeout: Duration,
        shards: usize,
        blacklist_path: Option<PathBuf>,
        trends_file: Option<PathBuf>,
        admin_token: Option<String>,
        trust_proxy_headers: bool,
    ) -> Self {
        let audit = Arc::new(AuditLog::new(blacklist_path.as_deref()));
        let state = Self {
            tracker: Arc::new(TrackerPool::new(interval, peer_timeout, shards)),
            trends: TrendsState::new(trends_file),
            blacklist: Arc::new(BlacklistStore::new(blacklist_path)),
            audit,
            admin_token,
            trust_proxy_headers,
            mcp_enabled: true,
            started_at: Instant::now(),
            rps: RpsMeter::new(),
            #[cfg(feature = "dashboard")]
            versioned_index: handlers::make_versioned_index(),
        };

        pool::spawn_expiry_sweep(state.tracker.clone());
        state.trends.spawn_sampling(state.tracker.clone());
        state.rps.spawn_sampler();
        blacklist::spawn_watcher(state.blacklist.clone(), blacklist::WATCH_INTERVAL);
        state
    }

    /// Enable or disable the MCP endpoint. MCP still requires an admin token
    /// to be configured; this only provides an explicit opt-out.
    pub fn with_mcp(mut self, enabled: bool) -> Self {
        self.mcp_enabled = enabled;
        self
    }
}

pub fn router(state: AppState) -> Router {
    let mut r = Router::new()
        .route("/api/stats", get(handlers::stats))
        .route("/api/trends", get(handlers::trends))
        .route("/api/clients", get(handlers::clients))
        .route("/api/clients/list", get(handlers::clients_list))
        .route("/api/top100", get(handlers::top100))
        .route(
            "/api/blacklist",
            get(admin::blacklist_get)
                .post(admin::add_blacklist)
                .delete(admin::remove_blacklist),
        )
        .route("/announce", get(handlers::announce))
        .route("/scrape", get(handlers::scrape))
        .route("/healthz", get(handlers::healthz));

    #[cfg(feature = "dashboard")]
    {
        r = r
            .route("/", get(handlers::index))
            .route("/assets/{*name}", get(handlers::asset))
            .fallback(handlers::spa_fallback);
    }

    #[cfg(not(feature = "dashboard"))]
    {
        r = r.fallback(handlers::not_found);
    }

    let mcp_enabled = state.mcp_enabled
        && state
            .admin_token
            .as_deref()
            .is_some_and(|token| !token.is_empty());
    if mcp_enabled {
        // Auth is layered on a dedicated sub-router so it only guards /mcp.
        let mcp_router = Router::new()
            .nest_service("/mcp", mcp::service(state.clone()))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                admin::mcp_auth,
            ));
        r = r.merge(mcp_router);
    }

    r.with_state(state)
}

pub async fn serve<F>(listeners: Vec<TcpListener>, app: Router, shutdown: F) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();
    tokio::spawn(async move {
        shutdown.await;
        tracing::trace!("received graceful shutdown signal");
        shutdown_handle.graceful_shutdown(Some(Duration::from_secs(10)));
    });

    let mut tasks = Vec::with_capacity(listeners.len());
    for listener in listeners {
        let app = app.clone();
        let handle = handle.clone();
        tasks.push(tokio::spawn(async move {
            let mut server = axum_server::from_tcp(listener.into_std()?)?;

            server
                .http_builder()
                .http1()
                .keep_alive(true)
                .writev(true)
                .pipeline_flush(false);

            server
                .handle(handle)
                .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                .await
        }));
    }

    for task in tasks {
        match task.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            // A panicking server task is a real error; surface it as an io
            // error instead of re-panicking.
            Err(join_err) => return Err(io::Error::other(join_err)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod serve_tests {
    use std::future;
    use std::net::SocketAddr;

    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    async fn start_server(app: Router) -> SocketAddr {
        let listener = TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            serve(vec![listener], app, future::pending()).await.unwrap();
        });
        addr
    }

    async fn send_request(stream: &mut TcpStream, path: &str) {
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
    }

    async fn read_response(stream: &mut TcpStream, expected_body: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 128];

        loop {
            let read = stream.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0, "connection closed before full response");
            buf.extend_from_slice(&chunk[..read]);
            if buf.windows(expected_body.len()).any(|w| w == expected_body) {
                return buf;
            }
        }
    }

    #[tokio::test]
    async fn keepalive_connection_accepts_later_request() {
        let app = Router::new().route("/healthz", get(|| async { "ok" }));
        let addr = start_server(app).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();

        send_request(&mut stream, "/healthz").await;
        let first = read_response(&mut stream, b"ok").await;
        assert!(first.windows(b"200 OK".len()).any(|w| w == b"200 OK"));

        tokio::time::sleep(Duration::from_millis(300)).await;

        send_request(&mut stream, "/healthz").await;
        let second = read_response(&mut stream, b"ok").await;
        assert!(second.windows(b"200 OK".len()).any(|w| w == b"200 OK"));
    }
}
