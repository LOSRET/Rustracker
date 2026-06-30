//! Trend data collection, in-memory caching, and JSONL persistence.
//!
//! TrendStore records snapshots at 10-minute granularity and retains
//! 7 days of data. Two JSONL files are maintained: one for swarm-level
//! trends and one for per-client top-N trends.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::core::tracker::TrackerSnapshot;
use crate::protocol::client_id;

// ── Response types exported for handlers ─────────────────────────────────────

pub(crate) const TOP_CLIENT_COUNT: usize = 15;

#[derive(Debug, Serialize)]
pub(crate) struct StatsResponse {
    pub interval: u64,
    pub peer_timeout: u64,
    pub torrents: usize,
    pub peers: usize,
    pub seeders: usize,
    pub leechers: usize,
    pub completed: u64,
    pub rps: f64,
    pub version: &'static str,
    pub uptime_secs: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct TrendsResponse {
    pub history: Arc<Vec<TrendPointResponse>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ClientsResponse {
    pub timestamp: u64,
    pub clients: Arc<Vec<String>>,
    pub tags: Arc<Vec<u8>>,
    pub history: Arc<Vec<ClientTrendPoint>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub(crate) struct TrendPointResponse {
    pub timestamp: u64,
    pub torrents: usize,
    pub peers: usize,
    pub seeders: usize,
    pub leechers: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ClientTrendPoint {
    pub timestamp: u64,
    pub tags: Arc<Vec<u8>>,
    pub counts: Vec<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct ClientTrendData {
    pub top_tags: Arc<Vec<u8>>,
    pub top_clients: Arc<Vec<String>>,
    pub history: Arc<Vec<ClientTrendPoint>>,
}

// ── TrendStore ───────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub(crate) struct TrendStore {
    points: Vec<TrendPointResponse>,
    filled_cache: Arc<Vec<TrendPointResponse>>,
    cache_start: u64,
    cache_end: u64,
    // Client trend tracking: store full distribution per snapshot
    client_points: Vec<(u64, Vec<(u8, u32)>)>,
    client_cache: ClientTrendData,
    client_cache_bucket: u64,
    client_cache_dirty: bool,
}

impl StatsResponse {
    pub(crate) fn from_snapshot(snapshot: TrackerSnapshot, uptime_secs: u64, rps: f64) -> Self {
        Self {
            interval: snapshot.interval,
            peer_timeout: snapshot.peer_timeout,
            torrents: snapshot.totals.torrents,
            peers: snapshot.totals.peers,
            seeders: snapshot.totals.seeders,
            leechers: snapshot.totals.leechers,
            completed: snapshot.totals.downloaded,
            rps,
            version: env!("CARGO_PKG_VERSION"),
            uptime_secs,
        }
    }
}

impl TrendStore {
    pub(crate) const RETENTION_SECS: u64 = 7 * 24 * 60 * 60;
    pub(crate) const SAMPLE_SECS: u64 = 10 * 60;

    pub(crate) fn record(
        &mut self,
        now: u64,
        snapshot: &TrackerSnapshot,
    ) -> Arc<Vec<TrendPointResponse>> {
        let bucket = now - (now % Self::SAMPLE_SECS);

        // 已有该 bucket 的点则直接返回缓存，不再覆盖（写入即冻结）
        if self.points.last().is_some_and(|p| p.timestamp == bucket) {
            return self.filled_cache.clone();
        }

        let point = TrendPointResponse {
            timestamp: bucket,
            torrents: snapshot.totals.torrents,
            peers: snapshot.totals.peers,
            seeders: snapshot.totals.seeders,
            leechers: snapshot.totals.leechers,
        };

        self.points.push(point);

        let min_timestamp = bucket.saturating_sub(Self::RETENTION_SECS);
        self.points.retain(|point| point.timestamp >= min_timestamp);

        self.filled_cache = Arc::new(self.filled_points(min_timestamp, bucket));
        self.cache_start = min_timestamp;
        self.cache_end = bucket;
        self.filled_cache.clone()
    }

    fn filled_points(&self, start: u64, end: u64) -> Vec<TrendPointResponse> {
        let mut points = Vec::with_capacity(((end - start) / Self::SAMPLE_SECS + 1) as usize);
        let mut timestamp = start;
        let mut recorded_index = 0;

        while timestamp <= end {
            while self
                .points
                .get(recorded_index)
                .is_some_and(|point| point.timestamp < timestamp)
            {
                recorded_index += 1;
            }

            let point = self
                .points
                .get(recorded_index)
                .filter(|point| point.timestamp == timestamp)
                .cloned()
                .unwrap_or(TrendPointResponse {
                    timestamp,
                    torrents: 0,
                    peers: 0,
                    seeders: 0,
                    leechers: 0,
                });

            points.push(point);
            timestamp = timestamp.saturating_add(Self::SAMPLE_SECS);
        }

        points
    }

    pub(crate) fn record_clients(&mut self, now: u64, clients: &[(u8, u64)]) -> &ClientTrendData {
        let bucket = now - (now % Self::SAMPLE_SECS);

        // Store full distribution
        let dist: Vec<(u8, u32)> = clients.iter().map(|(t, c)| (*t, *c as u32)).collect();

        // 已有该 bucket 的点则直接跳过，不再覆盖（写入即冻结）
        if self
            .client_points
            .last()
            .is_some_and(|(ts, _)| *ts == bucket)
        {
            return &self.client_cache;
        }

        self.client_points.push((bucket, dist));
        self.client_cache_dirty = true;

        // Prune old data
        let min_timestamp = bucket.saturating_sub(Self::RETENTION_SECS);
        let old_len = self.client_points.len();
        self.client_points.retain(|(ts, _)| *ts >= min_timestamp);
        if self.client_points.len() != old_len {
            self.client_cache_dirty = true;
        }

        // Rebuild cache only when data changed
        if self.client_cache_dirty || self.client_cache_bucket != bucket {
            self.client_cache_dirty = false;
            self.client_cache_bucket = bucket;

            // Derive top-N from the LATEST snapshot using index sort (no clone)
            let latest = self
                .client_points
                .last()
                .map(|(_, d)| d.as_slice())
                .unwrap_or(&[]);
            let mut indices: Vec<usize> = (0..latest.len()).collect();
            indices.sort_unstable_by(|&a, &b| latest[b].1.cmp(&latest[a].1));
            let top: Vec<u8> = indices
                .iter()
                .take(TOP_CLIENT_COUNT)
                .map(|&i| latest[i].0)
                .collect();

            // Build filled history
            let top_arc = Arc::new(top);
            let num = top_arc.len();
            let mut history = Vec::with_capacity(
                ((bucket.saturating_sub(min_timestamp)) / Self::SAMPLE_SECS + 1) as usize,
            );
            let mut timestamp = min_timestamp;
            let mut idx = 0;

            while timestamp <= bucket {
                while self
                    .client_points
                    .get(idx)
                    .is_some_and(|(ts, _)| *ts < timestamp)
                {
                    idx += 1;
                }

                let counts = match self.client_points.get(idx) {
                    Some((ts, dist)) if *ts == timestamp => top_arc
                        .iter()
                        .map(|tag| {
                            dist.iter()
                                .find(|(t, _)| t == tag)
                                .map(|(_, c)| *c)
                                .unwrap_or(0)
                        })
                        .collect(),
                    _ => vec![0u32; num],
                };

                history.push(ClientTrendPoint {
                    timestamp,
                    tags: top_arc.clone(),
                    counts,
                });
                timestamp = timestamp.saturating_add(Self::SAMPLE_SECS);
            }

            let top_clients = top_arc
                .iter()
                .map(|t| client_id::client_name(*t).to_string())
                .collect();
            self.client_cache = ClientTrendData {
                top_tags: top_arc,
                top_clients: Arc::new(top_clients),
                history: Arc::new(history),
            };
        }

        &self.client_cache
    }
}

// ── Persistence ──────────────────────────────────────────────────────────────

pub(crate) fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub(crate) fn save_trend_point(
    path: &Path,
    now: u64,
    snapshot: &TrackerSnapshot,
) -> std::io::Result<()> {
    use std::io::Write;
    let bucket = now - (now % TrendStore::SAMPLE_SECS);
    let line = serde_json::json!({
        "timestamp": bucket,
        "torrents": snapshot.totals.torrents,
        "peers": snapshot.totals.peers,
        "seeders": snapshot.totals.seeders,
        "leechers": snapshot.totals.leechers,
    });
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")?;
    f.flush()
}

pub(crate) fn save_client_point(
    path: &Path,
    now: u64,
    clients: &[(u8, u64)],
) -> std::io::Result<()> {
    use std::io::Write;
    let bucket = now - (now % TrendStore::SAMPLE_SECS);
    let line = serde_json::json!({
        "timestamp": bucket,
        "clients": clients,
    });
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")?;
    f.flush()
}

pub(crate) fn load_trends_from_file(
    path: &Path,
    top_clients_path: Option<&PathBuf>,
) -> anyhow::Result<TrendStore> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(TrendStore::default()),
        Err(e) => return Err(e.into()),
    };

    let retention = TrendStore::RETENTION_SECS;
    let min_ts = unix_timestamp().saturating_sub(retention);

    let points: Vec<TrendPointResponse> = content
        .lines()
        .filter_map(|line| match serde_json::from_str(line) {
            Ok(p) => Some(p),
            Err(err) => {
                tracing::debug!("skipping malformed trend line: {err}");
                None
            }
        })
        .filter(|p: &TrendPointResponse| p.timestamp >= min_ts)
        .collect();

    let mut client_points: Vec<(u64, Vec<(u8, u32)>)> = Vec::new();
    if let Some(cp) = top_clients_path {
        if let Ok(cc) = std::fs::read_to_string(cp) {
            client_points = cc
                .lines()
                .filter_map(|line| {
                    let v: serde_json::Value = match serde_json::from_str(line) {
                        Ok(v) => v,
                        Err(err) => {
                            tracing::debug!("skipping malformed client trend line: {err}");
                            return None;
                        }
                    };
                    let ts = v["timestamp"].as_u64()?;
                    let clients = v["clients"].as_array()?;
                    let dist: Vec<(u8, u32)> = clients
                        .iter()
                        .filter_map(|c| {
                            let arr = c.as_array()?;
                            Some((arr[0].as_u64()? as u8, arr[1].as_u64()? as u32))
                        })
                        .collect();
                    Some((ts, dist))
                })
                .filter(|(ts, _)| *ts >= min_ts)
                .collect();
        }
    }

    if points.is_empty() && client_points.is_empty() {
        return Ok(TrendStore::default());
    }

    let mut store = TrendStore {
        points,
        client_points,
        client_cache_dirty: true,
        ..Default::default()
    };

    // Rebuild caches
    if let Some(last_point) = store.points.last() {
        let bucket = last_point.timestamp;
        let min_t = bucket.saturating_sub(retention);
        store.filled_cache = Arc::new(store.filled_points(min_t, bucket));
        store.cache_start = min_t;
        store.cache_end = bucket;
    }

    if let Some(last_client) = store.client_points.last() {
        let bucket = last_client.0;
        let latest = store
            .client_points
            .last()
            .map(|(_, d)| d.as_slice())
            .unwrap_or(&[]);
        let mut indices: Vec<usize> = (0..latest.len()).collect();
        indices.sort_unstable_by(|&a, &b| latest[b].1.cmp(&latest[a].1));
        let top: Vec<u8> = indices
            .iter()
            .take(TOP_CLIENT_COUNT)
            .map(|&i| latest[i].0)
            .collect();

        let min_t = bucket.saturating_sub(retention);
        let top_arc = Arc::new(top);
        let num = top_arc.len();
        let mut history = Vec::with_capacity(
            ((bucket.saturating_sub(min_t)) / TrendStore::SAMPLE_SECS + 1) as usize,
        );
        let mut timestamp = min_t;
        let mut idx = 0;
        while timestamp <= bucket {
            while store
                .client_points
                .get(idx)
                .is_some_and(|(ts, _)| *ts < timestamp)
            {
                idx += 1;
            }
            let counts = match store.client_points.get(idx) {
                Some((ts, dist)) if *ts == timestamp => top_arc
                    .iter()
                    .map(|tag| {
                        dist.iter()
                            .find(|(t, _)| t == tag)
                            .map(|(_, c)| *c)
                            .unwrap_or(0)
                    })
                    .collect(),
                _ => vec![0u32; num],
            };
            history.push(ClientTrendPoint {
                timestamp,
                tags: top_arc.clone(),
                counts,
            });
            timestamp = timestamp.saturating_add(TrendStore::SAMPLE_SECS);
        }
        let top_clients = top_arc
            .iter()
            .map(|t| client_id::client_name(*t).to_string())
            .collect();
        store.client_cache = ClientTrendData {
            top_tags: top_arc,
            top_clients: Arc::new(top_clients),
            history: Arc::new(history),
        };
        store.client_cache_dirty = false;
    }

    tracing::info!(
        points = store.points.len(),
        client_points = store.client_points.len(),
        "trends loaded from disk"
    );
    Ok(store)
}
