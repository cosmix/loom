use super::*;
use std::fs;
use tempfile::TempDir;

pub(in crate::planning::guidance) fn fixture() -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    let files = [
        (
            "doc/product/f/index.md",
            "# Vision\n\n## Specification sections\n\n\
             | # | Section |\n| --- | --- |\n| 07 | Zeta |\n| 01 | Written |\n\
             | 03 | Alpha |\n| 09 | Omega |\n| 02 | Beta |\n| 06 | Gamma |\n| 04 | Delta |\n",
        ),
        ("doc/product/f/section_01.md", "# Written\n"),
        (
            "doc/plans/f/index.md",
            "# Metaplan\n\n## Zeta (`PLAN-zeta.md`)\n\n## Written (`PLAN-written.md`)\n\n\
             ## Running (`PLAN-running.md`)\n\n## Done (`PLAN-done.md`)\n\n\
             ## Retired (`PLAN-retired.md`)\n\n## Alpha (`PLAN-alpha.md`)\n\n\
             ## Follow-ups\n\n## Omega (`PLAN-omega.md`)\n\n\
             ## Beta (`PLAN-beta.md`)\n\n## Gamma (`PLAN-gamma.md`)\n\n## Delta (`PLAN-delta.md`)\n",
        ),
        ("doc/plans/f/PLAN-written.md", "# Written\n"),
        ("doc/plans/f/IN_PROGRESS-PLAN-running.md", "# Running\n"),
        ("doc/plans/f/DONE-PLAN-done.md", "# Done\n"),
        ("doc/plans/f/RETIRED-PLAN-retired.md", "# Retired\n"),
        (
            "doc/product/g/index.md",
            "# Other feature\n\n## Specification sections\n\n\
             | # | Section |\n| --- | --- |\n| 10 | Other |\n| 11 | Also other |\n",
        ),
    ];
    for (path, text) in files {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    settle_metaplan(temp.path());
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

/// Plan drafting waits for a settled metaplan, so the fixture settles its own.
fn settle_metaplan(root: &std::path::Path) {
    use crate::planning::trace::{settle, DocRole};
    let project = Project::load(root).unwrap();
    let doc = project
        .feature("f")
        .unwrap()
        .trace
        .documents
        .iter()
        .find(|doc| doc.role == DocRole::Metaplan)
        .unwrap();
    let path = root.join("doc/plans/f/index.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, settle::mark(&text, &doc.current)).unwrap();
}

#[test]
fn sections_include_only_missing_documents_in_vision_order_with_five_teammates() {
    let (_temp, project) = fixture();

    let run = draft_run(&project, "draft-all-sections", "f").unwrap();

    assert_eq!(
        run,
        DraftRun {
            feature: "f".into(),
            plans: false,
            documents: vec![
                "doc/product/f/section_07.md".into(),
                "doc/product/f/section_03.md".into(),
                "doc/product/f/section_09.md".into(),
                "doc/product/f/section_02.md".into(),
                "doc/product/f/section_06.md".into(),
                "doc/product/f/section_04.md".into(),
            ],
            bundles: vec![],
            teammates: 5,
        }
    );
    assert_eq!(MAX_TEAMMATES, 5);
}

#[test]
fn plans_include_only_missing_documents_and_bundles_in_metaplan_order() {
    let (_temp, project) = fixture();

    let run = draft_run(&project, "draft-all-plans", "f").unwrap();

    assert_eq!(
        run,
        DraftRun {
            feature: "f".into(),
            plans: true,
            documents: vec![
                "doc/plans/f/PLAN-zeta.md".into(),
                "doc/plans/f/PLAN-alpha.md".into(),
                "doc/plans/f/PLAN-omega.md".into(),
                "doc/plans/f/PLAN-beta.md".into(),
                "doc/plans/f/PLAN-gamma.md".into(),
                "doc/plans/f/PLAN-delta.md".into(),
            ],
            bundles: vec![
                "doc/plans/f/briefs/zeta".into(),
                "doc/plans/f/briefs/alpha".into(),
                "doc/plans/f/briefs/omega".into(),
                "doc/plans/f/briefs/beta".into(),
                "doc/plans/f/briefs/gamma".into(),
                "doc/plans/f/briefs/delta".into(),
            ],
            teammates: 5,
        }
    );
}

#[test]
fn teammates_follow_document_count_below_the_cap_and_stay_in_the_selected_feature() {
    let (_temp, project) = fixture();

    let run = draft_run(&project, "draft-all-sections", "g").unwrap();

    assert_eq!(
        run.documents,
        [
            PathBuf::from("doc/product/g/section_10.md"),
            PathBuf::from("doc/product/g/section_11.md"),
        ]
    );
    assert_eq!(run.teammates, 2);
}

#[test]
fn draft_run_requires_a_current_draft_all_offer_in_the_selected_feature() {
    let (_temp, project) = fixture();
    for (id, feature) in [
        ("draft-all-sections", "absent"),
        ("draft-all-plans", "g"),
        ("write-section:07", "f"),
        ("kickoff", "f"),
        ("unknown", "f"),
    ] {
        assert_eq!(
            draft_run(&project, id, feature),
            Err(StartError::NotOffered(id.into())),
        );
    }
}

#[test]
fn a_single_missing_section_or_plan_does_not_offer_a_team_run() {
    let (temp, _) = fixture();
    for (id, paths) in [
        (
            "draft-all-sections",
            vec![
                "doc/product/f/section_03.md",
                "doc/product/f/section_09.md",
                "doc/product/f/section_02.md",
                "doc/product/f/section_06.md",
                "doc/product/f/section_04.md",
            ],
        ),
        (
            "draft-all-plans",
            vec![
                "doc/plans/f/PLAN-alpha.md",
                "doc/plans/f/PLAN-omega.md",
                "doc/plans/f/PLAN-beta.md",
                "doc/plans/f/PLAN-gamma.md",
                "doc/plans/f/PLAN-delta.md",
            ],
        ),
    ] {
        for path in paths {
            fs::write(temp.path().join(path), "# Written\n").unwrap();
        }
        let project = Project::load(temp.path()).unwrap();

        assert_eq!(
            draft_run(&project, id, "f"),
            Err(StartError::NotOffered(id.into())),
        );
    }
}
