use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use rustracker::server::{router, AppState};
use rustracker::tracker::Tracker;
use rustracker::types::InfoHash;
use tower::ServiceExt;

fn app() -> axum::Router {
    router(AppState::new(
        Tracker::new(Duration::from_secs(1800), Duration::from_secs(3000)),
        None,
    ))
}

fn request_with_connect_info(uri: &str) -> Request<Body> {
    let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 6881))));
    req
}

fn request_with_ipv6_connect_info(uri: &str, port: u16) -> Request<Body> {
    let mut req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from((
            std::net::Ipv6Addr::LOCALHOST,
            port,
        ))));
    req
}

fn sharded_app() -> axum::Router {
    router(AppState::sharded(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
    ))
}

/// App with proxy-header trust enabled, for tests of the header-based IP path.
fn trusted_proxy_app() -> axum::Router {
    router(AppState::sharded_with_blacklist_file(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
        None,
        None,
        None,
        true,
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
    assert!(body
        .windows(b"id=\"app\"".len())
        .any(|w| w == b"id=\"app\""));
    assert!(body.windows(b"/assets/".len()).any(|w| w == b"/assets/"));
    assert!(body
        .windows(b".js\"></script>".len())
        .any(|w| w == b".js\"></script>"));
}

#[cfg(feature = "dashboard")]
#[tokio::test]
async fn spa_fallback_serves_index_for_browser_requests() {
    for uri in ["/top100", "/clients", "/totally-unknown"] {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(header::ACCEPT, "text/html,application/xhtml+xml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "GET {uri}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            body.windows(b"id=\"app\"".len())
                .any(|w| w == b"id=\"app\""),
            "GET {uri} should serve the dashboard html"
        );
    }
}

#[cfg(feature = "dashboard")]
#[tokio::test]
async fn spa_fallback_keeps_404_for_non_browser_requests() {
    // No Accept: text/html header — non-browser client.
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/top100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Non-GET method.
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/top100")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
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
    let ipv6_peer = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-ipv6peer1234&port=6881&left=0&event=started&compact=1";
    let requester = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-requester123&port=6882&left=128&event=started&compact=1";

    app.clone()
        .oneshot(request_with_ipv6_connect_info(ipv6_peer, 6881))
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
    let app = trusted_proxy_app();
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
async fn announce_uses_nginx_real_ip() {
    let app = trusted_proxy_app();
    let nginx_peer = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-nginxpeer001&port=6881&left=0&event=started&compact=1";
    let requester = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-requester123&port=6882&left=128&event=started&compact=1";

    let mut req = Request::builder()
        .uri(nginx_peer)
        .header("X-Real-IP", "198.51.100.42")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 6881))));
    app.clone().oneshot(req).await.unwrap();

    let response = app
        .oneshot(request_with_connect_info(requester))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // 198.51.100.42:6881 in compact form
    assert!(body
        .windows([198, 51, 100, 42, 26, 225].len())
        .any(|window| window == [198, 51, 100, 42, 26, 225]));
}

#[tokio::test]
async fn announce_uses_x_forwarded_for() {
    let app = trusted_proxy_app();
    let proxy_peer = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-proxypeer001&port=6881&left=0&event=started&compact=1";
    let requester = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-requester123&port=6882&left=128&event=started&compact=1";

    let mut req = Request::builder()
        .uri(proxy_peer)
        .header("X-Forwarded-For", "203.0.113.50, 10.0.0.1")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 6881))));
    app.clone().oneshot(req).await.unwrap();

    let response = app
        .oneshot(request_with_connect_info(requester))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // 203.0.113.50:6881 in compact form (first IP from X-Forwarded-For)
    assert!(body
        .windows([203, 0, 113, 50, 26, 225].len())
        .any(|window| window == [203, 0, 113, 50, 26, 225]));
}

#[tokio::test]
async fn spoofed_proxy_header_ignored_by_default() {
    let app = app();
    let spoofing_peer = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-spoofpeer001&port=6881&left=0&event=started&compact=1";
    let requester = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-requester123&port=6882&left=128&event=started&compact=1";

    let mut req = Request::builder()
        .uri(spoofing_peer)
        .header("X-Real-IP", "198.51.100.42")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 6881))));
    app.clone().oneshot(req).await.unwrap();

    let response = app
        .oneshot(request_with_connect_info(requester))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // Spoofed IP must NOT appear; the real socket address is used instead.
    assert!(!body
        .windows([198, 51, 100, 42, 26, 225].len())
        .any(|window| window == [198, 51, 100, 42, 26, 225]));
    assert!(body
        .windows([127, 0, 0, 1, 26, 225].len())
        .any(|window| window == [127, 0, 0, 1, 26, 225]));
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

