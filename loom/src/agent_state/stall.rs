//! The Claude stall rule table over one transcript's classified state.

use super::progress::SubagentState;

/// Floor under the stall threshold for a `Bash` tool-wait.
///
/// Deliberately NOT the Bash tool's nominal 600-second cap: a call held at a
/// permission prompt has not started running and is not subject to it, so the
/// cap does not bound what a transcript can show. Measured over 19,007 real
/// `Bash` tool-waits in this machine's own subagent transcripts: 71 ran past
/// 600s, 11 past 660s, and the longest reached 1,298s. 1,800s is the first
/// round number above that maximum -- the same convention, and the same cost
/// asymmetry, as [`super::progress::DEFAULT_DONE_DEBOUNCE_SECS`]. A
/// false stall would have an orchestrator kill and re-dispatch onto a LIVE
/// worker's file set, so the threshold buys headroom with latency that only
/// ever applies to an already-hung worker.
const BASH_STALL_FLOOR_SECS: i64 = 1_800;

/// The Claude stall rule table, over one transcript's classified state.
pub fn stall_reason(
    state: SubagentState,
    idle_secs: i64,
    last_tool: Option<&str>,
    budget_secs: i64,
) -> Option<String> {
    match state {
        SubagentState::Done if idle_secs > budget_secs => Some(format!(
            "turn ended {idle_secs}s ago with no SubagentStop record; harvest its report"
        )),
        SubagentState::Generating if idle_secs > budget_secs => Some(format!(
            "no transcript growth for {idle_secs}s while generating (stall budget {budget_secs}s)"
        )),
        SubagentState::ToolWait => tool_wait_reason(idle_secs, last_tool, budget_secs),
        // `Failed`, `Cancelled` and `Unknown` belong to the lifecycle index, and
        // the forward states belong to the forwarded job's own evidence; a
        // still-running `Done` or `Generating` worker is simply making progress.
        SubagentState::Done
        | SubagentState::Generating
        | SubagentState::Failed
        | SubagentState::Cancelled
        | SubagentState::Unknown
        | SubagentState::ForwardWait
        | SubagentState::ForwardFailed
        | SubagentState::ForwardUnknown => None,
    }
}

fn tool_wait_reason(idle_secs: i64, last_tool: Option<&str>, budget_secs: i64) -> Option<String> {
    match last_tool {
        // A nested spawn runs as long as its own work takes, with no cap to
        // measure it against.
        Some("Agent" | "Task") => None,
        Some("Bash") => {
            let threshold = budget_secs.max(BASH_STALL_FLOOR_SECS);
            (idle_secs > threshold).then(|| {
                format!(
                    "Bash call outstanding {idle_secs}s, past the configured stall threshold of {threshold}s"
                )
            })
        }
        tool => (idle_secs > budget_secs).then(|| {
            let tool = tool.unwrap_or("an unnamed tool");
            format!("waiting on {tool} for {idle_secs}s (stall budget {budget_secs}s)")
        }),
    }
}
