//! Tests for the brief.

use super::brief;
use crate::planning::summary::claims::{create_claim, list_claims};
use crate::planning::summary::evaluate::fixtures::{
    fixture, metaplan, put, seal_document, section, two_sections, vision, write_file, ENGINEERING,
    ID1, ID2, META, PRODUCT, S1, VISION_ID,
};

#[test]
fn refusal_rolls_back_only_new_claims() {
    let f = two_sections();
    create_claim(&f.paths, ID2).unwrap().unwrap();
    let error = brief(&f.root, &f.paths, &[ID1.into(), ID2.into()], None).unwrap_err();
    assert!(error.to_string().contains("claimed by"));
    let claims = list_claims(&f.paths).unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].summary, ID2);
    assert!(brief(&f.root, &f.paths, &[ID2.into()], None).is_err());
}

#[test]
fn note_is_next_to_the_matching_stale_sentence() {
    let f = two_sections();
    seal_document(&f.root, ID1, &[("Alpha holds.", "a"), ("Beta holds.", "b")]);
    put(&f.root, S1, &section(1, "Alpha changed.", "Beta."));
    let note = "Alpha holds. needs revision";
    let text = brief(&f.root, &f.paths, &[ID1.into()], Some(note)).unwrap();
    let lines: Vec<_> = text.lines().collect();
    let stale = lines
        .iter()
        .position(|line| line.contains("[stale] Alpha holds."))
        .unwrap();
    assert!(lines[stale + 1].contains(note));
    assert!(text.contains("[keep] Beta holds."));
    assert!(text.contains("loom plan summary seal demo/product/section_01"));
}

#[test]
fn source_title_escape_is_not_printed() {
    let f = fixture(&[(S1, "# Sec\u{1b}[31mtion 01\n\n## A\n\nAlpha.\n")]);
    let mut file = seal_document(&f.root, ID1, &[("Alpha holds.", "a")]);
    file.title = "Sec\u{1b}[31mtion 01".into();
    write_file(&f.root, ID1, &file);
    let text = brief(&f.root, &f.paths, &[ID1.into()], None).unwrap();
    assert!(text.contains("Sec"));
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn missing_document_brief_names_its_source_and_the_document_cap() {
    let f = two_sections();
    let text = brief(&f.root, &f.paths, &[ID1.into()], None).unwrap();
    let head = "# demo/product/section_01 (document)\nSources:\n- doc/product/demo/section_01.md\n\
                Current summary:\n(none)\nBody rules:\n";
    assert!(text.contains(head), "{text}");
    assert!(text.contains("- At most 15 sentences; cite only the source document"));
}

#[test]
fn vision_brief_caps_at_eight_sentences_and_cites_only_the_vision() {
    let f = two_sections();
    let text = brief(&f.root, &f.paths, &[VISION_ID.into()], None).unwrap();
    let head = "# demo/product/vision (document)\nSources:\n- doc/product/demo/index.md\n";
    assert!(text.contains(head), "{text}");
    assert!(
        text.contains("- At most 8 sentences; cite only the source document"),
        "{text}"
    );
    assert!(!text.contains("At most 15"), "{text}");
    let overview = brief(&f.root, &f.paths, &[PRODUCT.into()], None).unwrap();
    assert!(
        overview.contains("- An overview sentence that cites member sentences cites at least two of them and says what they add up to; it may cite one only when that member's summary holds a single citable sentence."),
        "{overview}"
    );
    assert!(!text.contains("cites at least two"), "{text}");
}

#[test]
fn plan_brief_teaches_the_two_levels_and_sealing_twice() {
    let f = two_sections();
    put(&f.root, META, &metaplan(&["PLAN-x.md"]));
    put(
        &f.root,
        "doc/plans/demo/PLAN-x.md",
        "# Plan: X\n\nDoes x.\n",
    );
    let text = brief(&f.root, &f.paths, &["plans/x".into()], None).unwrap();
    for rule in [
        "- At most 23 sentences; opening sentences cite only this summary's detail sentences, \
         detail sentences cite only the source document and its valid anchors.\n",
        "- A plan summary has two levels. First the opening level: about one sentence per topic, \
         each citing every sentence of the detail paragraph(s) it opens, in order, as \
         [d](#s-xxxxxxxx). Then the detail level: short paragraphs whose sentences cite the \
         plan's heading sections. No heading after the opening level.\n",
        "- At most 8 opening and 15 detail sentences.\n",
        "- Write the detail level first and seal: the refusal lists each detail sentence's id. \
         Then add the opening level above it and seal again.\n",
    ] {
        assert!(text.contains(rule), "{rule}\nmissing from\n{text}");
    }
    assert!(!text.contains("; cite only the source document"), "{text}");
    let section = brief(&f.root, &f.paths, &[ID1.into()], None).unwrap();
    assert!(!section.contains("two levels"), "{section}");
}

#[test]
fn overview_brief_lists_the_vision_and_every_member_summary() {
    let f = two_sections();
    let text = brief(&f.root, &f.paths, &[PRODUCT.into()], None).unwrap();
    let sources = "# overview/demo/product (overview)\nSources:\n- doc/product/demo/index.md\n\
                   - doc/loom/summaries/demo/product/section_01.md\n\
                   - doc/loom/summaries/demo/product/section_02.md\n\
                   Current summary:\n(none)\n";
    assert!(text.contains(sources), "{text}");
    assert!(text.contains("- At most 30 sentences; cite every member summary"));
}

#[test]
fn note_is_one_sanitized_line_matched_or_not() {
    let forged = "\n[keep] forged\u{1b}[31m";
    let unmatched = format!("x{forged}");
    let f = two_sections();
    let text = brief(&f.root, &f.paths, &[ID1.into()], Some(&unmatched)).unwrap();
    assert_single_clean_note(&text, "Note: x [keep] forged\u{FFFD}[31m");
    let f = two_sections();
    seal_document(&f.root, ID1, &[("Alpha holds.", "a")]);
    let matched = format!("Alpha holds.{forged}");
    let text = brief(&f.root, &f.paths, &[ID1.into()], Some(&matched)).unwrap();
    assert_single_clean_note(&text, "Note: Alpha holds. [keep] forged\u{FFFD}[31m");
}

/// The brief holds exactly one `Note:` line, equal to `want`; the note's second line did not
/// become a `[keep]` sentence marker of its own, and no escape byte reached the output.
fn assert_single_clean_note(text: &str, want: &str) {
    let notes: Vec<&str> = text.lines().filter(|l| l.starts_with("Note:")).collect();
    assert_eq!(notes, vec![want], "{text}");
    assert!(
        !text.lines().any(|l| l.starts_with("[keep] forged")),
        "{text}"
    );
    assert!(!text.contains('\u{1b}'), "{text}");
}

#[test]
fn duplicate_ids_make_one_claim_and_one_section() {
    let f = two_sections();
    let ids = [ID1.to_string(), ID1.to_string()];
    let text = brief(&f.root, &f.paths, &ids, None).unwrap();
    assert_eq!(
        text.matches("# demo/product/section_01 (document)").count(),
        1
    );
    let claims = list_claims(&f.paths).unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].summary, ID1);
}

