use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use std::time::UNIX_EPOCH;

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
use crate::client_id;
use crate::protocol::{
    announce_response, parse_announce_query, parse_scrape_query, peer_ip, scrape_response,
};
use crate::tracker::{AnnounceInput, Tracker, TrackerSnapshot};
use crate::types::InfoHash;

pub const DEFAULT_TRACKER_SHARDS: usize = 64;
const EXPIRE_MAINTENANCE_INTERVAL: Duration = Duration::from_secs(1);
const TREND_SAMPLE_INTERVAL: Duration = Duration::from_secs(10 * 60);
const BLACKLIST_WATCH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct AppState {
    tracker: Arc<TrackerPool>,
    trends: Arc<RwLock<TrendStore>>,
    blacklist: Arc<RwLock<Arc<HashSet<InfoHash>>>>,
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
            blacklist: Arc::new(RwLock::new(Arc::new(HashSet::new()))),
            versioned_index: make_versioned_index(),
        }
    }

    pub fn sharded(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        Self::sharded_with_blacklist_file(interval, peer_timeout, shards, None)
    }

    pub fn sharded_with_blacklist_file(
        interval: Duration,
        peer_timeout: Duration,
        shards: usize,
        blacklist_path: Option<PathBuf>,
    ) -> Self {
        let initial = blacklist_path
            .as_deref()
            .and_then(|path| match load_blacklist(path) {
                Ok(set) => Some(set),
                Err(err) => {
                    tracing::warn!("{err}");
                    None
                }
            })
            .unwrap_or_default();

        let state = Self {
            tracker: Arc::new(TrackerPool::new(interval, peer_timeout, shards)),
            trends: Arc::new(RwLock::new(TrendStore::default())),
            blacklist: Arc::new(RwLock::new(Arc::new(initial))),
            versioned_index: make_versioned_index(),
        };

        state.spawn_maintenance();
        if let Some(path) = blacklist_path {
            state.spawn_blacklist_watcher(path);
        }
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
                match load_blacklist(&path) {
                    Ok(new_set) => {
                        let count = new_set.len();
                        *blacklist.write().await = Arc::new(new_set);
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
    Router::new()
        .route("/", get(index))
        .route("/style.css", get(style))
        .route("/app.js", get(app_js))
        .route("/api/stats", get(stats))
        .route("/api/clients", get(clients))
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

async fn clients(State(state): State<AppState>) -> Json<ClientsResponse> {
    let snapshot = state.tracker.snapshot().await;
    let now = unix_timestamp();
    let mut trends = state.trends.write().await;
    let client_data = trends.record_clients(now, &snapshot.clients);
    Json(ClientsResponse {
        timestamp: now,
        tags: client_data.top_tags.clone(),
        clients: client_data.top_clients.clone(),
        history: client_data.history.clone(),
    })
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

    if state.blacklist.read().await.contains(&parsed.info_hash) {
        return bencoded_response(
            StatusCode::OK,
            bencode::failure("torrent is blacklisted"),
        );
    }

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
        client_tag: client_id::identify(parsed.peer_id.as_bytes()),
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

    let bl = state.blacklist.read().await;
    let allowed: Vec<InfoHash> = parsed
        .info_hashes
        .iter()
        .copied()
        .filter(|h| !bl.contains(h))
        .collect();
    drop(bl);
    let stats = state.tracker.scrape(&allowed).await;
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
            clients: Vec::new(),
        });

        combined.totals = Default::default();
        let mut client_map: HashMap<u8, u64> = HashMap::new();
        for snapshot in snapshots {
            combined.totals.torrents += snapshot.totals.torrents;
            combined.totals.peers += snapshot.totals.peers;
            combined.totals.seeders += snapshot.totals.seeders;
            combined.totals.leechers += snapshot.totals.leechers;
            combined.totals.downloaded = combined
                .totals
                .downloaded
                .saturating_add(snapshot.totals.downloaded);
            for (tag, count) in snapshot.clients {
                *client_map.entry(tag).or_insert(0) += count;
            }
        }
        combined.clients = client_map.into_iter().collect();

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
struct ClientsResponse {
    timestamp: u64,
    clients: Vec<String>,
    tags: Vec<u8>,
    history: Vec<ClientTrendPoint>,
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

#[derive(Clone, Debug, Serialize)]
struct ClientTrendPoint {
    timestamp: u64,
    tags: Vec<u8>,
    counts: Vec<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct ClientTrendData {
    top_tags: Vec<u8>,
    top_clients: Vec<String>,
    history: Vec<ClientTrendPoint>,
}

const TOP_CLIENT_COUNT: usize = 10;

#[derive(Debug, Default)]
struct TrendStore {
    points: Vec<TrendPointResponse>,
    filled_cache: Vec<TrendPointResponse>,
    cache_start: u64,
    cache_end: u64,
    // Client trend tracking: store full distribution per snapshot
    client_points: Vec<(u64, Vec<(u8, u32)>)>,
    client_cache: ClientTrendData,
    client_cache_bucket: u64,
    client_cache_dirty: bool,
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

    fn record_clients(&mut self, now: u64, clients: &[(u8, u64)]) -> &ClientTrendData {
        let bucket = now - (now % Self::SAMPLE_SECS);

        // Store full distribution
        let dist: Vec<(u8, u32)> = clients.iter().map(|(t, c)| (*t, *c as u32)).collect();

        match self.client_points.last_mut() {
            Some((ts, _)) if *ts == bucket => {
                *self.client_points.last_mut().unwrap() = (bucket, dist);
                self.client_cache_dirty = true;
            }
            _ => {
                self.client_points.push((bucket, dist));
                self.client_cache_dirty = true;
            }
        }

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

            // Derive top 10 from the LATEST snapshot using index sort (no clone)
            let latest = self.client_points.last().map(|(_, d)| d.as_slice()).unwrap_or(&[]);
            let mut indices: Vec<usize> = (0..latest.len()).collect();
            indices.sort_unstable_by(|&a, &b| latest[b].1.cmp(&latest[a].1));
            let top: Vec<u8> = indices
                .iter()
                .take(TOP_CLIENT_COUNT)
                .map(|&i| latest[i].0)
                .collect();

            // Build filled history
            let num = top.len();
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
                    Some((ts, dist)) if *ts == timestamp => top
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

                history.push(ClientTrendPoint { timestamp, tags: top.clone(), counts });
                timestamp = timestamp.saturating_add(Self::SAMPLE_SECS);
            }

            self.client_cache = ClientTrendData {
                top_tags: top.clone(),
                top_clients: top.iter().map(|t| client_id::client_name(*t).to_string()).collect(),
                history,
            };
        }

        &self.client_cache
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

fn file_mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Parse a blacklist file. Each non-empty, non-comment line is a 40-char hex info_hash.
pub fn load_blacklist(path: &Path) -> anyhow::Result<HashSet<InfoHash>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    let mut set = HashSet::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match InfoHash::from_hex(line) {
            Some(hash) => { set.insert(hash); }
            None => {
                tracing::warn!(
                    "{}:{}: invalid info_hash \"{}\", skipped",
                    path.display(),
                    line_no + 1,
                    line
                );
            }
        }
    }
    Ok(set)
}
