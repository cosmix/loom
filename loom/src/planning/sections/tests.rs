//! Tests for heading sections.

use super::{github_slug, normalized_hash, sections_of, DocSections};

/// `(anchor, start, end)` for every heading, in order.
fn layout(s: &DocSections) -> Vec<(&str, usize, usize)> {
    s.headings
        .iter()
        .map(|h| (h.anchor.as_str(), h.span.start_line, h.span.end_line))
        .collect()
}

fn bounds(s: &DocSections, anchor: &str) -> Option<(usize, usize)> {
    s.find(anchor).map(|span| (span.start_line, span.end_line))
}

#[test]
fn slug_examples_match_github() {
    let cases = [
        ("4.2 Line Grammar", "42-line-grammar"),
        ("Plans and their lifecycle", "plans-and-their-lifecycle"),
        ("`loom plan check`", "loom-plan-check"),
        ("01 — Workflow Model", "01--workflow-model"),
        ("Symlinks under `doc/`", "symlinks-under-doc"),
        ("See [the spec](https://x.y/a_b) now", "see-the-spec-now"),
        ("snake_case and *Émphasis*", "snake_case-and-émphasis"),
    ];
    for (title, want) in cases {
        assert_eq!(github_slug(title), want, "{title}");
    }
}

#[test]
fn repeated_slugs_skip_taken_anchors() {
    let s = sections_of("## A\n## A\n## A-1\n## A");
    let anchors: Vec<&str> = s.headings.iter().map(|h| h.anchor.as_str()).collect();
    assert_eq!(anchors, ["a", "a-1", "a-1-1", "a-2"]);
}

#[test]
fn heading_on_first_line_leaves_empty_preamble() {
    let s = sections_of("# Title\nbody\n## Sub\nmore\n");
    assert_eq!((s.preamble.start_line, s.preamble.end_line), (0, 0));
    assert_eq!(s.preamble.hash, normalized_hash(""));
    assert_eq!(layout(&s), [("title", 0, 4), ("sub", 2, 4)]);
}

#[test]
fn text_without_headings_is_all_preamble() {
    let s = sections_of("one\ntwo\nthree\n");
    assert!(s.headings.is_empty());
    assert_eq!(bounds(&s, ""), Some((0, 3)));
    assert_eq!(s.preamble.hash, s.file_hash);
}

#[test]
fn empty_text_has_empty_preamble() {
    let s = sections_of("");
    assert_eq!(bounds(&s, ""), Some((0, 0)));
    assert!(s.headings.is_empty() && s.metadata.is_none() && s.stages.is_empty());
}

#[test]
fn closing_sequence_and_empty_headings() {
    let s = sections_of("## Title ##\n##\n### C#\n## ##\n");
    let titles: Vec<&str> = s.headings.iter().map(|h| h.title.as_str()).collect();
    assert_eq!(titles, ["Title", "C#"]);
    assert_eq!(layout(&s), [("title", 0, 1), ("c", 2, 3)]);
    assert_eq!(s.headings[1].level, 3);
}

#[test]
fn tilde_fence_closes_only_on_a_long_enough_run() {
    let text = "# T\n~~~~\n## In\n~~~\n## Still in\n~~~~~\n## Out\n";
    let s = sections_of(text);
    assert_eq!(layout(&s), [("t", 0, 7), ("out", 6, 7)]);
}

#[test]
fn unclosed_fence_degrades_to_fence_unaware() {
    let s = sections_of("# T\n```\n## Inside\ntext\n");
    assert_eq!(layout(&s), [("t", 0, 4), ("inside", 2, 4)]);
}

#[test]
fn crlf_gives_the_same_spans_and_hashes_as_lf() {
    let lf = "intro\n# Doc\n\n## A\nalpha \n```\n# x\n```\n## B\nbeta\n";
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(sections_of(lf), sections_of(&crlf));
}

#[test]
fn span_hash_covers_its_lines_with_a_final_newline() {
    let s = sections_of("pre\n# A\nbody");
    assert_eq!(s.preamble.hash, normalized_hash("pre\n"));
    assert_eq!(s.headings[0].span.hash, normalized_hash("# A\nbody\n"));
    assert_eq!(s.file_hash, normalized_hash("pre\n# A\nbody"));
}

#[test]
fn normalized_hash_keeps_the_final_newline_state() {
    assert_eq!(normalized_hash("a \t\r\nb\rc"), normalized_hash("a\nb\nc"));
    assert_ne!(normalized_hash("a\n"), normalized_hash("a"));
    assert!(normalized_hash("a").starts_with("sha256:"));
}

const PLAN: &str = "# Plan\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  \
                    stages:\n    - id: alpha\n      name: Alpha\n    - id: \"beta-2\"\n      \
                    name: Beta\n```\n\n<!-- END loom METADATA -->\n\n## Notes\nText.\n";

#[test]
fn plan_stages_include_a_quoted_id() {
    let s = sections_of(PLAN);
    assert_eq!(bounds(&s, "loom-metadata"), Some((2, 15)));
    assert_eq!(bounds(&s, "stage:alpha"), Some((8, 10)));
    assert_eq!(bounds(&s, "stage:beta-2"), Some((10, 12)));
    let ids: Vec<&str> = s.stages.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["alpha", "beta-2"]);
}

#[test]
fn find_resolves_every_anchor_kind() {
    let s = sections_of(PLAN);
    assert_eq!(bounds(&s, ""), Some((0, 0)));
    assert_eq!(bounds(&s, "plan"), Some((0, 18)));
    assert_eq!(bounds(&s, "notes"), Some((16, 18)));
    assert_eq!(s.find("stage:alpha"), Some(&s.stages[0].1));
    assert_eq!(s.find("loom-metadata"), s.metadata.as_ref());
    assert_eq!(bounds(&s, "stage:gamma"), None);
    assert_eq!(bounds(&s, "Notes"), None, "anchors compare exactly");
    assert_eq!(
        bounds(&sections_of("# Loom metadata\n"), "loom-metadata"),
        None
    );
}
