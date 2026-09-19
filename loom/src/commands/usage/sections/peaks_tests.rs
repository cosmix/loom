//! Tests for the peak-resident-context-by-scope section.

use super::*;
use crate::commands::usage::transcript::{Entry, Request, TokenUsage};

fn request(resident: u64) -> Request {
    Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: "claude-sonnet-5".to_owned(),
        usage: TokenUsage {
            input: resident,
            ..Default::default()
        },
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
        normalization: Default::default(),
    }
}

fn transcript(scope: Scope, residents: &[u64]) -> Transcript {
    Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "session-1".to_owned(),
        agent_id: Some("agent-1".to_owned()),
        agent_type: None,
        stage_id: None,
        loom_session_id: None,
        forward_receipt: None,
        forward_candidate: false,
        first_user_entry: None,
        entries: residents
            .iter()
            .map(|resident| Entry::Assistant(Box::new(request(*resident))))
            .collect(),
        diagnostics: Default::default(),
    }
}

/// A transcript with no in-window requests contributes nothing to its
/// scope, matching the same window-filtering fix as `lifecycle` and
/// `agents`.
#[test]
fn a_transcript_with_no_requests_is_excluded() {
    let empty = transcript(Scope::Subagent, &[]);
    let peaks = build(&[empty]);
    assert_eq!(peaks.subagent.count, 0);
}

/// The peak is the maximum resident size across a transcript's requests,
/// and p50/p90/max are computed over those peaks.
#[test]
fn peak_is_the_maximum_resident_size_per_transcript() {
    let low = transcript(Scope::Main, &[100, 300]);
    let high = transcript(Scope::Main, &[900]);
    let peaks = build(&[low, high]);
    assert_eq!(peaks.main.count, 2);
    assert_eq!(peaks.main.max, 900);
}

/// Shares above the 250k/400k thresholds count transcripts, not requests,
/// and use a strict `>` comparison.
#[test]
fn shares_above_thresholds_are_strict_and_per_transcript() {
    let at_threshold = transcript(Scope::Main, &[250_000]);
    let above_threshold = transcript(Scope::Main, &[250_001, 500_000]);
    let peaks = build(&[at_threshold, above_threshold]);
    assert_eq!(peaks.main.share_above_250k, 50.0);
    assert_eq!(peaks.main.share_above_400k, 50.0);
}

/// The main scope never reports a boot-dominated share.
#[test]
fn main_scope_has_no_boot_dominated_share() {
    let transcript = transcript(Scope::Main, &[100]);
    let peaks = build(&[transcript]);
    assert_eq!(peaks.main.boot_dominated_share, None);
}

/// A subagent whose peak never exceeds twice its first request's resident
/// size counts as boot-dominated; one that grows past it does not.
#[test]
fn subagent_boot_dominated_share_compares_peak_to_first_request() {
    let boot_dominated = transcript(Scope::Subagent, &[100, 150]);
    let grew_past_boot = transcript(Scope::Subagent, &[100, 300]);
    let peaks = build(&[boot_dominated, grew_past_boot]);
    assert_eq!(peaks.subagent.boot_dominated_share, Some(50.0));
}
