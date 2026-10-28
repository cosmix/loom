//! Frozen contracts for the review-loop server routes: restores, suggestions, ref validation,
//! text at a commit and built markers. Every function under test takes plain data or a git
//! fixture, so no planning server runs.

#[path = "plan_review_contracts/support.rs"]
mod support;

use chrono::{TimeZone, Utc};
use loom::plan_server::review::built::{built_notes, parse_generated, BuiltNote};
use loom::plan_server::review::compare::{resolve_ref, text_at, RefError};
use loom::plan_server::review::restore::{
    restore_file, save_path_of, PinnedFile, RestoreResult, SaveResult,
};
use loom::plan_server::review::suggest::{SuggestRefusal, SuggestionBox};
use loom::planning::project::Project;

/// One call of the restore's save: (path, text, opened hash, force).
type SaveCall = (String, String, String, bool);

const RESTORE_PATH: &str = "plans/PLAN-x.md";

fn pinned(start_text: Option<&str>, end_hash: Option<&str>) -> PinnedFile {
    PinnedFile {
        start_text: start_text.map(String::from),
        end_hash: end_hash.map(String::from),
    }
}

/// Runs `restore_file` with a save that records every call; the first call gets `answer`.
fn restore_with(
    pin: Option<&PinnedFile>,
    turn_running: bool,
    answer: SaveResult,
) -> (RestoreResult, Vec<SaveCall>) {
    let mut calls: Vec<SaveCall> = Vec::new();
    let mut answer = Some(answer);
    let mut save = |path: &str, text: &str, hash: &str, force: bool| {
        calls.push((path.to_owned(), text.to_owned(), hash.to_owned(), force));
        answer
            .take()
            .unwrap_or_else(|| SaveResult::Refused("save called twice".to_owned()))
    };
    let result = restore_file(RESTORE_PATH, pin, turn_running, false, &mut save);
    (result, calls)
}

#[test]
fn restore_writes_pinned_text() {
    let pin = pinned(Some("old"), Some("sha256:e"));
    let (result, calls) = restore_with(Some(&pin), false, SaveResult::Saved);
    let expected: SaveCall = (
        RESTORE_PATH.to_owned(),
        "old".to_owned(),
        "sha256:e".to_owned(),
        false,
    );
    assert_eq!(
        calls,
        vec![expected],
        "save takes the turn-start text and opens with the turn-end hash"
    );
    assert!(
        matches!(result, RestoreResult::Restored),
        "a saved restore reports Restored"
    );
}

#[test]
fn restore_refused_while_working() {
    let pin = pinned(Some("old"), Some("sha256:e"));
    let (result, calls) = restore_with(Some(&pin), true, SaveResult::Saved);
    assert!(calls.is_empty(), "no save while a turn is running");
    assert!(
        matches!(result, RestoreResult::Refused(_)),
        "a restore during a running turn is Refused"
    );
}

#[test]
fn restore_refuses_created_file() {
    let pin = pinned(None, Some("sha256:e"));
    let (result, calls) = restore_with(Some(&pin), false, SaveResult::Saved);
    assert!(calls.is_empty(), "no save for a file the turn created");
    assert!(
        matches!(result, RestoreResult::Refused(_)),
        "restoring a file the turn created is Refused"
    );
}

#[test]
fn restore_mismatch_returns_merge_texts() {
    let pin = pinned(Some("old"), Some("sha256:e"));
    let mismatch = SaveResult::HashMismatch {
        current_text: "newer".to_owned(),
        current_hash: "sha256:n".to_owned(),
    };
    let (result, calls) = restore_with(Some(&pin), false, mismatch);
    assert_eq!(calls.len(), 1, "save is tried once");
    let RestoreResult::Changed {
        current_text,
        current_hash,
        turn_start_text,
    } = result
    else {
        panic!("a hash mismatch must be Changed, carrying the texts for the merge view");
    };
    assert_eq!(current_text, "newer");
    assert_eq!(current_hash, "sha256:n");
    assert_eq!(turn_start_text, "old", "turn_start_text comes from the pin");
}

#[test]
fn restore_path_from_changed_file() {
    assert_eq!(
        save_path_of("doc/plans/PLAN-x.md").map(String::from),
        Some("plans/PLAN-x.md".to_owned()),
        "the repo-relative ChangedFile path becomes the doc-relative save path"
    );
    assert_eq!(
        save_path_of("planning-omissions.md").map(String::from),
        None,
        "a path outside doc/ has no save path"
    );
    assert_eq!(
        save_path_of("doc/../x").map(String::from),
        None,
        "a path failing the request-path check has no save path"
    );
}

#[test]
fn text_at_refuses_escaping_path() {
    let repo = support::repo();
    support::put(&repo.root, "doc/product/section_01.md", "# Section 1\n");
    support::put(&repo.root, "secret.txt", "top secret\n");
    let head = support::commit(&repo.root, support::BEFORE_BUILD, "fixture");
    let inside = text_at(&repo.root, &head, "product/section_01.md").expect("text at head");
    assert_eq!(inside.as_deref(), Some("# Section 1\n"), "a doc path reads");
    match text_at(&repo.root, &head, "../secret.txt") {
        Err(_) => {}
        Ok(text) => panic!("a path escaping doc/ must be refused, read {text:?}"),
    }
}

#[test]
fn note_limit_counts_characters() {
    let (sender, holder) = support::reader_and_holder();
    let mut inbox = SuggestionBox::new();
    let at_limit = "é".repeat(1000);
    let accepted = inbox.submit(&sender, Some(&holder), support::passage(), &at_limit);
    assert!(
        accepted.is_ok(),
        "1,000 two-byte characters are within the limit"
    );
    let over = "é".repeat(1001);
    let refused = inbox.submit(&sender, Some(&holder), support::passage(), &over);
    assert!(
        matches!(refused, Err(SuggestRefusal::NoteTooLong)),
        "1,001 characters give NoteTooLong"
    );
}

