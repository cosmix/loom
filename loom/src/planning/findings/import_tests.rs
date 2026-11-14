use super::tests::{entry, ledger_of};
use super::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn project() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    let plan = root.join("doc/plans/demo/IN_PROGRESS-PLAN-first.md");
    fs::create_dir_all(plan.parent().unwrap()).unwrap();
    fs::write(&plan, "# First\n\nImplements: section_01#one, section_02\n").unwrap();
    (temp, root)
}

const PLAN: &str = "doc/plans/demo/IN_PROGRESS-PLAN-first.md";

fn review(findings: &[&str], questions: &[&str]) -> String {
    let bullets = |items: &[&str], empty: &str| {
        if items.is_empty() {
            return format!("{empty}\n");
        }
        items
            .iter()
            .map(|t| format!("- **Build** (`build`): {t}\n"))
            .collect::<String>()
    };
    format!(
        "# Code Review: First\n\n## Open Questions\n\n{}\n## Open Findings\n\n{}",
        bullets(questions, "No open questions."),
        bullets(findings, "No open findings.")
    )
}

fn ledger_text(root: &Path) -> String {
    fs::read_to_string(root.join("doc/plans/demo/findings.md")).unwrap()
}

#[test]
fn import_reads_findings_and_questions_with_their_sources() {
    let (_temp, root) = project();
    let text = review(&["Rename the flag"], &["Which flag?"]);
    assert_eq!(import_review(&root, Path::new(PLAN), &text).unwrap(), 2);
    let ledger = parse(&ledger_text(&root));
    assert_eq!(ledger.problems, Vec::new());
    let found = &ledger.findings;
    assert_eq!(
        found[0].about,
        vec!["PLAN-first.md#stage:build", "section_01#one", "section_02"]
    );
    assert!(found[0]
        .source
        .as_deref()
        .unwrap()
        .starts_with("REVIEW-PLAN-first.md#open-findings (build, "));
    assert!(found[1]
        .source
        .as_deref()
        .unwrap()
        .starts_with("REVIEW-PLAN-first.md#open-questions (build, "));
    assert_eq!(found[1].text, "Which flag?");
    assert_eq!(import_review(&root, Path::new(PLAN), &text).unwrap(), 0);
}

#[test]
fn import_keeps_existing_entries_and_adds_changed_items_as_new_ones() {
    let (_temp, root) = project();
    let existing = ledger_of(&[entry(
        "F-4",
        "Hand written",
        FindingStatus::Dismissed("kept as is".into()),
        "kept",
    )]);
    fs::write(root.join("doc/plans/demo/findings.md"), &existing).unwrap();

    let plan = Path::new(PLAN);
    assert_eq!(
        import_review(&root, plan, &review(&["First wording"], &[])).unwrap(),
        1
    );
    let after_first = ledger_text(&root);
    assert!(after_first.starts_with(&existing), "ledger:\n{after_first}");
    assert!(after_first.contains("\n## F-5 First wording\n"));

    assert_eq!(
        import_review(&root, plan, &review(&["First wording"], &[])).unwrap(),
        0
    );
    assert_eq!(
        import_review(&root, plan, &review(&["First wording, revised"], &[])).unwrap(),
        1
    );
    let ledger = parse(&ledger_text(&root));
    let ids: Vec<&str> = ledger.findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, vec!["F-4", "F-5", "F-6"]);
    assert_eq!(
        ledger.findings[0].status,
        FindingStatus::Dismissed("kept as is".into())
    );
}

#[test]
fn import_text_cannot_open_a_heading() {
    let (_temp, root) = project();
    import_review(&root, Path::new(PLAN), &review(&["## F-9 injected"], &[])).unwrap();
    let text = ledger_text(&root);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("## ")).count(),
        1,
        "{text}"
    );
    let parsed = parse(&text).findings;
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].text, "## F-9 injected");
}

#[test]
fn import_refuses_a_symlinked_ledger_file() {
    let (_temp, root) = project();
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("findings.md");
    fs::write(&target, "keep\n").unwrap();
    std::os::unix::fs::symlink(&target, root.join("doc/plans/demo/findings.md")).unwrap();
    assert!(import_review(&root, Path::new(PLAN), &review(&["x"], &[])).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep\n");
}

#[test]
fn import_refuses_a_symlinked_directory() {
    let (_temp, root) = project();
    let outside = TempDir::new().unwrap();
    fs::remove_dir_all(root.join("doc/plans/demo")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("doc/plans/demo")).unwrap();
    let result = import_review(&root, Path::new(PLAN), &review(&["x"], &[]));
    assert!(result.is_err());
    assert!(!outside.path().join("findings.md").exists());
}

#[test]
fn import_of_an_empty_review_writes_nothing() {
    let (_temp, root) = project();
    let text = review(&[], &[]);
    assert_eq!(import_review(&root, Path::new(PLAN), &text).unwrap(), 0);
    assert!(!root.join("doc/plans/demo/findings.md").exists());
}

#[test]
fn import_skips_a_reworded_repeat_and_anything_the_ledger_already_settled() {
    let (_temp, root) = project();
    let plan = Path::new(PLAN);
    let first = "web/src/a.ts:12 The reason renders twice on every open card; draw it once.";
    let again = "web/src/a.ts:40 The same reason renders twice on each open card, so draw it once.";
    let other = "web/src/a.ts:50 Confirm dismissal shows no busy state while the post runs.";
    let text = review(&[first, again, other], &[]);
    assert_eq!(import_review(&root, plan, &text).unwrap(), 2);
    let ledger = ledger_text(&root);
    let dismissed = set_status(&ledger, "F-1", &FindingStatus::Dismissed("fixed".into())).unwrap();
    fs::write(root.join("doc/plans/demo/findings.md"), dismissed).unwrap();
    let later =
        "web/src/a.ts:14 The reason is rendered twice on every open card; draw it once only.";
    assert_eq!(
        import_review(&root, plan, &review(&[later], &[])).unwrap(),
        0
    );
}
