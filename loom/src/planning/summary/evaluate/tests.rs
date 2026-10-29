//! Section 04's sentence-state and summary-state tables, row by row.

use std::fs;

use super::fixtures::*;
use super::*;
use crate::planning::summary::claims::Claim;

fn state_of(root: &std::path::Path, id: &str) -> SummaryState {
    evaluate(&load(root), &[]).summaries[id].state
}

fn sentence_states(eval: &Evaluation, id: &str) -> Vec<(String, SentenceState)> {
    eval.summaries[id]
        .sentences
        .iter()
        .map(|s| (s.text.clone(), s.state))
        .collect()
}

/// Both document summaries and a product overview citing `Alpha holds.` of each.
fn sealed_product() -> Fixture {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    seal_document(&fx.root, ID2, &[("Alpha holds.", "a")]);
    let cited = [(ID1, "Alpha holds."), (ID2, "Alpha holds.")];
    seal_overview(
        &fx.root,
        PRODUCT,
        &cited,
        &[(VISION, "specification-sections")],
    );
    fx
}

#[test]
fn document_sentence_current_while_cited_span_holds() {
    let fx = two_sections();
    seal_document(
        &fx.root,
        ID1,
        &[("Alpha holds.", "a"), ("Beta holds.", "b")],
    );
    let eval = evaluate(&load(&fx.root), &[]);
    let summary = &eval.summaries[ID1];
    assert_eq!(summary.state, SummaryState::Current);
    assert_eq!(summary.source.as_deref(), Some(std::path::Path::new(S1)));
    let lines: Vec<usize> = summary.sentences.iter().map(|s| s.line).collect();
    let text = fs::read_to_string(fx.root.join(store::summary_path(ID1))).unwrap();
    let alpha = text
        .lines()
        .position(|l| l.starts_with("Alpha holds."))
        .unwrap()
        + 1;
    assert_eq!(lines, vec![alpha, alpha + 2], "1-based file lines");
    assert!(summary.sentences[0].citations.iter().all(|c| c.current));
}

#[test]
fn document_sentence_stale_when_cited_span_changes_or_anchor_goes() {
    let fx = two_sections();
    seal_document(
        &fx.root,
        ID1,
        &[("Alpha holds.", "a"), ("Beta holds.", "b")],
    );
    put(&fx.root, S1, &section(1, "Alpha edited.", "Beta."));
    let eval = evaluate(&load(&fx.root), &[]);
    let want = vec![
        ("Alpha holds.".to_string(), SentenceState::Stale),
        ("Beta holds.".to_string(), SentenceState::Current),
    ];
    assert_eq!(sentence_states(&eval, ID1), want);
    assert_eq!(eval.summaries[ID1].state, SummaryState::Stale);
    put(
        &fx.root,
        S1,
        &section(1, "Alpha.", "Beta.").replace("## B", "## Renamed"),
    );
    let eval = evaluate(&load(&fx.root), &[]);
    assert_eq!(
        eval.summaries[ID1].sentences[1].state,
        SentenceState::Stale,
        "anchor gone"
    );
}

#[test]
fn document_sentence_needs_every_cited_span_to_hold() {
    let fx = two_sections();
    seal_document_citing(
        &fx.root,
        ID1,
        &[("Both hold.", vec!["a", "b"]), ("Alpha holds.", vec!["a"])],
    );
    assert_eq!(state_of(&fx.root, ID1), SummaryState::Current);
    put(&fx.root, S1, &section(1, "Alpha.", "Beta edited."));
    let eval = evaluate(&load(&fx.root), &[]);
    let want = vec![
        ("Both hold.".to_string(), SentenceState::Stale),
        ("Alpha holds.".to_string(), SentenceState::Current),
    ];
    assert_eq!(
        sentence_states(&eval, ID1),
        want,
        "only the second span changed"
    );
    let held: Vec<bool> = eval.summaries[ID1].sentences[0]
        .citations
        .iter()
        .map(|c| c.current)
        .collect();
    assert_eq!(held, vec![true, false]);
}

#[test]
fn document_sentence_stale_without_its_cites_entry() {
    let fx = two_sections();
    let mut file = seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    file.front.cites.clear();
    write_file(&fx.root, ID1, &file);
    assert_eq!(state_of(&fx.root, ID1), SummaryState::Stale);
}

