use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

use crate::git::signing::tests::{fake_signer, git_in};

#[path = "tests_merge.rs"]
mod merge;
#[path = "tests_paths.rs"]
mod paths;

const BRANCH: &str = "refs/heads/loom/s1";

/// A repository with one commit on `main`, checked out on `loom/s1`, with
/// signing off and the openpgp format pinned repo-locally.
fn repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_in(root, &["init", "-b", "main"]);
    git_in(root, &["config", "user.email", "t@t.com"]);
    git_in(root, &["config", "user.name", "t"]);
    git_in(root, &["config", "commit.gpgsign", "false"]);
    git_in(root, &["config", "gpg.format", "openpgp"]);
    stage(root, "README.md", "base\n");
    git_in(root, &["commit", "-m", "init"]);
    git_in(root, &["checkout", "-b", "loom/s1"]);
    temp
}

fn stage(root: &Path, path: &str, content: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
    git_in(root, &["add", "-f", path]);
}

/// Stage a gitlink at `path` naming the current HEAD.
fn stage_gitlink(root: &Path, path: &str) {
    let head = git_in(root, &["rev-parse", "HEAD"]);
    let entry = format!("160000,{head},{path}");
    git_in(root, &["update-index", "--add", "--cacheinfo", &entry]);
}

/// Commit `path` on `main`, staying on `loom/s1`.
fn commit_on_main(root: &Path, path: &str) {
    git_in(root, &["checkout", "main"]);
    stage(root, path, "main\n");
    git_in(root, &["commit", "-m", "main change"]);
    git_in(root, &["checkout", "loom/s1"]);
}

fn request(root: &Path, message: &str) -> CommitRequest {
    CommitRequest {
        message: message.to_string(),
        expected_head: git_in(root, &["rev-parse", "HEAD"]),
        expected_tree: git_in(root, &["write-tree"]),
    }
}

fn stage_scope() -> CommitScope {
    CommitScope::StageBranch {
        stage_id: "s1".to_string(),
    }
}

fn merge_scope() -> CommitScope {
    CommitScope::Merge {
        stage_id: "s1".to_string(),
    }
}

fn tip(root: &Path, reference: &str) -> String {
    git_in(root, &["rev-parse", reference])
}

