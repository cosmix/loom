//! The resolver's exit folds the main checkout's local approvals into the
//! loom-owned list, and never reads the resolver worktree's own settings.

use super::super::resolver_spawn::test_fixtures::{
    orchestrator_with_conflict, repo_with_stage_branches,
};
use crate::fs::permissions::approved::approved_path;

const RULE: &str = "Bash(cargo test:*)";

fn write_rule(dir: &std::path::Path) {
    let claude = dir.join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join("settings.local.json"),
        format!(r#"{{"permissions":{{"allow":["{RULE}"]}}}}"#),
    )
    .unwrap();
}

#[test]
fn approvals_in_the_main_checkout_are_recorded() {
    let repo = repo_with_stage_branches(&["s"]);
    let orchestrator = orchestrator_with_conflict(repo.path(), "s");
    write_rule(repo.path());

    orchestrator.fold_back_resolver_permissions();

    let approved = std::fs::read_to_string(approved_path(&orchestrator.config.work_dir)).unwrap();
    assert!(approved.contains(RULE), "{approved}");
}

#[test]
fn the_resolver_worktree_settings_are_not_a_source() {
    let repo = repo_with_stage_branches(&["s"]);
    let orchestrator = orchestrator_with_conflict(repo.path(), "s");
    write_rule(&repo.path().join(".worktrees").join("s"));

    orchestrator.fold_back_resolver_permissions();

    assert!(!approved_path(&orchestrator.config.work_dir).exists());
}
