use super::are_all_dependencies_satisfied;
use crate::models::stage::Stage;
use anyhow::Result;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// How long a cached dependency-satisfaction result stays valid before the
/// (potentially expensive) git-ancestry check is re-run, even if no dependency
/// stage file changed. Bounds staleness so a manually-edited git history is
/// noticed within a minute.
const DEP_CHECK_RECHECK_INTERVAL: Duration = Duration::from_secs(60);

/// One cached `are_all_dependencies_satisfied` result for a stage.
struct DepCheckCacheEntry {
    /// When the underlying check last actually ran.
    checked_at: Instant,
    /// Modification times of every dependency stage file at check time, in
    /// dependency order. A change here means a dependency advanced (e.g. became
    /// merged) so the check must re-run.
    dep_fingerprint: Vec<Option<SystemTime>>,
    /// Only conclusive outcomes are cached; operational errors retry next tick.
    result: bool,
}

#[derive(Hash, PartialEq, Eq)]
struct DepCheckKey {
    stage_id: String,
    dependencies: Vec<String>,
    work_dir: PathBuf,
    repo_root: PathBuf,
    target_branch: String,
}

thread_local! {
    /// Per-context cache of dependency-satisfaction results, used by
    /// [`are_all_dependencies_satisfied_cached`]. Lives on the orchestrator's
    /// (single) poll thread; never shared across threads.
    static DEP_CHECK_CACHE: RefCell<HashMap<DepCheckKey, DepCheckCacheEntry>> =
        RefCell::new(HashMap::new());
}

/// Dependency mtimes, or `None` when a file cannot be read reliably.
fn dependency_mtime_fingerprint(stage: &Stage, work_dir: &Path) -> Vec<Option<SystemTime>> {
    let stages_dir = work_dir.join("stages");
    stage
        .dependencies
        .iter()
        .map(|dep_id| {
            crate::fs::stage_files::find_stage_file(&stages_dir, dep_id)
                .ok()
                .flatten()
                .and_then(|path| path.metadata().ok())
                .and_then(|meta| meta.modified().ok())
        })
        .collect()
}

/// Cached form of [`are_all_dependencies_satisfied`] for the per-tick spawn guard.
///
/// The spawn-time phantom-merge guard runs on every orchestrator poll for every
/// not-yet-ready stage. The underlying check loads each dependency stage file
/// and shells out to `git merge-base --is-ancestor` per non-knowledge dep — work
/// that is wasted when nothing changed since last tick (P-6).
///
/// This wrapper memoizes a conclusive boolean result per context and only re-runs the
/// expensive check when either:
///   - a dependency stage file's mtime changed (a dep advanced, e.g. merged), or
///   - more than `DEP_CHECK_RECHECK_INTERVAL` elapsed since the last real run
///     (bounds staleness against out-of-band git history changes).
///
/// Operational errors, including transient git failures, are never cached, so
/// the next tick retries. Missing commits and phantom merges are conclusive
/// blocks and are cached until the dependency files or interval change.
pub fn are_all_dependencies_satisfied_cached(
    stage: &Stage,
    work_dir: &Path,
    repo_root: &Path,
    target_branch: &str,
) -> Result<bool> {
    // No deps → trivially satisfied, nothing to cache.
    if stage.dependencies.is_empty() {
        return Ok(true);
    }

    let fingerprint = dependency_mtime_fingerprint(stage, work_dir);
    if fingerprint.iter().any(Option::is_none) {
        return are_all_dependencies_satisfied(stage, work_dir, repo_root, target_branch);
    }
    let now = Instant::now();
    let key = DepCheckKey {
        stage_id: stage.id.clone(),
        dependencies: stage.dependencies.clone(),
        work_dir: work_dir.to_path_buf(),
        repo_root: repo_root.to_path_buf(),
        target_branch: target_branch.to_string(),
    };

    // Fast path: reuse the cached result if the dependency files are unchanged
    // and the recheck interval has not elapsed.
    let cached = cached_result(&key, &fingerprint, now);
    if let Some(result) = cached {
        return Ok(result);
    }

    // Slow path: run the real check and cache the boolean outcome.
    let result = are_all_dependencies_satisfied(stage, work_dir, repo_root, target_branch)?;
    DEP_CHECK_CACHE.with(|cache| {
        cache.borrow_mut().insert(
            key,
            DepCheckCacheEntry {
                checked_at: now,
                dep_fingerprint: fingerprint,
                result,
            },
        );
    });
    Ok(result)
}

fn cached_result(
    key: &DepCheckKey,
    fingerprint: &[Option<SystemTime>],
    now: Instant,
) -> Option<bool> {
    DEP_CHECK_CACHE.with(|cache| {
        let cache = cache.borrow();
        cache.get(key).and_then(|entry| {
            let fresh = now.duration_since(entry.checked_at) < DEP_CHECK_RECHECK_INTERVAL;
            if fresh && entry.dep_fingerprint == fingerprint {
                Some(entry.result)
            } else {
                None
            }
        })
    })
}
