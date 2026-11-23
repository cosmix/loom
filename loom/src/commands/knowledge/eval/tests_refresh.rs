//! The eval refreshes the local source-graph snapshot once before any case
//! runs, and keeps going when that fails.

use super::cases::{EvalCase, EvalMode};
use super::{ensure_local_snapshot, refresh_source_graph, run_cases};
use crate::context::freshness::GraphState;
use crate::context::refresh::SnapshotAction;
use crate::context::retrieve::{
    capture, retrieve_for_stage, retrieve_with, take_evaluate_state_call_count, StageQuery,
};
use crate::test_git::TestRepo;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

/// A git repo, with one committed Rust file when `committed` and no HEAD at
/// all otherwise.
fn repo(committed: bool) -> TempDir {
    let builder = TestRepo::builder();
    if committed {
        builder.seed("src.rs", "pub fn refreshed() {}\n", "seed")
    } else {
        builder.no_seed()
    }
    .create()
    .into_temp_dir()
}

#[test]
#[serial]
fn the_local_snapshot_is_built_for_the_committed_tree() {
    let temp = repo(true);

    let outcome = ensure_local_snapshot(temp.path()).unwrap();

    assert_ne!(
        outcome.action,
        SnapshotAction::Unavailable,
        "{}",
        outcome.reason
    );
    assert_eq!(outcome.state(), GraphState::Current);
    assert!(!outcome.revision.is_empty());
}

#[test]
#[serial]
fn a_snapshot_that_cannot_be_built_is_reported_and_the_eval_goes_on() {
    let temp = repo(false);

    let outcome = ensure_local_snapshot(temp.path()).unwrap();
    assert_eq!(outcome.action, SnapshotAction::Unavailable);

    // Returns rather than raising: the cases still run against what exists.
    refresh_source_graph(temp.path());
}

fn case(name: &str) -> EvalCase {
    EvalCase {
        name: name.to_string(),
        query: "q".to_string(),
        mode: EvalMode::Prompt,
        budget_tokens: None,
        stage_fields: Vec::new(),
        require_ids: Vec::new(),
        expect: Vec::new(),
        relevant: Vec::new(),
        forbid: Vec::new(),
        abstain: false,
        max_rendered_tokens: None,
    }
}

/// A project with a work directory and no knowledge tree, so a capture takes
/// the `evaluate_state` branch that `take_evaluate_state_call_count` counts.
fn bare_project() -> TempDir {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".loom/work")).unwrap();
    temp
}

#[test]
#[serial]
fn the_eval_captures_the_corpus_once() {
    let temp = bare_project();
    let cases = [case("one"), case("two"), case("three")];
    let config = crate::context::config::RetrievalConfig::default();

    take_evaluate_state_call_count();
    run_cases(&cases, None, temp.path(), &config).unwrap();
    assert_eq!(take_evaluate_state_call_count(), 1);

    for case in &cases {
        let query = StageQuery::new(temp.path(), case.query.clone());
        retrieve_for_stage(&query, 1_000).unwrap();
    }
    assert_eq!(take_evaluate_state_call_count(), 3);
}

#[test]
#[serial]
fn later_cases_score_the_captured_revision() {
    let temp = TempDir::new().unwrap();
    let knowledge = temp.path().join("doc/loom/knowledge");
    fs::create_dir_all(temp.path().join(".loom/work")).unwrap();
    fs::create_dir_all(&knowledge).unwrap();
    let file = knowledge.join("conventions.md");
    fs::write(
        &file,
        "# Conventions\n\n## Alpha rule\n\nGuidance about pelicans.\n",
    )
    .unwrap();

    let query = StageQuery::new(temp.path(), "pelicans guidance alpha rule");
    let captured = capture(&query).unwrap();
    fs::write(
        &file,
        "# Conventions\n\n## Beta rule\n\nGuidance about walruses.\n",
    )
    .unwrap();
    let pack = retrieve_with(&captured, &query, 2_000).unwrap();

    assert!(!captured.structural_revision().is_empty());
    assert_eq!(
        pack.structural_freshness.revision,
        captured.structural_revision()
    );
    let text = format!("{pack:?}");
    assert!(text.contains("pelicans"), "the captured text is served");
    assert!(!text.contains("walruses"), "the edit after capture is not");
}
