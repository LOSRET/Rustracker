use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use super::audit::{AuditContext, AuditEntry, AuditLog};
use super::blacklist::{BlacklistStore, DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT};
use super::handlers::cloudflare_connecting_ip;
use super::AppState;
use crate::core::types::InfoHash;

pub(crate) struct BlacklistAdmin {
    store: Arc<BlacklistStore>,
    audit: Arc<AuditLog>,
}

impl BlacklistAdmin {
    pub(crate) fn new(store: Arc<BlacklistStore>, audit: Arc<AuditLog>) -> Self {
        Self { store, audit }
    }

    pub(crate) async fn add(
        &self,
        info_hash: InfoHash,
        context: &AuditContext,
    ) -> std::io::Result<bool> {
        match self.store.insert(info_hash).await {
            Ok(added) => {
                self.audit
                    .record(&AuditEntry::success(
                        "add",
                        info_hash,
                        context,
                        Some(added),
                        None,
                    ))
                    .await;
                Ok(added)
            }
            Err(error) => {
                self.audit
                    .record(&AuditEntry::failure(
                        "add",
                        info_hash,
                        context,
                        error.to_string(),
                    ))
                    .await;
                Err(error)
            }
        }
    }

    pub(crate) async fn remove(
        &self,
        info_hash: InfoHash,
        context: &AuditContext,
    ) -> std::io::Result<bool> {
        match self.store.remove(info_hash).await {
            Ok(removed) => {
                self.audit
                    .record(&AuditEntry::success(
                        "remove",
                        info_hash,
                        context,
                        None,
                        Some(removed),
                    ))
                    .await;
                Ok(removed)
            }
            Err(error) => {
                self.audit
                    .record(&AuditEntry::failure(
                        "remove",
                        info_hash,
                        context,
                        error.to_string(),
                    ))
                    .await;
                Err(error)
            }
        }
    }
}

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
pub(crate) struct BlacklistQuery {
    pub info_hash: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RemoveBlacklistQuery {
    pub info_hash: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct BlacklistStatusResponse {
    pub ok: bool,
    pub blacklisted: bool,
    pub info_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListBlacklistResponse {
    pub ok: bool,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub entries: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoveBlacklistResponse {
    pub ok: bool,
    pub removed: bool,
    pub info_hash: Option<String>,
    pub error: Option<String>,
}

/// `GET /api/blacklist` with `info_hash` returns a status check; without it,
/// returns a paginated listing.
pub(crate) async fn blacklist_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<BlacklistQuery>,
) -> Response {
    let status_mode = query.info_hash.is_some();
    let Some(token) = configured_token(&state) else {
        return if status_mode {
            status_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "admin token is not configured",
            )
        } else {
            list_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "admin token is not configured",
                query.offset,
                query.limit,
            )
        };
    };
    if !authorized(&headers, token) {
        return if status_mode {
            status_error(StatusCode::UNAUTHORIZED, "unauthorized")
        } else {
            list_error(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                query.offset,
                query.limit,
            )
        };
    }

    if let Some(raw) = query.info_hash {
        let Some(info_hash) = InfoHash::from_hex(raw.trim()) else {
            return status_error(
                StatusCode::BAD_REQUEST,
                "info_hash must be a 40-char hex string",
            );
        };
        let blacklisted = state.blacklist.contains(&info_hash).await;
        return (
            StatusCode::OK,
            Json(BlacklistStatusResponse {
                ok: true,
                blacklisted,
                info_hash: Some(info_hash.to_string()),
                error: None,
            }),
        )
            .into_response();
    }

    let offset = query.offset.unwrap_or(0);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .min(MAX_LIST_LIMIT);
    let (total, hashes) = state.blacklist.page(offset, limit).await;
    let entries = hashes.into_iter().map(|hash| hash.to_string()).collect();
    (
        StatusCode::OK,
        Json(ListBlacklistResponse {
            ok: true,
            total,
            offset,
            limit,
            entries,
            error: None,
        }),
    )
        .into_response()
}

pub(crate) async fn add_blacklist(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Json(request): Json<AddBlacklistRequest>,
) -> (StatusCode, Json<AddBlacklistResponse>) {
    let Some(token) = configured_token(&state) else {
        return add_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "admin token is not configured",
        );
    };
    if !authorized(&headers, token) {
        return add_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    if state.blacklist.path().is_none() {
        return add_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "blacklist file is not configured",
        );
    }
    let Some(info_hash) = InfoHash::from_hex(request.info_hash.trim()) else {
        return add_error(
            StatusCode::BAD_REQUEST,
            "info_hash must be a 40-char hex string",
        );
    };

