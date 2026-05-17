//! Info-hash blacklist file watcher and parser.
//!
//! Reads a newline-separated file where each line is a 40-char hex
//! info_hash. Lines starting with `#` are treated as comments.

use std::collections::HashSet;
use std::path::Path;

use crate::core::types::InfoHash;

/// Parse a blacklist file. Each non-empty, non-comment line is a 40-char hex info_hash.
pub(crate) fn load_blacklist(path: &Path) -> anyhow::Result<HashSet<InfoHash>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {e}", path.display()))?;
    let mut set = HashSet::new();
    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match InfoHash::from_hex(line) {
            Some(hash) => {
                set.insert(hash);
            }
            None => {
                tracing::warn!(
                    "{}:{}: invalid info_hash \"{}\", skipped",
                    path.display(),
                    line_no + 1,
                    line
                );
            }
        }
    }
    Ok(set)
}
