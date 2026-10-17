//! MCP management endpoint for the torrent blacklist.
//!
//! Mounted at `/mcp` using the Streamable HTTP transport in stateless mode.
//! The endpoint reuses the admin bearer token and exposes four tools that
//! share the same `BlacklistAdmin` service as the HTTP API, so validation,
//! persistence, and auditing cannot drift between the two surfaces.

use std::net::SocketAddr;

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
use super::blacklist::{DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT};
use super::handlers::cloudflare_connecting_ip;
use super::AppState;
use crate::core::types::InfoHash;

#[derive(Clone)]
pub(crate) struct BlacklistMcp {
    state: AppState,
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
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct CheckOutput {
    pub ok: bool,
    pub blacklisted: bool,
    pub info_hash: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct AddOutput {
    pub ok: bool,
    pub added: bool,
    pub info_hash: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct ListOutput {
    pub ok: bool,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub entries: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub(crate) struct RemoveOutput {
    pub ok: bool,
    pub removed: bool,
    pub info_hash: String,
}

#[tool_router]
impl BlacklistMcp {
    pub(crate) fn new(state: AppState) -> Self {
        Self {
            state,
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
            ok: true,
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
        let info_hash = parse_info_hash(&args.info_hash)?;
        let audit_context = audit_context(&self.state, &context);
        let admin = self.admin();
        match admin.add(info_hash, &audit_context).await {
            Ok(added) => Ok(Json(AddOutput {
                ok: true,
                added,
                info_hash: info_hash.to_string(),
            })),
            Err(error) => Err(format!("failed to persist blacklist entry: {error}")),
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
            ok: true,
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
        let info_hash = parse_info_hash(&args.info_hash)?;
        let audit_context = audit_context(&self.state, &context);
        let admin = self.admin();
        match admin.remove(info_hash, &audit_context).await {
            Ok(removed) => Ok(Json(RemoveOutput {
                ok: true,
                removed,
                info_hash: info_hash.to_string(),
            })),
            Err(error) => Err(format!("failed to remove blacklist entry: {error}")),
        }
    }

    fn admin(&self) -> BlacklistAdmin {
        BlacklistAdmin::new(self.state.blacklist.clone(), self.state.audit.clone())
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "rustracker",
    instructions = "Manage the rustracker BitTorrent tracker blacklist. Use blacklist_check/list for read-only inspection, blacklist_add to block a torrent, and blacklist_remove to unblock it. All info_hash values are 40-character hexadecimal strings."
)]
impl ServerHandler for BlacklistMcp {}

pub(crate) fn service(state: AppState) -> StreamableHttpService<BlacklistMcp, NeverSessionManager> {
    StreamableHttpService::new(
        move || Ok(BlacklistMcp::new(state.clone())),
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

fn audit_context(state: &AppState, context: &RequestContext<RoleServer>) -> AuditContext {
    let parts = context.extensions.get::<Parts>();
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
