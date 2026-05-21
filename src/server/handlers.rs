//! HTTP request handlers for the tracker.
//!
//! Includes BitTorrent announce/scrape endpoints, web UI serving,
//! and JSON stats endpoints.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{ConnectInfo, OriginalUri, State};
use axum::http::{header, HeaderMap, HeaderValue, Response, StatusCode};
#[cfg(feature = "dashboard")]
use axum::response::Html;
use axum::Json;
use serde::Deserialize;
use serde::Serialize;

use super::trends::{self, ClientsResponse, StatsResponse, TrendsResponse};
use super::AppState;
use crate::core::tracker::AnnounceInput;
use crate::core::types::InfoHash;
use crate::protocol::announce::{
    announce_response, parse_announce_query, parse_scrape_query, peer_ip, scrape_response,
};
use crate::protocol::bencode;
use crate::protocol::client_id;

// ── Web UI ───────────────────────────────────────────────────────────────────

#[cfg(feature = "dashboard")]
pub(crate) const INDEX_HTML: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));
#[cfg(feature = "dashboard")]
pub(crate) const STYLE_CSS: &str = include_str!("../../assets/style.css");
#[cfg(feature = "dashboard")]
pub(crate) const APP_JS: &str = include_str!("../../assets/app.js");

#[cfg(feature = "dashboard")]
pub(crate) fn make_versioned_index() -> String {
    let hash = fnv1a_hash(STYLE_CSS.as_bytes(), APP_JS.as_bytes());
    let v = format!("{hash:08x}");
    INDEX_HTML
        .replace("/style.css", &format!("/style.css?v={v}"))
        .replace("/app.js", &format!("/app.js?v={v}"))
}

/// FNV-1a hash over two byte slices, computed once at startup.
#[cfg(feature = "dashboard")]
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

// ── Route handlers ───────────────────────────────────────────────────────────

#[cfg(feature = "dashboard")]
pub(crate) async fn index(State(state): State<AppState>) -> Html<String> {
    Html(state.versioned_index.clone())
}

#[cfg(feature = "dashboard")]
pub(crate) async fn style() -> Response<Body> {
    match Response::builder()
        .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(STYLE_CSS))
    {
        Ok(resp) => resp,
        Err(_) => Response::new(Body::empty()),
    }
}

#[cfg(feature = "dashboard")]
pub(crate) async fn app_js() -> Response<Body> {
    match Response::builder()
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(APP_JS))
    {
        Ok(resp) => resp,
        Err(_) => Response::new(Body::empty()),
    }
}

pub(crate) async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let snapshot = state.tracker.snapshot().await;
    let uptime_secs = state.started_at.elapsed().as_secs();
    Json(StatsResponse::from_snapshot(snapshot, uptime_secs))
}

pub(crate) async fn trends(State(state): State<AppState>) -> Json<TrendsResponse> {
    let snapshot = state.tracker.snapshot().await;
    let now = trends::unix_timestamp();
    let history = state.trends.write().await.record(now, &snapshot);
    Json(TrendsResponse { history })
}

pub(crate) async fn clients(State(state): State<AppState>) -> Json<ClientsResponse> {
    let snapshot = state.tracker.snapshot().await;
    let now = trends::unix_timestamp();
    let mut store = state.trends.write().await;
    let client_data = store.record_clients(now, &snapshot.clients);
    Json(ClientsResponse {
        timestamp: now,
        tags: client_data.top_tags.clone(),
        clients: client_data.top_clients.clone(),
        history: client_data.history.clone(),
    })
}

pub(crate) async fn healthz() -> &'static str {
    "ok"
}

// ── Admin API ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct AddBlacklistRequest {
    pub info_hash: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AddBlacklistResponse {
    pub ok: bool,
    pub added: bool,
    pub info_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BlacklistStatusQuery {
    pub info_hash: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct BlacklistStatusResponse {
    pub ok: bool,
    pub blacklisted: bool,
    pub info_hash: Option<String>,
    pub error: Option<String>,
}

pub(crate) async fn blacklist_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<BlacklistStatusQuery>,
) -> (StatusCode, Json<BlacklistStatusResponse>) {
    let Some(token) = state
        .admin_token
        .as_deref()
        .filter(|token| !token.is_empty())
    else {
        return blacklist_status_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "admin token is not configured",
        );
    };
    if !authorized(&headers, token) {
        return blacklist_status_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    let Some(info_hash) = InfoHash::from_hex(query.info_hash.trim()) else {
        return blacklist_status_error(
            StatusCode::BAD_REQUEST,
            "info_hash must be a 40-char hex string",
        );
    };

    blacklist_status_success(info_hash, state.blacklist.read().await.contains(&info_hash))
}

pub(crate) async fn add_blacklist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddBlacklistRequest>,
) -> (StatusCode, Json<AddBlacklistResponse>) {
    let Some(token) = state
        .admin_token
        .as_deref()
        .filter(|token| !token.is_empty())
    else {
        return blacklist_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "admin token is not configured",
        );
    };
    if !authorized(&headers, token) {
        return blacklist_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    let Some(path) = state.blacklist_path.as_deref() else {
        return blacklist_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "blacklist file is not configured",
        );
    };
    let Some(info_hash) = InfoHash::from_hex(request.info_hash.trim()) else {
        return blacklist_error(
            StatusCode::BAD_REQUEST,
            "info_hash must be a 40-char hex string",
        );
    };

    if state.blacklist.read().await.contains(&info_hash) {
        return blacklist_success(info_hash, false);
    }

    if let Err(error) = append_blacklist_entry(path, info_hash).await {
        tracing::warn!(%error, path = %path.display(), "failed to persist blacklist entry");
        return blacklist_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to persist blacklist entry",
        );
    }

    let mut blacklist = state.blacklist.write().await;
    let mut next = (**blacklist).clone();
    next.insert(info_hash);
    *blacklist = Arc::new(next);

    blacklist_success(info_hash, true)
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let Some(header) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    header == format!("Bearer {token}")
}

