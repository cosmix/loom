//! Score adjustments the source channel applies on top of the shared ladder:
//! identifier-shaped evidence for its exact rungs (A.1), the test-path
//! downweight (A.3), and dependency affinity (A.23).

use super::source_fixtures::{full_node, graph};
use crate::context::config::RetrievalConfig;
use crate::context::rank::{RankQuery, RankedCandidate};
use crate::context::rank_source::{normalize_dependency_path, rank_source};
use crate::context::schema::{Confidence, SelectionReason, SourceNode};

/// The signature every fixture node shares, so two nodes differ only in the
/// thing under test and their BM25 scores are genuinely identical.
const SHARED_SIGNATURE: &str = "fn render_frame(frame: Frame)";

/// The query those fixtures are scored against. It spells out both words of
/// `render_frame` because since `rank_source::candidacy` a node with no exact
/// rung is a candidate only when the prompt named its symbol — and every
/// comparison below needs two nodes that both survive to be compared.
const NAMES_THE_SYMBOL: &str = "which render frame is drawn";

/// Rank `nodes` — each in its own file — against a plain text query.
fn rank_nodes(text: &str, nodes: Vec<SourceNode>) -> Vec<RankedCandidate> {
    rank_with(
        RankQuery {
            text: text.to_string(),
            ..RankQuery::default()
        },
        nodes,
    )
}

fn rank_with(query: RankQuery, nodes: Vec<SourceNode>) -> Vec<RankedCandidate> {
    let paths: Vec<String> = nodes
        .iter()
        .map(|node| node.path.display().to_string())
        .collect();
    let files: Vec<(&str, Vec<SourceNode>)> = paths
        .iter()
        .map(String::as_str)
        .zip(nodes.into_iter().map(|node| vec![node]))
        .collect();
    rank_source(&query, &graph(files), &RetrievalConfig::default())
}

fn score_of(candidates: &[RankedCandidate], id: &str) -> f32 {
    candidates
        .iter()
        .find(|candidate| candidate.id.as_str() == id)
        .unwrap_or_else(|| panic!("{id} must be a candidate: {candidates:?}"))
        .score
}

/// Nine nodes whose signatures are full of "write", "home" and "doc", so all
/// three are ordinary vocabulary here rather than identifiers; three of them
/// are additionally NAMED after one of those words.
fn prose_named_nodes() -> Vec<SourceNode> {
    let mut nodes: Vec<SourceNode> = (0..6)
        .map(|index| {
            let name = format!("helper{index}");
            full_node(
                &format!("src/filler{index}.rs#function:{name}"),
                &format!("src/filler{index}.rs"),
                &[name.as_str()],
                "fn helper(write: Doc, home: Doc) -> Doc",
            )
        })
        .collect();
    for (path, name) in [
        ("src/io.rs", "write"),
        ("src/env.rs", "home"),
        ("src/docs.rs", "doc"),
    ] {
        nodes.push(full_node(
            &format!("{path}#function:{name}"),
            path,
            &[name],
            "fn helper(write: Doc, home: Doc) -> Doc",
        ));
    }
    nodes
}

/// The measured specimen: "write the recommendation in
/// /home/dkaponis/src/loom/doc" returned five `write` helpers and three
/// `home()` functions at `high` confidence. Every one of those matches came
/// from a word the writer used as English, two of them lifted straight out of
/// a filesystem path.
///
/// The prompt now buys them nothing at all, not merely no rung: each of `write`,
/// `home` and `doc` is one lowercase word, so `rank_source::candidacy` refuses
/// them lexical candidacy too, and the six `helperN` nodes were never named. The
/// next test is the proof this silence is the gate rather than a fixture that
/// matches nothing.
#[test]
fn prose_words_and_path_segments_earn_no_exact_rung() {
    let candidates = rank_nodes(
        "write the recommendation in /home/dkaponis/src/loom/doc",
        prose_named_nodes(),
    );

    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.reasons.iter().any(|reason| matches!(
                reason,
                SelectionReason::ExactSymbol | SelectionReason::ExactPath
            ))),
        "no ordinary word may claim an exact rung: {candidates:?}"
    );
    assert!(
        candidates.is_empty(),
        "nor may it make an unnamed symbol a candidate: {candidates:?}"
    );
}

