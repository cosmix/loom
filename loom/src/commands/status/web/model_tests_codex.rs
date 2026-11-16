//! The server stage's native Codex jobs for [`super::super::fixture_snapshot`].
use crate::commands::status::data::CodexJob;
use crate::native_worker::model::{Outcome, Phase};
use chrono::{TimeZone, Utc};

/// One succeeded job with its thread and one that failed before any thread started.
pub(super) fn fixture_codex_jobs() -> Vec<CodexJob> {
    let at = |h, m, s, ms| {
        Utc.with_ymd_and_hms(2026, 11, 16, h, m, s)
            .single()
            .unwrap()
            + chrono::Duration::milliseconds(ms)
    };
    let job = |unit: &str, invocation: &str| CodexJob {
        unit_id: unit.to_owned(),
        invocation_id: invocation.to_owned(),
        model: "gpt-6.1-sol".to_owned(),
        effort: "xhigh".to_owned(),
        phase: Phase::Terminal,
        outcome: None,
        outcome_reason: None,
        thread_id: None,
        started_at: at(18, 23, 55, 94),
        ended_at: None,
        deadline_secs: 1500,
    };
    vec![
        CodexJob {
            outcome: Some(Outcome::Succeeded),
            outcome_reason: Some("turn completed".to_owned()),
            thread_id: Some("01a1396b-099e-7ef2-92c0-52e2b3314eb2".to_owned()),
            ended_at: Some(at(18, 24, 18, 813)),
            ..job("hex-test", "inv-b63fd09ed5d66b5899fcaef4a521ffcd")
        },
        CodexJob {
            outcome: Some(Outcome::Failed),
            outcome_reason: Some("codex exited before starting a thread".to_owned()),
            started_at: at(18, 25, 2, 411),
            ended_at: Some(at(18, 25, 4, 911)),
            ..job("hex-lint", "inv-5c1e0a7d2f4b49c0a1e3b6d8f9072a14")
        },
    ]
}
