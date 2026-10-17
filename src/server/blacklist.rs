//! Info-hash blacklist store: file parsing, hot reload, persistence, and
//! atomic removal.
//!
//! Reads a newline-separated file where each line is a 40-char hex
//! info_hash. Lines starting with `#` are treated as comments.
//! The file is the source of truth: writes go to the file first, then
//! the in-memory set, so a concurrent reload can never roll back an
//! entry that was already persisted.
//!
//! All file mutations are serialized by `write_lock`. Removal rewrites a
//! temporary file and renames it over the original, so readers (including
//! the hot-reload watcher) never observe a half-written blacklist.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock};
use tokio::time::MissedTickBehavior;

use crate::core::types::InfoHash;

pub(crate) const WATCH_INTERVAL: Duration = Duration::from_secs(5);
pub(crate) const DEFAULT_LIST_LIMIT: usize = 100;
pub(crate) const MAX_LIST_LIMIT: usize = 1000;

pub(crate) struct BlacklistStore {
    path: Option<PathBuf>,
    set: RwLock<HashSet<InfoHash>>,
    write_lock: Mutex<()>,
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
            write_lock: Mutex::new(()),
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

    /// Return `(total, page)` with entries sorted lexicographically by
    /// info_hash, so list responses are stable across calls.
    pub(crate) async fn page(&self, offset: usize, limit: usize) -> (usize, Vec<InfoHash>) {
        let set = self.set.read().await;
        let total = set.len();
        if limit == 0 || offset >= total {
            return (total, Vec::new());
        }

        let mut hashes: Vec<_> = set.iter().copied().collect();
        hashes.sort_unstable();
        let entries = hashes.into_iter().skip(offset).take(limit).collect();
        (total, entries)
    }

    /// Append an entry to the file, then insert it into memory.
    /// Returns whether the entry was newly added.
    pub(crate) async fn insert(&self, info_hash: InfoHash) -> std::io::Result<bool> {
        let _guard = self.write_lock.lock().await;
        if self.set.read().await.contains(&info_hash) {
            return Ok(false);
        }
        let path = self.path.as_deref().ok_or_else(path_not_configured)?;
        append_entry(path, info_hash).await?;
        self.set.write().await.insert(info_hash);
        Ok(true)
    }

    /// Remove an entry by rewriting the file atomically, then dropping it
    /// from memory. Returns whether anything actually changed.
    pub(crate) async fn remove(&self, info_hash: InfoHash) -> std::io::Result<bool> {
        let _guard = self.write_lock.lock().await;
        let path = self.path.as_deref().ok_or_else(path_not_configured)?;

        // The file is the source of truth. A missing file is treated as an
        // empty blacklist so removal stays idempotent before the first add.
        let content = match tokio::fs::read_to_string(path).await {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };

        let mut filtered = String::with_capacity(content.len());
        let mut file_changed = false;
        for line in content.lines() {
            let trimmed = line.trim();
            let matches = !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && InfoHash::from_hex(trimmed).is_some_and(|hash| hash == info_hash);
            if matches {
                file_changed = true;
            } else {
                filtered.push_str(line);
                filtered.push('\n');
            }
        }

        if file_changed {
            write_atomic(path, filtered.as_bytes()).await?;
        }

        let memory_changed = self.set.write().await.remove(&info_hash);
        Ok(file_changed || memory_changed)
    }

    /// Re-read the file, replacing the in-memory set. Returns the new count.
    pub(crate) async fn reload(&self) -> anyhow::Result<usize> {
        let _guard = self.write_lock.lock().await;
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

fn path_not_configured() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "blacklist file is not configured",
    )
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
            match store.reload().await {
                Ok(count) => {
                    last_mtime = mtime;
                    tracing::info!(count, "blacklist reloaded");
                }
                Err(err) => {
                    // Leave `last_mtime` unchanged so a transient read error is
                    // retried on the next tick instead of waiting for another
                    // edit.
                    tracing::warn!("{err}");
                }
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
    // Avoid gluing the new hash onto a final line that lacks a newline.
    let needs_newline = match tokio::fs::File::open(path).await {
        Ok(mut file) => {
            let len = file.metadata().await?.len();
            if len == 0 {
                false
            } else {
                file.seek(std::io::SeekFrom::End(-1)).await?;
                let mut last = [0_u8; 1];
                file.read_exact(&mut last).await?;
                last[0] != b'\n'
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error),
    };

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    if needs_newline {
        file.write_all(b"\n").await?;
    }
    file.write_all(format!("{info_hash}\n").as_bytes()).await?;
    file.flush().await?;
    file.sync_all().await
}

/// Write `contents` to a sibling temp file and rename it over `path`.
async fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let temp = temp_path(path);
    // Preserve the original file mode where it exists; the temp file would
    // otherwise be created with the process umask and change permissions.
    let permissions = tokio::fs::metadata(path)
        .await
        .ok()
        .map(|metadata| metadata.permissions());
    let result = async {
        let mut file = tokio::fs::File::create(&temp).await?;
        file.write_all(contents).await?;
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        if let Some(permissions) = permissions {
            tokio::fs::set_permissions(&temp, permissions).await?;
        }
        tokio::fs::rename(&temp, path).await
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result
}

fn temp_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("blacklist");
    path.with_file_name(format!(".{name}.{}.{counter}.tmp", std::process::id()))
}

fn file_mtime(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rustracker-bl-unit-{}-{}-{name}.txt",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn hash(hex: &str) -> InfoHash {
        InfoHash::from_hex(hex).unwrap()
    }

    #[tokio::test]
    async fn concurrent_insert_appends_exactly_one_line() {
        let path = temp_path("concurrent");
        let store = Arc::new(BlacklistStore::new(Some(path.clone())));
        let target = hash("6161616161616161616161616161616161616161");

        let mut tasks = Vec::new();
        for _ in 0..16 {
            let store = store.clone();
            tasks.push(tokio::spawn(
                async move { store.insert(target).await.unwrap() },
            ));
        }

        let mut added = 0;
        for task in tasks {
            if task.await.unwrap() {
                added += 1;
            }
        }
        assert_eq!(added, 1, "only one concurrent insert should win");

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            content
                .lines()
                .filter(|line| *line == target.to_string())
                .count(),
            1
        );
        assert!(store.contains(&target).await);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn remove_preserves_comments_and_is_idempotent() {
        let path = temp_path("remove");
        let first = hash("6161616161616161616161616161616161616161");
        let second = hash("6262626262626262626262626262626262626262");
        std::fs::write(&path, format!("# keep me\n{first}\n\n{second}\n")).unwrap();
        let store = BlacklistStore::new(Some(path.clone()));

        assert!(store.remove(first).await.unwrap());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# keep me"));
        assert!(content.contains(&second.to_string()));
        assert!(!content.contains(&first.to_string()));
        assert!(!store.contains(&first).await);
        assert!(store.contains(&second).await);

        assert!(!store.remove(first).await.unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn page_is_sorted_and_paginated() {
        let path = temp_path("page");
        let a = hash("3333333333333333333333333333333333333333");
        let b = hash("1111111111111111111111111111111111111111");
        let c = hash("2222222222222222222222222222222222222222");
        std::fs::write(&path, format!("{a}\n{b}\n{c}\n")).unwrap();
        let store = BlacklistStore::new(Some(path.clone()));

        let (total, first) = store.page(0, 2).await;
        assert_eq!(total, 3);
        assert_eq!(first, vec![b, c]);

        let (total, second) = store.page(2, 2).await;
        assert_eq!(total, 3);
        assert_eq!(second, vec![a]);

        let _ = std::fs::remove_file(&path);
    }
}
