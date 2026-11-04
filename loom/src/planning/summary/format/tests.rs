//! Tests for the summary body and file format.

use std::collections::BTreeMap;
use std::path::Path;

use super::*;

const LINK: &str = "[§](../../../../product/alpha/section_01.md#a)";

fn draft(body: &str) -> Result<SummaryBody, Vec<FormatError>> {
    parse_draft("alpha/product/section_01", body)
}

fn lines_of(errors: &[FormatError]) -> Vec<usize> {
    errors.iter().map(|e| e.line).collect()
}

fn sentences(body: &SummaryBody) -> Vec<&Sentence> {
    let mut out = Vec::new();
    for block in &body.blocks {
        if let Block::Paragraph(list) = block {
            out.extend(list);
        }
    }
    out
}

#[test]
fn accepts_title_headings_and_sentences() {
    let text =
        format!("# Title\n\n## Part\n\nOne. {LINK}\nTwo. {LINK}\n\n# Again\n\nThree. {LINK}\n");
    let body = draft(&text).unwrap();
    assert_eq!(body.title, "Title");
    assert_eq!(body.blocks.len(), 4);
    let list = sentences(&body);
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].line, 4, "0-based line of the first sentence");
    assert!(matches!(
        &body.blocks[0],
        Block::Heading {
            level: 2,
            line: 2,
            ..
        }
    ));
}

#[test]
fn rejects_each_line_kind_at_its_line_with_its_own_message() {
    // Every line ends in a citation link, so only the rule under test can
    // reject it: deleting that rule would let the line parse as a sentence.
    let rejected = [
        (format!("- item {LINK}"), "list items are not allowed"),
        (format!("* item {LINK}"), "list items are not allowed"),
        (format!("+ item {LINK}"), "list items are not allowed"),
        (format!("1. item {LINK}"), "list items are not allowed"),
        (format!("2) item {LINK}"), "list items are not allowed"),
        (format!("| a | b | {LINK}"), "table rows are not allowed"),
        (format!("``` {LINK}"), "code fences are not allowed"),
        (format!("```rust {LINK}"), "code fences are not allowed"),
        (format!("~~~ {LINK}"), "code fences are not allowed"),
        (
            format!("    indented {LINK}"),
            "indented lines are not allowed",
        ),
        (
            format!("\tindented {LINK}"),
            "indented lines are not allowed",
        ),
        (format!("> quote {LINK}"), "block quotes are not allowed"),
        (format!("<b>x</b> {LINK}"), "HTML is not allowed"),
        (format!("Text <!-- c --> {LINK}"), "HTML is not allowed"),
        (format!("Text </p> {LINK}"), "HTML is not allowed"),
        (
            format!("### Deep {LINK}"),
            "headings deeper than level 2 are not allowed",
        ),
        (
            format!("## Heading {LINK}"),
            "a heading must not contain a link",
        ),
    ];
    assert_rejected_at_line_3(&rejected);
}

#[test]
fn rejects_each_sentence_defect_at_its_line_with_its_own_message() {
    let rejected = [
        ("No citation here.".to_string(), "no citation link"),
        (format!("{LINK} only a link"), "no citation link"),
        (
            format!("Mid {LINK} text {LINK}"),
            "must not contain links before its citations",
        ),
        (LINK.to_string(), "sentence has no text"),
        ("Spaced [x](a b)".to_string(), "malformed link target"),
        ("Angle [x](<a.md>)".to_string(), "malformed link target"),
    ];
    assert_rejected_at_line_3(&rejected);
}

/// Each `(line, message)` drafted under a title is rejected at line 3 with `message`.
fn assert_rejected_at_line_3(rejected: &[(String, &str)]) {
    for (line, message) in rejected {
        let errors = draft(&format!("# T\n\n{line}\n")).expect_err(line);
        assert_eq!(lines_of(&errors), vec![3], "{line:?}: {errors:?}");
        assert!(
            errors[0].message.contains(message),
            "{line:?}: want {message:?}, got {errors:?}"
        );
    }
}

