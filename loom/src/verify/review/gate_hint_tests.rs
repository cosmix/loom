//! The reviewer-spawn hint the review gate appends from the stage's
//! `starts.jsonl` and `stop-skips.jsonl` ledgers.

use super::tests::{fixture, STAGE};
use super::*;
use crate::verify::review::fingerprint::ChangeFingerprint;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Writes the stage's ledger `name`, one JSON object per row.
fn ledger(work_dir: &Path, name: &str, rows: &[Value]) {
    let dir = work_dir.join("subagents").join(STAGE);
    std::fs::create_dir_all(&dir).unwrap();
    let text: Vec<String> = rows.iter().map(Value::to_string).collect();
    std::fs::write(dir.join(name), text.join("\n")).unwrap();
}

fn start(agent_id: &str, agent_type: &str) -> Value {
    json!({ "agent_id": agent_id, "agent_type": agent_type })
}

fn skip(agent_id: &str, reason: &str) -> Value {
    json!({ "agent_id": agent_id, "agent_type": "loom-code-reviewer", "reason": reason })
}

fn reviewer_starts(ids: &[&str]) -> Vec<Value> {
    ids.iter()
        .map(|id| start(id, "loom-code-reviewer"))
        .collect()
}

#[test]
fn harvest_hint_counts_spawns_without_rounds() {
    let fx = fixture();
    ledger(
        &fx.work_dir,
        "starts.jsonl",
        &reviewer_starts(&["a1", "a2", "a3", "a3"]),
    );
    ledger(
        &fx.work_dir,
        "stop-skips.jsonl",
        &[skip("a2", "no_review_block")],
    );

    let hint = harvest_hint(&fx.work_dir, STAGE, 1).unwrap();

    let counts = "3 reviewer spawns, 1 rounds, 2 stop events not harvested.";
    assert!(
        hint.starts_with(&format!("Reviewer stop events: {counts}")),
        "{hint}"
    );
    assert!(
        hint.contains("\n  skipped stop: agent a2: no_review_block\n"),
        "{hint}"
    );
    assert!(
        hint.ends_with("why the SubagentStop hook skipped a stop."),
        "{hint}"
    );
}

#[test]
fn harvest_hint_is_none_when_rounds_cover_spawns() {
    let fx = fixture();
    ledger(&fx.work_dir, "starts.jsonl", &reviewer_starts(&["a1"]));

    assert_eq!(harvest_hint(&fx.work_dir, STAGE, 1), None);
    assert_eq!(harvest_hint(&fx.work_dir, "no-such-stage", 0), None);
}

#[test]
fn harvest_hint_ignores_non_reviewer_spawns() {
    let fx = fixture();
    let rows = [
        start("w1", "loom-software-engineer"),
        start("w2", "loom-code-reviewer-x"),
    ];
    ledger(&fx.work_dir, "starts.jsonl", &rows);

    assert_eq!(harvest_hint(&fx.work_dir, STAGE, 0), None);
}

#[test]
fn harvest_hint_sanitizes_ledger_text() {
    let fx = fixture();
    ledger(&fx.work_dir, "starts.jsonl", &reviewer_starts(&["a1"]));
    let reason = format!("bad reason\nwith\u{1b}[0m escapes{}", "x".repeat(100));
    ledger(
        &fx.work_dir,
        "stop-skips.jsonl",
        &[skip("a 1\u{1b}[31m", &reason)],
    );

    let hint = harvest_hint(&fx.work_dir, STAGE, 0).unwrap();

    let line = hint.lines().nth(1).unwrap();
    assert!(
        line.starts_with("  skipped stop: agent a131m: badreasonwith0mescapesx"),
        "{line}"
    );
    assert!(!hint.contains('\u{1b}'), "{hint}");
    assert!(
        line.len() <= "  skipped stop: agent : ".len() + 2 * 64,
        "{line}"
    );
}

#[test]
fn gate_failure_message_carries_the_harvest_hint() {
    let fx = fixture();
    ledger(
        &fx.work_dir,
        "starts.jsonl",
        &reviewer_starts(&["a1", "a2"]),
    );
    ledger(
        &fx.work_dir,
        "stop-skips.jsonl",
        &[skip("a1", "no_review_block")],
    );
    let current = ChangeFingerprint {
        value: "sha256:x".to_string(),
        base: "b".to_string(),
        files: BTreeMap::new(),
    };

    let error = check(&fx.stage, &fx.work_dir, &current).unwrap_err();

    let message = format!("{error:#}");
    let counts = "2 reviewer spawns, 0 rounds, 2 stop events not harvested";
    assert!(message.contains(counts), "{message}");
    assert!(message.contains("no_review_block"), "{message}");
}

#[test]
fn harvest_hint_treats_a_fifo_ledger_as_absent() {
    let fx = fixture();
    let dir = fx.work_dir.join("subagents").join(STAGE);
    std::fs::create_dir_all(&dir).unwrap();
    nix::unistd::mkfifo(&dir.join("starts.jsonl"), nix::sys::stat::Mode::S_IRWXU).unwrap();

    assert_eq!(harvest_hint(&fx.work_dir, STAGE, 0), None);
}