/// The same corpus and the same word, marked as code — proving the previous
/// test's silence is the gate working rather than the fixture matching nothing.
#[test]
fn the_same_word_backticked_earns_the_exact_symbol_rung() {
    let candidates = rank_nodes("where is `write` defined", prose_named_nodes());

    let writer = candidates
        .iter()
        .find(|candidate| candidate.id.as_str() == "src/io.rs#function:write")
        .expect("the backticked symbol must be a candidate");
    assert!(
        writer.reasons.contains(&SelectionReason::ExactSymbol),
        "backticks admit the rung whatever the df: {writer:?}"
    );
    assert_eq!(writer.confidence(), Confidence::High);
}

/// The cap is per-CANDIDATE, not per-rung: one full-strength rung restores
/// `High` even when a weaker rung fired alongside it. A writer who spells out
/// `src/gini.rs` has said what they meant, and the fact that `Gini` on its own
/// would only have been admitted by rarity takes nothing away from that.
///
/// `Gini` and not `gini`: `lexical::is_plain_word` refuses rarity to a name
/// spelled in nothing but lowercase letters, so an all-lowercase fixture would
/// fire one rung instead of the two this test is about.
#[test]
fn a_full_path_match_is_not_demoted_by_a_rare_only_rung_beside_it() {
    let candidates = rank_nodes(
        "look at src/gini.rs where Gini is used",
        vec![full_node(
            "src/gini.rs#type:Gini",
            "src/gini.rs",
            &["Gini"],
            "struct Gini",
        )],
    );

    let candidate = &candidates[0];
    assert!(
        candidate.reasons.contains(&SelectionReason::ExactPath)
            && candidate.reasons.contains(&SelectionReason::ExactSymbol),
        "the fixture must fire both rungs for the test to mean anything: {candidate:?}"
    );
    assert_eq!(candidate.confidence_ceiling, None);
    assert_eq!(candidate.confidence(), Confidence::High);
}

/// A test twin scoring identically to its implementation lands at
/// `test_path_factor` of its score and sorts below it — and would sort ABOVE it
/// without the downweight, since `src/widget.test.ts` precedes `src/widget.ts`
/// on the `(path, line_start)` tie-break.
#[test]
fn a_test_file_twin_is_downweighted_below_its_implementation() {
    let candidates = rank_nodes(
        NAMES_THE_SYMBOL,
        vec![
            full_node(
                "src/widget.ts#function:render_frame",
                "src/widget.ts",
                &["render_frame"],
                SHARED_SIGNATURE,
            ),
            full_node(
                "src/widget.test.ts#function:render_frame",
                "src/widget.test.ts",
                &["render_frame"],
                SHARED_SIGNATURE,
            ),
        ],
    );

    assert_eq!(
        candidates[0].id.as_str(),
        "src/widget.ts#function:render_frame",
        "the implementation must lead: {candidates:?}"
    );
    let implementation = score_of(&candidates, "src/widget.ts#function:render_frame");
    let twin = score_of(&candidates, "src/widget.test.ts#function:render_frame");
    assert!(
        (twin - implementation * RetrievalConfig::default().test_path_factor).abs() < 1e-6,
        "the twin must score exactly test_path_factor of the implementation: \
         {twin} vs {implementation}"
    );
}

/// Ordering pressure, never exclusion: when the only thing that matches is a
/// test, the test is the answer.
#[test]
fn a_test_file_that_is_the_only_match_still_appears() {
    let candidates = rank_nodes(
        NAMES_THE_SYMBOL,
        vec![full_node(
            "tests/widget_test.go#function:render_frame",
            "tests/widget_test.go",
            &["render_frame"],
            SHARED_SIGNATURE,
        )],
    );

    assert_eq!(candidates.len(), 1, "{candidates:?}");
    assert!(candidates[0].score > 0.0);
}

