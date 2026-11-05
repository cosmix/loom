//! Contracts for stage summary-store: the summary format, sealing rules, evaluation
//! states, overview scope and the engine's renames and deletions.

#[path = "planning_summary_support/mod.rs"]
mod support;

use std::fs;
use std::path::{Path, PathBuf};

use loom::context::source_graph::body_hash;
use loom::planning::summary::engine::SummaryEngine;
use loom::planning::summary::store;
use loom::planning::watch::{Batch, OwnWrites, Watcher};
use support::*;

#[test]
fn sentence_id_follows_text() {
    let hash = body_hash(b"The app keeps no state.");
    let want = format!("s-{}", &hash.strip_prefix("sha256:").unwrap()[..8]);
    let first =
        summary_with("The app keeps  no state. [§](../../../../product/demo/section_01.md#a)");
    let second =
        summary_with("The app keeps no state. [see](../../../../product/demo/section_01.md#b)");
    let (a, b) = (sentences(&first), sentences(&second));
    assert_eq!((a.len(), b.len()), (1, 1), "one sentence each");
    assert_eq!(
        a[0].text, "The app keeps no state.",
        "links removed, whitespace collapsed"
    );
    assert_eq!(a[0].id, want, "id over the collapsed text without links");
    assert_eq!(b[0].id, want, "a link edit keeps the id");
    assert_eq!(sid("The app keeps no state."), want);
}

#[test]
fn seal_rejects_body_rules() {
    let (_t, root, home) = sections_project(&[1]);
    let l = doc_link(1, "a");
    let bad = [
        format!("- item [§]({l})"),
        "| a | b |".to_string(),
        "```rust\ncode\n```".to_string(),
        format!("<b>x</b> [§]({l})"),
        "A sentence with no link.".to_string(),
        format!("## Heading [§]({l})"),
        "### Deep".to_string(),
    ];
    let summary = root.join(summary_rel(ID));
    for line in &bad {
        let result = seal_draft(&root, &home, ID, &format!("# S1\n\n{line}\n"));
        let err = result.expect_err(&format!("seal accepted line 3 {line:?}"));
        let at_line = err.lines().any(|m| m.starts_with("3:"));
        assert!(at_line, "{line:?} not reported at line 3: {err}");
        assert!(!summary.exists(), "{line:?} wrote a summary");
    }
    assert!(
        seal_draft(&root, &home, ID, &many(16)).is_err(),
        "16 sentences sealed"
    );
    assert!(!summary.exists(), "16 sentences wrote a summary");
    sealed(&root, &home, ID, &many(15));
    sealed(&root, &home, ID, &doc_draft(1, &[("Alpha holds.", "a")]));
    let over = seal_draft(&root, &home, PRODUCT, &overview_of(31));
    assert!(over.is_err(), "31 overview sentences sealed");
    assert!(
        !root.join(summary_rel(PRODUCT)).exists(),
        "31 sentences wrote an overview"
    );
    sealed(&root, &home, PRODUCT, &overview_of(30));
}

#[test]
fn seal_rejects_unresolved_anchor() {
    let s2 = "# Section 02\n\n## Detection rules\n\nRules.\n";
    let (_t, root, home) = project(&[(S1, &section(1, "Alpha.", "Beta.")), (S2, s2)]);
    let drafts = [
        doc_draft(1, &[("Alpha holds.", "no-such-heading")]),
        "# S1\n\nAlpha holds. [§](../../../../product/demo/section_02.md#detection-rules)\n"
            .to_string(),
    ];
    for draft in &drafts {
        assert!(
            seal_draft(&root, &home, ID, draft).is_err(),
            "sealed:\n{draft}"
        );
        assert!(
            !root.join(summary_rel(ID)).exists(),
            "wrote a summary for:\n{draft}"
        );
    }
    let preamble = "# S1\n\nAlpha holds. [§](../../../../product/demo/section_01.md)\n";
    sealed(&root, &home, ID, preamble);
    let file = parsed(ID, &read(&root, &summary_rel(ID)));
    let keys: Vec<&String> = file.front.cites.keys().collect();
    assert!(
        keys.iter().any(|k| *k == "doc/product/demo/section_01.md"),
        "no preamble cite: {keys:?}"
    );
}

