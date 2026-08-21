//! Identifier-shaped evidence for the exact rungs (A.1), on the knowledge
//! channel and on the primitives underneath it.
//!
//! Every case here is a measured specimen or its direct inverse: the boost
//! ladder is only worth having if an 80-point `exact-symbol` at `high`
//! confidence means the writer referred to a symbol, and before this gate it
//! mostly meant they used an English word that some symbol is also named after.

use super::rank_fixtures::{chunk, rank_text};
use crate::context::lexical::{admits_exact, backtick_spans, is_shaped, TermEvidence};
use crate::context::schema::{Confidence, KnowledgeChunk, SelectionReason};

/// Eight chunks that all talk about points, so `df("point") = 8` puts the word
/// well past `df_ident_max` — "point" is this corpus's ordinary vocabulary.
/// Two of them additionally declare a symbol spelled like the word.
fn point_corpus() -> Vec<KnowledgeChunk> {
    let mut chunks: Vec<KnowledgeChunk> = (0..6)
        .map(|index| chunk(&format!("filler-{index}"), "the point of this section", 10))
        .collect();
    let mut lerp = chunk(
        "lerp",
        "interpolates between a start point and an end point during repair",
        10,
    );
    lerp.symbols = vec!["lerpPoint".to_string()];
    let mut point_type = chunk("point-type", "a point in space", 10);
    point_type.symbols = vec!["Point".to_string()];
    chunks.push(lerp);
    chunks.push(point_type);
    chunks
}

/// The headline specimen: "why doesn't loom repair --fix do it, the point is"
/// pulled in `lerpPoint` and `type Point` at `high` confidence because the
/// prompt contained the English word "point". The words stay eligible on their
/// lexical merit; what they lose is the right to claim an exact match.
#[test]
fn an_ordinary_word_matching_a_symbol_name_earns_no_exact_rung() {
    let ranked = rank_text("why doesn't loom repair fix the point", &point_corpus());

    assert!(
        !ranked
            .iter()
            .any(|candidate| candidate.reasons.contains(&SelectionReason::ExactSymbol)),
        "a prose \"point\" must not claim `Point` or `lerpPoint`: {ranked:?}"
    );
    let lerp = ranked
        .iter()
        .find(|candidate| candidate.id.as_str() == "lerp")
        .expect("the chunk sharing the surviving term \"repair\" is still a candidate");
    assert_eq!(
        lerp.reasons,
        vec![SelectionReason::Lexical],
        "withholding the rung must not exclude the chunk: {lerp:?}"
    );
}

/// The inverse: the same corpus, the same word, marked as code by the writer.
/// Backticks are the one signal that survives any document frequency.
#[test]
fn a_backticked_word_earns_the_exact_symbol_rung() {
    let mut corpus = point_corpus();
    corpus[0].symbols = vec!["point".to_string()];

    let ranked = rank_text("what does `point` do in precip streaming", &corpus);

    let backticked = ranked
        .iter()
        .find(|candidate| candidate.id.as_str() == "filler-0")
        .expect("the backticked symbol must be a candidate");
    assert!(
        backticked.reasons.contains(&SelectionReason::ExactSymbol),
        "a backticked term must earn the rung whatever its df: {backticked:?}"
    );
    assert_eq!(backticked.confidence(), Confidence::High);
}

/// camelCase is not an English word, so a shaped name earns its rung even when
/// the corpus is saturated with it — `df` cannot argue with spelling.
#[test]
fn a_shaped_name_earns_the_rung_at_high_confidence() {
    let mut corpus: Vec<KnowledgeChunk> = (0..8)
        .map(|index| {
            chunk(
                &format!("filler-{index}"),
                "pruneEvictionWindow is discussed at length here",
                10,
            )
        })
        .collect();
    corpus[0].symbols = vec!["pruneEvictionWindow".to_string()];

    let ranked = rank_text("where is pruneEvictionWindow called", &corpus);

    let shaped = ranked
        .iter()
        .find(|candidate| candidate.id.as_str() == "filler-0")
        .expect("the shaped symbol must be a candidate");
    assert!(
        shaped.reasons.contains(&SelectionReason::ExactSymbol),
        "a camelCase name is admitted by shape, not by rarity: {shaped:?}"
    );
    assert_eq!(
        shaped.confidence(),
        Confidence::High,
        "shape is full-strength evidence: {shaped:?}"
    );
}