/// Rust keeps its unit tests inside the file they test, so a path check alone
/// would miss every one of them. A `tests` scope segment is the same signal.
#[test]
fn a_rust_tests_module_is_downweighted_without_a_test_path() {
    let candidates = rank_nodes(
        NAMES_THE_SYMBOL,
        vec![
            full_node(
                "src/a.rs#function:render_frame",
                "src/a.rs",
                &["helper", "render_frame"],
                SHARED_SIGNATURE,
            ),
            full_node(
                "src/b.rs#function:render_frame",
                "src/b.rs",
                &["tests", "render_frame"],
                SHARED_SIGNATURE,
            ),
        ],
    );

    let implementation = score_of(&candidates, "src/a.rs#function:render_frame");
    let in_tests_module = score_of(&candidates, "src/b.rs#function:render_frame");
    assert!(
        (in_tests_module - implementation * RetrievalConfig::default().test_path_factor).abs()
            < 1e-6,
        "a `tests` scope segment must downweight like a test path: \
         {in_tests_module} vs {implementation}"
    );
}

/// A node in a file a dependency stage owns outranks an equal-BM25 node from an
/// unrelated file, and says why.
#[test]
fn a_dependency_owned_file_outranks_an_unrelated_one() {
    let candidates = rank_with(
        RankQuery {
            text: NAMES_THE_SYMBOL.to_string(),
            dependency_paths: vec!["./src/foo.ts".to_string()],
            ..RankQuery::default()
        },
        vec![
            full_node(
                "src/foo.ts#function:render_frame",
                "src/foo.ts",
                &["render_frame"],
                SHARED_SIGNATURE,
            ),
            full_node(
                "src/unrelated.ts#function:render_frame",
                "src/unrelated.ts",
                &["render_frame"],
                SHARED_SIGNATURE,
            ),
        ],
    );

    assert_eq!(
        candidates[0].id.as_str(),
        "src/foo.ts#function:render_frame",
        "the dependency's file must lead: {candidates:?}"
    );
    assert!(candidates[0]
        .reasons
        .contains(&SelectionReason::StageDependency));
    let owned = score_of(&candidates, "src/foo.ts#function:render_frame");
    let unrelated = score_of(&candidates, "src/unrelated.ts#function:render_frame");
    assert!(
        (owned - unrelated - 30.0).abs() < 1e-4,
        "the boost is additive and worth exactly 30: {owned} vs {unrelated}"
    );
}

/// A dependency listing a DIRECTORY or a glob must boost nothing: a prefix
/// match on `src/` would hand 30 points to the entire tree.
#[test]
fn a_dependency_directory_prefix_boosts_nothing() {
    let candidates = rank_with(
        RankQuery {
            text: NAMES_THE_SYMBOL.to_string(),
            dependency_paths: vec!["src/".to_string(), "src/**/*.ts".to_string()],
            ..RankQuery::default()
        },
        vec![full_node(
            "src/foo.ts#function:render_frame",
            "src/foo.ts",
            &["render_frame"],
            SHARED_SIGNATURE,
        )],
    );

    assert!(
        !candidates[0]
            .reasons
            .contains(&SelectionReason::StageDependency),
        "only an exact path may claim the rung: {candidates:?}"
    );
}

#[test]
fn dependency_paths_normalize_to_one_comparable_form() {
    assert_eq!(normalize_dependency_path("  src/foo.ts  "), "src/foo.ts");
    assert_eq!(normalize_dependency_path("./src/foo.ts"), "src/foo.ts");
    assert_eq!(normalize_dependency_path("././src/foo.ts"), "src/foo.ts");
    assert_eq!(normalize_dependency_path("src\\foo.ts"), "src/foo.ts");
    assert_eq!(
        normalize_dependency_path("/abs/src/foo.ts"),
        "/abs/src/foo.ts",
        "an absolute path stays absolute, and so matches no project-relative node"
    );
}
