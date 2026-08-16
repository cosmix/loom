use crate::context::rank::*;
use crate::context::schema::*;
use std::path::PathBuf;

fn chunk(id: &str, body: &str, tokens: usize) -> KnowledgeChunk {
    KnowledgeChunk {
        id: id.to_string(),
        file: PathBuf::from(format!("{id}.md")),
        anchor: String::new(),
        heading: String::new(),
        body: body.to_string(),
        content_hash: String::new(),
        estimated_tokens: tokens,
        aliases: Vec::new(),
        category: None,
        source_paths: Vec::new(),
        symbols: Vec::new(),
        links: Vec::new(),
        state: LifecycleState::Active,
    }
}

#[test]
fn rule_13_required_id_adds_the_explicit_boost() {
    let ranked = rank(
        &RankQuery {
            required_ids: vec!["a".into()],
            ..RankQuery::default()
        },
        &[chunk("a", "", 1)],
        Channel::Knowledge,
    );
    assert!(
        (ranked[0].score - 1000.0).abs() < 1e-4,
        "got {}",
        ranked[0].score
    );
    assert_eq!(ranked[0].reasons, vec![SelectionReason::ExplicitId]);
}

#[test]
fn rule_14_exact_path_adds_one_boost_per_chunk() {
    let mut first = chunk("a", "", 1);
    first.source_paths = vec!["///".into(), "---".into()];
    let ranked = rank(
        &RankQuery {
            text: "change /// and ---".into(),
            ..RankQuery::default()
        },
        &[first],
        Channel::Knowledge,
    );
    assert!(
        (ranked[0].score - 100.0).abs() < 1e-4,
        "got {}",
        ranked[0].score
    );
    assert_eq!(ranked[0].reasons, vec![SelectionReason::ExactPath]);
}

#[test]
fn rule_15_exact_symbol_adds_one_boost_per_chunk() {
    let mut first = chunk("a", "", 1);
    first.symbols = vec!["$$$".into(), "@@@".into()];
    let ranked = rank(
        &RankQuery {
            text: "call $$$ then @@@".into(),
            ..RankQuery::default()
        },
        &[first],
        Channel::Knowledge,
    );
    assert!(
        (ranked[0].score - 80.0).abs() < 1e-4,
        "got {}",
        ranked[0].score
    );
    assert_eq!(ranked[0].reasons, vec![SelectionReason::ExactSymbol]);
}

/// Regression: knowledge prose is full of one- and two-character backticked
/// tokens. Under plain substring containment a symbol `n` matched every query
/// containing the letter, so a note about `rg` flags ranked first for "signal
/// generation" with a `high` confidence label. An exact-symbol match must be
/// delimited by identifier boundaries.
#[test]
fn short_symbol_does_not_match_inside_a_longer_word() {
    let mut noise = chunk("noise", "unrelated body", 1);
    noise.symbols = vec!["n".into(), "rg".into()];
    let ranked = rank(
        &RankQuery {
            text: "signal generation".into(),
            ..RankQuery::default()
        },
        &[noise],
        Channel::Knowledge,
    );
    assert!(
        !ranked
            .iter()
            .any(|candidate| candidate.reasons.contains(&SelectionReason::ExactSymbol)),
        "a bare `n` must not exact-match inside \"generation\": {ranked:?}"
    );
}

/// The boundary rule must not break legitimate matches: a real symbol still
/// fires when it appears as its own word, including a `::`-qualified path.
#[test]
fn whole_word_symbol_still_matches_including_qualified_paths() {
    let mut plain = chunk("plain", "body", 1);
    plain.symbols = vec!["ContextStore".into()];
    let mut qualified = chunk("qualified", "body", 1);
    qualified.symbols = vec!["fs::locking".into()];

    for (chunk_value, query_text) in [
        (plain, "how does ContextStore resolve its root"),
        (qualified, "see fs::locking for the lock order"),
    ] {
        let ranked = rank(
            &RankQuery {
                text: query_text.into(),
                ..RankQuery::default()
            },
            &[chunk_value],
            Channel::Knowledge,
        );
        assert!(
            ranked[0].reasons.contains(&SelectionReason::ExactSymbol),
            "expected an exact-symbol match for {query_text:?}"
        );
    }
}

