//! JSONL persistence for trend data.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::core::tracker::TrackerSnapshot;

use super::dto::TrendPointResponse;
use super::store::TrendStore;

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
                            let tag = arr.first().and_then(|v| v.as_u64()).map(|v| v as u8)?;
                            let count = arr.get(1).and_then(|v| v.as_u64()).map(|v| v as u32)?;
                            Some((tag, count))
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

    let point_count = points.len();
    let client_count = client_points.len();
    let store = TrendStore::from_loaded(points, client_points);

    tracing::info!(
        points = point_count,
        client_points = client_count,
        "trends loaded from disk"
    );
    Ok(store)
}
