use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::{ConnectInfo, OriginalUri, State};
use axum::http::{header, HeaderMap, HeaderValue, Response, StatusCode};
use axum::response::Html;
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Serialize;
use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::bencode;
use crate::protocol::{
    announce_response, parse_announce_query, parse_scrape_query, peer_ip, scrape_response,
};
use crate::tracker::{AnnounceInput, Tracker, TrackerSnapshot};

pub const DEFAULT_TRACKER_SHARDS: usize = 64;
const EXPIRE_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(1);
const TREND_SAMPLE_INTERVAL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone)]
pub struct AppState {
    tracker: Arc<TrackerPool>,
    trends: Arc<RwLock<TrendStore>>,
    versioned_index: String,
}

struct TrackerPool {
    shards: Vec<RwLock<Tracker>>,
}

impl AppState {
    pub fn new(tracker: Tracker) -> Self {
        Self {
            tracker: Arc::new(TrackerPool::single(tracker)),
            trends: Arc::new(RwLock::new(TrendStore::default())),
            versioned_index: make_versioned_index(),
        }
    }

    pub fn sharded(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        let state = Self {
            tracker: Arc::new(TrackerPool::new(interval, peer_timeout, shards)),
            trends: Arc::new(RwLock::new(TrendStore::default())),
            versioned_index: make_versioned_index(),
        };

        state.spawn_maintenance();
        state
    }

    fn spawn_maintenance(&self) {
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
                let now = unix_timestamp();
                trends.write().await.record(now, &snapshot);
            }
        });
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/style.css", get(style))
        .route("/app.js", get(app_js))
        .route("/api/stats", get(stats))
        .route("/announce", get(announce))
        .route("/scrape", get(scrape))
        .route("/healthz", get(healthz))
        .with_state(state)
}

async fn index(State(state): State<AppState>) -> Html<String> {
    Html(state.versioned_index.clone())
}

async fn style() -> Response<Body> {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(STYLE_CSS))
        .unwrap()
}

async fn app_js() -> Response<Body> {
    Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(APP_JS))
        .unwrap()
}

async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let snapshot = state.tracker.snapshot().await;
    let now = unix_timestamp();
    let history = state.trends.write().await.record(now, &snapshot);
    Json(StatsResponse::from_snapshot(snapshot, history))
}

async fn healthz() -> &'static str {
    "ok"
}

async fn announce(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
) -> Response<Body> {
    let query = uri.query().unwrap_or_default();
    let parsed = match parse_announce_query(query) {
        Ok(parsed) => parsed,
        Err(error) => {
            return bencoded_response(StatusCode::BAD_REQUEST, bencode::failure(error.to_string()))
        }
    };

    let input = AnnounceInput {
        info_hash: parsed.info_hash,
        peer_id: parsed.peer_id,
        ip: peer_ip(
            cloudflare_connecting_ip(&headers).or(parsed.ip),
            connect_info.map(|ConnectInfo(addr)| addr),
        ),
        port: parsed.port,
        uploaded: parsed.uploaded,
        downloaded: parsed.downloaded,
        left: parsed.left,
        event: parsed.event,
        numwant: parsed.numwant,
    };

    let output = state
        .tracker
        .announce(parsed.info_hash, input, Instant::now())
        .await;
    bencoded_response(StatusCode::OK, announce_response(output, parsed.compact))
}