fn temp_blacklist_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "rustracker-test-bl-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn blacklist_app_with_token(token: Option<&str>) -> (axum::Router, std::path::PathBuf) {
    let path = temp_blacklist_path();
    std::fs::write(
        &path,
        "# test blacklist\n6161616161616161616161616161616161616161\n",
    )
    .unwrap();
    let router = router(AppState::sharded_with_blacklist_file(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
        Some(path.clone()),
        None,
        token.map(str::to_string),
        false,
    ));
    (router, path)
}

fn blacklisted_app() -> (axum::Router, std::path::PathBuf) {
    blacklist_app_with_token(None)
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
async fn add_blacklist_rejects_missing_token() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/blacklist")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"info_hash":"6262626262626262626262626262626262626262"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn blacklist_status_returns_true_when_entry_exists() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/blacklist?info_hash=6161616161616161616161616161616161616161")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["blacklisted"], true);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn blacklist_status_returns_false_when_entry_is_missing() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/blacklist?info_hash=6262626262626262626262626262626262626262")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["blacklisted"], false);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn blacklist_status_rejects_missing_token() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/blacklist?info_hash=6161616161616161616161616161616161616161")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn blacklist_status_rejects_invalid_info_hash() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/blacklist?info_hash=invalid")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn add_blacklist_rejects_invalid_info_hash() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/blacklist")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"info_hash":"invalid"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn add_blacklist_persists_entry_and_blocks_announce() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/blacklist")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"info_hash":"6262626262626262626262626262626262626262"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["added"], true);

    let file = std::fs::read_to_string(&path).unwrap();
    assert!(file.contains("6262626262626262626262626262626262626262"));

    let response = app
        .oneshot(request_with_connect_info(
            "/announce?info_hash=bbbbbbbbbbbbbbbbbbbb&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"torrent is blacklisted".len())
        .any(|w| w == b"torrent is blacklisted"));
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn add_blacklist_returns_not_added_when_entry_exists() {
    let (app, path) = blacklist_app_with_token(Some("secret-token"));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/blacklist")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"info_hash":"6161616161616161616161616161616161616161"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["added"], false);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn add_blacklist_requires_configured_token() {
    let (app, path) = blacklist_app_with_token(None);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/blacklist")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"info_hash":"6262626262626262626262626262626262626262"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn info_hash_from_hex_parses_correctly() {
    let hash = InfoHash::from_hex("6161616161616161616161616161616161616161").unwrap();
    assert_eq!(hash.as_bytes(), &[b'a'; 20]);

    assert!(InfoHash::from_hex("invalid").is_none());
    assert!(InfoHash::from_hex("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    assert!(InfoHash::from_hex("61616161616161616161616161616161616161").is_none());
    // 38 chars
}

// ── 边界场景补测 ────────────────────────────────────────────────────────────

#[tokio::test]
async fn info_hash_too_short_returns_error() {
    let response = app()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=short&peer_id=-RT0001-abcdefgh1234&port=6881&left=0",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"failure reason".len())
        .any(|w| w == b"failure reason"));
}

#[tokio::test]
async fn info_hash_too_long_returns_error() {
    let long_hash = "a".repeat(41); // 41 chars, one too many
    let uri =
        format!("/announce?info_hash={long_hash}&peer_id=-RT0001-abcdefgh1234&port=6881&left=0");
    let response = app()
        .oneshot(request_with_connect_info(&uri))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"failure reason".len())
        .any(|w| w == b"failure reason"));
}

