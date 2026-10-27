//! Engine batches: same-hash rename rewrites, orphan deletion and new invalidations.

use std::fs;
use std::path::Path;

use super::*;
use crate::planning::summary::evaluate::fixtures::*;
use crate::planning::summary::evaluate::SentenceState;
use crate::planning::watch::Watcher;

const PLAN_X: &str = "doc/plans/PLAN-x.md";
const ARCHIVED_X: &str = "doc/plans/archive/DONE-PLAN-x.md";

fn engine(fx: &Fixture, own: OwnWrites) -> SummaryEngine {
    SummaryEngine::new(fx.root.clone(), fx.paths.clone(), own)
}

fn summary_text(root: &Path, id: &str) -> String {
    fs::read_to_string(root.join(store::summary_path(id))).expect("summary file")
}

fn poll_batch(watcher: &mut Watcher) -> Batch {
    (0..10)
        .find_map(|_| watcher.poll().expect("poll"))
        .expect("the watcher delivered no batch")
}

/// Seals `plans/x` from PLAN_X, then renames the plan into the archive.
/// Returns the fixture, its own-writes handle and a watcher that saw the seal.
fn sealed_then_archived() -> (Fixture, OwnWrites, Watcher) {
    let plan = "# Plan: X\n\nDoes x.\n\n## Detail\n\nMore.\n";
    let fx = fixture(&[(META, &metaplan(&["PLAN-x.md"])), (PLAN_X, plan)]);
    seal_document(
        &fx.root,
        "plans/x",
        &[("X does x.", "plan-x"), ("X has detail.", "")],
    );
    let own = OwnWrites::default();
    let mut watcher = Watcher::new(fx.root.clone(), own.clone());
    poll_batch(&mut watcher);
    fs::create_dir_all(fx.root.join("doc/plans/archive")).unwrap();
    fs::rename(fx.root.join(PLAN_X), fx.root.join(ARCHIVED_X)).unwrap();
    (fx, own, watcher)
}

#[test]
fn rewrites_plan_renamed_into_archive() {
    let (fx, own, mut watcher) = sealed_then_archived();
    let mut engine = engine(&fx, own);
    let update = engine
        .on_batch(&load(&fx.root), &Batch::default())
        .expect("on_batch");
    assert_eq!(update.rewritten, vec!["plans/x".to_string()]);
    assert_eq!(
        update.evaluation.summaries["plans/x"].state,
        SummaryState::Current
    );
    let x = "plans/x".to_string();
    assert!(
        !update.invalidated.contains(&x),
        "a rename re-queued plans/x"
    );
    let text = summary_text(&fx.root, "plans/x");
    let file = format::parse(&store::summary_path("plans/x"), &text).expect("parse");
    assert_eq!(file.front.of, ARCHIVED_X);
    let keys: Vec<&str> = file.front.cites.keys().map(String::as_str).collect();
    let title_key = format!("{ARCHIVED_X}#plan-x");
    assert_eq!(keys, vec![ARCHIVED_X, title_key.as_str()]);
    assert!(
        text.contains("X does x. [§](../../../plans/archive/DONE-PLAN-x.md#plan-x)"),
        "{text}"
    );
    assert!(
        text.contains("X has detail. [§](../../../plans/archive/DONE-PLAN-x.md)\n"),
        "{text}"
    );
    let delivered = poll_batch(&mut watcher);
    let summary = store::summary_path("plans/x");
    assert!(
        !delivered.changed.contains(&summary),
        "own write reported: {delivered:?}"
    );
    let again = engine
        .on_batch(&load(&fx.root), &Batch::default())
        .expect("second batch");
    assert!(again.rewritten.is_empty(), "rewrites are idempotent");
}

