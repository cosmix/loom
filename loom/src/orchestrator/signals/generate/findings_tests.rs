//! Selector and path-reading tests for the Dependency Findings section.
//! Rendering and the cap live in `findings_render_tests.rs`, a child module
//! that shares these fixtures.

use chrono::{TimeZone, Utc};

use super::*;
use crate::fs::memory::{Receipt, ReceiptOutcome};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// A recipient stage with an explicit id and declared files.
fn stage(id: &str, files: &[&str]) -> Stage {
    let mut stage = Stage::new(id.to_string(), None);
    stage.id = id.to_string();
    stage.files = files.iter().map(|f| f.to_string()).collect();
    stage
}

fn entry(kind: MemoryEntryType, content: &str, evidence: &[&str]) -> MemoryEntry {
    MemoryEntry::new(kind, content.to_string())
        .with_evidence(evidence.iter().map(|e| e.to_string()).collect())
}

/// `entry` recorded `second` seconds after a fixed instant, so order is explicit.
fn at(mut entry: MemoryEntry, second: i64) -> MemoryEntry {
    entry.timestamp = Utc.timestamp_opt(1_700_000_000 + second, 0).unwrap();
    entry
}

fn decision(content: &str, context: &str, evidence: &[&str]) -> MemoryEntry {
    let mut decision = entry(MemoryEntryType::Decision, content, evidence);
    decision.context = Some(context.to_string());
    decision
}

/// A Completed dependency with a commit, no range and `entries`.
fn producer(id: &str, entries: Vec<MemoryEntry>) -> Producer {
    let mut stage = stage(id, &[]);
    stage.completed_commit = Some(COMMIT.to_string());
    let change = DependencyChange {
        stage,
        range: None,
        paths: Vec::new(),
    };
    Producer { change, entries }
}

fn settle(event: &MemoryEntry) -> MemoryEntry {
    MemoryEntry::receipt(
        Receipt {
            event_id: event.id.clone(),
            outcome: ReceiptOutcome::Discarded,
            target: None,
        },
        "settled".to_string(),
    )
}

/// The ids `select` keeps, in order, with no repository root.
fn picked(recipient: &Recipient, producers: &[Producer]) -> Vec<String> {
    let settled = settled_ids(producers.iter().flat_map(|p| &p.entries));
    select(recipient, producers, &settled, None)
        .iter()
        .map(|(_, entry)| entry.id.clone())
        .collect()
}

fn recipient(stage: &Stage) -> Recipient {
    Recipient::new(stage, None)
}

#[test]
fn a_declared_glob_keeps_a_matching_entry_and_drops_an_unrelated_one() {
    let hit = entry(MemoryEntryType::Note, "parser rule", &["src/x/a.rs:4"]);
    let miss = entry(MemoryEntryType::Note, "docs rule", &["docs/a.md"]);
    let producers = [producer("up", vec![hit.clone(), miss])];
    let b = stage("b", &["src/**/*.rs"]);
    assert_eq!(picked(&recipient(&b), &producers), vec![hit.id]);
}

#[test]
fn a_glob_star_does_not_cross_a_separator() {
    let deep = entry(MemoryEntryType::Note, "deep rule", &["src/x/a.rs"]);
    let producers = [producer("up", vec![deep])];
    let b = stage("b", &["src/*.rs"]);
    assert!(picked(&recipient(&b), &producers).is_empty());
}

#[test]
fn a_brief_path_keeps_a_matching_entry() {
    let hit = entry(MemoryEntryType::Note, "brief rule", &["./src/brief.rs:7-9"]);
    let producers = [producer("up", vec![hit.clone()])];
    let mut b = recipient(&stage("b", &[]));
    b.brief_paths.insert("src/brief.rs".to_string());
    assert_eq!(picked(&b, &producers), vec![hit.id]);
}

#[test]
fn the_recipient_id_in_content_or_context_keeps_an_entry() {
    let in_content = entry(MemoryEntryType::Note, "b-stage: call init", &[]);
    let in_context = decision("call init first", "breaks (b-stage)", &[]);
    let longer_id = entry(MemoryEntryType::Note, "b-stage-two needs it", &[]);
    let producers = [producer(
        "up",
        vec![in_content.clone(), in_context.clone(), longer_id],
    )];
    let kept = picked(&recipient(&stage("b-stage", &[])), &producers);
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert!(kept.contains(&in_content.id) && kept.contains(&in_context.id));
}

#[test]
fn excluded_entries_never_reach_the_recipient() {
    let file = ["src/b.rs"];
    let settled_note = entry(MemoryEntryType::Note, "settled note", &file);
    let settled_question = entry(MemoryEntryType::Question, "settled q", &file);
    let settled_decision = decision("settled decision", "why", &file);
    let journal = vec![
        entry(MemoryEntryType::Change, "edited", &file),
        entry(MemoryEntryType::Note, "  MUTATION: survived", &file),
        settle(&settled_note),
        settle(&settled_question),
        settle(&settled_decision),
        settled_note,
        settled_question,
        settled_decision.clone(),
    ];
    let producers = [producer("up", journal)];
    let kept = picked(&recipient(&stage("b", &file)), &producers);
    assert_eq!(kept, vec![settled_decision.id]);
}