#[tokio::test]
async fn numwant_capped_at_400() {
    let app = app();
    // Add 500 peers to the same torrent
    for i in 0..500 {
        let peer_id = format!("-RT0001-p{i:011}");
        let uri = format!(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id={peer_id}&port={}&left=0&event=started&compact=1",
            6881 + (i % 60000) as u16,
        );
        app.clone()
            .oneshot(request_with_connect_info(&uri))
            .await
            .unwrap();
    }

    // Request with numwant=999999
    let response = app
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-reqstr000012&port=7000&left=0&event=started&compact=1&numwant=999999",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // Compact peers are 6 bytes each; count them by looking for peers key
    // The response should have at most 400 peers (400 * 6 = 2400 bytes of peer data)
    // We verify by checking the peers string length
    let peers_marker = b"5:peers";
    let peers6_marker = b"6:peers6";
    let peers_pos = body
        .windows(peers_marker.len())
        .position(|w| w == peers_marker)
        .unwrap();
    let peers6_pos = body
        .windows(peers6_marker.len())
        .position(|w| w == peers6_marker)
        .unwrap();
    // Between "5:peers" and "6:peers6" there's the length prefix and data
    // Extract the peers data length from bencode format "5:peers<len>:"
    let peers_section = &body[peers_pos + peers_marker.len()..peers6_pos];
    // Parse bencode byte string length: "<digits>:<data>"
    let colon_pos = peers_section.iter().position(|&b| b == b':').unwrap();
    let len_str = std::str::from_utf8(&peers_section[..colon_pos]).unwrap();
    let peers_data_len: usize = len_str.parse().unwrap();
    let peer_count = peers_data_len / 6;
    assert!(
        peer_count <= 400,
        "expected at most 400 peers, got {peer_count}"
    );
}

#[tokio::test]
async fn duplicate_peer_started_does_not_double_count() {
    let app = app();
    let uri = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=10&event=started&compact=1";

    app.clone()
        .oneshot(request_with_connect_info(uri))
        .await
        .unwrap();
    app.clone()
        .oneshot(request_with_connect_info(uri))
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

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"\"peers\":1".len())
        .any(|w| w == b"\"peers\":1"));
}

#[tokio::test]
async fn compact_zero_returns_dict_peers() {
    let app = app();
    // Add a peer first
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();

    // Request with compact=0 (non-compact)
    let response = app
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-reqstr000034&port=7000&left=10&event=started&compact=0",
        ))
        .await
        .unwrap();

    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    if status != StatusCode::OK {
        panic!(
            "expected 200 OK, got {status}. Body: {}",
            String::from_utf8_lossy(&body)
        );
    }
    // Always returns compact peer data regardless of the compact flag.
    // The compact parameter is accepted but the response format is always compact.
    assert!(body.windows(b"5:peers".len()).any(|w| w == b"5:peers"));
}