#[test]
fn link_scan_pairs_brackets_like_the_depth_count() {
    let links = |text: &str| -> Vec<(String, String)> {
        let body = draft(&format!("# T\n\n{text}\n")).unwrap();
        let first = sentences(&body)[0].clone();
        let pairs = first.links.into_iter();
        pairs.map(|l| (l.label, l.target)).collect()
    };
    let want = |label: &str| vec![(label.to_string(), "t".to_string())];
    assert_eq!(links("Text. [a [b] c](t)"), want("a [b] c"), "nested label");
    assert_eq!(links("Text ] more. [x](t)"), want("x"), "stray ] first");
    assert_eq!(links("Text [ more. [x](t)"), want("x"), "unmatched [ first");
    assert_eq!(links("Text. [a][b](t)"), want("b"), "adjacent brackets");
    // The outer pair is followed by an unclosed `(`, yet the inner link is
    // still scanned and its malformed target reported.
    let errors = draft("# T\n\nT [a [b](x y) c](t\n").unwrap_err();
    assert!(
        errors[0].message.contains("malformed link target"),
        "{errors:?}"
    );
}

/// Parses `text` as a draft and fails when that takes more than a generous
/// 2 s in a debug build; a quadratic scan needs minutes on these inputs.
fn assert_parses_fast(text: &str) {
    let started = std::time::Instant::now();
    let _ = draft(text);
    let took = started.elapsed();
    assert!(took.as_secs() < 2, "parse took {took:?}");
}

#[test]
fn link_scan_is_linear_on_bracket_heavy_lines() {
    let opens = "[".repeat(1 << 20);
    assert_parses_fast(&format!("# T\n\n{opens}\n"));
    let closes = "]".repeat(1 << 20);
    assert_parses_fast(&format!("# T\n\n{closes}\n"));
    let calls = "[a](".repeat(100_000);
    assert_parses_fast(&format!("# T\n\nText {calls}\n"));
    assert_parses_fast(&format!("# T\n\nText {calls})\n"));
    let nested = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
    assert_parses_fast(&format!("# T\n\nText {nested}\n"));
}

#[test]
fn html_in_a_link_target_is_not_html() {
    let ok = "Fine. [§](../x.md#<a)";
    assert!(
        draft(&format!("# T\n\n{ok}\n")).is_err(),
        "'<' in a target is malformed"
    );
    assert!(draft(&format!("# T\n\nA < b is fine. {LINK}\n")).is_ok());
}

#[test]
fn first_line_must_be_a_title() {
    let errors = draft(&format!("Text. {LINK}\n")).unwrap_err();
    assert_eq!(lines_of(&errors), vec![1]);
    assert!(draft("## T\n").is_err());
    assert!(draft("\n\n").is_err());
    assert!(
        draft(&format!("\n\n# T\n\nA. {LINK}\n")).is_ok(),
        "blank lines before the title"
    );
}

#[test]
fn trailing_links_keep_every_label() {
    let text = format!(
        "# T\n\nThe app keeps  no state. {LINK} [two](../../../../product/alpha/section_02.md#b)\n"
    );
    let body = draft(&text).unwrap();
    let sentence = sentences(&body)[0].clone();
    assert_eq!(sentence.text, "The app keeps no state.");
    let labels: Vec<_> = sentence.links.iter().map(|l| l.label.as_str()).collect();
    assert_eq!(labels, ["§", "two"]);
    assert_eq!(
        sentence.links[1].target,
        "../../../../product/alpha/section_02.md#b"
    );
}

#[test]
fn sentence_id_matches_the_contract() {
    let hash = crate::context::source_graph::body_hash(b"The app keeps no state.");
    let want = format!("s-{}", &hash["sha256:".len()..][..8]);
    assert_eq!(sentence_id("The app keeps no state."), want);
    assert_eq!(sentence_id("The app  keeps\tno state.  "), want);
}

#[test]
fn links_are_classified() {
    let text = "# T\n\nA. [s](../../../product/alpha/section_01.md#a)\nB. [t](../alpha/product/section_02.md#s-1234abcd)\nC. [u](https://x.org/a.md) [v](../../..)\n";
    let body = parse_draft("plans/p", text).unwrap();
    let list = sentences(&body);
    assert!(
        matches!(&list[0].links[0].resolved, LinkTarget::Source { path, anchor }
        if path == Path::new("doc/product/alpha/section_01.md") && anchor == "a")
    );
    assert_eq!(
        list[1].links[0].resolved,
        LinkTarget::Sentence {
            summary: "alpha/product/section_02".into(),
            id: "s-1234abcd".into()
        }
    );
    assert!(matches!(
        list[2].links[0].resolved,
        LinkTarget::Unresolved(_)
    ));
}

#[test]
fn duplicate_ids_name_both_lines() {
    let errors = draft(&format!("# T\n\nSame. {LINK}\nSame.  [x](../y.md)\n")).unwrap_err();
    assert_eq!(lines_of(&errors), vec![4]);
    assert!(errors[0].message.contains("line 3"), "{errors:?}");
}

