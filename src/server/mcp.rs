//! MCP endpoint for the torrent tracker: blacklist management and scrape.
//!
//! Mounted at `/mcp` using the Streamable HTTP transport in stateless mode.
//! The endpoint reuses the admin bearer token. The blacklist tools share the
//! same `BlacklistAdmin` service as the HTTP API, so validation, persistence,
//! and auditing cannot drift between the two surfaces; the scrape tool
//! mirrors the HTTP `/scrape` endpoint, blacklist filtering included.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::connect_info::ConnectInfo;
use axum::http::request::Parts;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{schemars, tool, tool_handler, tool_router, Json, RoleServer, ServerHandler};
use serde::{Deserialize, Serialize};

use super::admin::BlacklistAdmin;
use super::audit::AuditContext;
use super::blacklist::{BlacklistStore, DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT};
use super::handlers::cloudflare_connecting_ip;
use super::pool::TrackerPool;
use super::AppState;
use crate::core::types::InfoHash;
use crate::protocol::MAX_SCRAPE_HASHES;

/// The slice of [`AppState`] the MCP tools actually need. Holding the whole
/// app state would drag the trend store into every tool handler for no
/// reason.
#[derive(Clone)]
struct McpState {
    tracker: Arc<TrackerPool>,
    blacklist: Arc<BlacklistStore>,
    admin: Arc<BlacklistAdmin>,
    trust_proxy_headers: bool,
}

