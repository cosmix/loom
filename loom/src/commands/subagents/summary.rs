//! Builds summaries after classification has separated liveness from optional
//! transcript metadata. Keeping construction here preserves short, legible
//! classifier branches without making either output shape implicit.

use super::classify::{SubagentState, SubagentSummary};
use super::metrics::TranscriptMetrics;

/// Transcript-derived activity that [`TranscriptMetrics`] doesn't cover: how
/// many assistant turns appeared, the most recently invoked tool, and the
/// subagent's final report text (only ever `Some` when `state == Done`).
/// Grouped solely to keep [`with_last`]'s argument count under clippy's
/// `too_many_arguments` threshold -- these three share no concept beyond
/// "derived by scanning the parsed transcript entries".
pub(super) struct TranscriptActivity {
    pub(super) turns: usize,
    pub(super) last_tool: Option<String>,
    pub(super) final_report: Option<String>,
}

pub(super) fn with_last(
    agent_id: String,
    state: SubagentState,
    idle_secs: i64,
    activity: TranscriptActivity,
    agent_type: Option<String>,
    metrics: TranscriptMetrics,
    peak_tokens_over_ceiling: bool,
) -> SubagentSummary {
    SubagentSummary {
        agent_id,
        state,
        idle_secs,
        turns: activity.turns,
        last_tool: activity.last_tool,
        agent_type,
        model: metrics.model,
        request_count: Some(metrics.request_count),
        peak_resident_tokens: metrics.peak_resident_tokens,
        peak_tokens_over_ceiling,
        final_report: activity.final_report,
    }
}

/// Preserve metadata even for an empty or wholly malformed transcript: hooks
/// may have recorded a spawn before Claude writes the first usable row. Only
/// ever called when the transcript has no parseable entry at all, so every
/// transcript-derived field -- model, request count, peak tokens -- is
/// unconditionally absent; `agent_type` is the sole field a caller can supply.
pub(super) fn empty(
    agent_id: String,
    authoritative_done: bool,
    idle_secs: i64,
    agent_type: Option<String>,
) -> SubagentSummary {
    SubagentSummary {
        agent_id,
        state: if authoritative_done {
            SubagentState::Done
        } else {
            SubagentState::Unknown
        },
        idle_secs,
        turns: 0,
        last_tool: None,
        agent_type,
        model: None,
        request_count: None,
        peak_resident_tokens: None,
        peak_tokens_over_ceiling: false,
        final_report: None,
    }
}