#[test]
fn sentence_caps_depend_on_the_id() {
    let many = |n: usize| {
        let lines: Vec<_> = (0..n).map(|i| format!("S{i}. {LINK}")).collect();
        format!("# T\n\n{}\n", lines.join("\n"))
    };
    assert!(parse_draft("plans/p", &many(15)).is_ok());
    let errors = parse_draft("plans/p", &many(16)).unwrap_err();
    assert_eq!(lines_of(&errors), vec![18]);
    assert!(parse_draft("overview/alpha/product", &many(30)).is_ok());
    assert!(parse_draft("overview/alpha/product", &many(31)).is_err());
    assert!(parse_draft("overview/alpha/engineering", &many(30)).is_ok());
    assert!(parse_draft("overview/alpha/engineering", &many(31)).is_err());
}

#[test]
fn a_vision_draft_holds_at_most_eight_sentences() {
    let many = |n: usize| {
        let lines: Vec<_> = (0..n).map(|i| format!("S{i}. {LINK}")).collect();
        format!("# T\n\n{}\n", lines.join("\n"))
    };
    assert!(parse_draft("alpha/product/vision", &many(8)).is_ok());
    let errors = parse_draft("alpha/product/vision", &many(9)).unwrap_err();
    assert_eq!(lines_of(&errors), vec![11]);
    for (id, limit) in [
        ("alpha/product/vision", MAX_VISION_SENTENCES),
        ("alpha/product/section_01", MAX_DOCUMENT_SENTENCES),
        ("plans/p", MAX_DOCUMENT_SENTENCES),
        ("overview/alpha/product", MAX_OVERVIEW_SENTENCES),
    ] {
        assert_eq!(sentence_limit(id), limit, "{id}");
    }
    assert_eq!(MAX_VISION_SENTENCES, 8);
}

#[test]
fn draft_with_front_matter_or_conflicts_is_rejected() {
    assert_eq!(
        lines_of(&draft("---\nkind: document\n---\n").unwrap_err()),
        vec![1]
    );
    let text = format!("# T\n\nA. {LINK}\n<<<<<<< ours\nB. {LINK}\n=======\n>>>>>>> theirs\n");
    assert_eq!(lines_of(&draft(&text).unwrap_err()), vec![4, 6, 7]);
}

fn file(key: &str) -> SummaryFile {
    let cites = BTreeMap::from([(key.to_string(), "sha256:ab".to_string())]);
    let front = FrontMatter {
        kind: SummaryKind::Document,
        of: "doc/product/alpha/section_01.md".into(),
        file: Some("sha256:cd".into()),
        cites,
        members: BTreeMap::new(),
    };
    let body = draft(&format!(
        "# T\n\n## H\n\nOne. {LINK}\nTwo. {LINK}\n\nThree. {LINK}\n"
    ))
    .unwrap();
    SummaryFile {
        front,
        title: body.title,
        blocks: body.blocks,
    }
}

#[test]
fn file_round_trips_with_awkward_keys() {
    let rel = Path::new("doc/loom/summaries/alpha/product/section_01.md");
    for key in [
        "doc/product/alpha/section_01.md#a",
        "doc/p.md#a: b",
        "doc/p.md#q\"uote",
        "doc/p.md",
    ] {
        let original = file(key);
        let text = render(&original);
        assert_eq!(text, render(&original), "render is stable");
        let parsed = parse(rel, &text).unwrap_or_else(|e| panic!("{key}: {e:?}\n{text}"));
        assert_eq!(parsed.front, original.front, "{key}");
        assert_eq!(
            render(&parsed),
            text,
            "{key}: parse then render is a fixpoint"
        );
    }
}

#[test]
fn parse_rejects_bad_files() {
    let rel = Path::new("doc/loom/summaries/alpha/product/section_01.md");
    let good = render(&file("doc/p.md#a"));
    assert!(parse(rel, &good).is_ok());
    assert!(
        parse(rel, &good.replacen("---\n", "", 1)).is_err(),
        "front matter missing"
    );
    assert!(
        parse(Path::new("doc/other.md"), &good).is_err(),
        "outside the store"
    );
    let conflict = good.replace("Two.", "=======\nTwo.");
    let errors = parse(rel, &conflict).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "merge conflict marker");
    let line = good.lines().position(|l| l.starts_with("One.")).unwrap();
    let bad = good.replace("One.", "- One.");
    assert_eq!(
        lines_of(&parse(rel, &bad).unwrap_err()),
        vec![line + 1],
        "file line numbers"
    );
}
