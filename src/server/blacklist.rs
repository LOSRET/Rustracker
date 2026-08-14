//! Info-hash blacklist store: file parsing, hot reload, and persistence.
//!
//! Reads a newline-separated file where each line is a 40-char hex
//! info_hash. Lines starting with `#` are treated as comments.
//! The file is the source of truth: writes go to the file first, then
//! the in-memory set, so a concurrent reload can never roll back an
//! entry that was already persisted.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::RwLock;
use tokio::time::MissedTickBehavior;

use crate::core::types::InfoHash;

pub(crate) struct BlacklistStore {
    path: Option<PathBuf>,
    set: RwLock<HashSet<InfoHash>>,
}

impl BlacklistStore {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        let initial = path
            .as_deref()
            .and_then(|path| match load_blacklist(path) {
                Ok(set) => Some(set),
                Err(err) => {
                    tracing::warn!("{err}");
                    None
                }
            })
            .unwrap_or_default();
        Self {
            path,
            set: RwLock::new(initial),
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub(crate) async fn contains(&self, info_hash: &InfoHash) -> bool {
        self.set.read().await.contains(info_hash)
    }

    pub(crate) async fn filter_allowed(&self, info_hashes: &[InfoHash]) -> Vec<InfoHash> {
        let set = self.set.read().await;
        info_hashes
            .iter()
            .copied()
            .filter(|hash| !set.contains(hash))
            .collect()
    }

    /// Append an entry to the file, then insert it into memory.
    /// Returns whether the entry was newly added.
    pub(crate) async fn insert(&self, info_hash: InfoHash) -> std::io::Result<bool> {
        if self.set.read().await.contains(&info_hash) {
            return Ok(false);
        }
        let path = self.path.as_deref().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "blacklist file is not configured",
            )
        })?;
        append_entry(path, info_hash).await?;
        self.set.write().await.insert(info_hash);
        Ok(true)
    }

    /// Re-read the file, replacing the in-memory set. Returns the new count.
    pub(crate) async fn reload(&self) -> anyhow::Result<usize> {
        let path = self
            .path
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("blacklist file is not configured"))?;
        let new_set = load_blacklist(path)?;
        let count = new_set.len();
        *self.set.write().await = new_set;
        Ok(count)
    }
}

/// Spawn a background task that reloads the blacklist file whenever it
/// changes (checked by mtime every `interval`).
pub(crate) fn spawn_watcher(store: Arc<BlacklistStore>, interval: Duration) {
    let Some(path) = store.path().map(Path::to_path_buf) else {
        return;
    };
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(interval);
        timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut last_mtime = file_mtime(&path);

        loop {
            timer.tick().await;
            let mtime = file_mtime(&path);
            if mtime == last_mtime {
                continue;
            }
            last_mtime = mtime;
            match store.reload().await {
                Ok(count) => tracing::info!(count, "blacklist reloaded"),
                Err(err) => tracing::warn!("{err}"),
            }
        }
    });
}

/// Parse a blacklist file. Each non-empty, non-comment line is a 40-char hex info_hash.
pub(crate) fn load_blacklist(path: &Path) -> anyhow::Result<HashSet<InfoHash>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    let set: HashSet<_> = content
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .filter_map(|(line_no, line)| {
            let trimmed = line.trim();
            match InfoHash::from_hex(trimmed) {
                Some(hash) => Some(hash),
                None => {
                    tracing::warn!(
                        "{}:{}: invalid info_hash \"{}\", skipped",
                        path.display(),
                        line_no + 1,
                        trimmed
                    );
                    None
                }
            }
        })
        .collect();
    Ok(set)
}

async fn append_entry(path: &Path, info_hash: InfoHash) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(format!("{info_hash}\n").as_bytes()).await?;
    file.flush().await
}

fn file_mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}
