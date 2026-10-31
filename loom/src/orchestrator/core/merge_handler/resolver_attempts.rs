//! The per-stage merge-resolver attempt counter: written by the daemon's spawn
//! loop, read by it, by `loom status`, and by `loom stage merge`.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Maximum number of merge-resolver sessions the daemon will spawn for a single
/// stage before giving up and routing it to `NeedsHumanReview`. It counts every
/// resolver session, the first included.
///
/// Without this cap a resolver that fails fast and deterministically would be
/// respawned on every ~5s poll cycle (a proven-dead resolver's signal is
/// retired by discovery), each
/// spawn on `opus`/`xhigh` → unbounded token + window burn (O-3).
pub(crate) const MAX_MERGE_RESOLVER_ATTEMPTS: u32 = 6;

/// Directory holding per-stage merge-resolver attempt counters.
///
/// Stored on disk (rather than in memory) so the cap survives daemon
/// restarts — a resolver that crash-loops across restarts must not reset
/// its budget each time `loom run` starts.
pub(in crate::orchestrator::core) fn attempts_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("merge-resolver-attempts")
}

/// The counter file for `stage_id` under the `.loom/work` directory `work_dir`.
pub(in crate::orchestrator::core) fn attempts_file(work_dir: &Path, stage_id: &str) -> PathBuf {
    attempts_dir(work_dir).join(format!("{stage_id}.count"))
}

/// Resolver sessions spawned so far for `stage_id`; 0 when no counter exists.
/// A counter that exists but cannot be read or parsed, or a counter path that
/// is not a regular file, reads as `MAX_MERGE_RESOLVER_ATTEMPTS`: a count
/// nobody can read must never reopen the budget. Reads only, so status
/// collection may call it.
pub(crate) fn merge_resolver_attempts(work_dir: &Path, stage_id: &str) -> u32 {
    recorded_attempts(&attempts_file(work_dir, stage_id)).unwrap_or(MAX_MERGE_RESOLVER_ATTEMPTS)
}

/// The count recorded at `path`: `Some(0)` when no counter exists there,
/// `None` when one exists but cannot be read as a count. The metadata check
/// comes first so a FIFO or directory at `path` is never opened.
fn recorded_attempts(path: &Path) -> Option<u32> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return None,
        Err(error) => return counter_absent(&error).then_some(0),
    }
    match std::fs::read_to_string(path) {
        Ok(count) => count.trim().parse().ok(),
        Err(error) => counter_absent(&error).then_some(0),
    }
}

/// Whether `error` says no counter exists at the path (nothing there, or a
/// parent that is not a directory), as opposed to one that cannot be read.
fn counter_absent(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

/// A merge-resolver attempt recorded before its session spawns, so no resolver
/// ever runs uncounted. Dropped without [`ReservedAttempt::keep`] (the spawn
/// failed), it restores the count it replaced, so only spawned resolvers
/// consume budget; if that restore fails, the attempt stays consumed.
#[must_use = "dropping a reservation gives its attempt back"]
pub(super) struct ReservedAttempt {
    path: PathBuf,
    replaced: u32,
    kept: bool,
}

impl ReservedAttempt {
    /// Record attempt `replaced + 1` for `stage_id`, whose counter read
    /// `replaced`. An error means the attempt is not recorded, and the caller
    /// must not spawn.
    pub(super) fn record(work_dir: &Path, stage_id: &str, replaced: u32) -> std::io::Result<Self> {
        std::fs::create_dir_all(attempts_dir(work_dir))?;
        let path = attempts_file(work_dir, stage_id);
        std::fs::write(&path, replaced.saturating_add(1).to_string())?;
        Ok(Self {
            path,
            replaced,
            kept: false,
        })
    }

    /// The resolver spawned: its attempt stays recorded.
    pub(super) fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for ReservedAttempt {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        let restored = if self.replaced == 0 {
            std::fs::remove_file(&self.path)
        } else {
            std::fs::write(&self.path, self.replaced.to_string())
        };
        if let Err(error) = restored {
            tracing::warn!(
                path = %self.path.display(),
                %error,
                "Failed to give back the attempt of a merge resolver that did not spawn; \
                 it stays consumed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        attempts_dir, attempts_file, merge_resolver_attempts, ReservedAttempt,
        MAX_MERGE_RESOLVER_ATTEMPTS,
    };

    fn work_dir_with_count(count: &str) -> tempfile::TempDir {
        let work_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(attempts_dir(work_dir.path())).unwrap();
        std::fs::write(attempts_file(work_dir.path(), "s"), count).unwrap();
        work_dir
    }

    #[test]
    fn max_merge_resolver_attempts_is_six() {
        assert_eq!(super::MAX_MERGE_RESOLVER_ATTEMPTS, 6);
    }

    #[test]
    fn a_missing_counter_reads_as_zero() {
        let work_dir = tempfile::tempdir().unwrap();
        assert_eq!(merge_resolver_attempts(work_dir.path(), "s"), 0);
    }

    #[test]
    fn a_recorded_counter_reads_back_up_to_the_cap() {
        let work_dir = work_dir_with_count("2\n");
        assert_eq!(merge_resolver_attempts(work_dir.path(), "s"), 2);
        let at_cap = work_dir_with_count(&MAX_MERGE_RESOLVER_ATTEMPTS.to_string());
        let count = merge_resolver_attempts(at_cap.path(), "s");
        assert_eq!(count, MAX_MERGE_RESOLVER_ATTEMPTS);
    }

    #[test]
    fn an_unparseable_counter_reads_as_the_cap() {
        let work_dir = work_dir_with_count("garbage");
        let count = merge_resolver_attempts(work_dir.path(), "s");
        assert_eq!(count, MAX_MERGE_RESOLVER_ATTEMPTS);
    }

    #[test]
    fn a_directory_at_the_counter_path_reads_as_the_cap() {
        let work_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(attempts_file(work_dir.path(), "s")).unwrap();
        let count = merge_resolver_attempts(work_dir.path(), "s");
        assert_eq!(count, MAX_MERGE_RESOLVER_ATTEMPTS);
    }

    #[test]
    fn a_file_in_place_of_the_counter_dir_reads_as_zero_and_refuses_a_reservation() {
        let work_dir = tempfile::tempdir().unwrap();
        std::fs::write(attempts_dir(work_dir.path()), "not a directory").unwrap();
        assert_eq!(merge_resolver_attempts(work_dir.path(), "s"), 0);
        assert!(ReservedAttempt::record(work_dir.path(), "s", 0).is_err());
    }

    #[test]
    fn a_dropped_reservation_restores_the_count_it_replaced() {
        let work_dir = work_dir_with_count("1");
        let attempt = ReservedAttempt::record(work_dir.path(), "s", 1).unwrap();
        assert_eq!(merge_resolver_attempts(work_dir.path(), "s"), 2);
        drop(attempt);
        assert_eq!(merge_resolver_attempts(work_dir.path(), "s"), 1);

        let fresh = tempfile::tempdir().unwrap();
        drop(ReservedAttempt::record(fresh.path(), "s", 0).unwrap());
        assert!(!attempts_file(fresh.path(), "s").exists());
    }

    #[test]
    fn a_kept_reservation_stays_recorded() {
        let work_dir = tempfile::tempdir().unwrap();
        ReservedAttempt::record(work_dir.path(), "s", 0)
            .unwrap()
            .keep();
        assert_eq!(merge_resolver_attempts(work_dir.path(), "s"), 1);
    }
}
