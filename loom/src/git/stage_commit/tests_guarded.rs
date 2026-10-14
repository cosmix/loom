//! A merge commit and the plan's write denies: a protected path holds git's
//! merge result, or one side of it where git could not merge, never anything
//! else.

use super::*;
use crate::git::signing::tests::isolated_env;
use std::process::Command;

#[path = "tests_guarded_shell.rs"]
mod shell;
#[path = "tests_guarded_ways_out.rs"]
mod ways_out;

use shell::follow_printed;

const AUTO_BASE: &str = "a\nb\nc\nd\ne\nf\ng\n";
const AUTO_OURS: &str = "A\nb\nc\nd\ne\nf\ng\n";
const AUTO_THEIRS: &str = "a\nb\nc\nd\ne\nf\nG\n";
const AUTO_MERGED: &str = "A\nb\nc\nd\ne\nf\nG\n";

/// What every command a refusal prints starts with.
const RESTORE: &str = "git --literal-pathspecs restore --source=";

/// The globs the plan protects.
fn specs() -> Vec<String> {
    vec!["doc/specs/**".to_string()]
}

/// `loom/s1` merging `main`, `--no-commit`, with `doc/specs` protected and
/// `src` not. Both hold an `auto.txt` the two sides edited in different
/// places (git merges it cleanly) and a `conf.txt` they edited in the same
/// place (a conflict). `doc/specs/gone.txt` is edited on `loom/s1` and deleted
/// on `main`, a conflict too. `doc/specs/removed.txt` is deleted on `main` and
/// untouched on `loom/s1`: git deletes it cleanly.
fn protected_merge() -> TempDir {
    let temp = repo();
    let root = temp.path();
    let edit = |auto: &str, conf: &str| {
        for dir in ["doc/specs", "src"] {
            stage(root, &format!("{dir}/auto.txt"), auto);
            stage(root, &format!("{dir}/conf.txt"), conf);
        }
    };
    edit(AUTO_BASE, "base\n");
    stage(root, "doc/specs/gone.txt", "base\n");
    stage(root, "doc/specs/removed.txt", "base\n");
    git_in(root, &["commit", "-m", "base"]);
    git_in(root, &["branch", "-f", "main", "HEAD"]);

    edit(AUTO_OURS, "ours\n");
    stage(root, "doc/specs/gone.txt", "ours\n");
    git_in(root, &["commit", "-m", "ours"]);

    git_in(root, &["checkout", "main"]);
    edit(AUTO_THEIRS, "theirs\n");
    git_in(
        root,
        &[
            "rm",
            "-q",
            "--",
            "doc/specs/gone.txt",
            "doc/specs/removed.txt",
        ],
    );
    git_in(root, &["commit", "-m", "theirs"]);
    git_in(root, &["checkout", "loom/s1"]);

    begin_merge(root);
    temp
}

/// Start merging `main` into the checked-out branch without committing. The
/// merge conflicts by design, which `git_in` would panic on.
fn begin_merge(root: &Path) {
    let output = Command::new("git")
        .args(["merge", "--no-commit", "--no-ff", "main"])
        .current_dir(root)
        .env_clear()
        .envs(isolated_env(root))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// Settle `path`: take `main`'s version, or stage `content` there.
fn settle(root: &Path, path: &str, content: Option<&str>) {
    match content {
        Some(text) => stage(root, path, text),
        None => {
            git_in(root, &["checkout", "--theirs", "--", path]);
            git_in(root, &["add", "--", path]);
        }
    }
}

/// Put `path` back to the entry `rev` holds: `HEAD` is ours, `main` theirs.
fn take_side(root: &Path, rev: &str, path: &str) {
    git_in(root, &["checkout", rev, "--", path]);
}

/// Resolve every conflict of [`protected_merge`]: the protected and the
/// unprotected `conf.txt` as given (`None` takes `main`'s), `gone.txt` deleted
/// as `main` deleted it.
fn resolve(root: &Path, protected: Option<&str>, unprotected: Option<&str>) {
    settle(root, "doc/specs/conf.txt", protected);
    settle(root, "src/conf.txt", unprotected);
    git_in(root, &["rm", "-q", "--", "doc/specs/gone.txt"]);
}

/// Commit the index as a merge of `main`, guarding `globs`.
fn commit_guarding(root: &Path, globs: &[String]) -> Result<String, CommitRefusal> {
    let git = WorktreeGit::discovered(root);
    Committer::new(&git, root)
        .merging_into("main")
        .guarding_paths(globs)
        .commit_staged(&merge_scope(), &request(root, "merge main"))
}

/// The refusal of a guarded commit, with `BRANCH` unmoved.
fn guarded_refusal(root: &Path) -> String {
    let before = tip(root, BRANCH);
    let refusal = reason(commit_guarding(root, &specs()));
    assert_eq!(tip(root, BRANCH), before, "a refusal moved {BRANCH}");
    refusal
}

#[test]
fn a_protected_path_taking_theirs_and_gits_clean_merge_commits() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, None);

    let id = commit_guarding(root, &specs()).unwrap();

    assert_eq!(
        git_in(root, &["show", &format!("{id}:doc/specs/conf.txt")]),
        "theirs"
    );
    assert_eq!(
        git_in(root, &["show", &format!("{id}:doc/specs/auto.txt")]),
        AUTO_MERGED.trim_end()
    );
}

#[test]
fn a_protected_path_taking_ours_commits() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, None);
    settle(root, "doc/specs/conf.txt", Some("ours\n"));

    let id = commit_guarding(root, &specs()).unwrap();

    assert_eq!(
        git_in(root, &["show", &format!("{id}:doc/specs/conf.txt")]),
        "ours"
    );
}