#[test]
fn rule_16_direct_link_neighbours_get_the_linked_from_boost() {
    let mut required = chunk("required", "", 1);
    required.file = PathBuf::from("required.md");
    required.links = vec![("to target".into(), "target.md".into())];
    let mut points_to_required = chunk("outbound", "", 1);
    points_to_required.links = vec![("to required".into(), "required.md".into())];
    let mut target = chunk("target", "", 1);
    target.file = PathBuf::from("target.md");
    let ranked = rank(
        &RankQuery {
            required_ids: vec!["required".into()],
            ..RankQuery::default()
        },
        &[required, points_to_required, target],
        Channel::Knowledge,
    );
    let linked: Vec<_> = ranked
        .iter()
        .filter(|candidate| candidate.id.as_str() != "required")
        .collect();
    assert!(linked
        .iter()
        .all(|candidate| (candidate.score - 40.0).abs() < 1e-4));
    assert!(linked
        .iter()
        .all(|candidate| candidate.reasons == vec![SelectionReason::LinkedFrom]));
}

/// Regression: `rule_16`'s fixture puts every chunk at the root, which is
/// exactly why a link target resolved against the wrong base (the knowledge
/// root instead of the linking file's own directory) slipped through. A real
/// tier-2 link is written relative to its own file's directory.
#[test]
fn linked_from_boost_resolves_link_targets_relative_to_the_source_file() {
    let mut required = chunk("required", "", 1);
    required.file = PathBuf::from("mistakes/a.md");
    required.links = vec![
        ("same dir".into(), "verification-harness.md".into()),
        (
            "up and over".into(),
            "../architecture/codex-plugin.md".into(),
        ),
        ("unrelated".into(), "other.md".into()),
    ];
    let mut same_dir_target = chunk("same-dir-target", "", 1);
    same_dir_target.file = PathBuf::from("mistakes/verification-harness.md");
    let mut cousin_target = chunk("cousin-target", "", 1);
    cousin_target.file = PathBuf::from("architecture/codex-plugin.md");
    let mut decoy = chunk("decoy", "", 1);
    decoy.file = PathBuf::from("architecture/other.md");

    let ranked = rank(
        &RankQuery {
            required_ids: vec!["required".into()],
            ..RankQuery::default()
        },
        &[required, same_dir_target, cousin_target, decoy],
        Channel::Knowledge,
    );

    let boosted = |id: &str| {
        ranked
            .iter()
            .find(|candidate| candidate.id.as_str() == id)
            .map(|candidate| candidate.reasons.contains(&SelectionReason::LinkedFrom))
            .unwrap_or(false)
    };
    assert!(
        boosted("same-dir-target"),
        "same-directory link must resolve"
    );
    assert!(
        boosted("cousin-target"),
        "parent-relative `../` link must resolve"
    );
    assert!(
        !boosted("decoy"),
        "a link naming a same-named file in a different directory must not match"
    );
}

#[test]
fn rule_17_stage_dependency_adds_its_boost() {
    let ranked = rank(
        &RankQuery {
            stage_dependency_ids: vec!["a".into()],
            ..RankQuery::default()
        },
        &[chunk("a", "", 1)],
        Channel::Knowledge,
    );
    assert!(
        (ranked[0].score - 30.0).abs() < 1e-4,
        "got {}",
        ranked[0].score
    );
    assert_eq!(ranked[0].reasons, vec![SelectionReason::StageDependency]);
}

#[test]
fn rule_18_final_score_sums_all_boosts_and_bm25_and_drops_zeroes() {
    let mut first = chunk("a", "alpha", 123);
    first.file = PathBuf::from("a.md");
    first.source_paths = vec!["///".into()];
    first.symbols = vec!["$$$".into()];
    first.links = vec![("self".into(), "a.md".into())];
    let ranked = rank(
        &RankQuery {
            text: "alpha /// $$$".into(),
            required_ids: vec!["a".into()],
            stage_dependency_ids: vec!["a".into()],
        },
        &[first, chunk("b", "beta", 1)],
        Channel::Knowledge,
    );
    assert_eq!(ranked.len(), 1);
    assert!(
        (ranked[0].score - 1_250.693_1).abs() < 1e-4,
        "got {}",
        ranked[0].score
    );
    assert_eq!(
        ranked[0].reasons,
        vec![
            SelectionReason::ExplicitId,
            SelectionReason::ExactPath,
            SelectionReason::ExactSymbol,
            SelectionReason::LinkedFrom,
            SelectionReason::StageDependency,
            SelectionReason::Lexical,
        ]
    );
}

#[test]
fn rule_19_ranked_candidate_copies_estimated_tokens() {
    let ranked = rank(
        &RankQuery {
            required_ids: vec!["a".into()],
            ..RankQuery::default()
        },
        &[chunk("a", "", 321)],
        Channel::Knowledge,
    );
    assert_eq!(ranked[0].token_count, 321);
}
