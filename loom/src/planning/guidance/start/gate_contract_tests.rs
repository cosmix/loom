//! Contract: a plan is written only once its feature's metaplan is settled at its current text.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::planning::guidance::{AuthoringStart, StartError};
use crate::planning::project::Project;
use crate::planning::trace::gate::metaplan_settled;
use crate::planning::trace::{settle, DocRole, SettleState, TraceDoc};

const METAPLAN: &str = "doc/plans/f/index.md";

/// Three entries with only the first plan written, so the checklist offers `write-plan:f-02`
/// and `draft-all-plans` (offered once two or more plans are missing).
const FILES: &[(&str, &str)] = &[
    (
        "doc/product/f/index.md",
        "# Vision\n\n## Specification sections\n\n\
        | # | Section |\n| --- | --- |\n| 05 | Concept mode |\n",
    ),
    ("doc/product/f/section_05.md", "# Concept mode\n\nText.\n"),
    (
        METAPLAN,
        "# Metaplan\n\n## 1. First (`PLAN-f-01.md`)\n\nOne.\n\n\
        ## 2. Second (`PLAN-f-02.md`)\n\nTwo.\n\n## 3. Third (`PLAN-f-03.md`)\n\nThree.\n",
    ),
    ("doc/plans/f/PLAN-f-01.md", "# Plan: f-01\n"),
];

fn write_files() -> TempDir {
    let temp = tempfile::tempdir().unwrap();
    for (path, text) in FILES {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    temp
}

fn metaplan_doc(project: &Project) -> &TraceDoc {
    let feature = project.feature("f").expect("feature f");
    let found = feature
        .trace
        .documents
        .iter()
        .find(|doc| doc.role == DocRole::Metaplan);
    found.expect("the metaplan is a trace document")
}

/// Write the metaplan's current `Settled:` line into its file and reload the project.
fn settle_metaplan(root: &Path) -> Project {
    let line = metaplan_doc(&Project::load(root).unwrap()).current.clone();
    let text = fs::read_to_string(root.join(METAPLAN)).unwrap();
    fs::write(root.join(METAPLAN), settle::mark(&text, &line)).unwrap();
    let project = Project::load(root).unwrap();
    assert_eq!(metaplan_doc(&project).settle, SettleState::Settled);
    project
}

fn start(project: &Project, id: &str) -> Result<AuthoringStart, StartError> {
    AuthoringStart::from_offer_id(id, project, Some("f"))
}

#[test]
fn write_plan_waits_for_a_settled_metaplan() {
    let temp = write_files();
    let root = temp.path();
    let settled = settle_metaplan(root);
    assert!(metaplan_settled(settled.feature("f").unwrap()));

    let text = fs::read_to_string(root.join(METAPLAN)).unwrap();
    fs::write(
        root.join(METAPLAN),
        format!("{text}\nAn edit after settling.\n"),
    )
    .unwrap();
    let edited = Project::load(root).unwrap();
    assert_eq!(metaplan_doc(&edited).settle, SettleState::Revised);
    assert!(!metaplan_settled(edited.feature("f").unwrap()));
    for id in ["write-plan:f-02", "draft-all-plans"] {
        assert_eq!(
            start(&edited, id),
            Err(StartError::MetaplanUnsettled),
            "{id} must wait for the edited metaplan to be settled again"
        );
    }
    assert_eq!(
        StartError::MetaplanUnsettled.to_string(),
        "Settle the metaplan first"
    );

    let resettled = settle_metaplan(root);
    assert!(metaplan_settled(resettled.feature("f").unwrap()));
    match start(&resettled, "write-plan:f-02") {
        Ok(AuthoringStart::Offer(offer)) => assert_eq!(offer.id(), "write-plan:f-02"),
        other => panic!("a settled metaplan lets write-plan:f-02 start, got {other:?}"),
    }
}
