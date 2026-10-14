//! The `tokens-tracker` daemon thread and the scan pass it runs: read the
//! lines every stage's transcripts gained, update the checkpoint, and publish
//! `.loom/work/tokens/flow.json` for the status collector.

use super::checkpoint::Checkpoint;
use super::discover::{discover, Source};
use super::flow_build::{build_flow, fit_flow};
use super::model::{
    TokenFlow, BUCKET_SECS, FLOW_FILE, MAX_FLOW_BYTES, STAGE_SERIES_LEN, TOKENS_DIR,
};
use super::quota_points::quota_left;
use super::tail;
use crate::quota::cache::{atomic_write, create_quota_dir, reject_symlink};
use anyhow::{bail, Context, Result};
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const TRACKER_INTERVAL_SECS: u64 = 30;
pub const CHECKPOINT_FILE: &str = "checkpoint.json";
/// How often a sleeping tracker looks at the shutdown flag.
const SLEEP_SLICE: Duration = Duration::from_millis(250);

/// What [`scan_pass`] shares with the thread it runs on: the shutdown flag
/// that stops its file loop early (set only on a `tokens-tracker` thread) and
/// the warning of the last pass that finished. Thread-local, so trackers on
/// different threads never see each other's flag.
struct TrackerLocal {
    stop: Option<Arc<AtomicBool>>,
    warning: Option<String>,
}

thread_local! {
    static TRACKER: RefCell<TrackerLocal> = const {
        RefCell::new(TrackerLocal {
            stop: None,
            warning: None,
        })
    };
}

/// One pass at `now` over the `.loom/work` directory `work_root`: read every
/// stage's new transcript lines, write the checkpoint, then `flow.json`, and
/// return the flow as written. Codex rollouts are looked up under
/// `codex_sessions_root`, else `~/.codex/sessions`.
///
/// On the `tokens-tracker` thread the pass stops reading files once the
/// tracker's shutdown flag is set, and leaves its warning for the thread to
/// log, through the thread-local `TRACKER`.
pub fn scan_pass(
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    now: i64,
) -> Result<TokenFlow> {
    let stop = TRACKER.with_borrow(|tracker| tracker.stop.clone());
    let never = AtomicBool::new(false);
    let stop = stop.as_deref().unwrap_or(&never);
    let pass = scan_until(work_root, codex_sessions_root, now, stop)?;
    TRACKER.with_borrow_mut(|tracker| tracker.warning = pass.warning);
    Ok(pass.flow)
}

/// Starts the `tokens-tracker` thread: a pass now, then one every
/// [`TRACKER_INTERVAL_SECS`], until `shutdown` is set. A pass already running
/// stops reading files when it is.
pub fn spawn_token_tracker(work_root: PathBuf, shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("tokens-tracker".to_string())
        .spawn(move || run_tracker(&work_root, shutdown))
        .expect("failed to spawn tokens-tracker thread")
}

/// A finished pass and a problem it worked around, if any.
struct Pass {
    flow: TokenFlow,
    warning: Option<String>,
}

/// [`scan_pass`] that stops reading files once `stop` is set; what was read
/// up to then is still checkpointed and published. A file that cannot be read
/// is skipped and reported as the pass's warning. A checkpoint that cannot be
/// written fails the pass, after `flow.json` is still written.
fn scan_until(
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    now: i64,
    stop: &AtomicBool,
) -> Result<Pass> {
    let dir = ensure_tokens_dir(work_root)?;
    let mut checkpoint = Checkpoint::load(&dir);
    let sources = discover(work_root, codex_sessions_root, &mut checkpoint.threads, now)?;
    let mut warning = None;
    for source in &sources {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if let Err(error) = read_source(&mut checkpoint, source) {
            warning.get_or_insert_with(|| format!("cannot read {}: {error}", source.path));
        }
    }
    checkpoint.prune(now.div_euclid(BUCKET_SECS) - (STAGE_SERIES_LEN as i64 - 1));
    let mut flow = build_flow(&checkpoint.stages, now);
    flow.quota_left = quota_left(work_root, now);
    let saved = checkpoint.save(&dir);
    if !write_flow(&dir, &mut flow)? {
        warning = Some(format!(
            "flow over {MAX_FLOW_BYTES} bytes without idle series; {FLOW_FILE} not written"
        ));
    }
    saved?;
    Ok(Pass { flow, warning })
}

