//! Contracts for the seal-time restatement check: an overview sentence that repeats the member
//! sentence it cites, word for word or nearly, is rejected.

use std::fs;

use super::{seal, SealError};
use crate::planning::summary::evaluate::fixtures::{
    seal_document, two_sections, Fixture, ID1, ID2, PRODUCT,
};
use crate::planning::summary::format::sentence_id;
use crate::planning::summary::store::{link_from_summary, summary_path};

const A2: &str = "Alpha two holds.";
const VISION_LINK: &str = "[v](../../../../product/demo/index.md#specification-sections)";

/// A link from the product overview to the sentence of member `id` whose text is `text`.
fn member(id: &str, text: &str) -> String {
    format!(
        "[m]({}#{})",
        link_from_summary(PRODUCT, &summary_path(id)),
        sentence_id(text)
    )
}

/// Seals ID1 holding `member_text` and ID2 holding [`A2`], then seals a product overview
/// whose first sentence is `overview_text` citing ID1's sentence. The draft cites both members
/// and the vision, so the only possible defect is the restatement.
fn seal_overview_over(member_text: &str, overview_text: &str) -> (Fixture, Result<(), SealError>) {
    let f = two_sections();
    seal_document(&f.root, ID1, &[(member_text, "a")]);
    seal_document(&f.root, ID2, &[(A2, "a")]);
    let draft = format!(
        "# Overview\n\n{overview_text} {}\n\nThe second section is covered. {}\n\nVision. {VISION_LINK}\n",
        member(ID1, member_text),
        member(ID2, A2),
    );
    let path = f.paths.drafts.join(format!("{PRODUCT}.md"));
    fs::create_dir_all(path.parent().expect("parent")).expect("drafts dir");
    fs::write(&path, draft).expect("write draft");
    let result = seal(&f.root, &f.paths, PRODUCT, false).map(|_| ());
    (f, result)
}

/// Fails unless the seal was rejected for restating the member sentence `member_text`.
fn assert_restates(result: Result<(), SealError>, member_text: &str) {
    let messages: Vec<String> = match result {
        Err(SealError::Rejected(errors)) => errors.iter().map(ToString::to_string).collect(),
        other => panic!("expected a rejected seal, got {other:?}"),
    };
    let cited = sentence_id(member_text);
    assert!(
        messages
            .iter()
            .any(|m| m.contains("restates") && m.contains(&cited)),
        "no restatement of #{cited} among {messages:?}"
    );
}

#[test]
fn overview_restating_its_member_is_rejected() {
    let text = "Alpha one holds.";
    let (_f, result) = seal_overview_over(text, text);
    assert_restates(result, text);
}

#[test]
fn overview_near_restatement_is_rejected() {
    let member_text = "File mode is the planning app's reader and editor, adding server document, \
                       tree, save, diff and search routes plus in-process plan validation.";
    let overview_text =
        "File mode adds the server's document, tree, save, diff and search routes, \
                         with in-process plan validation.";
    let (_f, result) = seal_overview_over(member_text, overview_text);
    assert_restates(result, member_text);
}
