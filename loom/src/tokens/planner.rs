//! The planner's token pass: the per-agent token figures of the plan chat
//! session, read by the same reader, flow builder and checkpoint the daemon
//! uses for stages. The checkpoint lives in a caller-given tokens directory
//! and the pass publishes no `flow.json`, so the daemon's output is untouched.

use super::discover::{main_source, subagent_source, Source};
use super::flow_build::build_flow;
use super::model::{AgentFlow, BUCKET_SECS, STAGE_SERIES_LEN};
use super::pass_memo::PassMemo;
use super::tracker::{
    ensure_dir, guarded, log_change, read_sources, sleep_unless_shutdown, TRACKER,
    TRACKER_INTERVAL_SECS,
};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// The agent rows of each group: `(group id, rows)`, in group order.
pub type GroupFlows = Vec<(String, Vec<AgentFlow>)>;

/// One chat session's transcripts: the main transcript and the subagents to
/// read, as `(agent id, transcript path, agent type)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerGroup {
    pub id: String,
    pub main: PathBuf,
    pub subagents: Vec<(String, PathBuf, Option<String>)>,
}

/// The agent rows of each group in `groups`, in group order, at `now`.
///
/// Reads exactly the files the groups name, keeps the read offsets in
/// `<tokens_dir>/checkpoint.json` and drops a group the caller no longer
/// passes from it. A group none of whose files exist yet has no entry in the
/// result. Writes no `flow.json` and computes no `quota_left`.
///
/// On a `planner-tokens` thread the pass stops reading files once the
/// thread's stop flag is set; a problem it worked around is left as the
/// thread's warning, through the thread-local `TRACKER`.
pub fn planner_pass(tokens_dir: &Path, groups: &[PlannerGroup], now: i64) -> Result<GroupFlows> {
    let stop = TRACKER.with_borrow(|tracker| tracker.stop.clone());
    let never = AtomicBool::new(false);
    let stop = stop.as_deref().unwrap_or(&never);
    let mut memo = TRACKER
        .with_borrow_mut(|tracker| tracker.memo.take())
        .filter(|memo| memo.is_for(tokens_dir))
        .unwrap_or_else(|| PassMemo::new(tokens_dir));
    let pass = run_planner_pass(&mut memo, tokens_dir, groups, now, stop);
    TRACKER.with_borrow_mut(|tracker| {
        tracker.memo = Some(memo);
        tracker.warning = pass.as_ref().ok().and_then(|(_, warning)| warning.clone());
    });
    pass.map(|(flows, _)| flows)
}

/// [`planner_pass`] with `memo`: the rows and the pass's warning, if any.
fn run_planner_pass(
    memo: &mut PassMemo,
    tokens_dir: &Path,
    groups: &[PlannerGroup],
    now: i64,
    stop: &AtomicBool,
) -> Result<(GroupFlows, Option<String>)> {
    let dir = ensure_dir(tokens_dir)?;
    let sources: Vec<Source> = groups.iter().flat_map(group_sources).collect();
    let (mut checkpoint, load_note) = memo.take_checkpoint(&dir);
    checkpoint.retain_stages(|id| groups.iter().any(|group| group.id == id));
    let mut problems: Vec<String> = load_note.into_iter().collect();
    let (advanced, read_failure) = read_sources(&mut checkpoint, &sources, stop);
    problems.extend(read_failure);
    checkpoint.prune(now.div_euclid(BUCKET_SECS) - (STAGE_SERIES_LEN as i64 - 1));
    let flow = build_flow(&checkpoint.stages, now);
    let saved = memo.save_checkpoint(checkpoint, &dir, &advanced)?;
    problems.extend(saved);
    let mut rows = flow.stages;
    let flows = groups
        .iter()
        .filter_map(|group| {
            let at = rows.iter().position(|stage| stage.stage_id == group.id)?;
            Some((group.id.clone(), rows.swap_remove(at).agents))
        })
        .collect();
    let warning = (!problems.is_empty()).then(|| problems.join("; "));
    Ok((flows, warning))
}

/// The sources of `group`: its main transcript and its subagents, each only
/// when its id passes the same safety rules as the daemon's sources.
fn group_sources(group: &PlannerGroup) -> Vec<Source> {
    let main = group
        .main
        .to_str()
        .and_then(|main| main_source(&group.id, main));
    let subagents = group.subagents.iter().filter_map(|(id, path, agent_type)| {
        subagent_source(&group.id, id, path, agent_type.clone())
    });
    main.into_iter().chain(subagents).collect()
}

/// Starts the `planner-tokens` thread: a pass now, then one every
/// [`TRACKER_INTERVAL_SECS`], until `stop` is set; a pass already running stops
/// reading files when it is. Each pass asks `groups` for the groups to read and
/// hands the rows of a pass that succeeded to `sink`, after the pass returns.
/// A failed pass is logged only when its message differs from the last one.
pub fn spawn_planner_tokens(
    tokens_dir: PathBuf,
    groups: Box<dyn Fn() -> Vec<PlannerGroup> + Send>,
    sink: Box<dyn Fn(GroupFlows) + Send>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("planner-tokens".to_string())
        .spawn(move || run_planner(&tokens_dir, &*groups, &*sink, &stop))
}

fn run_planner(
    tokens_dir: &Path,
    groups: &dyn Fn() -> Vec<PlannerGroup>,
    sink: &dyn Fn(GroupFlows),
    stop: &Arc<AtomicBool>,
) {
    TRACKER.with_borrow_mut(|tracker| tracker.stop = Some(Arc::clone(stop)));
    let mut last_logged = None;
    while !stop.load(Ordering::SeqCst) {
        let now = chrono::Utc::now().timestamp();
        let pass = guarded(|| planner_pass(tokens_dir, &groups(), now));
        let problem = match pass {
            Ok(flows) => {
                sink(flows);
                TRACKER.with_borrow_mut(|tracker| tracker.warning.take())
            }
            Err(problem) => Some(problem),
        };
        log_change(&mut last_logged, problem);
        sleep_unless_shutdown(stop, Duration::from_secs(TRACKER_INTERVAL_SECS));
    }
}

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;
