//! HTTP response DTOs for the trend/stats endpoints.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::core::tracker::TrackerSnapshot;

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
