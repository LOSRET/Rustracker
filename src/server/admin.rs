use std::path::Path;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing;

use super::AppState;
use crate::core::types::InfoHash;

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
    blacklist.insert(info_hash);

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
    file.write_all(format!("{info_hash}\n").as_bytes()).await?;
    file.flush().await
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
