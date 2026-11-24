//! Contract: a second adjudication request while the stage needs adjudication joins its reason
//! to the first and archives nothing as superseded, since the first dispute is still open.

use super::{IssueDisposition, Stage, StageStatus};

#[test]
fn a_second_filing_does_not_supersede_the_first() {
    let mut stage = Stage::default();
    stage.status = StageStatus::Executing;

    stage
        .try_request_adjudication(Some("first reason".into()))
        .expect("the first filing from Executing");
    stage
        .try_request_adjudication(Some("second reason".into()))
        .expect("a second filing while the stage needs adjudication");

    let reason = stage.review_reason.as_deref().unwrap_or("");
    for expected in ["first reason", "second reason"] {
        assert!(
            reason.contains(expected),
            "review_reason lacks {expected:?}: {reason:?}"
        );
    }
    let superseded: Vec<_> = stage
        .issue_history
        .iter()
        .filter(|issue| issue.disposition == Some(IssueDisposition::Superseded))
        .collect();
    assert!(
        superseded.is_empty(),
        "an open dispute's reason was archived as superseded: {superseded:?}"
    );
}
