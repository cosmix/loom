//! Tests for the brief.

use super::brief;
use crate::planning::summary::claims::{create_claim, list_claims};
use crate::planning::summary::evaluate::fixtures::{
    fixture, put, seal_document, section, two_sections, write_file, ID1, ID2, S1,
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
    assert!(text.contains("loom plan summary seal product/section_01"));
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
fn long_sentence_keeps_its_citation_links_whole() {
    let f = two_sections();
    let sentence = format!("Alpha {} holds.", "word ".repeat(60));
    seal_document(&f.root, ID1, &[(sentence.as_str(), "a")]);
    let text = brief(&f.root, &f.paths, &[ID1.into()], None).unwrap();
    let line = text.lines().find(|l| l.contains("Alpha word")).unwrap();
    assert!(line.chars().count() > 200);
    assert!(line.contains("](") && line.trim_end().ends_with(')'));
}