#[test]
fn twenty_first_suggestion_refused() {
    let (sender, holder) = support::reader_and_holder();
    let mut inbox = SuggestionBox::new();
    for n in 1..=20 {
        let note = format!("note {n}");
        let delivered = inbox.submit(&sender, Some(&holder), support::passage(), &note);
        assert!(delivered.is_ok(), "suggestion {n} of 20 is accepted");
    }
    let refused = inbox.submit(&sender, Some(&holder), support::passage(), "one more");
    assert!(
        matches!(refused, Err(SuggestRefusal::TooMany)),
        "the 21st pending suggestion gives TooMany"
    );
    assert_eq!(inbox.pending(), 20);
}

#[test]
fn suggestion_reaches_holder_only() {
    let (sender, holder) = support::reader_and_holder();
    let mut inbox = SuggestionBox::new();
    let submitted = inbox.submit(&sender, Some(&holder), support::passage(), "tighten this");
    let Ok(delivery) = submitted else {
        panic!("a suggestion to a holder is delivered");
    };
    assert_eq!(
        delivery.to_browser, "h-secret",
        "routed to the holder alone"
    );
    let message = &delivery.message;
    assert_eq!(message["type"], "review");
    assert_eq!(message["kind"], "suggestion");
    assert_eq!(message["from"], "Sam", "the sender travels by display name");
    assert_eq!(message["note"], "tighten this");
    let text = serde_json::to_string(message).expect("serialise message");
    assert!(!text.contains("s-secret"), "no sender browser id: {text}");
    assert!(!text.contains("h-secret"), "no holder browser id: {text}");
}

#[test]
fn suggestion_refused_without_holder() {
    let (sender, _) = support::reader_and_holder();
    let mut inbox = SuggestionBox::new();
    let refused = inbox.submit(&sender, None, support::passage(), "anyone?");
    assert!(
        matches!(refused, Err(SuggestRefusal::NoHolder)),
        "no holder gives NoHolder"
    );
    assert_eq!(inbox.pending(), 0, "nothing is queued for nobody");
}

#[test]
fn rejects_option_like_ref() {
    let repo = support::repo();
    support::put(&repo.root, "doc/product/section_01.md", "# Section 1\n");
    let head = support::commit(&repo.root, support::BEFORE_BUILD, "fixture");
    for raw in ["--output=x", "main\n"] {
        let result = resolve_ref(&repo.root, raw);
        assert!(
            matches!(result, Err(RefError::Invalid(_))),
            "{raw:?} must be Invalid before git runs"
        );
    }
    assert!(!repo.root.join("x").exists(), "git wrote the option's file");
    let resolved = resolve_ref(&repo.root, "main");
    assert!(
        matches!(&resolved, Ok(id) if *id == head),
        "a plain branch name resolves to its commit"
    );
}

#[test]
fn generated_stamp_parsed() {
    let review = "# Code Review: a\n\n**Plan:** PLAN-a | **Generated:** 2026-10-20 14:05 UTC\n\n";
    let expected = Utc.with_ymd_and_hms(2026, 10, 20, 14, 5, 0).single();
    assert_eq!(parse_generated(review), expected);
}

/// The base commit of a ChangedAfter note for `slug`, if `notes` holds one.
fn changed_after(notes: &[BuiltNote], slug: &str) -> Option<Option<String>> {
    notes.iter().find_map(|note| match note {
        BuiltNote::ChangedAfter {
            slug: s,
            base_commit,
            ..
        } if s == slug => Some(base_commit.clone()),
        _ => None,
    })
}

#[test]
fn marker_after_build_date() {
    let files = [
        ("doc/plans/archive/DONE-PLAN-a.md", support::plan_doc("a")),
        (
            "doc/plans/archive/REVIEW-PLAN-a.md",
            support::review_doc("PLAN-a", support::BUILT),
        ),
        ("doc/plans/IN_PROGRESS-PLAN-b.md", support::plan_doc("b")),
    ];
    let (repo, base) = support::section_changed_after_build(&files);
    // The file time predates the build, so only the git history shows the later change.
    support::set_mtime(&repo.root, support::SECTION_02, support::BEFORE_BUILD);
    let project = Project::load(&repo.root).expect("load fixture project");
    let notes = built_notes(&project, "product/section_02.md").expect("built notes");
    assert_eq!(
        changed_after(&notes, "a"),
        Some(Some(base)),
        "plan a is ChangedAfter with the last section commit before its build"
    );
    let running = notes
        .iter()
        .any(|note| matches!(note, BuiltNote::Running { slug } if slug == "b"));
    assert!(running, "running plan b gives Running");
}

#[test]
fn marker_finds_in_progress_named_review() {
    let files = [
        ("doc/plans/DONE-PLAN-c.md", support::plan_doc("c")),
        (
            "doc/plans/archive/REVIEW-IN_PROGRESS-PLAN-c.md",
            support::review_doc("IN_PROGRESS-PLAN-c", support::BUILT),
        ),
    ];
    let (repo, base) = support::section_changed_after_build(&files);
    let project = Project::load(&repo.root).expect("load fixture project");
    let notes = built_notes(&project, "product/section_02.md").expect("built notes");
    assert_eq!(
        changed_after(&notes, "c"),
        Some(Some(base)),
        "the REVIEW-IN_PROGRESS- review of plan c is found and dates its build"
    );
}