/// A name that is neither backticked nor shaped, but occurs in almost no
/// document, is still admitted — and comes out one confidence step lower,
/// because "uncommon in this corpus" is real evidence and weaker evidence.
///
/// The proposal's own example for this rule, `repairGini`, does not actually
/// exercise it: `repairGini` is camelCase and so shaped. `gini` is the part of
/// it that rests on rarity alone.
#[test]
fn a_rare_only_match_is_admitted_but_demoted_to_medium() {
    let mut corpus = point_corpus();
    corpus[0].symbols = vec!["gini".to_string()];

    let ranked = rank_text("where is gini used", &corpus);

    let rare = ranked
        .iter()
        .find(|candidate| candidate.id.as_str() == "filler-0")
        .expect("a rare symbol must still be admitted");
    assert!(
        rare.reasons.contains(&SelectionReason::ExactSymbol),
        "corpus rarity admits the rung: {rare:?}"
    );
    assert_eq!(
        rare.confidence(),
        Confidence::Medium,
        "rarity alone must not publish a `high` claim: {rare:?}"
    );
    assert_eq!(rare.confidence_ceiling, Some(Confidence::Medium));
}

/// The second measured specimen: "write the recommendation in
/// /home/dkaponis/src/loom/doc" pulled in five `write` helpers and three
/// `home()` functions. A filesystem path in a prompt is not a symbol
/// reference, and its segments are not identifiers just because a file is
/// named after them.
#[test]
fn words_lifted_out_of_a_filesystem_path_earn_no_exact_rung() {
    let mut corpus: Vec<KnowledgeChunk> = (0..6)
        .map(|index| {
            chunk(
                &format!("filler-{index}"),
                "write to the home doc when recording",
                10,
            )
        })
        .collect();
    for (id, symbol) in [("writer", "write"), ("env", "home"), ("docs", "doc")] {
        let mut named = chunk(id, "write to the home doc when recording", 10);
        named.symbols = vec![symbol.to_string()];
        corpus.push(named);
    }

    let ranked = rank_text(
        "write the recommendation in /home/dkaponis/src/loom/doc",
        &corpus,
    );

    assert!(
        !ranked
            .iter()
            .any(|candidate| candidate.reasons.iter().any(|reason| matches!(
                reason,
                SelectionReason::ExactSymbol | SelectionReason::ExactPath
            ))),
        "no segment of a filesystem path may claim an exact rung: {ranked:?}"
    );
    assert!(
        ranked.is_empty(),
        "with every path segment stopworded and no rung admitted, the honest \
         answer to this prompt is nothing at all: {ranked:?}"
    );
}

/// The full-relative-path arm is deliberately ungated: nobody types
/// `src/context/pack.rs` into a prompt by accident, whatever its df.
#[test]
fn a_full_path_still_earns_the_exact_path_rung() {
    let mut corpus: Vec<KnowledgeChunk> = (0..8)
        .map(|index| {
            chunk(
                &format!("filler-{index}"),
                "packing budgets are described in src/context/pack.rs",
                10,
            )
        })
        .collect();
    corpus[0].source_paths = vec!["src/context/pack.rs".to_string()];

    let ranked = rank_text("look at src/context/pack.rs for the packer", &corpus);

    let packer = ranked
        .iter()
        .find(|candidate| candidate.id.as_str() == "filler-0")
        .expect("the chunk naming the path must be a candidate");
    assert!(
        packer.reasons.contains(&SelectionReason::ExactPath),
        "a whole path is always deliberate: {packer:?}"
    );
    assert_eq!(packer.confidence(), Confidence::High);
}

#[test]
fn backtick_spans_cover_the_bytes_between_paired_backticks() {
    assert_eq!(backtick_spans("a `b` c"), vec![(3, 4)]);
    assert_eq!(backtick_spans("`a` and `bb`"), vec![(1, 2), (9, 11)]);
    assert_eq!(
        backtick_spans("unclosed `tail"),
        Vec::new(),
        "a lone backtick opens nothing"
    );
    assert_eq!(backtick_spans("no ticks"), Vec::new());
}

#[test]
fn shape_recognizes_identifier_spellings_and_rejects_words() {
    for shaped in ["snake_case", "camelCase", "Foo::Bar", "fs::locking"] {
        assert!(is_shaped(shaped), "{shaped} is identifier-shaped");
    }
    for word in ["point", "Point", "write", "QUALITY", "doc"] {
        assert!(
            !is_shaped(word),
            "{word} is an ordinary word, capitalized or not"
        );
    }
}

#[test]
fn any_single_signal_admits_an_exact_rung() {
    let none = TermEvidence {
        backticked: false,
        shaped: false,
        rare: false,
    };
    assert!(!admits_exact(&none));
    for evidence in [
        TermEvidence {
            backticked: true,
            ..none
        },
        TermEvidence {
            shaped: true,
            ..none
        },
        TermEvidence { rare: true, ..none },
    ] {
        assert!(admits_exact(&evidence), "{evidence:?} admits on its own");
    }
}