/// The reason of a refused commit; panics on anything else.
fn reason(result: Result<String, CommitRefusal>) -> String {
    match result {
        Err(CommitRefusal::Refused { reason }) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Commit `request` in `scope` and expect a refusal mentioning `needle`,
/// with `BRANCH` unmoved.
fn assert_refused_request(root: &Path, scope: &CommitScope, request: &CommitRequest, needle: &str) {
    let before = tip(root, BRANCH);
    let refusal = reason(commit_staged(root, scope, request));
    assert!(refusal.contains(needle), "{refusal}");
    assert_eq!(tip(root, BRANCH), before, "a refusal moved {BRANCH}");
}

/// [`assert_refused_request`] for the current index.
fn assert_refused(root: &Path, scope: &CommitScope, needle: &str) {
    assert_refused_request(root, scope, &request(root, "feat(s1): x"), needle);
}

#[test]
fn commits_the_staged_tree_with_head_as_parent() {
    let temp = repo();
    let root = temp.path();
    let head = tip(root, BRANCH);
    stage(root, "a.txt", "a\n");
    let request = request(root, "feat(s1): add a.txt");

    let id = commit_staged(root, &stage_scope(), &request).unwrap();

    assert_eq!(tip(root, BRANCH), id);
    assert_eq!(tip(root, &format!("{id}^{{tree}}")), request.expected_tree);
    assert_eq!(tip(root, &format!("{id}^")), head);
    let subject = git_in(root, &["log", "-1", "--format=%s", &id]);
    assert_eq!(subject, "feat(s1): add a.txt");
}

#[test]
fn commits_through_a_pinned_worktree() {
    let temp = repo();
    let root = temp.path();
    git_in(root, &["checkout", "main"]);
    let worktrees = TempDir::new().unwrap();
    let worktree = worktrees.path().join("s1");
    let path = worktree.to_str().unwrap();
    git_in(root, &["worktree", "add", path, "loom/s1"]);
    stage(&worktree, "a.txt", "a\n");
    let git = WorktreeGit::pinned(root, &worktree).unwrap();

    let id = Committer::new(&git, root)
        .commit_staged(&stage_scope(), &request(&worktree, "feat(s1): a"))
        .unwrap();

    assert_eq!(tip(root, BRANCH), id);
}

#[test]
fn signs_when_gpgsign_is_true() {
    let temp = repo();
    let root = temp.path();
    let log = fake_signer(root, false);
    stage(root, "a.txt", "a\n");

    let id = commit_staged(root, &stage_scope(), &request(root, "feat(s1): a")).unwrap();

    let object = git_in(root, &["cat-file", "commit", &id]);
    let signed = object.lines().any(|line| line.starts_with("gpgsig "));
    assert!(signed, "{object}");
    assert_eq!(fs::read_to_string(log).unwrap().lines().count(), 1);
}

#[test]
fn signing_failure_leaves_the_ref_unmoved() {
    let temp = repo();
    let root = temp.path();
    let before = tip(root, BRANCH);
    fake_signer(root, true);
    stage(root, "a.txt", "a\n");

    let result = commit_staged(root, &stage_scope(), &request(root, "feat(s1): a"));

    let Err(CommitRefusal::Signing { detail }) = result else {
        panic!("expected a signing refusal, got {result:?}");
    };
    assert!(detail.contains("fake signer refused"), "{detail}");
    assert_eq!(tip(root, BRANCH), before);
}

#[test]
fn refuses_a_moved_head() {
    let temp = repo();
    let root = temp.path();
    let mut stale = request(root, "feat(s1): c");
    stage(root, "b.txt", "b\n");
    git_in(root, &["commit", "-m", "moved"]);
    stage(root, "c.txt", "c\n");
    stale.expected_tree = git_in(root, &["write-tree"]);

    assert_refused_request(root, &stage_scope(), &stale, "HEAD moved");
}

#[test]
fn refuses_a_tree_mismatch() {
    let temp = repo();
    let root = temp.path();
    stage(root, "a.txt", "a\n");
    let seen = request(root, "feat(s1): a");
    stage(root, "b.txt", "b\n");

    assert_refused_request(root, &stage_scope(), &seen, "the index changed");
}

#[test]
fn refuses_the_wrong_branch() {
    let temp = repo();
    stage(temp.path(), "a.txt", "a\n");
    let other = CommitScope::StageBranch {
        stage_id: "s2".to_string(),
    };

    assert_refused(temp.path(), &other, "refs/heads/loom/s2");
}

#[test]
fn refuses_a_detached_head() {
    let temp = repo();
    git_in(temp.path(), &["checkout", "--detach"]);
    stage(temp.path(), "a.txt", "a\n");

    assert_refused(temp.path(), &stage_scope(), "detached");
}

#[test]
fn refuses_a_gitlink() {
    let temp = repo();
    stage_gitlink(temp.path(), "sub");

    assert_refused(temp.path(), &stage_scope(), "gitlink");
}

#[test]
fn refuses_a_staged_state_path() {
    let temp = repo();
    stage(temp.path(), ".loom/work/x", "state\n");

    assert_refused(temp.path(), &stage_scope(), ".loom/work/x");
}

#[test]
fn refuses_a_staged_state_path_in_another_case() {
    let temp = repo();
    stage(temp.path(), ".LOOM/work/x", "state\n");

    assert_refused(temp.path(), &stage_scope(), ".LOOM/work/x");
}

#[test]
fn merge_scope_accepts_the_targets_gitlink() {
    let temp = repo();
    let root = temp.path();
    git_in(root, &["checkout", "main"]);
    stage_gitlink(root, "sub");
    git_in(root, &["commit", "-m", "main adds a gitlink"]);
    git_in(root, &["checkout", "loom/s1"]);
    git_in(root, &["merge", "--no-commit", "--no-ff", "main"]);
    stage_gitlink(root, "other");

    assert_refused(root, &merge_scope(), "other");

    git_in(root, &["update-index", "--force-remove", "other"]);
    let id = commit_staged(root, &merge_scope(), &request(root, "merge main")).unwrap();
    let entry = git_in(root, &["ls-tree", &id, "sub"]);
    assert!(entry.starts_with("160000 commit"), "{entry}");
}

#[test]
fn refuses_merge_head_outside_a_merge_scope() {
    let temp = repo();
    let root = temp.path();
    commit_on_main(root, "m.txt");
    git_in(root, &["merge", "--no-commit", "--no-ff", "main"]);

    assert_refused(root, &stage_scope(), "MERGE_HEAD");
}

#[test]
fn merge_scope_takes_merge_head_as_second_parent_and_quits() {
    let temp = repo();
    let root = temp.path();
    commit_on_main(root, "m.txt");
    let head = tip(root, BRANCH);
    let target = tip(root, "main");
    git_in(root, &["merge", "--no-commit", "--no-ff", "main"]);

    let id = commit_staged(root, &merge_scope(), &request(root, "merge main")).unwrap();

    assert_eq!(tip(root, BRANCH), id);
    assert_eq!(tip(root, &format!("{id}^1")), head);
    assert_eq!(tip(root, &format!("{id}^2")), target);
    assert!(!root.join(".git/MERGE_HEAD").exists());
}

#[test]
fn merge_scope_without_merge_head_refuses() {
    let temp = repo();
    stage(temp.path(), "a.txt", "a\n");

    assert_refused(temp.path(), &merge_scope(), "MERGE_HEAD");
}

fn knowledge_scope() -> CommitScope {
    CommitScope::Knowledge {
        target_branch: "main".to_string(),
        prefix: PathBuf::from("doc/loom/knowledge"),
    }
}

#[test]
fn knowledge_scope_refuses_a_path_outside_the_prefix() {
    let temp = repo();
    let root = temp.path();
    git_in(root, &["checkout", "main"]);
    let before = tip(root, "main");
    stage(root, "doc/loom/knowledge/a.md", "a\n");
    stage(root, "doc/loom/knowledge-extra/b.md", "b\n");

    let docs = request(root, "docs: k");

    let refusal = reason(commit_staged(root, &knowledge_scope(), &docs));

    assert!(refusal.contains("knowledge-extra/b.md"), "{refusal}");
    assert_eq!(tip(root, "main"), before);
}

#[test]
fn knowledge_scope_commits_under_the_prefix() {
    let temp = repo();
    let root = temp.path();
    git_in(root, &["checkout", "main"]);
    stage(root, "doc/loom/knowledge/a.md", "a\n");

    let id = commit_staged(root, &knowledge_scope(), &request(root, "docs: k")).unwrap();

    assert_eq!(tip(root, "refs/heads/main"), id);
}

#[test]
fn refuses_an_empty_commit() {
    let temp = repo();

    assert_refused(temp.path(), &stage_scope(), "nothing to commit");
}

#[test]
fn refuses_a_bad_message_or_object_id() {
    let temp = repo();
    let root = temp.path();
    stage(root, "a.txt", "a\n");
    let good = request(root, "feat(s1): a");
    let mut bad = vec![good.clone(), good.clone(), good.clone(), good.clone()];
    bad[0].message = "feat: a\n\nCo-Authored-By: Claude <noreply@anthropic.com>".to_string();
    bad[1].expected_head = "HEAD".to_string();
    bad[2].expected_tree = good.expected_tree.to_uppercase();
    bad[3].expected_head = format!("--{}", &good.expected_head[2..]);

    for request in &bad {
        assert_refused_request(root, &stage_scope(), request, "");
    }

    commit_staged(root, &stage_scope(), &good).unwrap();
}

#[test]
fn planted_hooks_never_run() {
    let temp = repo();
    let root = temp.path();
    let markers = TempDir::new().unwrap();
    let hooks = root.join(".git").join("hooks");
    let names = [
        "pre-commit",
        "commit-msg",
        "post-commit",
        "reference-transaction",
    ];
    for name in names {
        let hook = hooks.join(name);
        let marker = markers.path().join(name);
        let body = format!("#!/bin/sh\ntouch '{}'\n", marker.display());
        fs::write(&hook, body).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git_in(root, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
    stage(root, "a.txt", "a\n");

    commit_staged(root, &stage_scope(), &request(root, "feat(s1): a")).unwrap();

    for name in names {
        assert!(!markers.path().join(name).exists(), "hook {name} ran");
    }
}

#[test]
fn a_stale_old_value_fails_the_update_ref() {
    let temp = repo();
    let root = temp.path();
    let head = tip(root, BRANCH);
    let tree = tip(root, "HEAD^{tree}");
    let new = git_in(root, &["commit-tree", &tree, "-p", &head, "-m", "new"]);
    let stale = git_in(root, &["commit-tree", &tree, "-m", "stale"]);
    let git = WorktreeGit::discovered(root);

    let refusal = Committer::new(&git, root).move_ref(BRANCH, &new, &stale, "feat: x");

    let refused = matches!(refusal, Err(CommitRefusal::Refused { .. }));
    assert!(refused, "{refusal:?}");
    assert_eq!(tip(root, BRANCH), head);
}
