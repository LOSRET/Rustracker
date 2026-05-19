use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use rustracker::server::{router, AppState};
use rustracker::tracker::Tracker;
use rustracker::types::InfoHash;
use tower::ServiceExt;

fn app() -> axum::Router {
    router(AppState::new(Tracker::new(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
    ), None))
}

fn request_with_connect_info(uri: &str) -> Request<Body> {
    let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 6881))));
    req
}

fn sharded_app() -> axum::Router {
    router(AppState::sharded(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
    ))
}

#[tokio::test]
async fn healthz_returns_ok() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[cfg(feature = "dashboard")]
#[tokio::test]
async fn index_returns_dashboard_html() {
    let response = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.windows(b"Tracker".len()).any(|w| w == b"Tracker"));
    assert!(body.windows(b"/app.js".len()).any(|w| w == b"/app.js"));
    assert!(body.windows(b"echarts".len()).any(|w| w == b"echarts"));
    assert!(body
        .windows(b"trendChart".len())
        .any(|w| w == b"trendChart"));
    assert!(body
        .windows("Tracker 免责说明".as_bytes().len())
        .any(|w| w == "Tracker 免责说明".as_bytes()));
    assert!(!body.windows(b"peerTable".len()).any(|w| w == b"peerTable"));
    assert!(!body
        .windows(b"torrentTable".len())
        .any(|w| w == b"torrentTable"));
}

#[tokio::test]
async fn stats_api_returns_json_totals() {
    let app = app();
    let seeder_announce_uri = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1";
    let leecher_announce_uri = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-bcdefghi1234&port=6882&left=128&event=started&compact=1";
    let completed_announce_uri = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-bcdefghi1234&port=6882&left=0&event=completed&compact=1";

    app.clone()
        .oneshot(request_with_connect_info(seeder_announce_uri))
        .await
        .unwrap();
    app.clone()
        .oneshot(request_with_connect_info(leecher_announce_uri))
        .await
        .unwrap();
    app.clone()
        .oneshot(request_with_connect_info(completed_announce_uri))
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"\"torrents\":1".len())
        .any(|w| w == b"\"torrents\":1"));
    assert!(body
        .windows(b"\"peers\":2".len())
        .any(|w| w == b"\"peers\":2"));
    assert!(body
        .windows(b"\"seeders\":2".len())
        .any(|w| w == b"\"seeders\":2"));
    assert!(body
        .windows(b"\"leechers\":0".len())
        .any(|w| w == b"\"leechers\":0"));
    assert!(body
        .windows(b"\"completed\":1".len())
        .any(|w| w == b"\"completed\":1"));
    assert!(!body
        .windows(b"\"history\"".len())
        .any(|w| w == b"\"history\""));
    assert!(body
        .windows(b"\"peer_timeout\"".len())
        .any(|w| w == b"\"peer_timeout\""));
}

#[tokio::test]
async fn trends_api_returns_history() {
    let app = app();
    let seeder_announce_uri = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1";

    app.clone()
        .oneshot(request_with_connect_info(seeder_announce_uri))
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/trends")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"\"history\"".len())
        .any(|w| w == b"\"history\""));
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["history"].as_array().unwrap().len(), 1009);
    assert!(body
        .windows(b"\"timestamp\"".len())
        .any(|w| w == b"\"timestamp\""));
}

#[tokio::test]
async fn handles_concurrent_announces_across_shards() {
    let app = sharded_app();
    let mut tasks = Vec::new();

    for index in 0..64_u8 {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            let info_hash = char::from(b'a' + (index % 26)).to_string().repeat(20);
            let peer_id = format!("-RT0001-concurrent{index:02}");
            let uri = format!(
                "/announce?info_hash={info_hash}&peer_id={peer_id}&port={}&left=0&event=started&compact=1",
                6881 + u16::from(index),
            );

            app.oneshot(request_with_connect_info(&uri))
                .await
                .unwrap()
                .status()
        }));
    }

    for task in tasks {
        assert_eq!(task.await.unwrap(), StatusCode::OK);
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"\"peers\":64".len())
        .any(|window| window == b"\"peers\":64"));
}

#[tokio::test]
async fn announce_then_scrape_reports_peer() {
    let app = app();
    let announce_uri = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1";

    let response = app
        .clone()
        .oneshot(request_with_connect_info(announce_uri))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.starts_with(b"d"));
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));
}

#[tokio::test]
async fn compact_announce_includes_ipv6_peers6() {
    let app = app();
    let ipv6_peer = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-ipv6peer1234&port=6881&left=0&event=started&compact=1&ip=::1";
    let requester = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-requester123&port=6882&left=128&event=started&compact=1&ip=127.0.0.1";

    app.clone()
        .oneshot(request_with_connect_info(ipv6_peer))
        .await
        .unwrap();

    let response = app
        .oneshot(request_with_connect_info(requester))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"6:peers618:".len())
        .any(|window| window == b"6:peers618:"));
}

#[tokio::test]
async fn announce_uses_cloudflare_connecting_ip() {
    let app = app();
    let cloudflare_peer = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-cloudflare01&port=6881&left=0&event=started&compact=1";
    let requester = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-requester123&port=6882&left=128&event=started&compact=1&ip=127.0.0.1";

    let mut cf_req = Request::builder()
        .uri(cloudflare_peer)
        .header("CF-Connecting-IP", "203.0.113.7")
        .body(Body::empty())
        .unwrap();
    cf_req
        .extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 6881))));
    app.clone().oneshot(cf_req).await.unwrap();

    let response = app
        .oneshot(request_with_connect_info(requester))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows([203, 0, 113, 7, 26, 225].len())
        .any(|window| window == [203, 0, 113, 7, 26, 225]));
}

#[tokio::test]
async fn invalid_announce_returns_bencoded_failure() {
    let response = app()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=short&peer_id=short&port=6881",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"failure reason".len())
        .any(|w| w == b"failure reason"));
}

fn blacklisted_app() -> (axum::Router, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "rustracker-test-bl-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, "# test blacklist\n6161616161616161616161616161616161616161\n").unwrap();
    let router = router(AppState::sharded_with_blacklist_file(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
        Some(path.clone()),
        None,
    ));
    (router, path)
}

#[tokio::test]
async fn blacklisted_announce_returns_403() {
    let (app, _tmp) = blacklisted_app();
    let response = app
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"torrent is blacklisted".len())
        .any(|w| w == b"torrent is blacklisted"));
}

#[tokio::test]
async fn non_blacklisted_announce_works() {
    let (app, _tmp) = blacklisted_app();
    let response = app
        .oneshot(request_with_connect_info(
            "/announce?info_hash=bbbbbbbbbbbbbbbbbbbb&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn scrape_excludes_blacklisted_torrents() {
    let (app, _tmp) = blacklisted_app();

    // Seed a non-blacklisted torrent first
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=bbbbbbbbbbbbbbbbbbbb&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();

    // Scrape both blacklisted and non-blacklisted
    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa&info_hash=bbbbbbbbbbbbbbbbbbbb")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // Non-blacklisted torrent should have stats
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));
}

#[tokio::test]
async fn info_hash_from_hex_parses_correctly() {
    let hash = InfoHash::from_hex("6161616161616161616161616161616161616161").unwrap();
    assert_eq!(hash.as_bytes(), &[b'a'; 20]);

    assert!(InfoHash::from_hex("invalid").is_none());
    assert!(InfoHash::from_hex("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    assert!(InfoHash::from_hex("61616161616161616161616161616161616161").is_none()); // 38 chars
}