async fn append_blacklist_entry(path: &Path, info_hash: InfoHash) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(format!("{info_hash}\n").as_bytes()).await
}

fn blacklist_success(info_hash: InfoHash, added: bool) -> (StatusCode, Json<AddBlacklistResponse>) {
    (
        StatusCode::OK,
        Json(AddBlacklistResponse {
            ok: true,
            added,
            info_hash: Some(info_hash.to_string()),
            error: None,
        }),
    )
}

fn blacklist_error(
    status: StatusCode,
    error: &'static str,
) -> (StatusCode, Json<AddBlacklistResponse>) {
    (
        status,
        Json(AddBlacklistResponse {
            ok: false,
            added: false,
            info_hash: None,
            error: Some(error.to_string()),
        }),
    )
}

fn blacklist_status_success(
    info_hash: InfoHash,
    blacklisted: bool,
) -> (StatusCode, Json<BlacklistStatusResponse>) {
    (
        StatusCode::OK,
        Json(BlacklistStatusResponse {
            ok: true,
            blacklisted,
            info_hash: Some(info_hash.to_string()),
            error: None,
        }),
    )
}

fn blacklist_status_error(
    status: StatusCode,
    error: &'static str,
) -> (StatusCode, Json<BlacklistStatusResponse>) {
    (
        status,
        Json(BlacklistStatusResponse {
            ok: false,
            blacklisted: false,
            info_hash: None,
            error: Some(error.to_string()),
        }),
    )
}

// ── top100 ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct Top100LimitQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Top100Entry {
    pub info_hash: String,
    pub seeders: usize,
    pub leechers: usize,
    pub peers: usize,
    pub downloaded: u64,
}

#[derive(Debug, Serialize)]
pub(crate) struct Top100Response {
    pub peers: Vec<Top100Entry>,
    pub seeders: Vec<Top100Entry>,
    pub leechers: Vec<Top100Entry>,
    pub downloaded: Vec<Top100Entry>,
}

pub(crate) async fn top100(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<Top100LimitQuery>,
) -> Json<Top100Response> {
    let limit = query.limit.unwrap_or(100).min(500);
    let all = state.tracker.top_torrents_all(limit).await;
    Json(Top100Response {
        peers: top100_map(all.peers),
        seeders: top100_map(all.seeders),
        leechers: top100_map(all.leechers),
        downloaded: top100_map(all.downloaded),
    })
}

fn top100_map(entries: Vec<(InfoHash, usize, usize, u64)>) -> Vec<Top100Entry> {
    entries
        .into_iter()
        .map(|(info_hash, seeders, leechers, downloaded)| Top100Entry {
            info_hash: format!("{info_hash}"),
            seeders,
            leechers,
            peers: seeders + leechers,
            downloaded,
        })
        .collect()
}

// ── BitTorrent announce / scrape ─────────────────────────────────────────────

pub(crate) async fn announce(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response<Body> {
    let query = uri.query().unwrap_or_default();
    let parsed = match parse_announce_query(query) {
        Ok(parsed) => parsed,
        Err(error) => {
            return bencoded_response(StatusCode::BAD_REQUEST, bencode::failure(error.to_string()))
        }
    };

    if state.blacklist.read().await.contains(&parsed.info_hash) {
        return bencoded_response(StatusCode::OK, bencode::failure("torrent is blacklisted"));
    }

    let input = AnnounceInput {
        info_hash: parsed.info_hash,
        peer_id: parsed.peer_id,
        ip: peer_ip(cloudflare_connecting_ip(&headers), Some(addr)),
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

pub(crate) async fn scrape(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
) -> Response<Body> {
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

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn bencoded_response(status: StatusCode, body: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=ISO-8859-1"),
    );
    response
}

pub(crate) fn cloudflare_connecting_ip(headers: &HeaderMap) -> Option<std::net::IpAddr> {
    // CF-Connecting-IP: Cloudflare
    // X-Real-IP: nginx
    // X-Forwarded-For: generic proxy (take first IP)
    headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            headers
                .get("x-forwarded-for")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(',').next())
                .and_then(|first| first.trim().parse().ok())
        })
}