#[test]
fn document_incomplete_after_uncited_edit() {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    put(&fx.root, S1, &section(1, "Alpha.", "Beta edited."));
    let eval = evaluate(&load(&fx.root), &[]);
    assert_eq!(eval.summaries[ID1].state, SummaryState::Incomplete);
    assert_eq!(
        eval.summaries[ID1].sentences[0].state,
        SentenceState::Current
    );
}

#[test]
fn claimed_summary_is_rebuilding_with_sentence_states() {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    put(&fx.root, S1, &section(1, "Alpha edited.", "Beta."));
    let claim = Claim {
        summary: ID1.to_string(),
        session: Some("s1".into()),
        pid: 1,
        created_at: "2026-10-27T00:00:00Z".into(),
    };
    let eval = evaluate(&load(&fx.root), std::slice::from_ref(&claim));
    let summary = &eval.summaries[ID1];
    assert_eq!(summary.state, SummaryState::Rebuilding);
    assert_eq!(summary.claim.as_ref(), Some(&claim));
    assert_eq!(
        summary.sentences[0].state,
        SentenceState::Stale,
        "sentences keep states"
    );
}

#[test]
fn missing_rows_absent_invalid_and_conflicted() {
    let fx = two_sections();
    let eval = evaluate(&load(&fx.root), &[]);
    assert_eq!(eval.summaries[ID1].state, SummaryState::Missing, "no file");
    assert!(eval.summaries[ID1].error.is_some());
    let text = format::render(&seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]));
    let rel = store::summary_path(ID1).to_string_lossy().into_owned();
    for broken in [
        format!("{text}<<<<<<< ours\n=======\n>>>>>>> theirs\n"),
        format!("{text}\n- a list [x](../../../../product/demo/section_01.md#a)\n"),
        text.replace("kind: document", "kind: overview"),
        text.replace(
            "of: doc/product/demo/section_01.md",
            "of: doc/product/demo/section_02.md",
        ),
    ] {
        put(&fx.root, &rel, &broken);
        let eval = evaluate(&load(&fx.root), &[]);
        assert_eq!(eval.summaries[ID1].state, SummaryState::Missing, "{broken}");
        assert!(eval.summaries[ID1].file.is_none());
    }
}

#[test]
fn overview_current_then_stale_when_member_sentence_reworded() {
    let fx = sealed_product();
    assert_eq!(state_of(&fx.root, PRODUCT), SummaryState::Current);
    seal_document(&fx.root, ID1, &[("Alpha holds firmly.", "a")]);
    let eval = evaluate(&load(&fx.root), &[]);
    let product = &eval.summaries[PRODUCT];
    assert_eq!(product.state, SummaryState::Stale);
    assert_eq!(
        product.sentences[0].state,
        SentenceState::Stale,
        "member sentence gone"
    );
    assert_eq!(product.sentences[1].state, SentenceState::Current);
}

#[test]
fn overview_sentence_stale_when_member_sentence_stale() {
    let fx = sealed_product();
    put(&fx.root, S1, &section(1, "Alpha edited.", "Beta."));
    let eval = evaluate(&load(&fx.root), &[]);
    assert_eq!(eval.summaries[ID1].state, SummaryState::Stale);
    let product = &eval.summaries[PRODUCT];
    assert_eq!(product.sentences[0].state, SentenceState::Stale);
    assert_eq!(product.state, SummaryState::Stale);
}

#[test]
fn overview_sentence_stale_when_raw_span_changes() {
    let fx = sealed_product();
    put(
        &fx.root,
        VISION,
        &vision(&[1, 2]).replace("| S1 |", "| First |"),
    );
    let eval = evaluate(&load(&fx.root), &[]);
    let product = &eval.summaries[PRODUCT];
    let last = product.sentences.last().unwrap();
    assert_eq!(
        (last.text.as_str(), last.state),
        ("Point 0 holds.", SentenceState::Stale)
    );
}

#[test]
fn overview_sentence_stale_when_raw_source_is_not_one_of_its_sources() {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    seal_document(&fx.root, ID2, &[("Alpha holds.", "a")]);
    let cited = [(ID1, "Alpha holds."), (ID2, "Alpha holds.")];
    // The recorded hash of the section span matches, so only the sources check can reject it.
    let raw = [(VISION, "specification-sections"), (S1, "a")];
    seal_overview(&fx.root, PRODUCT, &cited, &raw);
    let eval = evaluate(&load(&fx.root), &[]);
    let states: Vec<_> = sentence_states(&eval, PRODUCT)
        .into_iter()
        .map(|(_, state)| state)
        .collect();
    let (current, stale) = (SentenceState::Current, SentenceState::Stale);
    assert_eq!(states, vec![current, current, current, stale]);
}

