//! Why a harvest recorded no round, in the form `subagent-stop.sh` reads back.

use std::fmt;

/// The stderr prefix `subagent-stop.sh` reads a harvest's cause from.
pub(super) const CAUSE_PREFIX: &str = "loom-review-harvest-cause=";

/// A failure the review gate can act on.
#[derive(Debug, Clone, Copy)]
pub(super) enum HarvestFailure {
    TranscriptUnusable,
    NoStartEvidence,
    WorktreeChanged,
}

impl HarvestFailure {
    /// The token `subagent-stop.sh` appends to the stop-skips row's reason.
    fn cause(self) -> &'static str {
        match self {
            Self::TranscriptUnusable => "transcript_unusable",
            Self::NoStartEvidence => "no_start_evidence",
            Self::WorktreeChanged => "worktree_changed",
        }
    }
}

impl fmt::Display for HarvestFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TranscriptUnusable => "this stop's reviewer transcript cannot be used",
            Self::NoStartEvidence => {
                "this reviewer has no usable start evidence; launch a new review"
            }
            Self::WorktreeChanged => {
                "the worktree changed since this reviewer started; launch a new review"
            }
        })
    }
}

impl std::error::Error for HarvestFailure {}

/// The cause token of a failed harvest; `None` for a failure outside
/// [`HarvestFailure`].
pub(super) fn failure_cause(error: &anyhow::Error) -> Option<&'static str> {
    error
        .downcast_ref::<HarvestFailure>()
        .map(|failure| failure.cause())
}
