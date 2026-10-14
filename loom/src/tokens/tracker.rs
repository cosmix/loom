//! The `tokens-tracker` daemon thread and the scan pass it runs: read the
//! lines every stage's transcripts gained, update the checkpoint, and publish
//! `.loom/work/tokens/flow.json` for the status collector.

use super::checkpoint::{Checkpoint, Saved, MAX_CHECKPOINT_BYTES};
use super::discover::{discover, Source};
use super::flow_build::{build_flow, fit_flow};
use super::model::{
    TokenFlow, BUCKET_SECS, FLOW_FILE, MAX_FLOW_BYTES, STAGE_SERIES_LEN, TOKENS_DIR,
};
use super::quota_points::quota_left;
use super::tail::{self, Line};
use crate::context::untrusted::inline_safe;
use crate::quota::cache::{atomic_write, create_quota_dir, reject_symlink};
use anyhow::{bail, Context, Result};
use std::cell::RefCell;
use std::collections::HashSet;
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
/// is skipped and reported as the pass's warning, as is a checkpoint or flow
/// that is too big to write. A checkpoint or flow that cannot be written fails
/// the pass, after the other is still written, naming both when both fail.
fn scan_until(
    work_root: &Path,
    codex_sessions_root: Option<&Path>,
    now: i64,
    stop: &AtomicBool,
) -> Result<Pass> {
    let dir = ensure_tokens_dir(work_root)?;
    let (mut checkpoint, load_note) = Checkpoint::load_reporting(&dir);
    let sources = discover(work_root, codex_sessions_root, &mut checkpoint.threads, now)?;
    let mut problems: Vec<String> = load_note.into_iter().collect();
    let (advanced, read_failure) = read_sources(&mut checkpoint, &sources, stop);
    problems.extend(read_failure);
    checkpoint.prune(now.div_euclid(BUCKET_SECS) - (STAGE_SERIES_LEN as i64 - 1));
    let mut flow = build_flow(&checkpoint.stages, now);
    flow.quota_left = quota_left(work_root, now);
    let saved = save_checkpoint(&mut checkpoint, &dir, &advanced);
    let written = write_flow(&dir, &mut flow);
    problems.extend(writes_outcome(saved, written)?);
    let warning = (!problems.is_empty()).then(|| problems.join("; "));
    Ok(Pass { flow, warning })
}

/// Reads every source until `stop` is set. Returns the `(stage id, agent key)`
/// of each agent whose file was read, and the first file that could not be.
fn read_sources<'a>(
    checkpoint: &mut Checkpoint,
    sources: &'a [Source],
    stop: &AtomicBool,
) -> (HashSet<(&'a str, &'a str)>, Option<String>) {
    let mut advanced = HashSet::new();
    let mut failure = None;
    for source in sources {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match read_source(checkpoint, source, stop) {
            Ok(false) => continue,
            Ok(true) => {}
            // A read that failed part way may still have moved the offset.
            Err(error) => {
                failure.get_or_insert_with(|| {
                    format!("cannot read {}: {error}", inline_safe(&source.path))
                });
            }
        }
        advanced.insert((source.stage_id.as_str(), source.key.as_str()));
    }
    (advanced, failure)
}

/// Reads the lines `source`'s file gained since the checkpoint, and says
/// whether there were any. A missing file is skipped and its agent keeps its
/// totals; a new inode, a shrink below the saved offset, or a different path
/// restarts the agent from offset 0. A line over [`tail::MAX_LINE_BYTES`] is
/// skipped and counted as unreadable.
fn read_source(
    checkpoint: &mut Checkpoint,
    source: &Source,
    stop: &AtomicBool,
) -> std::io::Result<bool> {
    let path = Path::new(&source.path);
    let Some(stat) = tail::stat(path) else {
        return Ok(false);
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
        return Ok(false);
    }
    let mut offset = agent.file.offset;
    let read = tail::read_lines(path, &mut offset, stat.len, stop, |line| match line {
        Line::Text(text) => agent.ingest(text),
        Line::TooLong => agent.count_unreadable(),
    });
    agent.file.offset = offset;
    read.map(|()| true)
}

/// Saves `checkpoint`. Over [`MAX_CHECKPOINT_BYTES`], first empties the message
/// tables of every agent not in `advanced` (their files did not change this
/// pass) and tries again. The warning when it is still over: nothing is
/// written, and the next pass starts from the checkpoint before this one.
fn save_checkpoint(
    checkpoint: &mut Checkpoint,
    dir: &Path,
    advanced: &HashSet<(&str, &str)>,
) -> Result<Option<String>> {
    let mut saved = checkpoint.save(dir)?;
    if matches!(saved, Saved::OverCap(_)) {
        checkpoint.clear_tables_except(advanced);
        saved = checkpoint.save(dir)?;
    }
    Ok(match saved {
        Saved::Written => None,
        Saved::OverCap(bytes) => Some(format!(
            "checkpoint is {bytes} bytes, over {MAX_CHECKPOINT_BYTES} even without idle \
             message tables; {CHECKPOINT_FILE} not written"
        )),
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