#[test]
fn renamed_source_with_new_content_is_not_rewritten() {
    let fx = fixture(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
    ]);
    seal_document(&fx.root, "plans/x", &[("X does x.", "plan-x")]);
    let before = summary_text(&fx.root, "plans/x");
    fs::remove_file(fx.root.join(PLAN_X)).unwrap();
    put(
        &fx.root,
        "doc/plans/IN_PROGRESS-PLAN-x.md",
        "# Plan: X\n\nDoes x, edited.\n",
    );
    let update = engine(&fx, OwnWrites::default()).on_batch(&load(&fx.root), &Batch::default());
    let update = update.expect("on_batch");
    assert!(update.rewritten.is_empty());
    assert_eq!(summary_text(&fx.root, "plans/x"), before);
    assert_eq!(
        update.evaluation.summaries["plans/x"].state,
        SummaryState::Stale
    );
}

#[test]
fn deletes_archived_plan_no_metaplan_names() {
    let fx = fixture(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
        ("doc/plans/PLAN-d.md", "# Plan: D\n\nDoes d.\n"),
    ]);
    seal_document(&fx.root, "plans/d", &[("D does d.", "plan-d")]);
    fs::create_dir_all(fx.root.join("doc/plans/archive")).unwrap();
    let archived = fx.root.join("doc/plans/archive/DONE-PLAN-d.md");
    fs::rename(fx.root.join("doc/plans/PLAN-d.md"), archived).unwrap();
    let mut engine = engine(&fx, OwnWrites::default());
    let update = engine
        .on_batch(&load(&fx.root), &Batch::default())
        .expect("on_batch");
    assert_eq!(update.deleted, vec!["plans/d".to_string()]);
    assert!(!fx.root.join(store::summary_path("plans/d")).exists());
    assert!(!update.evaluation.summaries.contains_key("plans/d"));
    assert!(update.evaluation.orphans.is_empty());
}

#[test]
fn deleted_source_deletes_its_summary_and_stales_the_overview_citing_it() {
    let fx = fixture(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
    ]);
    seal_document(&fx.root, "plans/x", &[("X does x.", "plan-x")]);
    seal_overview(&fx.root, "engineering", &[("plans/x", "X does x.")], &[]);
    let mut engine = engine(&fx, OwnWrites::default());
    let before = engine
        .on_batch(&load(&fx.root), &Batch::default())
        .expect("first batch");
    let overview = &before.evaluation.summaries["engineering"];
    assert_eq!(overview.state, SummaryState::Current);
    fs::remove_file(fx.root.join(PLAN_X)).unwrap();
    let update = engine
        .on_batch(&load(&fx.root), &Batch::default())
        .expect("batch after the delete");
    assert_eq!(update.deleted, vec!["plans/x".to_string()]);
    assert!(!fx.root.join(store::summary_path("plans/x")).exists());
    let overview = &update.evaluation.summaries["engineering"];
    assert_eq!(overview.state, SummaryState::Stale);
    assert_eq!(overview.sentences[0].state, SentenceState::Stale);
    assert_eq!(update.invalidated, vec!["engineering".to_string()]);
}

#[test]
fn invalidated_lists_only_newly_out_of_date_ids() {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    seal_document(&fx.root, ID2, &[("Alpha holds.", "a")]);
    let mut engine = engine(&fx, OwnWrites::default());
    let mut batch = |root: &Path| {
        engine
            .on_batch(&load(root), &Batch::default())
            .expect("batch")
    };
    let first = batch(&fx.root);
    assert_eq!(
        first.invalidated,
        vec!["product".to_string()],
        "the missing overview"
    );
    put(&fx.root, S1, &section(1, "Alpha edited.", "Beta."));
    let second = batch(&fx.root);
    assert_eq!(second.invalidated, vec![ID1.to_string()]);
    put(&fx.root, S2, &section(2, "Alpha.", "Beta edited."));
    let third = batch(&fx.root);
    assert_eq!(
        third.invalidated,
        vec![ID2.to_string()],
        "an incomplete summary; ID1 still stale"
    );
    assert_eq!(third.evaluation.summaries[ID1].state, SummaryState::Stale);
}