#[test]
fn a_protected_path_with_new_content_is_refused_naming_the_way_out() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, Some("mine\n"), None);

    let refusal = guarded_refusal(root);

    assert!(refusal.contains("'doc/specs/conf.txt'"), "{refusal}");
    assert!(refusal.contains(&format!("{RESTORE}HEAD ")), "{refusal}");
    assert!(
        refusal.contains(&format!("{RESTORE}MERGE_HEAD ")),
        "{refusal}"
    );
    assert!(!refusal.contains("auto.txt"), "{refusal}");
    assert!(!refusal.contains("src/"), "{refusal}");
}

#[test]
fn an_unprotected_path_with_new_content_commits() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, Some("mine\n"));

    let id = commit_guarding(root, &specs()).unwrap();

    assert_eq!(
        git_in(root, &["show", &format!("{id}:src/conf.txt")]),
        "mine"
    );
}

#[test]
fn new_content_commits_when_no_path_is_guarded() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, Some("mine\n"), None);

    commit_guarding(root, &[]).unwrap();
}

#[test]
fn a_clean_merge_of_a_protected_path_edited_further_is_refused() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, None);
    stage(root, "doc/specs/auto.txt", &format!("{AUTO_MERGED}extra\n"));

    let refusal = guarded_refusal(root);

    assert!(refusal.contains("'doc/specs/auto.txt'"), "{refusal}");
    assert!(refusal.contains(RESTORE), "{refusal}");
    assert!(!refusal.contains("--source=HEAD"), "{refusal}");
    assert!(!refusal.contains("--source=MERGE_HEAD"), "{refusal}");
}

#[test]
fn taking_either_side_of_a_cleanly_merged_protected_path_is_refused() {
    for (rev, side) in [("HEAD", "ours"), ("main", "theirs")] {
        let temp = protected_merge();
        let root = temp.path();
        resolve(root, None, None);
        take_side(root, rev, "doc/specs/auto.txt");

        let refusal = guarded_refusal(root);

        assert!(
            refusal.contains("'doc/specs/auto.txt'"),
            "{side}: {refusal}"
        );
        assert!(refusal.contains(RESTORE), "{side}: {refusal}");
    }
}

#[test]
fn restoring_the_merge_result_the_refusal_names_lets_the_commit_through() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, None);
    take_side(root, "HEAD", "doc/specs/auto.txt");
    let refusal = guarded_refusal(root);

    follow_printed(root, &refusal, None, "doc/specs/auto.txt");
    let id = commit_guarding(root, &specs()).unwrap();

    assert_eq!(
        git_in(root, &["show", &format!("{id}:doc/specs/auto.txt")]),
        AUTO_MERGED.trim_end()
    );
}

#[test]
fn deleting_a_protected_path_neither_side_deleted_is_refused() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, None);
    git_in(root, &["rm", "-q", "-f", "--", "doc/specs/auto.txt"]);

    let refusal = guarded_refusal(root);

    assert!(refusal.contains("'doc/specs/auto.txt'"), "{refusal}");
    follow_printed(root, &refusal, None, "doc/specs/auto.txt");
    commit_guarding(root, &specs()).unwrap();
}

#[test]
fn bringing_back_a_protected_path_git_deleted_cleanly_is_refused_until_it_is_removed_again() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, None);
    take_side(root, "HEAD", "doc/specs/removed.txt");

    let refusal = guarded_refusal(root);

    assert!(refusal.contains("'doc/specs/removed.txt'"), "{refusal}");
    follow_printed(root, &refusal, None, "doc/specs/removed.txt");
    let id = commit_guarding(root, &specs()).unwrap();
    let listed = git_in(root, &["ls-tree", "--name-only", &id, "doc/specs/"]);
    assert!(!listed.contains("removed.txt"), "{listed}");
}

#[test]
fn a_conflicted_protected_path_kept_as_ours_commits() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, None, None);
    take_side(root, "HEAD", "doc/specs/gone.txt");

    let id = commit_guarding(root, &specs()).unwrap();

    assert_eq!(
        git_in(root, &["show", &format!("{id}:doc/specs/gone.txt")]),
        "ours"
    );
}

#[test]
fn a_refusal_names_the_way_out_of_each_kind_of_path() {
    let temp = protected_merge();
    let root = temp.path();
    resolve(root, Some("mine\n"), None);
    take_side(root, "HEAD", "doc/specs/auto.txt");

    let refusal = guarded_refusal(root);

    assert!(refusal.contains("'doc/specs/conf.txt'"), "{refusal}");
    assert!(refusal.contains(&format!("{RESTORE}HEAD ")), "{refusal}");
    assert!(refusal.contains("'doc/specs/auto.txt'"), "{refusal}");
    assert!(refusal.contains(RESTORE), "{refusal}");
    assert_eq!(refusal.lines().count(), 1, "{refusal}");
}

#[test]
fn a_directory_glob_with_a_trailing_slash_or_other_case_still_guards() {
    for glob in ["doc/specs/", "doc/specs/**/", "DOC/Specs/**"] {
        let temp = protected_merge();
        let root = temp.path();
        resolve(root, Some("mine\n"), None);
        let before = tip(root, BRANCH);

        let refusal = reason(commit_guarding(root, &[glob.to_string()]));

        assert!(
            refusal.contains("'doc/specs/conf.txt'"),
            "{glob}: {refusal}"
        );
        assert_eq!(tip(root, BRANCH), before, "{glob}");
    }
}