#[test]
fn overview_sentence_stale_when_cited_summary_is_not_a_member_in_scope() {
    let plan = "# Plan: X\n\nDoes x.\n";
    let fx = fixture(&[
        (VISION, &vision(&[1, 2])),
        (S1, &section(1, "Alpha.", "Beta.")),
        (S2, &section(2, "Alpha.", "Beta.")),
        (META, &metaplan(&["PLAN-x.md"])),
        ("doc/plans/demo/PLAN-x.md", plan),
    ]);
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    seal_document(&fx.root, ID2, &[("Alpha holds.", "a")]);
    seal_document(&fx.root, "plans/x", &[("X does x.", "plan-x")]);
    let cited = [
        (ID1, "Alpha holds."),
        (ID2, "Alpha holds."),
        ("plans/x", "X does x."),
    ];
    seal_overview(
        &fx.root,
        PRODUCT,
        &cited,
        &[(VISION, "specification-sections")],
    );
    let eval = evaluate(&load(&fx.root), &[]);
    assert_eq!(eval.summaries["plans/x"].state, SummaryState::Current);
    let states = sentence_states(&eval, PRODUCT);
    let member = |i: usize| states[i].1;
    assert_eq!(member(0), SentenceState::Current);
    assert_eq!(member(1), SentenceState::Current);
    assert_eq!(
        member(2),
        SentenceState::Stale,
        "plans/x is an engineering member"
    );
    assert_eq!(eval.summaries[PRODUCT].state, SummaryState::Stale);
}

#[test]
fn overview_incomplete_when_member_changes_is_added_or_removed() {
    let fx = sealed_product();
    seal_document(
        &fx.root,
        ID2,
        &[("Alpha holds.", "a"), ("Beta holds.", "b")],
    );
    assert_eq!(
        state_of(&fx.root, PRODUCT),
        SummaryState::Incomplete,
        "member changed"
    );
    let fx = sealed_product();
    fs::remove_file(fx.root.join(store::summary_path(ID2))).unwrap();
    let product = &evaluate(&load(&fx.root), &[]).summaries[PRODUCT];
    assert_eq!(product.state, SummaryState::Stale, "a cited member removed");
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    let cited = [(ID1, "Alpha holds.")];
    seal_overview(
        &fx.root,
        PRODUCT,
        &cited,
        &[(VISION, "specification-sections")],
    );
    assert_eq!(state_of(&fx.root, PRODUCT), SummaryState::Current);
    seal_document(&fx.root, ID2, &[("Alpha holds.", "a")]);
    assert_eq!(
        state_of(&fx.root, PRODUCT),
        SummaryState::Incomplete,
        "member added"
    );
}

#[test]
fn renamed_plan_with_equal_hash_is_current_before_rewrite() {
    let plan = "# Plan: X\n\nDoes x.\n";
    let fx = fixture(&[
        (META, &metaplan(&["PLAN-x.md"])),
        ("doc/plans/demo/PLAN-x.md", plan),
    ]);
    seal_document(&fx.root, "plans/x", &[("X does x.", "plan-x")]);
    let archived = fx.root.join("doc/plans/demo/archive/DONE-PLAN-x.md");
    fs::create_dir_all(archived.parent().unwrap()).unwrap();
    fs::rename(fx.root.join("doc/plans/demo/PLAN-x.md"), &archived).unwrap();
    let eval = evaluate(&load(&fx.root), &[]);
    let x = &eval.summaries["plans/x"];
    assert_eq!(
        x.source.as_deref(),
        Some(std::path::Path::new(
            "doc/plans/demo/archive/DONE-PLAN-x.md"
        ))
    );
    assert_eq!(x.state, SummaryState::Current);
}

#[test]
fn store_files_without_an_expected_id_are_orphans() {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    put(&fx.root, "doc/loom/summaries/plans/gone.md", "anything\n");
    put(
        &fx.root,
        "doc/loom/summaries/plans/.gone.md.tmp-1",
        "temp\n",
    );
    put(&fx.root, "doc/loom/summaries/notes.md", "not an id\n");
    let eval = evaluate(&load(&fx.root), &[]);
    assert_eq!(eval.orphans, vec!["plans/gone".to_string()]);
    assert_eq!(
        store::scope(&load(&fx.root), "demo").product,
        vec![ID1.to_string(), ID2.to_string()]
    );
}
