//! Contract: Plan follow-ups starts for any feature with a metaplan, done or not.

use std::fs;

use tempfile::TempDir;

use crate::planning::checklist::Offer;
use crate::planning::guidance::{opening_message, AuthoringStart, StartError};
use crate::planning::project::Project;

const FILES: &[(&str, &str)] = &[
    (
        "doc/product/f/index.md",
        "# Vision\n\n## Specification sections\n\n\
        | # | Section |\n| --- | --- |\n| 05 | Concept mode |\n",
    ),
    ("doc/product/f/section_05.md", "# Concept mode\n"),
    (
        "doc/plans/f/index.md",
        "# Metaplan\n\n## 1. First (`PLAN-f-01.md`)\n",
    ),
    ("doc/plans/f/PLAN-f-01.md", "# Plan: f-01\n"),
    ("doc/product/g/index.md", "# G\n"),
];

fn project() -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    for (path, text) in FILES {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

#[test]
fn plan_follow_ups_starts_for_a_feature_with_a_metaplan_done_or_not() {
    let (_temp, project) = project();
    let checklist = &project.feature("f").expect("feature f").checklist;
    let offered: Vec<String> = checklist
        .primary
        .iter()
        .chain(&checklist.secondary)
        .map(Offer::id)
        .collect();
    assert!(
        !offered.iter().any(|id| id == "plan-follow-ups"),
        "f has a written plan, so its checklist must not offer Plan follow-ups: {offered:?}"
    );

    let start = AuthoringStart::from_offer_id("plan-follow-ups", &project, Some("f"));
    let start = match start {
        Ok(start @ AuthoringStart::Offer(_)) => start,
        other => panic!("expected an offer start for f, got {other:?}"),
    };
    if let AuthoringStart::Offer(offer) = &start {
        assert_eq!(offer.id(), "plan-follow-ups");
    }
    let opening = opening_message(&start, &project, Some("f")).expect("opening for f");
    assert!(opening.contains("doc/plans/f/index.md"), "{opening}");

    let without_metaplan = AuthoringStart::from_offer_id("plan-follow-ups", &project, Some("g"));
    assert!(
        matches!(without_metaplan, Err(StartError::NotOffered(_))),
        "a feature without a metaplan is refused: {without_metaplan:?}"
    );
    assert_eq!(
        AuthoringStart::from_offer_id("plan-follow-ups", &project, None),
        Err(StartError::FeatureRequired)
    );
}
