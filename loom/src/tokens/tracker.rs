//! The `tokens-tracker` daemon thread and the scan pass it runs: read the
//! lines every stage's transcripts gained, update the checkpoint, and publish
//! `.loom/work/tokens/flow.json` for the status collector.

use super::discover::discover;
use super::flow_build::{build_flow, fit_flow};
use super::model::{
    TokenFlow, BUCKET_SECS, FLOW_FILE, MAX_FLOW_BYTES, STAGE_SERIES_LEN, TOKENS_DIR,
};
use super::pass_memo::PassMemo;
use super::source_read::read_sources;
use crate::fs::cache_io::{atomic_write, create_dir, reject_symlink};
use anyhow::{bail, Context, Result};
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Seconds the tracker sleeps after each pass.
pub const TRACKER_INTERVAL_SECS: u64 = 5;
pub const CHECKPOINT_FILE: &str = "checkpoint.json";
/// How often a sleeping tracker looks at the shutdown flag.
const SLEEP_SLICE: Duration = Duration::from_millis(250);

/// What [`scan_pass`] shares with the thread it runs on: the shutdown flag
/// that stops its file loop early (set only on a `tokens-tracker` thread),
/// the warning of the last pass that finished, and the memo of the work root
/// it last scanned. Thread-local, so trackers on different threads never see
/// each other's flag or memo.
pub(super) struct TrackerLocal {
    pub(super) stop: Option<Arc<AtomicBool>>,
    pub(super) warning: Option<String>,
    pub(super) memo: Option<PassMemo>,
}

thread_local! {
    pub(super) static TRACKER: RefCell<TrackerLocal> = const {
        RefCell::new(TrackerLocal {
            stop: None,
            warning: None,
            memo: None,
        })
    };
}

/// One pass at `now` over the `.loom/work` directory `work_root`: read every
/// stage's new transcript lines, write the checkpoint, then `flow.json`, each
/// only when the pass memo shows the file does not already hold it, and
/// return the flow the pass built. Codex rollouts are looked up under
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
    let pass = scan_until(work_root, codex_sessions_root, now, stop, LastLine::Wait)?;
    TRACKER.with_borrow_mut(|tracker| tracker.warning = pass.warning);
    Ok(pass.flow)
}

/// The pass that ends a run, on the thread that finished it, after the
/// tracker thread was joined: [`scan_pass`] that no shutdown flag stops, which
/// also counts an unfinished last line that is a whole JSON value (a writer
/// that never added the `\n`), and keeps every stage's agent totals in memory
/// for [`super::plan_totals::plan_tokens`] with this `work_root`, also when the
/// checkpoint is too big to write. Its warning is logged. A transcript that
/// could not be read makes the scan [`FinalScan::Incomplete`].
pub fn final_scan(
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    now: i64,
) -> Result<FinalScan> {
    let never = AtomicBool::new(false);
    let pass = scan_until(work_root, codex_sessions_root, now, &never, LastLine::Count)?;
    log_change(&mut None, pass.warning);
    Ok(pass
        .incomplete
        .map_or(FinalScan::Complete, FinalScan::Incomplete))
}

/// How a final scan ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalScan {
    /// Every transcript was read to its end.
    Complete,
    /// A transcript could not be read, so the totals miss it; the text names the first one.
    Incomplete(String),
}

/// What a pass does with an unfinished last line of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LastLine {
    /// Leave it for a later pass, which reads it once its `\n` is written.
    Wait,
    /// Count it when it is a whole JSON value, and move the offset past it.
    Count,
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
    /// The first transcript that could not be read, if any.
    incomplete: Option<String>,
}

/// [`scan_pass`] that stops reading files once `stop` is set; what was read
/// up to then is still checkpointed and published. A file that cannot be read
/// is skipped and reported as the pass's warning, as is a checkpoint or flow
/// that is too big to write. A checkpoint or flow that cannot be written fails
/// the pass, after the other is still written, naming both when both fail.
///
/// The pass runs with this thread's memo of `work_root`, a fresh one when the
/// thread last scanned another root, and leaves it for the next pass.
fn scan_until(
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    now: i64,
    stop: &AtomicBool,
    last_line: LastLine,
) -> Result<Pass> {
    let mut memo = TRACKER
        .with_borrow_mut(|tracker| tracker.memo.take())
        .filter(|memo| memo.is_for(work_root))
        .unwrap_or_else(|| PassMemo::new(work_root));
    let pass = run_pass(
        &mut memo,
        work_root,
        codex_sessions_root,
        now,
        stop,
        last_line,
    );
    TRACKER.with_borrow_mut(|tracker| tracker.memo = Some(memo));
    pass
}

