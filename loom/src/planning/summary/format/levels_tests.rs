//! Tests for the two levels of a body: self-links and the split.

use super::*;

const ID: &str = "plans/p";
const SOURCE: &str = "[§](../../../plans/PLAN-p.md#plan-p)";

fn body(text: &str) -> SummaryBody {
    parse_draft(ID, text).unwrap_or_else(|errors| panic!("{errors:?}"))
}

fn ids(sentences: &[&Sentence]) -> Vec<String> {
    sentences.iter().map(|s| s.id.clone()).collect()
}

#[test]
fn a_fragment_only_link_is_a_sentence_of_the_summary_itself() {
    let text = format!("# P\n\nP opens. [d](#s-1234abcd)\n\nP does p. {SOURCE}\n");
    let parsed = body(&text);
    let Block::Paragraph(opening) = &parsed.blocks[0] else {
        panic!("{parsed:?}")
    };
    assert_eq!(
        opening[0].links[0].resolved,
        LinkTarget::Sentence {
            summary: ID.into(),
            id: "s-1234abcd".into()
        }
    );
    assert!(cites_own(ID, &opening[0]));
    assert!(!cites_own("plans/q", &opening[0]), "another summary's id");
    let Block::Paragraph(detail) = &parsed.blocks[1] else {
        panic!("{parsed:?}")
    };
    assert!(!cites_own(ID, &detail[0]), "a source link");
}

#[test]
fn a_body_without_self_links_shows_every_block() {
    let text = format!("# P\n\nOne holds. {SOURCE}\n\n## Part\n\nTwo holds. {SOURCE}\n");
    let parsed = body(&text);
    let levels = split_levels(ID, &parsed.blocks);
    let all: Vec<&Block> = parsed.blocks.iter().collect();
    assert_eq!(levels.shown, all);
    assert!(levels.detail.is_empty());
}

#[test]
fn a_two_level_body_shows_headings_and_opening_paragraphs_and_keeps_the_rest_as_detail() {
    let (d1, d2, d3) = ("Parsing runs first.", "Checks follow.", "Risks stay low.");
    let text = format!(
        "# P\n\n## Overview\n\nP parses and checks. [d](#{}) [d](#{})\nP is safe. [d](#{})\n\n\
         {d1} {SOURCE}\n{d2} {SOURCE}\n\n{d3} {SOURCE}\n",
        sentence_id(d1),
        sentence_id(d2),
        sentence_id(d3)
    );
    let parsed = body(&text);
    let levels = split_levels(ID, &parsed.blocks);
    assert_eq!(levels.shown, vec![&parsed.blocks[0], &parsed.blocks[1]]);
    let want = [d1, d2, d3].map(sentence_id).to_vec();
    assert_eq!(ids(&levels.detail), want);
    let other = split_levels("plans/q", &parsed.blocks);
    assert_eq!(
        other.shown.len(),
        parsed.blocks.len(),
        "self-links of another id"
    );
}
