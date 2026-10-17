//! Append-only JSONL audit log for blacklist mutations.
//!
//! The log lives next to the blacklist file (`<name>.audit.jsonl`) so that
//! enabling auditing needs no extra configuration. Each line is a standalone
//! JSON object; writes are serialized to keep concurrent admin/MCP calls from
//! interleaving partial lines.

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::core::types::InfoHash;

pub(crate) struct AuditLog {
    path: Option<PathBuf>,
    write_lock: Mutex<()>,
}

/// Where a mutation came from, for the audit record.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AuditContext {
    pub source: &'static str,
    pub client_ip: Option<IpAddr>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AuditEntry {
    pub ts: String,
    pub action: &'static str,
    pub info_hash: String,
    pub source: &'static str,
    pub result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AuditLog {
    pub(crate) fn new(blacklist_path: Option<&Path>) -> Self {
        Self {
            path: blacklist_path.map(|path| path.with_extension("audit.jsonl")),
            write_lock: Mutex::new(()),
        }
    }

    /// Append one record. Audit failures are logged but never fail the
    /// underlying mutation: the blacklist write is the operation that matters.
    pub(crate) async fn record(&self, entry: &AuditEntry) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let line = match serde_json::to_vec(entry) {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(%error, "failed to serialize audit log entry");
                return;
            }
        };

        let _guard = self.write_lock.lock().await;
        if let Err(error) = append_line(path, &line).await {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to append audit log entry"
            );
        }
    }
}

impl AuditEntry {
    pub(crate) fn success(
        action: &'static str,
        info_hash: InfoHash,
        context: &AuditContext,
        added: Option<bool>,
        removed: Option<bool>,
    ) -> Self {
        Self {
            ts: now(),
            action,
            info_hash: info_hash.to_string(),
            source: context.source,
            result: "ok",
            client_ip: context.client_ip.map(|ip| ip.to_string()),
            added,
            removed,
            error: None,
        }
    }

    pub(crate) fn failure(
        action: &'static str,
        info_hash: InfoHash,
        context: &AuditContext,
        error: impl Into<String>,
    ) -> Self {
        Self {
            ts: now(),
            action,
            info_hash: info_hash.to_string(),
            source: context.source,
            result: "error",
            client_ip: context.client_ip.map(|ip| ip.to_string()),
            added: None,
            removed: None,
            error: Some(error.into()),
        }
    }
}

async fn append_line(path: &Path, line: &[u8]) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(line).await?;
    file.write_all(b"\n").await?;
    file.flush().await?;
    file.sync_all().await
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
