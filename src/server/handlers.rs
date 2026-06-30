//! HTTP request handlers for the tracker.
//!
//! Includes BitTorrent announce/scrape endpoints, web UI serving,
//! and JSON stats endpoints.

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{ConnectInfo, OriginalUri, State};
use axum::http::{header, HeaderMap, HeaderValue, Response, StatusCode};
#[cfg(feature = "dashboard")]
use axum::response::Html;
use axum::Json;
use serde::{Deserialize, Serialize};

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
include!(concat!(env!("OUT_DIR"), "/assets_manifest.rs"));

#[cfg(feature = "dashboard")]
pub(crate) fn make_versioned_index() -> axum::body::Bytes {
    axum::body::Bytes::from(INDEX_HTML)
}

// ── Route handlers ───────────────────────────────────────────────────────────

#[cfg(feature = "dashboard")]
pub(crate) async fn index(State(state): State<AppState>) -> Html<axum::body::Bytes> {
    Html(state.versioned_index.clone())
}

#[cfg(feature = "dashboard")]
pub(crate) async fn asset(
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response<Body> {
    let bytes = ASSETS.iter().find(|(n, _)| *n == name).map(|(_, b)| *b);
    match bytes {
        Some(data) => {
            let ct = content_type_for(&name);
            match Response::builder()
                .header(header::CONTENT_TYPE, ct)
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .body(Body::from(data))
            {
                Ok(resp) => resp,
                Err(_) => Response::new(Body::empty()),
            }
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap_or_else(|_| Response::new(Body::empty())),
    }
}

#[cfg(feature = "dashboard")]
fn content_type_for(name: &str) -> &'static str {
    if name.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if name.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if name.ends_with(".woff2") {
        "font/woff2"
    } else if name.ends_with(".png") {
        "image/png"
    } else if name.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

pub(crate) async fn stats(State(state): State<AppState>) -> Json<StatsResponse> {
    let snapshot = state.tracker.snapshot().await;
    let uptime_secs = state.started_at.elapsed().as_secs();
    let rps = f64::from_bits(state.current_rps.load(Ordering::Relaxed));
    Json(StatsResponse::from_snapshot(snapshot, uptime_secs, rps))
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

pub(crate) async fn not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "404 Not Found")
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
    state.rps_counter.fetch_add(1, Ordering::Relaxed);
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
    state.rps_counter.fetch_add(1, Ordering::Relaxed);
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