fn cloudflare_connecting_ip(headers: &HeaderMap) -> Option<std::net::IpAddr> {
    headers
        .get("cf-connecting-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

async fn scrape(State(state): State<AppState>, OriginalUri(uri): OriginalUri) -> Response<Body> {
    let query = uri.query().unwrap_or_default();
    let parsed = match parse_scrape_query(query) {
        Ok(parsed) => parsed,
        Err(error) => {
            return bencoded_response(StatusCode::BAD_REQUEST, bencode::failure(error.to_string()))
        }
    };

    let stats = state.tracker.scrape(&parsed.info_hashes).await;
    bencoded_response(StatusCode::OK, scrape_response(stats))
}

fn bencoded_response(status: StatusCode, body: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=ISO-8859-1"),
    );
    response
}

impl TrackerPool {
    fn single(tracker: Tracker) -> Self {
        Self {
            shards: vec![RwLock::new(tracker)],
        }
    }

    fn new(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        let shard_count = shards.max(1);
        let shards = (0..shard_count)
            .map(|_| RwLock::new(Tracker::new(interval, peer_timeout)))
            .collect();

        Self { shards }
    }

    async fn announce(
        &self,
        info_hash: crate::types::InfoHash,
        input: AnnounceInput,
        now: Instant,
    ) -> crate::tracker::AnnounceOutput {
        self.shard(info_hash).write().await.announce(input, now)
    }

    async fn scrape(
        &self,
        info_hashes: &[crate::types::InfoHash],
    ) -> HashMap<crate::types::InfoHash, crate::types::TorrentStats> {
        let mut stats = HashMap::with_capacity(info_hashes.len());
        let mut by_shard = HashMap::<usize, Vec<crate::types::InfoHash>>::new();

        for &info_hash in info_hashes {
            by_shard
                .entry(self.shard_index(info_hash))
                .or_default()
                .push(info_hash);
        }

        for (shard_index, shard_info_hashes) in by_shard {
            let shard_stats = self.shards[shard_index]
                .read()
                .await
                .scrape(&shard_info_hashes);
            stats.extend(shard_stats);
        }

        stats
    }

    async fn snapshot(&self) -> TrackerSnapshot {
        let mut snapshots = Vec::with_capacity(self.shards.len());

        for shard in &self.shards {
            snapshots.push(shard.read().await.snapshot());
        }

        let mut combined = snapshots.first().cloned().unwrap_or(TrackerSnapshot {
            interval: 0,
            peer_timeout: 0,
            totals: Default::default(),
        });

        combined.totals = Default::default();
        for snapshot in snapshots {
            combined.totals.torrents += snapshot.totals.torrents;
            combined.totals.peers += snapshot.totals.peers;
            combined.totals.seeders += snapshot.totals.seeders;
            combined.totals.leechers += snapshot.totals.leechers;
            combined.totals.downloaded = combined
                .totals
                .downloaded
                .saturating_add(snapshot.totals.downloaded);
        }

        combined
    }

    fn shard(&self, info_hash: crate::types::InfoHash) -> &RwLock<Tracker> {
        &self.shards[self.shard_index(info_hash)]
    }

    fn shard_index(&self, info_hash: crate::types::InfoHash) -> usize {
        let mut hasher = DefaultHasher::new();
        info_hash.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }

    fn expire_due(&self, now: Instant) {
        for shard in &self.shards {
            if let Ok(mut tracker) = shard.try_write() {
                tracker.expire_due(now);
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct StatsResponse {
    interval: u64,
    peer_timeout: u64,
    torrents: usize,
    peers: usize,
    seeders: usize,
    leechers: usize,
    completed: u64,
    history: Vec<TrendPointResponse>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct TrendPointResponse {
    timestamp: u64,
    torrents: usize,
    peers: usize,
    seeders: usize,
    leechers: usize,
}

#[derive(Debug, Default)]
struct TrendStore {
    points: Vec<TrendPointResponse>,
    filled_cache: Vec<TrendPointResponse>,
    cache_start: u64,
    cache_end: u64,
}

impl StatsResponse {
    fn from_snapshot(snapshot: TrackerSnapshot, history: Vec<TrendPointResponse>) -> Self {
        Self {
            interval: snapshot.interval,
            peer_timeout: snapshot.peer_timeout,
            torrents: snapshot.totals.torrents,
            peers: snapshot.totals.peers,
            seeders: snapshot.totals.seeders,
            leechers: snapshot.totals.leechers,
            completed: snapshot.totals.downloaded,
            history,
        }
    }
}

impl TrendStore {
    const RETENTION_SECS: u64 = 7 * 24 * 60 * 60;
    const SAMPLE_SECS: u64 = 10 * 60;

    fn record(&mut self, now: u64, snapshot: &TrackerSnapshot) -> Vec<TrendPointResponse> {
        let bucket = now - (now % Self::SAMPLE_SECS);
        let point = TrendPointResponse {
            timestamp: bucket,
            torrents: snapshot.totals.torrents,
            peers: snapshot.totals.peers,
            seeders: snapshot.totals.seeders,
            leechers: snapshot.totals.leechers,
        };

        let mut changed = false;

        match self.points.last_mut() {
            Some(last) if last.timestamp == bucket => {
                if *last != point {
                    *last = point;
                    changed = true;
                }
            }
            _ => {
                self.points.push(point);
                changed = true;
            }
        }

        let min_timestamp = bucket.saturating_sub(Self::RETENTION_SECS);
        let old_len = self.points.len();
        self.points.retain(|point| point.timestamp >= min_timestamp);
        changed |= self.points.len() != old_len;

        if changed || self.cache_start != min_timestamp || self.cache_end != bucket {
            self.filled_cache = self.filled_points(min_timestamp, bucket);
            self.cache_start = min_timestamp;
            self.cache_end = bucket;
        }

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
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

const INDEX_HTML: &str = include_str!("../assets/index.html");
const STYLE_CSS: &str = include_str!("../assets/style.css");
const APP_JS: &str = include_str!("../assets/app.js");

fn make_versioned_index() -> String {
    let hash = fnv1a_hash(STYLE_CSS.as_bytes(), APP_JS.as_bytes());
    let v = format!("{hash:08x}");
    INDEX_HTML
        .replace("/style.css", &format!("/style.css?v={v}"))
        .replace("/app.js", &format!("/app.js?v={v}"))
}

/// FNV-1a hash over two byte slices, computed once at startup.
fn fnv1a_hash(a: &[u8], b: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &byte in a {
        h ^= byte as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    for &byte in b {
        h ^= byte as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}
