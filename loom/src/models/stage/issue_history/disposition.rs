use super::{IssueDisposition as D, StageStatus as S};

pub(super) fn transition_disposition(from: &S, to: &S) -> D {
    match to {
        S::WaitingForDeps => D::Reset,
        S::Skipped => D::Skipped,
        S::MergeConflict => D::Superseded,
        S::Queued | S::Executing if matches!(from, S::Blocked | S::CompletedWithFailures) => {
            D::Retried
        }
        _ => D::Resolved,
    }
}

fn old_status(value: &str) -> Option<S> {
    Some(match value {
        "WaitingForDeps" => S::WaitingForDeps,
        "Queued" => S::Queued,
        "Executing" => S::Executing,
        "WaitingForInput" => S::WaitingForInput,
        "Blocked" => S::Blocked,
        "Completed" => S::Completed,
        "NeedsHandoff" => S::NeedsHandoff,
        "Skipped" => S::Skipped,
        "MergeConflict" => S::MergeConflict,
        "CompletedWithFailures" => S::CompletedWithFailures,
        "MergeBlocked" => S::MergeBlocked,
        "NeedsHumanReview" => S::NeedsHumanReview,
        "NeedsAdjudication" => S::NeedsAdjudication,
        _ => return None,
    })
}

/// Interpret historical explanations only at the status compatibility boundary.
pub fn legacy_disposition(value: &str) -> D {
    if let Some((from, to)) = value.split_once(" -> ") {
        return match (old_status(from), old_status(to)) {
            (Some(from), Some(to)) => transition_disposition(&from, &to),
            _ => D::Unknown,
        };
    }
    if let Some(verdict) = value.strip_prefix("adjudicator verdict: ") {
        return match verdict {
            "accept" => D::Accepted,
            "reject" => D::Rejected,
            "needs-more-evidence" => D::NeedsEvidence,
            "rulings" => D::Ruled,
            _ => D::Unknown,
        };
    }
    if value.starts_with("loom stage reset") {
        D::Reset
    } else if value.starts_with("loom stage retry") {
        D::Retried
    } else if value.starts_with("replaced by ") || value == "blocked by loom stage block" {
        D::Superseded
    } else if value == "human review rejected" {
        D::Rejected
    } else if value.starts_with("closed ") || value.starts_with("ended without ") {
        D::Closed
    } else if matches!(value, "merge block cleared" | "adjudicator verdict applied") {
        D::Resolved
    } else {
        D::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_endings_keep_known_meaning_and_reject_unknowns() {
        for (text, expected) in [
            ("Blocked -> WaitingForDeps", D::Reset),
            ("loom stage reset", D::Reset),
            ("Blocked -> Queued", D::Retried),
            ("CompletedWithFailures -> Queued", D::Retried),
            ("Blocked -> Executing", D::Retried),
            ("loom stage retry --force", D::Retried),
            ("NeedsHumanReview -> Queued", D::Resolved),
            ("MergeBlocked -> Queued", D::Resolved),
            ("MergeConflict -> Completed", D::Resolved),
            ("NeedsHumanReview -> Completed", D::Resolved),
            ("merge block cleared", D::Resolved),
            ("adjudicator verdict applied", D::Resolved),
            ("MergeBlocked -> MergeConflict", D::Superseded),
            ("Blocked -> Skipped", D::Skipped),
            ("replaced by a newer failure", D::Superseded),
            ("replaced by a newer review reason", D::Superseded),
            ("blocked by loom stage block", D::Superseded),
            ("human review rejected", D::Rejected),
            ("adjudicator verdict: accept", D::Accepted),
            ("adjudicator verdict: reject", D::Rejected),
            ("adjudicator verdict: needs-more-evidence", D::NeedsEvidence),
            ("adjudicator verdict: rulings", D::Ruled),
            ("closed without a verdict", D::Closed),
            ("closed before its verdict was applied", D::Closed),
            ("ended without a readable verdict", D::Closed),
            ("unrecognized explanation", D::Unknown),
            ("Blocked -> Imaginary", D::Unknown),
        ] {
            assert_eq!(legacy_disposition(text), expected, "{text}");
        }
    }
}
