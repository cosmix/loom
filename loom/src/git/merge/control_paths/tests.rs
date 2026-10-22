//! The control-path gate: the pure path rules, the diff against scratch
//! repositories, and `merge_stage` holding or bypassing a gated branch.

use super::super::test_support::{commit_count, git_ok, git_out, init_repo, lock_dir, rev};
use super::super::{merge_stage, MergeResult};
use super::*;

/// Create `loom/<id>` from `main` with one commit adding every path in
/// `paths` (parent directories created), then return to `main`.
fn branch_adding(root: &Path, id: &str, paths: &[&str]) {
    git_ok(root, &["checkout", "-b", &format!("loom/{id}"), "main"]);
    for path in paths {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file, "x").unwrap();
        git_ok(root, &["add", "--", path]);
    }
    git_ok(root, &["commit", "-m", "stage work"]);
    git_ok(root, &["checkout", "main"]);
}

fn violation(root: &Path, id: &str) -> Option<String> {
    control_path_violation(root, "main", &format!("loom/{id}")).unwrap()
}

#[test]
fn control_paths_match_exact_names_and_everything_under_them() {
    for path in [
        ".claude",
        ".claude/x",
        ".CLAUDE/x",
        ".Claude",
        ".mcp.json",
        ".MCP.JSON",
        ".Mcp.json",
        ".loom",
        ".loom/work/x",
        ".LOOM/work/x",
    ] {
        assert!(is_control_path(path, None), "{path} is a control path");
    }
}

#[test]
fn hooks_directory_matches_exact_nested_and_upper_cased() {
    let prefix = Some("loom/.githooks/");
    for path in [
        "loom/.githooks",
        "loom/.githooks/pre-commit",
        "loom/.githooks/sub/x",
        "LOOM/.GitHooks/pre-commit",
    ] {
        assert!(is_control_path(path, prefix), "{path} is under the hooks");
        assert!(!is_control_path(path, None), "{path} without a hooks dir");
    }
    assert!(is_control_path("githooks/x", Some("./githooks/")));
}

#[test]
fn near_misses_are_not_control_paths() {
    for path in [
        ".claudex/y",
        "x/.claude/y",
        ".mcp.json.bak",
        ".loomy",
        "src/x.rs",
        "claude/x",
        "loom/.githooks-extra/x",
    ] {
        assert!(
            !is_control_path(path, Some("loom/.githooks/")),
            "{path} is not a control path"
        );
    }
}

#[test]
fn a_non_ascii_path_under_claude_is_detected() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "accent", &[".claude/\u{e9}.md"]);

    let reason = violation(root, "accent").expect("the quoted-path bypass must not recur");
    assert!(reason.contains(".claude/\u{e9}.md"), "{reason}");
}

#[test]
fn a_tab_or_newline_in_a_path_keeps_the_reason_on_one_line() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "ctl", &[".claude/a\tb.md", ".claude/c\nd.md"]);

    let reason = violation(root, "ctl").expect("control characters must not hide a path");
    assert_eq!(reason.lines().count(), 1, "{reason:?}");
    assert!(reason.contains("a\\tb.md") && reason.contains("c\\nd.md"));
}

#[test]
fn a_file_named_dot_loom_is_detected() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "dotloom", &[".loom"]);

    assert!(violation(root, "dotloom").is_some());
}

#[test]
fn an_ordinary_branch_has_no_violation() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "clean", &["src/ok.rs"]);

    assert_eq!(violation(root, "clean"), None);
}

#[test]
fn a_file_named_like_the_branch_ref_does_not_make_the_diff_ambiguous() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "s1", &["src/ok.rs"]);
    // The file shares the branch ref's path, which git refuses as ambiguous
    // unless the revisions end with `--`.
    std::fs::create_dir_all(root.join("refs/heads/loom")).unwrap();
    std::fs::write(root.join("refs/heads/loom/s1"), "x").unwrap();

    assert_eq!(violation(root, "s1"), None);
}

#[test]
fn a_gitlink_change_is_listed_even_when_gitmodules_ignores_it() {
    let repo = init_repo();
    let root = repo.path();
    let seed = rev(root, "main");
    let gitmodules = "[submodule \"sub\"]\n\tpath = sub\n\turl = ./sub\n\tignore = all\n";
    std::fs::write(root.join(".gitmodules"), gitmodules).unwrap();
    git_ok(root, &["add", ".gitmodules"]);
    let gitlink = |commit: &str| format!("160000,{commit},sub");
    git_ok(
        root,
        &["update-index", "--add", "--cacheinfo", &gitlink(&seed)],
    );
    git_ok(root, &["commit", "-m", "add sub"]);
    let before = rev(root, "main");
    git_ok(root, &["update-index", "--cacheinfo", &gitlink(&before)]);
    git_ok(root, &["commit", "-m", "move sub"]);

    let paths = changed_paths(root, &before, &rev(root, "main")).unwrap();

    assert_eq!(paths, vec!["sub"]);
}