/// [`scan_until`]'s pass with `memo`, which it reads and updates. A final
/// pass ([`LastLine::Count`]) registers the agent totals it read before
/// anything is written, so a write that fails or is over its cap loses none.
fn run_pass(
    memo: &mut PassMemo,
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    now: i64,
    stop: &AtomicBool,
    last_line: LastLine,
) -> Result<Pass> {
    let dir = ensure_tokens_dir(work_root)?;
    let (mut checkpoint, load_note) = memo.take_checkpoint(&dir);
    if last_line == LastLine::Count {
        // The last pass looks every missed thread up again, however recent
        // the miss: a macOS native worker's rollout reaches the host sessions
        // tree only when its boundary is reclaimed, after the passes that
        // missed it.
        checkpoint
            .threads
            .retain(|_, lookup| lookup.rollout.is_some());
    }
    let sources = discover(work_root, codex_sessions_root, &mut checkpoint.threads, now)?;
    let mut problems: Vec<String> = load_note.into_iter().collect();
    let (advanced, read_failure) = read_sources(&mut checkpoint, &sources, stop, last_line);
    let incomplete = read_failure.clone();
    problems.extend(read_failure);
    if last_line == LastLine::Count {
        super::plan_totals::register(work_root, &checkpoint.stages);
    }
    checkpoint.prune(now.div_euclid(BUCKET_SECS) - (STAGE_SERIES_LEN as i64 - 1));
    let mut flow = build_flow(&checkpoint.stages, now);
    flow.quota_left = memo.quota_left(work_root, now);
    let saved = memo.save_checkpoint(checkpoint, &dir, &advanced);
    let written = memo.publish_flow(&dir, &mut flow, now);
    problems.extend(writes_outcome(saved, written)?);
    let warning = (!problems.is_empty()).then(|| problems.join("; "));
    Ok(Pass {
        flow,
        warning,
        incomplete,
    })
}

/// The warnings of the two writes, or their errors: both when both failed, so
/// a failed `flow.json` does not hide a checkpoint that could not be saved.
fn writes_outcome(saved: Result<Option<String>>, written: Result<bool>) -> Result<Vec<String>> {
    match (saved, written) {
        (Ok(save_warning), Ok(flow_written)) => {
            let flow_warning = (!flow_written).then(|| {
                format!(
                    "flow over {MAX_FLOW_BYTES} bytes without idle series; {FLOW_FILE} not written"
                )
            });
            Ok(save_warning.into_iter().chain(flow_warning).collect())
        }
        (Err(saved), Err(written)) => bail!("{written:#}; {saved:#}"),
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
    }
}

/// `<work_root>/tokens`, created with mode 0700; see [`ensure_dir`].
fn ensure_tokens_dir(work_root: &Path) -> Result<PathBuf> {
    ensure_dir(&work_root.join(TOKENS_DIR))
}

/// `dir`, created with mode 0700. A symlink there is refused before anything
/// is created: `create_dir` alone would accept a symlink to a directory, and
/// `reject_symlink` checks only the last component.
pub(super) fn ensure_dir(dir: &Path) -> Result<PathBuf> {
    reject_symlink(dir)?;
    create_dir(dir)?;
    let metadata =
        std::fs::symlink_metadata(dir).with_context(|| format!("inspecting {}", dir.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("token flow path is not a directory: {}", dir.display());
    }
    Ok(dir.to_path_buf())
}

/// Writes `flow` atomically when it fits [`MAX_FLOW_BYTES`] (after shedding
/// idle series); `false` when it does not and nothing was written.
pub(super) fn write_flow(dir: &Path, flow: &mut TokenFlow) -> Result<bool> {
    let Some(json) = fit_flow(flow)? else {
        return Ok(false);
    };
    atomic_write(&dir.join(FLOW_FILE), &json).context("writing the token flow")?;
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

/// One [`scan_pass`] at the current time; the problem to log, if any.
fn guarded_pass(work_root: &Path) -> Option<String> {
    let now = chrono::Utc::now().timestamp();
    match guarded(|| scan_pass(work_root, None, now)) {
        Ok(_) => TRACKER.with_borrow_mut(|tracker| tracker.warning.take()),
        Err(problem) => Some(problem),
    }
}

/// `pass`'s value, or the problem to log. A panic is caught and reported like
/// an error, so the thread outlives it.
pub(super) fn guarded<T>(pass: impl FnOnce() -> Result<T>) -> std::result::Result<T, String> {
    match panic::catch_unwind(AssertUnwindSafe(pass)) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(format!("{error:#}")),
        Err(_) => Err("scan pass panicked".to_string()),
    }
}

/// Logs `problem` unless it repeats the one logged last; a clean pass clears
/// the memory, so a problem that comes back is logged again.
pub(super) fn log_change(last_logged: &mut Option<String>, problem: Option<String>) {
    if let Some(message) = &problem {
        if last_logged.as_ref() != Some(message) {
            eprintln!("tokens: {message}");
        }
    }
    *last_logged = problem;
}

pub(super) fn sleep_unless_shutdown(shutdown: &AtomicBool, total: Duration) {
    let deadline = Instant::now() + total;
    while !shutdown.load(Ordering::SeqCst) && Instant::now() < deadline {
        thread::sleep(SLEEP_SLICE);
    }
}

#[cfg(test)]
#[path = "tracker_tests.rs"]
mod tests;