#[test]
fn integration_verify_keeps_unrelated_entries_and_v2_drops_suggestions() {
    let unrelated = entry(MemoryEntryType::Note, "unrelated", &["x/y.rs"]);
    let suggestion = entry(MemoryEntryType::Suggestion, "rename it", &[]);
    let producers = [producer("up", vec![unrelated.clone(), suggestion.clone()])];
    let mut verify = stage("verify", &[]);
    verify.stage_type = StageType::IntegrationVerify;
    verify.plan_version = 1;
    let v1 = picked(&recipient(&verify), &producers);
    assert!(
        v1.contains(&unrelated.id) && v1.contains(&suggestion.id),
        "{v1:?}"
    );
    verify.plan_version = 2;
    assert_eq!(picked(&recipient(&verify), &producers), vec![unrelated.id]);
}

#[test]
fn order_is_naming_then_ranked_notes_then_reasoned_decisions_then_the_rest() {
    let file = ["src/b.rs"];
    let plain = at(entry(MemoryEntryType::Decision, "plain", &file), 0);
    let reasoned = at(decision("reasoned", "because", &file), 1);
    let found = at(entry(MemoryEntryType::Note, "found: x", &file), 5);
    let mistake = at(entry(MemoryEntryType::Note, "Mistake: y", &file), 2);
    let both = at(entry(MemoryEntryType::Note, "found/gotcha: z", &file), 4);
    let gotcha = at(entry(MemoryEntryType::Note, " GOTCHA: w", &file), 3);
    let naming = at(entry(MemoryEntryType::Note, "b needs v", &[]), 9);
    let journal = vec![
        plain.clone(),
        reasoned.clone(),
        found.clone(),
        mistake.clone(),
        both.clone(),
        gotcha.clone(),
        naming.clone(),
    ];
    let producers = [producer("up", journal)];
    let expected: Vec<String> = [naming, mistake, gotcha, both, found, reasoned, plain]
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(picked(&recipient(&stage("b", &file)), &producers), expected);
}

#[test]
fn two_dependencies_interleave_by_dependency_order_on_equal_rank() {
    let file = ["src/b.rs"];
    let a_reasoned = at(decision("a reasoned", "why", &file), 3);
    let a_plain = at(entry(MemoryEntryType::Question, "a plain", &file), 0);
    let z_reasoned = at(decision("z reasoned", "why", &file), 1);
    let z_plain = at(entry(MemoryEntryType::Question, "z plain", &file), 2);
    let producers = [
        producer("a", vec![a_plain.clone(), a_reasoned.clone()]),
        producer("z", vec![z_plain.clone(), z_reasoned.clone()]),
    ];
    let expected = vec![a_reasoned.id, z_reasoned.id, a_plain.id, z_plain.id];
    assert_eq!(picked(&recipient(&stage("b", &file)), &producers), expected);
}

#[test]
fn equal_timestamps_order_by_id() {
    let file = ["src/b.rs"];
    let mut first = at(entry(MemoryEntryType::Question, "one", &file), 0);
    let mut second = at(entry(MemoryEntryType::Question, "two", &file), 0);
    first.id = "b".repeat(32);
    second.id = "a".repeat(32);
    let producers = [producer("up", vec![first.clone(), second.clone()])];
    let expected = vec![second.id, first.id];
    assert_eq!(picked(&recipient(&stage("b", &file)), &producers), expected);
}

fn candidates(evidence: &str, content: &str, producer_dir: Option<&str>) -> Vec<String> {
    let entry = entry(MemoryEntryType::Note, content, &[evidence]);
    path_candidates(&entry, producer_dir, Some("/repo"))
}

#[test]
fn absolute_paths_read_relative_to_the_worktree_or_the_repository_root() {
    let worktree = candidates("/x/.worktrees/up/loom/src/a.rs:12", "n", Some("loom"));
    assert_eq!(worktree, vec!["loom/src/a.rs"]);
    let under_root = candidates("/repo/loom/src/a.rs", "n", Some("loom"));
    assert_eq!(under_root, vec!["loom/src/a.rs"]);
    assert!(candidates("/elsewhere/src/a.rs", "n", Some("loom")).is_empty());
    assert!(candidates("/repository/src/a.rs", "n", None).is_empty());
}

#[test]
fn relative_paths_read_both_as_repository_and_producer_relative() {
    let short = candidates("src/a.rs", "n", Some("loom"));
    assert_eq!(short, vec!["src/a.rs", "loom/src/a.rs"]);
    assert_eq!(
        candidates("../web/b.ts", "n", Some("loom")),
        vec!["web/b.ts"]
    );
    let full = candidates("loom/src/a.rs:3-9", "n", Some("loom"));
    assert!(full.contains(&"loom/src/a.rs".to_string()), "{full:?}");
}

#[test]
fn whitespace_evidence_is_no_path_and_content_tokens_are_trimmed() {
    assert!(candidates("review round 1", "no slash here", None).is_empty());
    let token = candidates("", "see `src/a.rs`. and (web/b.ts:4),", None);
    assert_eq!(token, vec!["src/a.rs", "web/b.ts"]);
}

#[test]
fn a_line_span_is_dropped_only_when_it_is_digits() {
    assert_eq!(strip_line_span("src/a.rs:12"), "src/a.rs");
    assert_eq!(strip_line_span("src/a.rs:12-30"), "src/a.rs");
    assert_eq!(strip_line_span("src/a.rs:main"), "src/a.rs:main");
    assert_eq!(strip_line_span("src/a.rs:12-"), "src/a.rs:12-");
}

#[path = "findings_render_tests.rs"]
mod render_tests;
