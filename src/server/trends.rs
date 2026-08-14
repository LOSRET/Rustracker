//! Trend data collection, in-memory caching, and JSONL persistence.
//!
//! `dto` holds the HTTP response shapes, `store` the in-memory ring of
//! sampled points, and `persist` the JSONL load/save. [`TrendsState`]
//! ties them together and owns the background sampling task.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::core::tracker::TrackerSnapshot;
use crate::server::pool::TrackerPool;

pub(crate) mod dto;
pub(crate) mod persist;
pub(crate) mod store;

pub(crate) use dto::{ClientsResponse, StatsResponse, TrendsResponse};
pub(crate) use persist::unix_timestamp;

use dto::{ClientTrendData, TrendPointResponse};
use persist::{load_trends_from_file, save_client_point, save_trend_point};
use store::TrendStore;

pub(crate) const TREND_SAMPLE_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Trend store plus optional JSONL persistence paths.
#[derive(Clone)]
pub(crate) struct TrendsState {
    store: Arc<RwLock<TrendStore>>,
    file: Option<PathBuf>,
    top_clients_file: Option<PathBuf>,
}

impl TrendsState {
    pub(crate) fn new(trends_file: Option<PathBuf>) -> Self {
        let top_clients_file = trends_file.as_ref().map(|p| {
            p.parent()
                .unwrap_or(Path::new("."))
                .join("top_clients.jsonl")
        });
        let store = match trends_file
            .as_ref()
            .map(|p| load_trends_from_file(p, top_clients_file.as_ref()))
            .transpose()
        {
            Ok(Some(store)) => store,
            Ok(None) => TrendStore::default(),
            Err(err) => {
                tracing::warn!("failed to load trend data: {err}");
                TrendStore::default()
            }
        };
        Self {
            store: Arc::new(RwLock::new(store)),
            file: trends_file,
            top_clients_file,
        }
    }

    pub(crate) async fn record(
        &self,
        now: u64,
        snapshot: &TrackerSnapshot,
    ) -> Arc<Vec<TrendPointResponse>> {
        self.store.write().await.record(now, snapshot)
    }

    pub(crate) async fn record_clients(&self, now: u64, clients: &[(u8, u64)]) -> ClientTrendData {
        self.store
            .write()
            .await
            .record_clients(now, clients)
            .clone()
    }

    /// Record a sample point and persist it to disk (sampling task only).
    /// The write lock is released before the disk write (record is
    /// idempotent per bucket), so block I/O never stalls sampling readers.
    pub(crate) async fn record_and_persist(&self, now: u64, snapshot: &TrackerSnapshot) {
        self.store.write().await.record(now, snapshot);
        if let Some(ref path) = self.file {
            let _ = save_trend_point(path, now, snapshot);
        }
    }

    /// Record a client distribution point and persist it (sampling task only).
    pub(crate) async fn record_clients_and_persist(&self, now: u64, clients: &[(u8, u64)]) {
        self.store.write().await.record_clients(now, clients);
        if let Some(ref path) = self.top_clients_file {
            let _ = save_client_point(path, now, clients);
        }
    }

    /// Background task: sample tracker state every 10 minutes.
    pub(crate) fn spawn_sampling(&self, tracker: Arc<TrackerPool>) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(TREND_SAMPLE_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            interval.tick().await;

            loop {
                interval.tick().await;
                let snapshot = tracker.snapshot().await;
                let now = unix_timestamp();
                state.record_and_persist(now, &snapshot).await;
                state
                    .record_clients_and_persist(now, &snapshot.clients)
                    .await;
            }
        });
    }
}
