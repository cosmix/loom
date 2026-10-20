//! Guidance for a stage whose merge needs attention: a conflict, an error, or
//! a typed merge block.

use crate::commands::status::data::{ActivityStatus, StageSummary};
use crate::models::session::SessionBackendKind;
use crate::orchestrator::core::MAX_MERGE_RESOLVER_ATTEMPTS;

use super::Guidance;

/// The daemon spawns a merge resolver for a conflict and for a merge error
/// without a typed block, up to [`MAX_MERGE_RESOLVER_ATTEMPTS`], then routes
/// the stage to human review.
///
/// A live resolver is told apart from one whose turn ended by its tool calls:
/// `Stale` is no tool call for the stage's `subagent_timeout_secs` budget with
/// no call open, measured from its own heartbeat or from its spawn. A resolver
/// that ended its turn without running `loom stage merge --resolved` stays
/// alive and waits at its prompt forever, so that state is the operator's to
/// act on, not the daemon's. One command running past the budget (an
/// acceptance rerun) reads `Busy` while its transcript shows the call open, and
/// `Quiet` inside the budget; neither is an entry. Stale can still be a long
/// call the transcript did not show, so the note sends the operator to look
/// before it offers the kill, which spends a resolver attempt.
pub(super) fn merge_guidance(stage: &StageSummary) -> Guidance {
    let used = stage.merge_resolver_attempts.unwrap_or(0);
    let max = MAX_MERGE_RESOLVER_ATTEMPTS;
    let Some(session) = stage.merge_resolver_session.as_deref() else {
        return Guidance::automatic(format!(
            "waiting for the daemon to start a merge resolver ({used} of {max} attempts used)"
        ));
    };
    let view = resolver_view(stage);
    if stage.activity_status != ActivityStatus::Stale {
        let view = view.map_or_else(String::new, |view| format!("; {view}"));
        return Guidance::automatic(format!(
            "merge resolver {session} is working (attempt {used} of {max}){view}"
        ));
    }
    let minutes = stage.staleness_secs.unwrap_or(0) / 60;
    let look = view.map_or_else(String::new, |view| format!(" Look first: {view}."));
    Guidance::manual(format!(
        "merge resolver {session} made no tool call for {minutes} min and has not run loom stage \
         merge {} --resolved: it may be inside one long command or may have stopped (attempt \
         {used} of {max}).{look} Only if it is stuck, loom sessions kill {session} ends it and \
         the daemon starts the next attempt",
        stage.id
    ))
}

/// How to look at the resolver: `loom attach` reaches a tmux session, and the
/// web terminal follows the same session; a native session runs in a terminal
/// window titled with its tracking key.
fn resolver_view(stage: &StageSummary) -> Option<String> {
    Some(match stage.session_backend? {
        SessionBackendKind::Tmux => format!(
            "run loom attach {} to watch it, or open the stage's terminal in the web UI",
            stage.id
        ),
        SessionBackendKind::Native => {
            format!(
                "it runs in the terminal window titled loom-merge-{}",
                stage.id
            )
        }
    })
}

/// A typed block never gets a merge resolver: the daemon retries the merge
/// every tick and lands it once the block's cause is gone.
pub(super) fn merge_block_guidance(sentence: &str) -> Guidance {
    Guidance::automatic(format!(
        "{sentence}; loom retries the merge automatically once that changes"
    ))
}
