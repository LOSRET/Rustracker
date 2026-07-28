#![allow(clippy::type_complexity)]

use std::collections::HashSet;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::core::tracker::Tracker;
use crate::core::types::InfoHash;

mod admin;
mod blacklist;
pub(crate) mod handlers;
mod pool;
mod trends;

use pool::TrackerPool;
pub use pool::DEFAULT_TRACKER_SHARDS;

use trends::TrendStore;

fn load_trends(trends_file: &Option<PathBuf>) -> TrendStore {
    let top_clients_file = trends_file.as_ref().map(|p| {
        p.parent()
            .unwrap_or(Path::new("."))
            .join("top_clients.jsonl")
    });
    match trends_file
        .as_ref()
        .map(|p| trends::load_trends_from_file(p, top_clients_file.as_ref()))
        .transpose()
    {
        Ok(Some(store)) => store,
        Ok(None) => TrendStore::default(),
        Err(err) => {
            tracing::warn!("failed to load trend data: {err}");
            TrendStore::default()
        }
    }
}

const EXPIRE_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(1);
const TREND_SAMPLE_INTERVAL: Duration = Duration::from_secs(10 * 60);
const BLACKLIST_WATCH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct AppState {
    pub(crate) tracker: Arc<TrackerPool>,
    pub(crate) trends: Arc<RwLock<TrendStore>>,
    pub(crate) blacklist: Arc<RwLock<HashSet<InfoHash>>>,
    pub(crate) blacklist_path: Option<PathBuf>,
    pub(crate) admin_token: Option<String>,
    pub(crate) started_at: Instant,
    pub(crate) rps_counter: Arc<AtomicU64>,
    pub(crate) current_rps: Arc<AtomicU64>,
    #[cfg(feature = "dashboard")]
    pub(crate) versioned_index: axum::body::Bytes,
}

impl AppState {
    pub fn new(tracker: Tracker, trends_file: Option<PathBuf>) -> Self {
        Self {
            tracker: Arc::new(TrackerPool::single(tracker)),
            trends: Arc::new(RwLock::new(load_trends(&trends_file))),
            blacklist: Arc::new(RwLock::new(HashSet::new())),
            blacklist_path: None,
            admin_token: None,
            started_at: Instant::now(),
            rps_counter: Arc::new(AtomicU64::new(0)),
            current_rps: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "dashboard")]
            versioned_index: handlers::make_versioned_index(),
        }
    }

    pub fn sharded(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        Self::sharded_with_blacklist_file(interval, peer_timeout, shards, None, None, None)
    }

    pub fn sharded_with_blacklist_file(
        interval: Duration,
        peer_timeout: Duration,
        shards: usize,
        blacklist_path: Option<PathBuf>,
        trends_file: Option<PathBuf>,
        admin_token: Option<String>,
    ) -> Self {
        let initial = blacklist_path
            .as_deref()
            .and_then(|path| match blacklist::load_blacklist(path) {
                Ok(set) => Some(set),
                Err(err) => {
                    tracing::warn!("{err}");
                    None
                }
            })
            .unwrap_or_default();

        let state = Self {
            tracker: Arc::new(TrackerPool::new(interval, peer_timeout, shards)),
            trends: Arc::new(RwLock::new(load_trends(&trends_file))),
            blacklist: Arc::new(RwLock::new(initial)),
            blacklist_path: blacklist_path.clone(),
            admin_token,
            started_at: Instant::now(),
            rps_counter: Arc::new(AtomicU64::new(0)),
            current_rps: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "dashboard")]
            versioned_index: handlers::make_versioned_index(),
        };

        state.spawn_maintenance(trends_file);
        state.spawn_rps_sampler();
        if let Some(path) = blacklist_path {
            state.spawn_blacklist_watcher(path);
        }
        state
    }

    fn spawn_maintenance(&self, trends_file: Option<PathBuf>) {
        let tracker = self.tracker.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(EXPIRE_MAINTENANCE_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                interval.tick().await;
                tracker.expire_due(Instant::now());
            }
        });

        let tracker = self.tracker.clone();
        let trends = self.trends.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(TREND_SAMPLE_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            interval.tick().await;

            loop {
                interval.tick().await;
                let snapshot = tracker.snapshot().await;
                let now = trends::unix_timestamp();
                let mut store = trends.write().await;
                store.record(now, &snapshot);
                if let Some(ref path) = trends_file {
                    let _ = trends::save_trend_point(path, now, &snapshot);
                }
                store.record_clients(now, &snapshot.clients);
                if let Some(ref path) = trends_file {
                    let _ = trends::save_client_point(
                        &path
                            .parent()
                            .unwrap_or(Path::new("."))
                            .join("top_clients.jsonl"),
                        now,
                        &snapshot.clients,
                    );
                }
            }
        });
    }

    fn spawn_rps_sampler(&self) {
        let rps_counter = self.rps_counter.clone();
        let current_rps = self.current_rps.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                let count = rps_counter.swap(0, Ordering::Relaxed);
                current_rps.store((count as f64).to_bits(), Ordering::Relaxed);
            }
        });
    }

    fn spawn_blacklist_watcher(&self, path: PathBuf) {
        let blacklist = self.blacklist.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(BLACKLIST_WATCH_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            let mut last_mtime = file_mtime(&path);

            loop {
                interval.tick().await;
                let mtime = file_mtime(&path);
                if mtime == last_mtime {
                    continue;
                }
                last_mtime = mtime;
                match blacklist::load_blacklist(&path) {
                    Ok(new_set) => {
                        let count = new_set.len();
                        *blacklist.write().await = new_set;
                        tracing::info!(count, "blacklist reloaded");
                    }
                    Err(err) => {
                        tracing::warn!("{err}");
                    }
                }
            }
        });
    }
}

pub fn router(state: AppState) -> Router {
    let r = Router::new()
        .route("/api/stats", get(handlers::stats))
        .route("/api/trends", get(handlers::trends))
        .route("/api/clients", get(handlers::clients))
        .route("/api/clients/list", get(handlers::clients_list))
        .route("/api/top100", get(handlers::top100))
        .route(
            "/api/blacklist",
            get(admin::blacklist_status).post(admin::add_blacklist),
        )
        .route("/announce", get(handlers::announce))
        .route("/scrape", get(handlers::scrape))
        .route("/healthz", get(handlers::healthz));

    #[cfg(feature = "dashboard")]
    let r = r
        .route("/", get(handlers::index))
        .route("/assets/{*name}", get(handlers::asset));

    r.fallback(handlers::not_found).with_state(state)
}

fn file_mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
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
        task.await.expect("server task panicked")?;
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
