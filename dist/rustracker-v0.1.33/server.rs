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

#[derive(Clone)]
pub struct AppState {
    tracker: Arc<TrackerPool>,
    trends: Arc<RwLock<TrendStore>>,
}

struct TrackerPool {
    shards: Vec<RwLock<Tracker>>,
}

impl AppState {
    pub fn new(tracker: Tracker) -> Self {
        Self {
            tracker: Arc::new(TrackerPool::single(tracker)),
            trends: Arc::new(RwLock::new(TrendStore::default())),
        }
    }

    pub fn sharded(interval: Duration, peer_timeout: Duration, shards: usize) -> Self {
        let state = Self {
            tracker: Arc::new(TrackerPool::new(interval, peer_timeout, shards)),
            trends: Arc::new(RwLock::new(TrendStore::default())),
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
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/stats", get(stats))
        .route("/announce", get(announce))
        .route("/scrape", get(scrape))
        .route("/healthz", get(healthz))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
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

const INDEX_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>rustracker</title>
    <link rel="preconnect" href="https://fonts.googleapis.com">
    <link href="https://fonts.googleapis.com/css2?family=Sora:wght@700&display=swap" rel="stylesheet">
    <style>
        :root {
            color-scheme: light;
            --ink: #1f2937;
            --muted: #64748b;
            --line: #d8dee8;
            --panel: #ffffff;
            --soft: #f4f7fb;
            --blue: #2563eb;
            --blue-dark: #1d4ed8;
            --green: #15803d;
            --amber: #b45309;
            --violet: #6d28d9;
            --red: #b91c1c;
        }

        * { box-sizing: border-box; }

        body {
            margin: 0;
            font-family: Inter, "Segoe UI", Arial, sans-serif;
            color: var(--ink);
            background: var(--soft);
            letter-spacing: 0;
        }

        button, input { font: inherit; }

        .app {
            min-height: 100vh;
            display: grid;
            grid-template-columns: 248px minmax(0, 1fr);
        }

        .side {
            background: #172033;
            color: #f8fafc;
            padding: 24px 20px;
        }

        .brand {
            display: flex;
            align-items: center;
            gap: 10px;
            font-size: 20px;
            font-weight: 700;
            margin-bottom: 28px;
        }

        .mark {
            width: 28px;
            height: 28px;
            background: var(--blue);
            display: grid;
            place-items: center;
            color: #fff;
            font-size: 15px;
            font-weight: 800;
            font-family: 'Sora', sans-serif;
        }

        .nav-label {
            color: #9ca3af;
            font-size: 12px;
            text-transform: uppercase;
            margin-bottom: 10px;
        }

        .nav-item {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 10px 12px;
            background: #22304a;
            border-left: 4px solid var(--blue);
            color: #fff;
            font-size: 14px;
        }

        .nav-link {
            margin-top: 10px;
            text-decoration: none;
        }

        .side-note {
            margin-top: 24px;
            color: #cbd5e1;
            font-size: 13px;
            line-height: 1.6;
        }

        .main {
            min-width: 0;
            padding: 28px;
        }

        .topbar {
            display: flex;
            justify-content: space-between;
            align-items: flex-start;
            gap: 20px;
            margin-bottom: 24px;
        }

        h1 {
            margin: 0 0 6px;
            font-size: 28px;
            line-height: 1.2;
        }

        .subtle {
            margin: 0;
            color: var(--muted);
            font-size: 14px;
            line-height: 1.5;
        }

        .actions {
            display: flex;
            gap: 10px;
            align-items: center;
        }

        .button {
            border: 0;
            background: var(--blue);
            color: #fff;
            padding: 10px 14px;
            min-height: 40px;
            cursor: pointer;
        }

        .button:hover { background: var(--blue-dark); }

        .button.secondary {
            background: #e2e8f0;
            color: var(--ink);
        }

        .button.secondary:hover { background: #cbd5e1; }

        .metrics {
            display: grid;
            grid-template-columns: repeat(4, minmax(150px, 1fr));
            gap: 12px;
            margin-bottom: 20px;
        }

        .metric {
            background: var(--panel);
            border: 1px solid var(--line);
            padding: 16px;
            min-height: 104px;
        }

        .metric span {
            display: block;
            color: var(--muted);
            font-size: 12px;
            text-transform: uppercase;
            margin-bottom: 10px;
        }

        .metric strong {
            display: block;
            font-size: 30px;
            line-height: 1;
        }

        .metric.peers { border-top: 4px solid #475569; }
    .metric.seeders { border-top: 4px solid var(--green); }
    .metric.leechers { border-top: 4px solid var(--amber); }
    .metric.completed { border-top: 4px solid var(--violet); }

        .chart-panel {
            background: var(--panel);
            border: 1px solid var(--line);
            padding: 16px;
            margin-bottom: 20px;
        }

        .disclaimer {
            background: var(--panel);
            border: 1px solid var(--line);
            padding: 18px;
            margin-bottom: 20px;
        }

        .disclaimer h2 {
            margin: 0 0 12px;
            font-size: 16px;
            line-height: 1.4;
        }

        .disclaimer p {
            margin: 0 0 10px;
            color: var(--muted);
            line-height: 1.7;
        }

        .disclaimer-contact {
            margin-top: 16px;
            text-align: center;
            font-size: 18px;
            font-weight: 700;
            line-height: 1.8;
        }

        .disclaimer-contact a {
            color: var(--blue);
            text-decoration: none;
        }

        .chart-head {
            display: flex;
            align-items: baseline;
            justify-content: space-between;
            gap: 16px;
            margin-bottom: 12px;
        }

        .chart-title {
            margin: 0;
            font-size: 16px;
            line-height: 1.4;
        }

        .chart-note {
            color: var(--muted);
            font-size: 12px;
        }

        .range-group {
            display: flex;
            gap: 0;
            flex-shrink: 0;
        }

        .range-btn {
            border: 1px solid var(--line);
            background: var(--panel);
            color: var(--muted);
            padding: 4px 12px;
            font-size: 12px;
            cursor: pointer;
            min-height: 28px;
            transition: background 0.15s, color 0.15s;
        }

        .range-btn:first-child { border-radius: 4px 0 0 4px; }
        .range-btn:last-child { border-radius: 0 4px 4px 0; }
        .range-btn + .range-btn { border-left: 0; }
        .range-btn:hover { background: #e8ecf2; }
        .range-btn.active {
            background: var(--blue);
            border-color: var(--blue);
            color: #fff;
        }
        .range-btn.active:hover { background: var(--blue-dark); }

        .lang-bar { margin-bottom: 20px; display: flex; align-items: center; gap: 8px; }
        .lang-icon { font-size: 13px; font-weight: 700; color: #9ca3af; flex-shrink: 0; letter-spacing: -0.5px; }
        .lang-sel {
            width: 100%;
            background: #22304a;
            color: #f8fafc;
            border: 1px solid #334155;
            padding: 6px 10px;
            font-size: 13px;
            cursor: pointer;
        }

        #trendChart {
            width: 100%;
            height: 320px;
        }

        .status-line {
            color: var(--muted);
            font-size: 12px;
            margin-top: 6px;
        }

        .error { color: var(--red); }

        @media (max-width: 900px) {
            .app { grid-template-columns: 1fr; }
            .side { padding: 18px; }
            .main { padding: 18px; }
            .topbar, .chart-head { flex-direction: column; align-items: stretch; }
            .metrics { grid-template-columns: repeat(2, minmax(130px, 1fr)); }
        }

        @media (max-width: 560px) {
            .metrics { grid-template-columns: 1fr; }
            h1 { font-size: 24px; }
        }
    </style>
    <script defer src="https://u.7471.top/script.js" data-website-id="dabdcda9-0b8c-4cc6-8d16-d99ba68462cb"></script>
</head>
<body>
    <div class="app">
        <aside class="side">
            <div class="brand"><span class="mark">R</span><span>rustracker</span></div>
            <div class="lang-bar">
                <span class="lang-icon">文A</span>
                <select id="langSelect" class="lang-sel">
                    <option value="zh">中文</option>
                    <option value="en">English</option>
                </select>
            </div>
            <div class="nav-label" data-i18n="monitoring">监控</div>
            <div class="nav-item"><span data-i18n="overview">Tracker 概览</span><span id="navState" data-i18n="running">运行中</span></div>
            <a class="nav-item nav-link" href="#disclaimer"><span data-i18n="disc_link">Tracker 免责说明</span><span data-i18n="view">查看</span></a>
            <p class="side-note" data-i18n="side_note">HTTP tracker 的连接、做种、下载和完成统计。</p>
        </aside>

        <main class="main">
            <section class="topbar">
                <div>
                    <h1 data-i18n="title">Tracker 控制台</h1>
                    <p class="subtle" data-i18n="subtitle">查看当前端口上的 peer、做种和下载状态。</p>
                    <div class="status-line" id="statusText" data-i18n="loading">正在加载...</div>
                    <div class="status-line" id="configText">上报间隔：- | 超时：-</div>
                </div>
                <div class="actions">
                    <button class="button secondary" id="pauseBtn" type="button" data-i18n="pause">暂停刷新</button>
                    <button class="button" id="refreshBtn" type="button" data-i18n="refresh">刷新</button>
                </div>
            </section>

            <section class="metrics" aria-label="Tracker 指标">
                <div class="metric peers"><span>Peers</span><strong id="metricPeers">0</strong></div>
                <div class="metric seeders"><span>Seeders</span><strong id="metricSeeders">0</strong></div>
                <div class="metric leechers"><span>Leechers</span><strong id="metricLeechers">0</strong></div>
                <div class="metric completed"><span>Completed</span><strong id="metricCompleted">0</strong></div>
            </section>

            <section class="chart-panel" aria-label="Tracker 趋势图">
                <div class="chart-head">
                    <h2 class="chart-title" data-i18n="chart_title">Tracker 趋势</h2>
                    <div style="display:flex;align-items:center;gap:12px;flex-wrap:wrap;">
                        <span class="chart-note" data-i18n="chart_note">Torrents、Peers、Seeders 和 Leechers 随时间变化</span>
                        <div class="range-group" id="rangeGroup">
                            <button class="range-btn active" data-range="24h" type="button" data-i18n="range_24h">24小时</button>
                            <button class="range-btn" data-range="3d" type="button" data-i18n="range_3d">3天</button>
                            <button class="range-btn" data-range="7d" type="button" data-i18n="range_7d">7天</button>
                        </div>
                    </div>
                </div>
                <div id="trendChart"></div>
            </section>

            <section class="disclaimer" id="disclaimer" aria-label="Tracker 免责说明">
                <h2 data-i18n="disc_title">Tracker 免责说明</h2>
                <p data-i18n="disc_p1">本站 Tracker 仅提供连接协调、状态记录与统计展示，不存储、不托管、不分发任何实际资源内容。</p>
                <p data-i18n="disc_p2">页面中的 torrents、peers、seeders、leechers、客户端类型及趋势数据，来源于客户端上报与系统采样，可能存在延迟、缺失、偏差或伪造，不代表资源真实状态。</p>
                <p data-i18n="disc_p3">本页面信息不代表任何资源的真实性、完整性、可用性、安全性或合法性，也不构成任何服务承诺或结果保证。</p>
                <p data-i18n="disc_p4">对于第三方客户端行为、资源内容、传输结果及由此产生的任何直接或间接后果，本站不承担责任，使用者应自行判断并承担相关风险。</p>
                <p data-i18n="disc_p5">受 Tracker 工作机制限制，本站不保留可用于长期识别、追踪或还原单个连接历史的完整日志，也无法对既往连接行为提供持续、完整或可验证的回溯记录。</p>
                <div class="disclaimer-contact">
                    <div><span data-i18n="blog_label">Blog</span>：<a href="https://blog.7471.top/" rel="noopener noreferrer" target="_blank">https://blog.7471.top/</a></div>
                    <div><span data-i18n="contact_label">如有问题请联系</span>：<a href="mailto:tracker@mail.7471.top">tracker@mail.7471.top</a></div>
                </div>
            </section>
        </main>
    </div>

    <script src="https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js"></script>
    <script>
        const state = { data: null, paused: false, range: "24h", lang: "zh" };
        const $ = (id) => document.getElementById(id);
        const chart = echarts.init($("trendChart"), null, { renderer: "canvas" });

        const T = {
            zh: {
                monitoring: "监控", overview: "Tracker 概览", running: "运行中",
                error: "异常", paused_state: "已暂停", disc_link: "Tracker 免责说明",
                view: "查看", side_note: "HTTP tracker 的连接、做种、下载和完成统计。",
                title: "Tracker 控制台", subtitle: "查看当前端口上的 peer、做种和下载状态。",
                loading: "正在加载...", last_update: "最后更新", read_error: "读取失败",
                refresh_paused: "自动刷新已暂停", refresh_resumed: "自动刷新已恢复",
                pause: "暂停刷新", resume: "继续刷新", refresh: "刷新",
                chart_title: "Tracker 趋势",
                chart_note: "Torrents、Peers、Seeders 和 Leechers 随时间变化",
                range_24h: "24小时", range_3d: "3天", range_7d: "7天",
                config_fmt: (i, t) => `上报间隔：${i} 秒 | 超时：${t} 秒`,
                disc_title: "Tracker 免责说明",
                disc_p1: "本站 Tracker 仅提供连接协调、状态记录与统计展示，不存储、不托管、不分发任何实际资源内容。",
                disc_p2: "页面中的 torrents、peers、seeders、leechers、客户端类型及趋势数据，来源于客户端上报与系统采样，可能存在延迟、缺失、偏差或伪造，不代表资源真实状态。",
                disc_p3: "本页面信息不代表任何资源的真实性、完整性、可用性、安全性或合法性，也不构成任何服务承诺或结果保证。",
                disc_p4: "对于第三方客户端行为、资源内容、传输结果及由此产生的任何直接或间接后果，本站不承担责任，使用者应自行判断并承担相关风险。",
                disc_p5: "受 Tracker 工作机制限制，本站不保留可用于长期识别、追踪或还原单个连接历史的完整日志，也无法对既往连接行为提供持续、完整或可验证的回溯记录。",
                blog_label: "Blog", contact_label: "如有问题请联系",
            },
            en: {
                monitoring: "Monitoring", overview: "Tracker Overview", running: "Running",
                error: "Error", paused_state: "Paused", disc_link: "Disclaimer",
                view: "View", side_note: "HTTP tracker connection, seeding, downloading and completion statistics.",
                title: "Tracker Console", subtitle: "View peer, seeding and download status on the current port.",
                loading: "Loading...", last_update: "Last updated", read_error: "Read failed",
                refresh_paused: "Auto refresh paused", refresh_resumed: "Auto refresh resumed",
                pause: "Pause", resume: "Resume", refresh: "Refresh",
                chart_title: "Tracker Trends",
                chart_note: "Torrents, Peers, Seeders and Leechers over time",
                range_24h: "24h", range_3d: "3D", range_7d: "7D",
                config_fmt: (i, t) => `Interval: ${i}s | Timeout: ${t}s`,
                disc_title: "Disclaimer",
                disc_p1: "This tracker only provides connection coordination, status recording and statistical display. It does not store, host or distribute any actual resource content.",
                disc_p2: "Torrents, peers, seeders, leechers, client types and trend data displayed on this page are derived from client reports and system sampling. They may contain delays, omissions, deviations or falsification, and do not represent the true state of resources.",
                disc_p3: "The information on this page does not represent the authenticity, completeness, availability, security or legality of any resource, nor does it constitute any service commitment or result guarantee.",
                disc_p4: "This site assumes no responsibility for third-party client behavior, resource content, transmission results, or any direct or indirect consequences arising therefrom. Users should exercise their own judgment and bear associated risks.",
                disc_p5: "Due to tracker operational limitations, this site does not retain complete logs that could be used for long-term identification, tracking or reconstruction of individual connection histories, and cannot provide continuous, complete or verifiable retrospective records of past connection activity.",
                blog_label: "Blog", contact_label: "Contact",
            }
        };

        function t(key) { return (T[state.lang] || T.zh)[key] ?? T.zh[key] ?? key; }
        function tf(key, ...a) { const f = (T[state.lang] || T.zh)[key]; return typeof f === "function" ? f(...a) : key; }

        function setLang(lang) {
            state.lang = lang;
            document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
            document.querySelectorAll("[data-i18n]").forEach((el) => {
                const key = el.getAttribute("data-i18n");
                const val = (T[lang] || T.zh)[key];
                if (typeof val === "string") el.textContent = val;
            });
            $("pauseBtn").textContent = state.paused ? t("resume") : t("pause");
            if (state.data) render();
        }

        function number(value) {
            return new Intl.NumberFormat(state.lang === "zh" ? "zh-CN" : "en-US").format(value || 0);
        }

        function setStatus(text, error = false) {
            $("statusText").textContent = text;
            $("statusText").className = error ? "status-line error" : "status-line";
            $("navState").textContent = error ? t("error") : state.paused ? t("paused_state") : t("running");
        }

        async function loadStats() {
            try {
                const response = await fetch("/api/stats", { cache: "no-store" });
                if (!response.ok) throw new Error(`HTTP ${response.status}`);
                state.data = await response.json();
                render();
                setStatus(`${t("last_update")} ${new Date().toLocaleTimeString(state.lang === "zh" ? "zh-CN" : "en-US")}`);
            } catch (error) {
                setStatus(`${t("read_error")}: ${error.message}`, true);
            }
        }

        function render() {
            const data = state.data || {};
            $("metricPeers").textContent = number(data.peers);
            $("metricSeeders").textContent = number(data.seeders);
            $("metricLeechers").textContent = number(data.leechers);
            $("metricCompleted").textContent = number(data.completed);
            $("configText").textContent = tf("config_fmt", data.interval || "-", data.peer_timeout || "-");
            renderChart();
        }

        function filterHistory() {
            const history = state.data?.history || [];
            if (!history.length) return history;
            const ranges = { "24h": 86400, "3d": 259200, "7d": 604800 };
            const secs = ranges[state.range] || 86400;
            const cutoff = Math.floor(Date.now() / 1000) - secs;
            return history.filter((item) => item.timestamp >= cutoff);
        }

        function renderChart() {
            const history = filterHistory();
            const labels = history.map((item) => new Date(item.timestamp * 1000).toLocaleString("zh-CN", {
                month: "2-digit",
                day: "2-digit",
                hour: "2-digit",
                minute: "2-digit",
                hour12: false
            }));
            chart.setOption({
                color: ["#2563eb", "#475569", "#15803d", "#b45309"],
                tooltip: { trigger: "axis" },
                legend: {
                    top: 0,
                    right: 0,
                    data: ["Torrents", "Peers", "Seeders", "Leechers"]
                },
                grid: { left: 44, right: 20, top: 52, bottom: 36, containLabel: true },
                xAxis: {
                    type: "category",
                    boundaryGap: false,
                    data: labels,
                    axisLine: { lineStyle: { color: "#d8dee8" } },
                    axisLabel: { color: "#64748b" }
                },
                yAxis: {
                    type: "value",
                    minInterval: 1,
                    axisLabel: { color: "#64748b" },
                    splitLine: { lineStyle: { color: "#e6ebf2" } }
                },
                series: [
                    { name: "Torrents", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.torrents) },
                    { name: "Peers", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.peers) },
                    { name: "Seeders", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.seeders) },
                    { name: "Leechers", type: "line", smooth: true, showSymbol: false, data: history.map((item) => item.leechers) }
                ]
            });
        }

        $("rangeGroup").addEventListener("click", (e) => {
            const btn = e.target.closest(".range-btn");
            if (!btn || btn.classList.contains("active")) return;
            $("rangeGroup").querySelectorAll(".range-btn").forEach((b) => b.classList.remove("active"));
            btn.classList.add("active");
            state.range = btn.dataset.range;
            renderChart();
        });

        $("langSelect").addEventListener("change", (e) => setLang(e.target.value));

        $("refreshBtn").addEventListener("click", loadStats);
        $("pauseBtn").addEventListener("click", () => {
            state.paused = !state.paused;
            $("pauseBtn").textContent = state.paused ? t("resume") : t("pause");
            setStatus(state.paused ? t("refresh_paused") : t("refresh_resumed"));
            if (!state.paused) loadStats();
        });
        loadStats();
        window.addEventListener("resize", () => chart.resize());
        setInterval(() => {
            if (!state.paused) loadStats();
        }, 5000);
    </script>
</body>
</html>"##;