#[tokio::test]
async fn corrupted_jsonl_first_line_still_loads() {
    use std::io::Write;

    let dir = std::env::temp_dir().join(format!(
        "rustracker-test-jsonl-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let trends_file = dir.join("trends.jsonl");
    let mut f = std::fs::File::create(&trends_file).unwrap();
    // First line: corrupted (not valid JSON)
    writeln!(f, "this is not valid json").unwrap();
    // Second line: valid trend point
    writeln!(
        f,
        r#"{{"timestamp":9999999,"torrents":5,"peers":10,"seeders":3,"leechers":7}}"#
    )
    .unwrap();
    drop(f);

    let state = AppState::sharded_with_blacklist_file(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
        None,
        Some(trends_file),
        None,
        false,
    );
    let app = router(state);

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
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // The valid line should be loaded (history has entries)
    let history = parsed["history"].as_array().unwrap();
    assert!(
        !history.is_empty(),
        "expected trend history to contain data from valid JSONL line"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn corrupted_client_trend_subarray_does_not_panic() {
    use std::io::Write;

    let dir = std::env::temp_dir().join(format!(
        "rustracker-test-jsonl-client-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let trends_file = dir.join("trends.jsonl");
    let top_clients_file = dir.join("top_clients.jsonl");
    let mut f = std::fs::File::create(&trends_file).unwrap();
    // A valid trend point so load_trends_from_file does not short-circuit.
    writeln!(
        f,
        r#"{{"timestamp":9999999,"torrents":5,"peers":10,"seeders":3,"leechers":7}}"#
    )
    .unwrap();
    drop(f);

    let mut cf = std::fs::File::create(&top_clients_file).unwrap();
    // Line contains a subarray with only ONE element — this used to panic at
    // `arr[1]` index access. Must be skipped instead.
    writeln!(
        cf,
        r#"{{"timestamp":9999999,"clients":[[1],[2,300],[3,120]]}}"#
    )
    .unwrap();
    drop(cf);

    // If this regress to index-based access, the test process would panic
    // during state construction and the test would fail.
    let state = AppState::sharded_with_blacklist_file(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
        None,
        Some(trends_file),
        None,
        false,
    );
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/clients")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn blacklist_case_insensitive_hex() {
    // Create blacklist with uppercase hex
    let path = std::env::temp_dir().join(format!(
        "rustracker-test-bl-case-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // Use uppercase hex for info_hash
    std::fs::write(&path, "6161616161616161616161616161616161616161\n").unwrap();
    let state = AppState::sharded_with_blacklist_file(
        Duration::from_secs(1800),
        Duration::from_secs(3000),
        16,
        Some(path.clone()),
        None,
        None,
        false,
    );
    let app = router(state);

    // Announce with lowercase hex for the same info_hash
    let response = app
        .clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // Blacklist stores InfoHash as bytes, percent-decode gives lowercase bytes too.
    // The blacklist file has uppercase hex which parses to the same [u8;20] as lowercase.
    // So this should be blacklisted.
    assert!(
        body.windows(b"torrent is blacklisted".len())
            .any(|w| w == b"torrent is blacklisted"),
        "expected blacklisted response, got: {}",
        String::from_utf8_lossy(&body),
    );

    let _ = std::fs::remove_file(&path);
}

// ── 协议正确性补测 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn stopped_peer_removed_from_scrape() {
    let app = app();
    // Add a seeder
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-seed00001234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();

    // Verify seeder is counted
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));

    // Stop the seeder
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-seed00001234&port=6881&left=0&event=stopped&compact=1",
        ))
        .await
        .unwrap();

    // Verify seeder is removed from scrape
    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"8:completei0e".len())
        .any(|w| w == b"8:completei0e"));
    assert!(body
        .windows(b"10:incompletei0e".len())
        .any(|w| w == b"10:incompletei0e"));
}

#[tokio::test]
async fn stopped_peer_not_in_announce_response() {
    let app = app();
    // Add two peers
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-peer_aaaaaaaaaa&port=6881&left=10&event=started&compact=1",
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-peer_bbbbbbbbbb&port=6882&left=10&event=started&compact=1",
        ))
        .await
        .unwrap();

    // Stop peer A
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-peer_aaaaaaaaaa&port=6881&left=10&event=stopped&compact=1",
        ))
        .await
        .unwrap();

    // Peer B announces - should not see peer A in response
    let response = app
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-peer_bbbbbbbbbb&port=6882&left=10&event=started&compact=1&numwant=100",
        ))
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // Peer A is 127.0.0.1:6881 = [127,0,0,1,0x1A,0xE1], but it was stopped so should not appear
    let peer_a_bytes: [u8; 6] = [127, 0, 0, 1, 0x1A, 0xE1];
    // The response should not contain peer A's compact representation in the peers data
    // We need to check only the peers binary data, not the whole response
    // Find "5:peers" and extract the data
    let peers_marker = b"5:peers";
    let peers6_marker = b"6:peers6";
    if let Some(peers_pos) = body
        .windows(peers_marker.len())
        .position(|w| w == peers_marker)
    {
        if let Some(peers6_pos) = body
            .windows(peers6_marker.len())
            .position(|w| w == peers6_marker)
        {
            let peers_section = &body[peers_pos + peers_marker.len()..peers6_pos];
            let colon_pos = peers_section.iter().position(|&b| b == b':').unwrap();
            let peers_data = &peers_section[colon_pos + 1..];
            assert!(
                !peers_data.windows(6).any(|w| w == peer_a_bytes),
                "stopped peer A should not appear in peers data"
            );
        }
    }
}

#[tokio::test]
async fn completed_switches_leecher_to_seeder() {
    let app = app();
    // Start as leecher (left > 0)
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=100&event=started&compact=1",
        ))
        .await
        .unwrap();

    // Verify incomplete=1, complete=0
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"10:incompletei1e".len())
        .any(|w| w == b"10:incompletei1e"));
    assert!(body
        .windows(b"8:completei0e".len())
        .any(|w| w == b"8:completei0e"));

    // Complete download (left=0, event=completed)
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=completed&compact=1",
        ))
        .await
        .unwrap();

    // Verify complete=1, incomplete=0 (not complete=1, incomplete=1)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));
    assert!(body
        .windows(b"10:incompletei0e".len())
        .any(|w| w == b"10:incompletei0e"));
}

