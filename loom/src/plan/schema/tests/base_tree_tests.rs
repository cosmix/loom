//! Base-tree evaluation of search criteria against a temporary repository.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::plan::schema::tests::make_stage;
use crate::plan::schema::types::{AcceptanceCriterion, StageDefinition, StageType, TruthCheck};
use crate::plan::schema::validation::base_tree::check_base_tree;

const UNTOUCHED: &str = "criterion passes on the untouched tree";

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository whose only commit tracks `src/lib.rs` with `fn present`.
fn repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn present() {}\n").unwrap();
    git(root, &["add", "src/lib.rs"]);
    git(root, &["commit", "-q", "-m", "base"]);
    temp
}

fn simple(command: &str) -> AcceptanceCriterion {
    AcceptanceCriterion::Simple(command.to_string())
}

fn expecting(command: &str, exit_code: i32) -> AcceptanceCriterion {
    AcceptanceCriterion::Extended(TruthCheck {
        command: command.to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: Some(exit_code),
        description: None,
    })
}

fn standard_stage(acceptance: Vec<AcceptanceCriterion>) -> StageDefinition {
    let mut stage = make_stage("feature", "Feature");
    stage.stage_type = Some(StageType::Standard);
    stage.acceptance = acceptance;
    stage
}

/// Warnings about criterion number `index` (1-based).
fn about(warnings: &[String], index: usize) -> Vec<&String> {
    let marker = format!("acceptance criterion #{index} `");
    warnings.iter().filter(|w| w.contains(&marker)).collect()
}

#[test]
fn warns_only_on_criteria_already_green_at_head() {
    let repo = repo();
    let stage = standard_stage(vec![
        simple(r#"rg -q "fn present" src/lib.rs"#),
        simple(r#"rg -q "fn absent" src/lib.rs"#),
        simple("rg -q anything src/new.rs"),
        expecting(r#"rg -q "fn present" src/lib.rs"#, 1),
        expecting(r#"rg -q "fn absent" src/lib.rs"#, 1),
        simple("rg -q present src"),
        simple("grep -qw present src/lib.rs"),
        simple("rg -q present src/lib.rs | cat"),
        simple("rg -qi PRESENT src/lib.rs"),
    ]);
    let report = check_base_tree(&[stage], Some(repo.path()));
    let warnings = &report.warnings;

    assert!(report.note.is_none());
    for green in [1, 5, 6, 7, 9] {
        let found = about(warnings, green);
        assert!(
            found.len() == 1 && found[0].contains(UNTOUCHED),
            "criterion #{green}: {warnings:?}"
        );
    }
    for quiet in [2, 3, 4, 8] {
        assert!(
            about(warnings, quiet).is_empty(),
            "criterion #{quiet}: {warnings:?}"
        );
    }
}

#[test]
fn reads_head_rather_than_the_checkout() {
    let repo = repo();
    fs::write(repo.path().join("src/lib.rs"), "pub fn later() {}\n").unwrap();
    let stage = standard_stage(vec![
        simple(r#"rg -q "fn present" src/lib.rs"#),
        simple(r#"rg -q "fn later" src/lib.rs"#),
    ]);
    let warnings = check_base_tree(&[stage], Some(repo.path())).warnings;
    assert_eq!(about(&warnings, 1).len(), 1, "{warnings:?}");
    assert!(about(&warnings, 2).is_empty(), "{warnings:?}");
}

#[test]
fn warns_that_worktrees_cannot_see_an_untracked_operand() {
    let repo = repo();
    fs::write(repo.path().join("notes.txt"), "present\n").unwrap();
    let stage = standard_stage(vec![simple("rg -q present notes.txt")]);
    let warnings = check_base_tree(&[stage], Some(repo.path())).warnings;
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("not tracked at HEAD"), "{warnings:?}");
}

#[test]
fn resolves_operands_against_the_working_dir() {
    let repo = repo();
    let mut stage = standard_stage(vec![simple("rg -q present lib.rs")]);
    stage.working_dir = "src".to_string();
    let warnings = check_base_tree(&[stage], Some(repo.path())).warnings;
    assert_eq!(about(&warnings, 1).len(), 1, "{warnings:?}");
}

#[test]
fn knowledge_stages_keep_repo_wide_gates() {
    let repo = repo();
    let mut stage = standard_stage(vec![simple("rg -q present src/lib.rs")]);
    stage.stage_type = Some(StageType::Knowledge);
    let report = check_base_tree(&[stage], Some(repo.path()));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
}

#[test]
fn skips_with_one_note_when_there_is_no_repository_root() {
    let stage = standard_stage(vec![simple("rg -q present src/lib.rs")]);
    let report = check_base_tree(&[stage], None);
    assert!(report.warnings.is_empty());
    let note = report.note.expect("a skipped check leaves a note");
    assert!(note.contains("no git repository"), "{note}");
}

#[test]
fn skips_with_one_note_when_head_does_not_resolve() {
    let temp = TempDir::new().unwrap();
    git(temp.path(), &["init", "-q", "-b", "main"]);
    let stage = standard_stage(vec![simple("rg -q present src/lib.rs")]);
    let report = check_base_tree(&[stage], Some(temp.path()));
    assert!(report.warnings.is_empty());
    assert!(report.note.is_some_and(|note| note.contains("HEAD")));
}