#[derive(Clone)]
pub(crate) struct TrackerMcp {
    state: McpState,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct HashArgs {
    /// A 40-character hexadecimal BitTorrent info_hash.
    pub info_hash: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ListArgs {
    /// Number of entries to skip. Defaults to 0.
    #[serde(default)]
    pub offset: usize,
    /// Maximum entries to return. Defaults to 100, capped at 1000.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub(crate) struct ScrapeArgs {
    /// BitTorrent info_hashes as 40-character hexadecimal strings.
    /// At most 200; duplicates collapse.
    pub info_hashes: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct CheckOutput {
    pub blacklisted: bool,
    pub info_hash: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct AddOutput {
    pub added: bool,
    pub info_hash: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct ListOutput {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub entries: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct ScrapeEntry {
    pub info_hash: String,
    /// Seeders (peers that have the complete torrent).
    pub complete: u64,
    /// Leechers (peers still downloading).
    pub incomplete: u64,
    /// Times the torrent has been reported fully downloaded.
    pub downloaded: u64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct ScrapeOutput {
    pub entries: Vec<ScrapeEntry>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct RemoveOutput {
    pub removed: bool,
    pub info_hash: String,
}

#[tool_router]
impl TrackerMcp {
    pub(crate) fn new(state: &AppState) -> Self {
        Self {
            state: McpState {
                tracker: state.tracker.clone(),
                blacklist: state.blacklist.clone(),
                admin: state.admin.clone(),
                trust_proxy_headers: state.trust_proxy_headers,
            },
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "blacklist_check",
        description = "Check whether a 40-character hex BitTorrent info_hash is currently blacklisted. Read-only.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn blacklist_check(
        &self,
        Parameters(args): Parameters<HashArgs>,
    ) -> Result<Json<CheckOutput>, String> {
        let info_hash = parse_info_hash(&args.info_hash)?;
        let blacklisted = self.state.blacklist.contains(&info_hash).await;
        Ok(Json(CheckOutput {
            blacklisted,
            info_hash: info_hash.to_string(),
        }))
    }

    #[tool(
        name = "blacklist_add",
        description = "Append an info_hash to the tracker blacklist. Takes effect immediately: announces for this torrent fail and scrape omits it. Idempotent; re-adding an existing entry returns added=false.",
        annotations(
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn blacklist_add(
        &self,
        Parameters(args): Parameters<HashArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<AddOutput>, String> {
        self.ensure_blacklist_configured()?;
        let info_hash = parse_info_hash(&args.info_hash)?;
        let audit_context = audit_context(&self.state, &context);
        match self.state.admin.add(info_hash, &audit_context).await {
            Ok(added) => Ok(Json(AddOutput {
                added,
                info_hash: info_hash.to_string(),
            })),
            Err(error) => {
                tracing::warn!(%error, "failed to persist blacklist entry via MCP");
                Err(format!("failed to persist blacklist entry: {error}"))
            }
        }
    }

    #[tool(
        name = "blacklist_list",
        description = "List blacklisted info_hashes in lexicographic order. Use offset/limit for pagination; limit defaults to 100 and is capped at 1000.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn blacklist_list(
        &self,
        Parameters(args): Parameters<ListArgs>,
    ) -> Result<Json<ListOutput>, String> {
        let offset = args.offset;
        let limit = args.limit.unwrap_or(DEFAULT_LIST_LIMIT).min(MAX_LIST_LIMIT);
        let (total, hashes) = self.state.blacklist.page(offset, limit).await;
        let entries = hashes.into_iter().map(|hash| hash.to_string()).collect();
        Ok(Json(ListOutput {
            total,
            offset,
            limit,
            entries,
        }))
    }

    #[tool(
        name = "blacklist_remove",
        description = "Remove an info_hash from the tracker blacklist. The file is rewritten atomically. Idempotent; removing a missing entry returns removed=false.",
        annotations(
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn blacklist_remove(
        &self,
        Parameters(args): Parameters<HashArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<Json<RemoveOutput>, String> {
        self.ensure_blacklist_configured()?;
        let info_hash = parse_info_hash(&args.info_hash)?;
        let audit_context = audit_context(&self.state, &context);
        match self.state.admin.remove(info_hash, &audit_context).await {
            Ok(removed) => Ok(Json(RemoveOutput {
                removed,
                info_hash: info_hash.to_string(),
            })),
            Err(error) => {
                tracing::warn!(%error, "failed to remove blacklist entry via MCP");
                Err(format!("failed to remove blacklist entry: {error}"))
            }
        }
    }

    #[tool(
        name = "scrape",
        description = "Fetch swarm statistics (seeders, leechers, completed downloads) for up to 200 info_hashes. Mirrors the HTTP /scrape endpoint: blacklisted info_hashes are omitted and unknown info_hashes report zeros.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn scrape(
        &self,
        Parameters(args): Parameters<ScrapeArgs>,
    ) -> Result<Json<ScrapeOutput>, String> {
        let info_hashes = parse_info_hashes(&args.info_hashes)?;
        let allowed = self.state.blacklist.filter_allowed(&info_hashes).await;
        let stats = self.state.tracker.scrape(&allowed).await;
        let mut entries: Vec<_> = stats
            .into_iter()
            .map(|(info_hash, stats)| ScrapeEntry {
                info_hash: info_hash.to_string(),
                complete: stats.complete as u64,
                incomplete: stats.incomplete as u64,
                downloaded: stats.downloaded,
            })
            .collect();
        // Same deterministic order as the bencoded scrape response.
        entries.sort_by(|a, b| a.info_hash.cmp(&b.info_hash));
        Ok(Json(ScrapeOutput { entries }))
    }

    /// Mutations need a blacklist file to write to. The HTTP admin surface
    /// answers `503` in that case; MCP reports the same condition as a tool
    /// error so a misconfiguration is not mistaken for a failed write.
    fn ensure_blacklist_configured(&self) -> Result<(), String> {
        if self.state.blacklist.path().is_none() {
            return Err("blacklist file is not configured".to_string());
        }
        Ok(())
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "rustracker",
    instructions = "Operate the rustracker BitTorrent tracker. Use scrape to read swarm statistics (seeders/leechers/completed) for info_hashes; use blacklist_check/list for read-only blacklist inspection, blacklist_add to block a torrent, and blacklist_remove to unblock it. All info_hash values are 40-character hexadecimal strings."
)]
impl ServerHandler for TrackerMcp {}

pub(crate) fn service(state: AppState) -> StreamableHttpService<TrackerMcp, NeverSessionManager> {
    StreamableHttpService::new(
        move || Ok(TrackerMcp::new(&state)),
        NeverSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            // Stateless: no Mcp-Session-Id and no session affinity required.
            .with_legacy_session_mode(false)
            // JSON responses are simpler for admin tooling than SSE.
            .with_json_response(true)
            // The endpoint is intended for public deployments behind TLS with
            // a bearer token; rmcp's default loopback-only Host allowlist would
            // reject legitimate requests.
            .disable_allowed_hosts()
            .with_max_request_body_bytes(1024 * 1024),
    )
}

fn parse_info_hash(raw: &str) -> Result<InfoHash, String> {
    InfoHash::from_hex(raw.trim())
        .ok_or_else(|| "info_hash must be a 40-char hex string".to_string())
}

fn parse_info_hashes(raw: &[String]) -> Result<Vec<InfoHash>, String> {
    if raw.is_empty() {
        return Err("info_hashes must not be empty".to_string());
    }
    if raw.len() > MAX_SCRAPE_HASHES {
        return Err(format!(
            "too many info_hash values: {} (max {MAX_SCRAPE_HASHES})",
            raw.len()
        ));
    }
    raw.iter()
        .enumerate()
        .map(|(index, value)| {
            parse_info_hash(value).map_err(|error| format!("info_hashes[{index}]: {error}"))
        })
        .collect()
}

fn audit_context(state: &McpState, context: &RequestContext<RoleServer>) -> AuditContext {
    audit_context_from_parts(state, context.extensions.get::<Parts>())
}

fn audit_context_from_parts(state: &McpState, parts: Option<&Parts>) -> AuditContext {
    let header_ip = if state.trust_proxy_headers {
        parts.and_then(|parts| cloudflare_connecting_ip(&parts.headers))
    } else {
        None
    };
    let peer_ip = parts
        .and_then(|parts| parts.extensions.get::<ConnectInfo<SocketAddr>>())
        .map(|ConnectInfo(addr)| addr.ip());
    AuditContext {
        source: "mcp",
        client_ip: header_ip.or(peer_ip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::IpAddr;
    use std::time::Duration;

    use axum::http::Request;

    use crate::core::tracker::Tracker;
    use crate::protocol::MAX_SCRAPE_HASHES;

    const HASH: &str = "abcdef0123456789abcdef0123456789abcdef01";

    fn test_state(trust_proxy_headers: bool) -> McpState {
        let blacklist = Arc::new(BlacklistStore::new(None));
        McpState {
            tracker: Arc::new(TrackerPool::single(Tracker::new(
                Duration::from_secs(1800),
                Duration::from_secs(3000),
            ))),
            admin: Arc::new(BlacklistAdmin::new(
                blacklist.clone(),
                Arc::new(super::super::audit::AuditLog::new(None)),
            )),
            blacklist,
            trust_proxy_headers,
        }
    }

    #[test]
    fn new_carries_the_proxy_trust_flag_from_the_app_state() {
        let mut app_state = AppState::new(
            Tracker::new(Duration::from_secs(1800), Duration::from_secs(3000)),
            None,
        );
        app_state.trust_proxy_headers = true;

        let mcp = TrackerMcp::new(&app_state);

        assert!(mcp.state.trust_proxy_headers);
    }

    /// Build the request parts rmcp hands to a tool handler, with the
    /// connect info a real server attaches.
    fn request_parts(proxy_ip: Option<&str>, peer: SocketAddr) -> Parts {
        let mut builder = Request::builder();
        if let Some(proxy_ip) = proxy_ip {
            builder = builder.header("cf-connecting-ip", proxy_ip);
        }
        let (mut parts, _) = builder.body(()).unwrap().into_parts();
        parts.extensions.insert(ConnectInfo(peer));
        parts
    }

    #[test]
    fn parse_info_hash_accepts_trimmed_hex_in_any_case() {
        let lower = parse_info_hash(HASH).expect("valid hash");
        assert_eq!(lower.to_string(), HASH);

        let upper = parse_info_hash(&HASH.to_ascii_uppercase()).expect("valid hash");
        assert_eq!(upper.to_string(), HASH);

        let padded = parse_info_hash(&format!("  {HASH}\n")).expect("valid hash");
        assert_eq!(padded.to_string(), HASH);
    }

    #[test]
    fn parse_info_hash_rejects_malformed_input() {
        let cases = [
            String::new(),
            "not-a-hash".to_string(),
            "a".repeat(39),
            "a".repeat(41),
            "z".repeat(40),
            format!("{}zz", "a".repeat(38)),
        ];
        for raw in &cases {
            assert!(
                parse_info_hash(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
    }

    #[test]
    fn parse_info_hashes_rejects_empty_and_over_limit_input() {
        assert!(parse_info_hashes(&[]).is_err());

        let too_many: Vec<String> = (0..=MAX_SCRAPE_HASHES).map(|_| HASH.to_string()).collect();
        let error = parse_info_hashes(&too_many).expect_err("over the limit");
        assert!(error.contains("too many"), "unexpected error: {error}");

        let at_limit: Vec<String> = (0..MAX_SCRAPE_HASHES).map(|_| HASH.to_string()).collect();
        assert_eq!(
            parse_info_hashes(&at_limit).expect("at the limit").len(),
            MAX_SCRAPE_HASHES
        );
    }

    #[test]
    fn parse_info_hashes_reports_the_offending_index() {
        let raw = vec![HASH.to_string(), "not-a-hash".to_string()];
        let error = parse_info_hashes(&raw).expect_err("malformed entry");
        assert!(
            error.contains("info_hashes[1]"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn audit_context_uses_peer_ip_when_proxy_headers_are_untrusted() {
        let state = test_state(false);
        let parts = request_parts(
            Some("203.0.113.7"),
            SocketAddr::from(([198, 51, 100, 1], 4242)),
        );

        let context = audit_context_from_parts(&state, Some(&parts));

        assert_eq!(context.source, "mcp");
        assert_eq!(context.client_ip, Some(IpAddr::from([198, 51, 100, 1])));
    }

    #[test]
    fn audit_context_prefers_proxy_header_when_trusted() {
        let state = test_state(true);
        let parts = request_parts(
            Some("203.0.113.7"),
            SocketAddr::from(([198, 51, 100, 1], 4242)),
        );

        let context = audit_context_from_parts(&state, Some(&parts));

        assert_eq!(context.source, "mcp");
        assert_eq!(context.client_ip, Some(IpAddr::from([203, 0, 113, 7])));
    }

    #[test]
    fn audit_context_without_request_parts_has_no_client_ip() {
        let state = test_state(true);

        let context = audit_context_from_parts(&state, None);

        assert_eq!(context.source, "mcp");
        assert_eq!(context.client_ip, None);
    }
}
