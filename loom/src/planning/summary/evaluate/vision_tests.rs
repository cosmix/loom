//! A feature's vision summary: judged from its own cited spans and sealed under an eight-sentence cap.

use std::fs;
use std::path::Path;

use super::fixtures::*;
use super::*;
use crate::planning::summary::seal::{seal, SealError};

fn state(root: &Path) -> SummaryState {
    evaluate(&load(root), &[]).summaries[VISION_ID].state
}

#[test]
fn a_vision_summary_is_judged_from_its_own_cited_spans() {
    let fx = two_sections();
    seal_document(
        &fx.root,
        VISION_ID,
        &[("The vision lists sections.", "specification-sections")],
    );
    assert_eq!(state(&fx.root), SummaryState::Current);

    put(&fx.root, S1, &section(1, "Edited.", "Beta."));
    assert_eq!(state(&fx.root), SummaryState::Current, "a section edit");

    put(&fx.root, VISION, &vision(&[1]));
    assert_eq!(
        state(&fx.root),
        SummaryState::Stale,
        "an edit of the cited span"
    );
}

/// A draft of `count` distinct sentences, each citing the vision's section table.
fn draft(count: usize) -> String {
    let link = store::link_from_summary(VISION_ID, Path::new(VISION));
    let lines: Vec<_> = (0..count)
        .map(|i| {
            format!("Sentence number {i} says something new. [§]({link}#specification-sections)")
        })
        .collect();
    format!("# Vision\n\n{}\n", lines.join("\n"))
}

fn seal_draft(fx: &Fixture, count: usize) -> Result<(), SealError> {
    let path = fx.paths.drafts.join(format!("{VISION_ID}.md"));
    fs::create_dir_all(path.parent().expect("parent")).expect("drafts dir");
    fs::write(&path, draft(count)).expect("write draft");
    seal(&fx.root, &fx.paths, VISION_ID, false).map(|_| ())
}

#[test]
fn a_vision_draft_of_nine_sentences_is_refused_and_one_of_eight_seals() {
    let fx = two_sections();
    match seal_draft(&fx, 9) {
        Err(SealError::Rejected(errors)) => {
            let text = errors[0].to_string();
            assert!(text.contains("more than 8 sentences"), "{text}");
        }
        other => panic!("expected a rejected seal, got {other:?}"),
    }
    assert!(!fx.root.join(store::summary_path(VISION_ID)).exists());

    seal_draft(&fx, 8).expect("eight sentences seal");
    assert_eq!(state(&fx.root), SummaryState::Current);
}