#[test]
fn seal_rejects_duplicate_sentence() {
    let (_t, root, home) = project(&[(S1, &section(1, "Alpha.", "Beta."))]);
    let (a, b) = (doc_link(1, "a"), doc_link(1, "b"));
    let draft = format!("# S1\n\nSame words. [§]({a})\n\nSame words. [x]({b})\n");
    let err = seal_draft(&root, &home, ID, &draft).expect_err("duplicate sentence sealed");
    assert!(
        err.contains('3') && err.contains('5'),
        "both lines not named: {err}"
    );
    assert!(!root.join(summary_rel(ID)).exists());
}

#[test]
fn seal_rejects_stale_cited_sentence() {
    let (_t, root, home) = sections_project(&[1]);
    let member = doc_draft(1, &[("Alpha holds.", "a")]);
    sealed(&root, &home, ID, &member);
    put(&root, S1, section(1, "Alpha changed.", "Beta."));
    let result = seal_draft(&root, &home, PRODUCT, &overview_of(2));
    let err = result.expect_err("a stale member sentence was cited");
    assert!(
        err.lines().any(|m| m.starts_with("3:")),
        "not reported at line 3: {err}"
    );
    assert!(!root.join(summary_rel(PRODUCT)).exists());
    sealed(&root, &home, ID, &member);
    sealed(&root, &home, PRODUCT, &overview_of(2));
}

#[test]
fn seal_rejects_uncited_member() {
    let (_t, root, home) = sections_project(&[1, 2]);
    sealed(&root, &home, ID, &doc_draft(1, &[("Alpha holds.", "a")]));
    let beta = doc_draft(2, &[("Beta holds.", "a")]);
    sealed(&root, &home, "demo/product/section_02", &beta);
    let result = seal_draft(&root, &home, PRODUCT, &overview_of(2));
    let err = result.expect_err("an in-scope member dropped out");
    assert!(
        err.contains("demo/product/section_02"),
        "uncited member not named: {err}"
    );
    assert!(!root.join(summary_rel(PRODUCT)).exists());
}

#[test]
fn stale_and_incomplete_states() {
    let (_t, root, home) = sections_project(&[1]);
    reseal(&root, &home);
    assert_eq!(state(&eval(&root, &[]), ID), "current");
    let alpha = |s: &'static str| vec![("Alpha holds.".to_string(), s)];
    put(&root, S1, section(1, "Alpha.", "Beta edited."));
    let e = eval(&root, &[]);
    assert_eq!(state(&e, ID), "incomplete", "an uncited edit");
    assert_eq!(sentence_states(&e, ID), alpha("current"));
    put(&root, S1, section(1, "Alpha edited.", "Beta edited."));
    let e = eval(&root, &[]);
    assert_eq!(state(&e, ID), "stale", "a cited edit");
    assert_eq!(sentence_states(&e, ID), alpha("stale"));
    reseal(&root, &home);
    let renamed = section(1, "Alpha.", "Beta.").replace("## A", "## A renamed");
    put(&root, S1, renamed);
    assert_eq!(
        state(&eval(&root, &[]), ID),
        "stale",
        "a renamed cited heading"
    );
    reseal(&root, &home);
    assert_hand_edits(&root);
}

