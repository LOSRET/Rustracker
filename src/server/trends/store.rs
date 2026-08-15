//! TrendStore: 10-minute sampling, 7-day retention, cached history.

use std::sync::Arc;

use crate::core::tracker::TrackerSnapshot;
use crate::protocol::client_id;

use super::dto::{ClientTrendData, ClientTrendPoint, TrendPointResponse};

pub(crate) const TOP_CLIENT_COUNT: usize = 15;

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
            self.rebuild_client_cache(bucket);
        }

        &self.client_cache
    }

    /// Build a `TrendStore` from points loaded off disk, rebuilding caches.
    pub(crate) fn from_loaded(
        points: Vec<TrendPointResponse>,
        client_points: Vec<(u64, Vec<(u8, u32)>)>,
    ) -> Self {
        let mut store = Self {
            points,
            client_points,
            client_cache_dirty: true,
            ..Default::default()
        };

        if let Some(last_point) = store.points.last() {
            let bucket = last_point.timestamp;
            let min_timestamp = bucket.saturating_sub(Self::RETENTION_SECS);
            store.filled_cache = Arc::new(store.filled_points(min_timestamp, bucket));
            store.cache_start = min_timestamp;
            store.cache_end = bucket;
        }

        if let Some(last_client) = store.client_points.last() {
            store.rebuild_client_cache(last_client.0);
        }

        store
    }

    /// Rebuild the client top-N cache from the stored distribution points.
    fn rebuild_client_cache(&mut self, bucket: u64) {
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
        let min_timestamp = bucket.saturating_sub(Self::RETENTION_SECS);
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
}