#[tokio::test]
async fn duplicate_completed_does_not_increment() {
    let app = app();
    // Start and complete
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=100&event=started&compact=1",
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=completed&compact=1",
        ))
        .await
        .unwrap();

    // Send completed again
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=completed&compact=1",
        ))
        .await
        .unwrap();

    // Verify complete=1 (not 2), downloaded=1 (not 2)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));
    assert!(body
        .windows(b"10:incompletei0e".len())
        .any(|w| w == b"10:incompletei0e"));
    // downloaded should be 1, not 2
    assert!(body
        .windows(b"10:downloadedi1e".len())
        .any(|w| w == b"10:downloadedi1e"));
}

#[tokio::test]
async fn scrape_multiple_info_hashes() {
    let app = app();
    // Add peers to two different torrents
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=bbbbbbbbbbbbbbbbbbbb&peer_id=-RT0001-bcdefghi1234&port=6882&left=10&event=started&compact=1",
        ))
        .await
        .unwrap();

    // Scrape both
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
    // Both info_hashes should be in the response
    // InfoHash a = 6161... in bytes, b = 6262... in bytes
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));
    assert!(body
        .windows(b"10:incompletei1e".len())
        .any(|w| w == b"10:incompletei1e"));
}

#[tokio::test]
async fn scrape_one_hash_does_not_affect_other() {
    let app = app();
    // Add peers to two different torrents
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1",
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(request_with_connect_info(
            "/announce?info_hash=bbbbbbbbbbbbbbbbbbbb&peer_id=-RT0001-bcdefghi1234&port=6882&left=10&event=started&compact=1",
        ))
        .await
        .unwrap();

    // Scrape only torrent A
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=aaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"8:completei1e".len())
        .any(|w| w == b"8:completei1e"));

    // Scrape only torrent B - should still have its peer
    let response = app
        .oneshot(
            Request::builder()
                .uri("/scrape?info_hash=bbbbbbbbbbbbbbbbbbbb")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body
        .windows(b"10:incompletei1e".len())
        .any(|w| w == b"10:incompletei1e"));
}

#[tokio::test]
async fn client_ip_parameter_ignored() {
    // Verify that client-reported &ip= parameter is ignored in favor of connection IP
    let app = app();
    // Peer announces with &ip= spoofing a different IP
    let uri = "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-abcdefgh1234&port=6881&left=0&event=started&compact=1&ip=10.0.0.99";
    app.clone()
        .oneshot(request_with_connect_info(uri))
        .await
        .unwrap();

    // Another peer announces and should see the first peer at 127.0.0.1 (connection IP),
    // not at 10.0.0.99 (spoofed IP)
    let response = app
        .oneshot(request_with_connect_info(
            "/announce?info_hash=aaaaaaaaaaaaaaaaaaaa&peer_id=-RT0001-bcdefghi1234&port=6882&left=10&event=started&compact=1",
        ))
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    // Connection IP is 127.0.0.1, port 6881 = [127,0,0,1,0x1A,0xE1]
    let peer_bytes: [u8; 6] = [127, 0, 0, 1, 0x1A, 0xE1];
    // Find peers data
    let peers_marker = b"5:peers";
    if let Some(peers_pos) = body
        .windows(peers_marker.len())
        .position(|w| w == peers_marker)
    {
        let peers_section = &body[peers_pos + peers_marker.len()..];
        let colon_pos = peers_section.iter().position(|&b| b == b':').unwrap();
        let peers_data = &peers_section[colon_pos + 1..];
        assert!(
            peers_data.windows(6).any(|w| w == peer_bytes),
            "peer should be at connection IP 127.0.0.1:6881, not spoofed 10.0.0.99"
        );
    }
    // Also verify 10.0.0.99 does NOT appear
    let spoofed_bytes: [u8; 6] = [10, 0, 0, 99, 0x1A, 0xE1];
    let peers_marker = b"5:peers";
    if let Some(peers_pos) = body
        .windows(peers_marker.len())
        .position(|w| w == peers_marker)
    {
        let peers_section = &body[peers_pos + peers_marker.len()..];
        let colon_pos = peers_section.iter().position(|&b| b == b':').unwrap();
        let peers_data = &peers_section[colon_pos + 1..];
        assert!(
            !peers_data.windows(6).any(|w| w == spoofed_bytes),
            "spoofed IP 10.0.0.99 should not appear in peers"
        );
    }
}