#[test]
fn overview_members_ignore_link_edits() {
    let (_t, root, home) = sections_project(&[1, 2]);
    sealed(&root, &home, ID, &doc_draft(1, &[("Alpha holds.", "a")]));
    let beta = doc_draft(2, &[("Beta holds.", "a")]);
    sealed(&root, &home, "demo/product/section_02", &beta);
    let members = [
        (ID, "Alpha holds."),
        ("demo/product/section_02", "Beta holds."),
    ];
    sealed(&root, &home, PRODUCT, &product_overview(&members));
    assert_eq!(state(&eval(&root, &[]), PRODUCT), "current");
    let (a, b) = (doc_link(1, "a"), doc_link(1, "b"));
    let relinked = format!("# S1\n\nAlpha holds. [see]({a}) [and]({b})\n");
    sealed(&root, &home, ID, &relinked);
    let after_links = state(&eval(&root, &[]), PRODUCT);
    assert_eq!(after_links, "current", "a link edit of a member");
    sealed(
        &root,
        &home,
        ID,
        &doc_draft(1, &[("Alpha holds firmly.", "a")]),
    );
    let after = state(&eval(&root, &[]), PRODUCT);
    assert!(
        after == "incomplete" || after == "stale",
        "a reworded member left {after}"
    );
}

fn summary_scope_project() -> (tempfile::TempDir, PathBuf, PathBuf) {
    project_of(&[
        (VISION, vision(&[1])),
        (S1, section(1, "A.", "B.")),
        (S2, section(2, "A.", "B.")),
        (META, metaplan(&["PLAN-a.md", "DONE-PLAN-b.md"])),
        ("doc/plans/demo/PLAN-a.md", "# Plan: A\n".into()),
        (
            "doc/plans/PLAN-c.md",
            "# Plan: C\n\nImplements: none\n".into(),
        ),
        ("doc/plans/demo/PLAN-unlisted.md", "# Unlisted\n".into()),
        (
            "doc/plans/demo/archive/DONE-PLAN-b.md",
            "# Plan: B\n".into(),
        ),
        (
            "doc/plans/demo/archive/DONE-PLAN-d.md",
            "# Plan: D\n".into(),
        ),
    ])
}

#[test]
fn summary_scope_table() {
    let (_t, root, _home) = summary_scope_project();
    let p = load(&root);
    let mut ids = store::expected_ids(&p);
    ids.sort();
    let want = [
        "demo/metaplan/index",
        ID,
        "demo/product/section_02",
        "demo/product/vision",
        ENGINEERING,
        PRODUCT,
        "plans/a",
        "plans/b",
        "plans/c",
        "plans/unlisted",
    ];
    assert_eq!(ids, want);
    let scope = store::scope(&p, "demo");
    assert_eq!(scope.product, vec![ID]);
    assert_eq!(scope.engineering, vec!["plans/a", "plans/b"]);
    let mut outside = scope.outside.clone();
    outside.sort();
    assert_eq!(outside, vec!["demo/product/section_02", "plans/unlisted"]);
    let id_for = |name: &str| store::summary_id_for(&p, Path::new(name));
    assert_eq!(
        id_for("doc/plans/demo/archive/DONE-PLAN-b.md").as_deref(),
        Some("plans/b")
    );
    assert_eq!(id_for("doc/plans/demo/archive/DONE-PLAN-d.md"), None);
    assert_eq!(
        id_for("doc/plans/demo/PLAN-unlisted.md").as_deref(),
        Some("plans/unlisted")
    );
}

#[test]
fn plan_summary_cites_stage_anchor() {
    let (_t, root, home) = project(&[
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN_X, &staged_plan("First.", "Second.")),
    ]);
    let draft = "# X\n\nAlpha runs first. [§](../../../plans/demo/PLAN-x.md#stage:alpha)\n";
    sealed(&root, &home, "plans/x", draft);
    assert_eq!(state(&eval(&root, &[]), "plans/x"), "current");
    put(&root, PLAN_X, staged_plan("First.", "Second, edited."));
    let e = eval(&root, &[]);
    assert_eq!(
        state(&e, "plans/x"),
        "incomplete",
        "an edit of another stage"
    );
    let want = vec![("Alpha runs first.".to_string(), "current")];
    assert_eq!(sentence_states(&e, "plans/x"), want);
    put(
        &root,
        PLAN_X,
        staged_plan("First, edited.", "Second, edited."),
    );
    let cited = state(&eval(&root, &[]), "plans/x");
    assert_eq!(cited, "stale", "an edit of the cited stage");
}

