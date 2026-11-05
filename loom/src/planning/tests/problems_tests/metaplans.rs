//! Metaplan entry diagnostics.

use super::{found, tree, Path, Project};

#[test]
fn each_index_reports_entries_without_plan_files_and_other_files_report_none() {
    let text = "# Metaplan\n\n## 1. Missing plan\n";
    let temp = tree(&[
        ("doc/plans/a/index.md", text),
        ("doc/plans/a/metaplan-02.md", text),
        ("doc/plans/b/index.md", text),
        ("doc/plans/index.md", text),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let code = "metaplan-entry-without-plan";
    let want = [
        (code, Path::new("doc/plans/a/index.md"), Some(3)),
        (code, Path::new("doc/plans/b/index.md"), Some(3)),
        (code, Path::new("doc/plans/index.md"), Some(3)),
    ];
    assert_eq!(found(&project, &[code]), want);
}

#[test]
fn foreign_plans_report_the_entry_line_and_actual_home() {
    let metaplan = "doc/plans/a/index.md";
    let text = "# Metaplan\n\n## 1. Standalone (`PLAN-solo.md`)\n\n\
        ## 2. Foreign (`PLAN-other.md`)\n\n## 3. Archived (`PLAN-old.md`)\n\n\
        ## 4. Local (`PLAN-local.md`)\n\n## 5. Missing (`PLAN-missing.md`)\n";
    let temp = tree(&[
        (metaplan, text),
        ("doc/plans/PLAN-solo.md", "# Solo\n"),
        ("doc/plans/b/PLAN-other.md", "# Other\nImplements: none\n"),
        ("doc/plans/b/archive/DONE-PLAN-old.md", "# Old\n"),
        ("doc/plans/a/PLAN-local.md", "# Local\nImplements: none\n"),
    ]);
    let project = Project::load(temp.path()).unwrap();
    let code = "metaplan-entry-foreign-plan";
    assert_eq!(
        found(&project, &[code]),
        [
            (code, Path::new(metaplan), Some(3)),
            (code, Path::new(metaplan), Some(5)),
            (code, Path::new(metaplan), Some(7)),
        ]
    );
    let messages: Vec<_> = project
        .problems
        .iter()
        .filter(|problem| problem.code == code)
        .map(|problem| problem.message.as_str())
        .collect();
    assert_eq!(messages, [
        "plan solo is listed here but lives at doc/plans/PLAN-solo.md; move the plan into doc/plans/a/ or list it in its own metaplan",
        "plan other is listed here but lives at doc/plans/b/PLAN-other.md; move the plan into doc/plans/a/ or list it in its own metaplan",
        "plan old is listed here but lives at doc/plans/b/archive/DONE-PLAN-old.md; move the plan into doc/plans/a/ or list it in its own metaplan",
    ]);
}

#[test]
fn locked_metaplans_report_no_entry_problems() {
    let path = Path::new("doc/plans/a/index.md");
    let temp = tree(&[
        (
            "doc/plans/a/index.md",
            "# Metaplan\n\n## 1. Missing\n\n## 2. Foreign (`PLAN-x.md`)\n",
        ),
        ("doc/plans/b/PLAN-x.md", "# X\nImplements: none\n"),
    ]);
    let mut project = Project::load(temp.path()).unwrap();
    let codes = ["metaplan-entry-without-plan", "metaplan-entry-foreign-plan"];
    assert_eq!(found(&project, &codes).len(), 2);

    project.docs.get_mut(path).unwrap().lock = Some(crate::planning::paths::LockReason::Archive);
    project.problems = crate::planning::problems::collect(&project);

    assert!(found(&project, &codes).is_empty());
}