#[test]
fn a_missing_revision_is_an_error_not_a_pass() {
    let repo = init_repo();

    assert!(control_path_violation(repo.path(), "main", "loom/absent").is_err());
}

#[test]
fn enforce_holds_a_control_path_branch_and_writes_nothing() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "s1", &[".claude/settings.json"]);
    let before = rev(root, "main");
    let commits = commit_count(root);
    let work = lock_dir();

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Enforce).unwrap();

    match result {
        MergeResult::Held { reason } => {
            assert!(reason.contains("loom/s1") && reason.contains(".claude/settings.json"));
        }
        other => panic!("expected Held, got {other:?}"),
    }
    assert_eq!(rev(root, "main"), before);
    assert_eq!(commit_count(root), commits);
}

#[test]
fn bypass_merges_a_control_path_branch() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "s1", &[".claude/settings.json"]);
    let work = lock_dir();

    let result = merge_stage("s1", "main", root, work.path(), MergeGate::Bypass).unwrap();

    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    assert!(git_out(root, &["ls-tree", "-r", "main", "--name-only"])
        .lines()
        .any(|path| path == ".claude/settings.json"));
}

#[test]
fn hooks_dir_prefix_reads_a_relative_core_hooks_path() {
    let repo = init_repo();
    let root = repo.path();
    git_ok(root, &["config", "core.hooksPath", "loom/.githooks"]);

    assert_eq!(hooks_dir_prefix(root), Some("loom/.githooks/".to_string()));
}

#[test]
fn hooks_dir_prefix_reads_a_worktree_scoped_core_hooks_path() {
    let repo = init_repo();
    let root = repo.path();
    git_ok(root, &["config", "extensions.worktreeConfig", "true"]);
    git_ok(
        root,
        &["config", "--worktree", "core.hooksPath", ".githooks"],
    );

    assert_eq!(hooks_dir_prefix(root), Some(".githooks/".to_string()));
    assert!(is_control_path(
        ".githooks/pre-commit",
        hooks_dir_prefix(root).as_deref()
    ));
}

#[test]
fn hooks_dir_prefix_is_none_when_unset() {
    let repo = init_repo();

    assert_eq!(hooks_dir_prefix(repo.path()), None);
}

#[test]
fn resolve_hooks_dir_prefix_local_beats_global() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, Some("local/hooks"), Some("global/hooks"), None),
        Some("local/hooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_uses_a_relative_global_when_local_is_unset() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, None, Some(".githooks"), None),
        Some(".githooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_falls_back_to_system_when_local_and_global_are_unset() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, None, None, Some("system/hooks")),
        Some("system/hooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_is_none_for_an_absolute_path_outside_the_repo() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, Some("/elsewhere/hooks"), None, None),
        None
    );
}

#[test]
fn resolve_hooks_dir_prefix_resolves_an_absolute_path_inside_the_repo() {
    let root = Path::new("/repo");
    assert_eq!(
        resolve_hooks_dir_prefix(root, Some("/repo/loom/.githooks"), None, None),
        Some("loom/.githooks/".to_string())
    );
}

#[test]
fn resolve_hooks_dir_prefix_is_none_when_every_scope_is_unset() {
    let root = Path::new("/repo");
    assert_eq!(resolve_hooks_dir_prefix(root, None, None, None), None);
}

#[test]
fn the_landing_tree_diff_names_control_paths() {
    let repo = init_repo();
    let root = repo.path();
    branch_adding(root, "gated", &[".claude/settings.json", "src/ok.rs"]);
    branch_adding(root, "plain", &["src/ok.rs"]);
    let tree_of = |id: &str| rev(root, &format!("refs/heads/loom/{id}^{{tree}}"));
    let old = rev(root, "refs/heads/main");

    let reason = tree_change_violation(root, &old, &tree_of("gated"), "loom/gated")
        .unwrap()
        .expect("a control path in the landing diff is held");
    assert!(reason.contains(".claude/settings.json"), "{reason}");
    assert!(!reason.contains("src/ok.rs"), "{reason}");
    assert_eq!(
        tree_change_violation(root, &old, &tree_of("plain"), "loom/plain").unwrap(),
        None
    );
}