/// Reads the lines `source`'s file gained since the checkpoint. A missing file
/// is skipped and its agent keeps its totals; a new inode, a shrink below the
/// saved offset, or a different path restarts the agent from offset 0.
fn read_source(checkpoint: &mut Checkpoint, source: &Source) -> std::io::Result<()> {
    let path = Path::new(&source.path);
    let Some(stat) = tail::stat(path) else {
        return Ok(());
    };
    let agent = checkpoint.agent(source);
    if source.agent_type.is_some() {
        agent.agent_type.clone_from(&source.agent_type);
    }
    let file = &agent.file;
    if file.path != source.path || file.inode != stat.inode || stat.len < file.offset {
        agent.restart(&source.path, stat.inode);
    }
    if stat.len == agent.file.offset {
        return Ok(());
    }
    let mut offset = agent.file.offset;
    let read = tail::read_lines(path, &mut offset, stat.len, |line| agent.ingest(line));
    agent.file.offset = offset;
    read
}

/// `<work_root>/tokens`, created with mode 0700. A symlink there is refused
/// before anything is created: `create_quota_dir` alone would accept a symlink
/// to a directory, and `reject_symlink` checks only the last component.
fn ensure_tokens_dir(work_root: &Path) -> Result<PathBuf> {
    let dir = work_root.join(TOKENS_DIR);
    reject_symlink(&dir)?;
    create_quota_dir(&dir)?;
    let metadata =
        std::fs::symlink_metadata(&dir).with_context(|| format!("inspecting {}", dir.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("token flow path is not a directory: {}", dir.display());
    }
    Ok(dir)
}

/// Writes `flow` atomically when it fits [`MAX_FLOW_BYTES`] (after shedding
/// idle series); `false` when it does not and nothing was written.
fn write_flow(dir: &Path, flow: &mut TokenFlow) -> Result<bool> {
    let Some(json) = fit_flow(flow)? else {
        return Ok(false);
    };
    atomic_write(&dir.join(FLOW_FILE), &json)?;
    Ok(true)
}

fn run_tracker(work_root: &Path, shutdown: Arc<AtomicBool>) {
    TRACKER.with_borrow_mut(|tracker| tracker.stop = Some(Arc::clone(&shutdown)));
    let mut last_logged = None;
    while !shutdown.load(Ordering::SeqCst) {
        log_change(&mut last_logged, guarded_pass(work_root));
        sleep_unless_shutdown(&shutdown, Duration::from_secs(TRACKER_INTERVAL_SECS));
    }
}

/// One [`scan_pass`] at the current time; the problem to log, if any. A panic
/// is caught and reported like an error, so the thread outlives it.
fn guarded_pass(work_root: &Path) -> Option<String> {
    let now = chrono::Utc::now().timestamp();
    let pass = panic::catch_unwind(AssertUnwindSafe(|| scan_pass(work_root, None, now)));
    match pass {
        Ok(Ok(_)) => TRACKER.with_borrow_mut(|tracker| tracker.warning.take()),
        Ok(Err(error)) => Some(format!("{error:#}")),
        Err(_) => Some("scan pass panicked".to_string()),
    }
}

/// Logs `problem` unless it repeats the one logged last; a clean pass clears
/// the memory, so a problem that comes back is logged again.
fn log_change(last_logged: &mut Option<String>, problem: Option<String>) {
    if let Some(message) = &problem {
        if last_logged.as_ref() != Some(message) {
            eprintln!("tokens: {message}");
        }
    }
    *last_logged = problem;
}

fn sleep_unless_shutdown(shutdown: &AtomicBool, total: Duration) {
    let deadline = Instant::now() + total;
    while !shutdown.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(SLEEP_SLICE);
    }
}

#[cfg(test)]
#[path = "tracker_tests.rs"]
mod tests;
