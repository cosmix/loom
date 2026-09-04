//! Provider quota tracking: an on-disk cache, HTTP/subprocess pollers for
//! Claude and Codex, and the daemon thread that keeps them warm.
//!
//! `loom status` and the web dashboard both read [`read_snapshot`] - neither
//! polls directly, so a missing or stale cache never blocks status
//! reporting. See [`poller`] for the thread that populates the cache.

pub mod cache;
pub mod claude;
pub mod codex;
pub mod credentials;
pub mod model;
pub mod poller;

pub use model::*;

use std::path::Path;

/// Read the cached quota snapshot for both providers. Each provider is read
/// independently, so a missing or corrupt cache file for one never hides the
/// other.
pub fn read_snapshot(work_root: &Path) -> QuotaSnapshot {
    QuotaSnapshot {
        claude: cache::read_provider(work_root, "claude"),
        codex: cache::read_provider(work_root, "codex"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn an_absent_quota_directory_yields_no_providers() {
        let dir = tempdir().unwrap();
        let snapshot = read_snapshot(dir.path());
        assert!(snapshot.claude.is_none());
        assert!(snapshot.codex.is_none());
    }

    #[test]
    fn one_cached_provider_is_read_independently_of_the_other() {
        let dir = tempdir().unwrap();
        let quota = ProviderQuota {
            observed_at: 100,
            windows: vec![],
            plan: None,
            error: None,
        };
        cache::write_provider(dir.path(), "claude", &quota).unwrap();

        let snapshot = read_snapshot(dir.path());
        assert!(snapshot.claude.is_some());
        assert!(snapshot.codex.is_none());
    }
}