    let context = http_audit_context(&state, &headers, connect_info);
    match state.admin.add(info_hash, &context).await {
        Ok(added) => (
            StatusCode::OK,
            Json(AddBlacklistResponse {
                ok: true,
                added,
                info_hash: Some(info_hash.to_string()),
                error: None,
            }),
        ),
        Err(error) => {
            tracing::warn!(%error, "failed to persist blacklist entry");
            add_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to persist blacklist entry",
            )
        }
    }
}

pub(crate) async fn remove_blacklist(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    Query(query): Query<RemoveBlacklistQuery>,
) -> (StatusCode, Json<RemoveBlacklistResponse>) {
    let Some(token) = configured_token(&state) else {
        return remove_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "admin token is not configured",
        );
    };
    if !authorized(&headers, token) {
        return remove_error(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    if state.blacklist.path().is_none() {
        return remove_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "blacklist file is not configured",
        );
    }
    let Some(info_hash) = InfoHash::from_hex(query.info_hash.trim()) else {
        return remove_error(
            StatusCode::BAD_REQUEST,
            "info_hash must be a 40-char hex string",
        );
    };

    let context = http_audit_context(&state, &headers, connect_info);
    match state.admin.remove(info_hash, &context).await {
        Ok(removed) => (
            StatusCode::OK,
            Json(RemoveBlacklistResponse {
                ok: true,
                removed,
                info_hash: Some(info_hash.to_string()),
                error: None,
            }),
        ),
        Err(error) => {
            tracing::warn!(%error, "failed to remove blacklist entry");
            remove_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to remove blacklist entry",
            )
        }
    }
}

/// Auth guard for the MCP endpoint. The route is only mounted when a token is
/// configured, but fail closed if that invariant is ever broken.
pub(crate) async fn mcp_auth(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let Some(token) = configured_token(&state) else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    if authorized(request.headers(), token) {
        next.run(request).await
    } else {
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        response
    }
}

pub(crate) fn authorized(headers: &HeaderMap, token: &str) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(provided) = value.strip_prefix("Bearer ") else {
        return false;
    };
    provided.as_bytes().ct_eq(token.as_bytes()).into()
}

fn configured_token(state: &AppState) -> Option<&str> {
    state
        .admin_token
        .as_deref()
        .filter(|token| !token.is_empty())
}

fn http_audit_context(
    state: &AppState,
    headers: &HeaderMap,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
) -> AuditContext {
    let header_ip = if state.trust_proxy_headers {
        cloudflare_connecting_ip(headers)
    } else {
        None
    };
    let peer_ip = connect_info.map(|Extension(ConnectInfo(addr))| addr.ip());
    AuditContext {
        source: "http",
        client_ip: header_ip.or(peer_ip),
    }
}

fn status_error(status: StatusCode, error: &'static str) -> Response {
    (
        status,
        Json(BlacklistStatusResponse {
            ok: false,
            blacklisted: false,
            info_hash: None,
            error: Some(error.to_string()),
        }),
    )
        .into_response()
}

fn list_error(
    status: StatusCode,
    error: &'static str,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Response {
    (
        status,
        Json(ListBlacklistResponse {
            ok: false,
            total: 0,
            offset: offset.unwrap_or(0),
            limit: limit.unwrap_or(DEFAULT_LIST_LIMIT).min(MAX_LIST_LIMIT),
            entries: Vec::new(),
            error: Some(error.to_string()),
        }),
    )
        .into_response()
}

fn add_error(status: StatusCode, error: &'static str) -> (StatusCode, Json<AddBlacklistResponse>) {
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

fn remove_error(
    status: StatusCode,
    error: &'static str,
) -> (StatusCode, Json<RemoveBlacklistResponse>) {
    (
        status,
        Json(RemoveBlacklistResponse {
            ok: false,
            removed: false,
            info_hash: None,
            error: Some(error.to_string()),
        }),
    )
}
