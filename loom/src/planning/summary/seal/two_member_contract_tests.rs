//! Contract: an overview sentence that cites member sentences cites at least two of them, so
//! opening it shows more than one sentence.

use std::fs;

use super::{seal, SealError};
use crate::planning::summary::evaluate::fixtures::{
    seal_document, two_sections, ID1, ID2, PRODUCT,
};
use crate::planning::summary::format::sentence_id;
use crate::planning::summary::store::{link_from_summary, summary_path};

const A1: &str = "Alpha one holds.";
const A2: &str = "Alpha two holds.";
const B1: &str = "Beta one holds.";
const VISION_LINK: &str = "[v](../../../../product/demo/index.md#specification-sections)";

/// A link from the product overview to the sentence of member `id` whose text is `text`.
fn member(id: &str, text: &str) -> String {
    format!(
        "[m]({}#{})",
        link_from_summary(PRODUCT, &summary_path(id)),
        sentence_id(text)
    )
}

#[test]
fn an_overview_sentence_citing_one_member_sentence_is_rejected() {
    let f = two_sections();
    seal_document(&f.root, ID1, &[(A1, "a"), (A2, "b")]);
    seal_document(&f.root, ID2, &[(B1, "a")]);
    // Every member and the vision are cited; ID2 has one sentence, so citing it alone is
    // allowed. The only defect is the first sentence opening to one of ID1's two sentences.
    let draft = format!(
        "# Overview\n\nThe planner keeps one thing. {}\n\nThe second section is covered. {}\n\n\
         Vision. {VISION_LINK}\n",
        member(ID1, A1),
        member(ID2, B1),
    );
    let path = f.paths.drafts.join(format!("{PRODUCT}.md"));
    fs::create_dir_all(path.parent().expect("parent")).expect("drafts dir");
    fs::write(&path, draft).expect("write draft");

    let messages: Vec<String> = match seal(&f.root, &f.paths, PRODUCT, false) {
        Err(SealError::Rejected(errors)) => errors.iter().map(ToString::to_string).collect(),
        other => panic!("expected a rejected seal, got {:?}", other.map(|_| ())),
    };
    assert!(
        messages.iter().any(|m| m.contains("at least two")),
        "no 'at least two' rejection among {messages:?}"
    );
}