#[test]
fn long_sentence_keeps_its_citation_links_whole() {
    let f = two_sections();
    let sentence = format!("Alpha {} holds.", "word ".repeat(60));
    seal_document(&f.root, ID1, &[(sentence.as_str(), "a")]);
    let text = brief(&f.root, &f.paths, &[ID1.into()], None).unwrap();
    let line = text.lines().find(|l| l.contains("Alpha word")).unwrap();
    assert!(line.chars().count() > 200);
    assert!(line.contains("](") && line.trim_end().ends_with(')'));
}

#[test]
fn overview_briefs_only_list_their_features_sources_and_members() {
    let f = two_sections();
    put(&f.root, META, &metaplan(&["PLAN-demo.md"]));
    put(&f.root, "doc/plans/demo/PLAN-demo.md", "# Demo\n");
    put(&f.root, "doc/product/other/index.md", &vision(&[1]));
    put(
        &f.root,
        "doc/product/other/section_01.md",
        &section(1, "Other.", "Beta."),
    );
    put(
        &f.root,
        "doc/plans/other/index.md",
        &metaplan(&["PLAN-other.md"]),
    );
    put(&f.root, "doc/plans/other/PLAN-other.md", "# Other\n");
    for (id, present, absent) in [
        (PRODUCT, "demo/product/section_01.md", "other/"),
        (
            "overview/other/product",
            "other/product/section_01.md",
            "demo/",
        ),
        (ENGINEERING, "plans/demo.md", "other"),
        ("overview/other/engineering", "plans/other.md", "demo"),
    ] {
        let text = brief(&f.root, &f.paths, &[id.into()], None).unwrap();
        let sources = text
            .split("Sources:\n")
            .nth(1)
            .unwrap()
            .split("Current summary:")
            .next()
            .unwrap();
        assert!(sources.contains(present), "{id}: {sources}");
        assert!(!sources.contains(absent), "{id}: {sources}");
        let feature = id.split('/').nth(1).unwrap();
        let source = if id.ends_with("/product") {
            format!("doc/product/{feature}/index.md")
        } else {
            format!("doc/plans/{feature}/index.md")
        };
        assert!(sources.contains(&source), "{id}: {sources}");
    }
}