#[test]
fn rename_rewrites_paths() {
    let meta = metaplan(&["PLAN-x.md"]);
    let (_t, root, home) = plan_fixture(&[(META, &meta), (PLAN_X, "# Plan: X\n\nDoes x.\n")]);
    let own = OwnWrites::default();
    let mut watcher = Watcher::new(root.clone(), own.clone());
    next_batch(&mut watcher);
    let to = "doc/plans/demo/IN_PROGRESS-PLAN-x.md";
    fs::rename(root.join(PLAN_X), root.join(to)).unwrap();
    let mut engine = SummaryEngine::new(root.clone(), paths(&root, &home), own.clone());
    let renamed = vec![(PathBuf::from(PLAN_X), PathBuf::from(to))];
    let batch = Batch {
        renamed,
        ..Batch::default()
    };
    let update = engine.on_batch(&load(&root), &batch).expect("on_batch");
    let x = "plans/x".to_string();
    assert!(
        update.rewritten.contains(&x),
        "not rewritten: {:?}",
        update.rewritten
    );
    assert!(
        !update.invalidated.contains(&x),
        "a rename re-queued plans/x"
    );
    assert_eq!(state(&update.evaluation, "plans/x"), "current");
    let text = read(&root, &summary_rel("plans/x"));
    let file = parsed("plans/x", &text);
    assert_eq!(file.front.of, to);
    let keys: Vec<&String> = file.front.cites.keys().collect();
    assert!(keys.iter().all(|k| k.starts_with(to)), "{keys:?}");
    assert!(
        text.contains("](../../../plans/demo/IN_PROGRESS-PLAN-x.md#plan-x)"),
        "{text}"
    );
    assert!(!text.contains("](../../../plans/demo/PLAN-x.md"), "{text}");
    let delivered = next_batch(&mut watcher);
    let summary = PathBuf::from(summary_rel("plans/x"));
    let reported = delivered.changed.contains(&summary) || delivered.added.contains(&summary);
    assert!(
        !reported,
        "the rewrite was not registered with OwnWrites: {delivered:?}"
    );
}

#[test]
fn deleted_source_deletes_summary() {
    let entries = "## 1. Y (`PLAN-y.md`)\n\n## 2. X (`PLAN-x.md`)\n";
    let meta = format!("# Metaplan\n\n## Notes\n\nOrder.\n\n{entries}");
    let files = [
        (META, meta.as_str()),
        (PLAN_X, "# Plan: X\n"),
        (PLAN_Y, "# Plan: Y\n"),
    ];
    let (_t, root, home) = plan_fixture(&files);
    let y = "# Y\n\nY does y. [§](../../../plans/demo/PLAN-y.md#plan-y)\n";
    sealed(&root, &home, "plans/y", y);
    let (sx, sy) = (sid("X does x."), sid("Y does y."));
    let overview = format!(
        "# Engineering\n\nX is planned. [§](../../plans/x.md#{sx})\n\nY is planned. [§](../../plans/y.md#{sy})\n\n\
         The metaplan orders them. [§](../../../../plans/demo/index.md#notes)\n"
    );
    sealed(&root, &home, ENGINEERING, &overview);
    assert_eq!(state(&eval(&root, &[]), ENGINEERING), "current");
    fs::remove_file(root.join(PLAN_X)).unwrap();
    put(&root, META, meta.replace("## 2. X (`PLAN-x.md`)\n", ""));
    let mut engine = SummaryEngine::new(root.clone(), paths(&root, &home), OwnWrites::default());
    let (removed, changed) = (vec![PathBuf::from(PLAN_X)], vec![PathBuf::from(META)]);
    let batch = Batch {
        removed,
        changed,
        ..Batch::default()
    };
    let update = engine.on_batch(&load(&root), &batch).expect("on_batch");
    assert!(
        !root.join(summary_rel("plans/x")).exists(),
        "the orphaned summary was kept"
    );
    let listed = update.deleted.contains(&"plans/x".to_string());
    assert!(listed, "not listed as deleted: {:?}", update.deleted);
    assert_ne!(state(&update.evaluation, ENGINEERING), "current");
}
