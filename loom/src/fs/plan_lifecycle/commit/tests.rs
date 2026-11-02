use super::*;
use crate::git::signing::tests::fake_signer;
use crate::git::target_guard::test_support::{activate, commit_file, git, repo, Repo};
use crate::git::target_guard::{check, GuardState, LEDGER_FILE};

const OLD: &str = "doc/plans/IN_PROGRESS-PLAN-x.md";
const NEW: &str = "doc/plans/DONE-PLAN-x.md";
const OTHER_PLAN: &str = "doc/plans/PLAN-other.md";
const REVIEW: &str = "doc/plans/REVIEW-x.md";
const OTHER_REVIEW: &str = "doc/plans/REVIEW-other.md";

/// An attesting repository with the plan, another plan, both plans'
/// review documents, a knowledge file and a source file committed, and
/// `main` recorded.
fn plan_repo() -> Repo {
    let repo = repo();
    activate(&repo.root);
    let paths = [OLD, OTHER_PLAN, REVIEW, OTHER_REVIEW];
    for path in paths
        .into_iter()
        .chain(["doc/loom/knowledge/a.md", "src/x.rs"])
    {
        commit_file(&repo.root, path, "v1\n");
    }
    let first = check(&repo.root, &repo.work, "main").unwrap();
    assert!(matches!(first, Some(GuardState::Clear { .. })));
    repo
}

/// Rename the plan and run the post-completion commit, as
/// `rename_completed_plan` does.
fn complete(repo: &Repo) {
    std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
    let work_dir = WorkDir::new(&repo.root).unwrap();
    let (old, new) = (repo.root.join(OLD), repo.root.join(NEW));
    commit_post_completion_changes(&work_dir, &old, &new).unwrap();
}

/// Edit a committed file and stage it, so the next commit carries it.
fn stage_edit(repo: &Repo, path: &str) {
    std::fs::write(repo.root.join(path), "v2\n").unwrap();
    git(&repo.root, &["add", path]);
}

#[test]
fn unrelated_tracked_edits_stay_out_of_the_plan_done_commit() {
    let repo = plan_repo();
    std::fs::write(repo.root.join("src/x.rs"), "operator wip\n").unwrap();
    std::fs::write(repo.root.join("doc/loom/knowledge/a.md"), "wip\n").unwrap();

    complete(&repo);

    let files = git(&repo.root, &["show", "--name-status", "--format=", "HEAD"]);
    assert_eq!(files.lines().count(), 1, "only the rename: {files}");
    assert!(files.contains(OLD) && files.contains(NEW), "{files}");
    let status = git(
        &repo.root,
        &["status", "--porcelain", "--untracked-files=no"],
    );
    assert_eq!(git(&repo.root, &["diff", "--cached", "--name-only"]), "");
    assert!(status.contains("M src/x.rs"), "{status}");
    assert!(status.contains("M doc/loom/knowledge/a.md"), "{status}");
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

/// Name `x` as the active plan in the state directory's `config.toml`.
fn configure_plan_x(repo: &Repo) {
    std::fs::write(repo.work.join("config.toml"), "[plan]\nplan_id = \"x\"\n").unwrap();
}

fn ledger(repo: &Repo) -> String {
    std::fs::read_to_string(repo.work.join(LEDGER_FILE)).unwrap_or_default()
}

fn guard(repo: &Repo) -> GuardState {
    check(&repo.root, &repo.work, "main").unwrap().unwrap()
}

#[test]
fn the_plan_done_commit_signs_with_the_signing_environment() {
    let repo = plan_repo();
    let argv_log = fake_signer(&repo.root, false);

    complete(&repo);

    let commit = git(&repo.root, &["cat-file", "commit", "HEAD"]);
    assert!(
        commit.contains("gpgsig"),
        "the DONE commit is signed: {commit}"
    );
    let calls = std::fs::read_to_string(argv_log).unwrap();
    assert_eq!(calls.lines().count(), 1);
}

#[test]
fn unrelated_staged_and_partially_staged_changes_remain_intact() {
    let repo = plan_repo();
    for path in ["src/x.rs", OTHER_PLAN, "doc/loom/knowledge/a.md"] {
        stage_edit(&repo, path);
    }
    std::fs::write(repo.root.join("src/x.rs"), "unstaged continuation\n").unwrap();
    let staged = git(&repo.root, &["diff", "--cached", "--binary"]);
    let worktree = git(&repo.root, &["diff", "--binary"]);
    complete(&repo);
    assert_eq!(git(&repo.root, &["diff", "--cached", "--binary"]), staged);
    assert_eq!(git(&repo.root, &["diff", "--binary"]), worktree);
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

#[test]
fn a_failing_signer_fails_the_plan_done_commit() {
    let repo = plan_repo();
    fake_signer(&repo.root, true);
    stage_edit(&repo, "src/x.rs");
    let staged = git(&repo.root, &["diff", "--cached", "--binary"]);
    let head = git(&repo.root, &["rev-parse", "HEAD"]);
    std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
    let work_dir = WorkDir::new(&repo.root).unwrap();
    let (old, new) = (repo.root.join(OLD), repo.root.join(NEW));

    let error = commit_post_completion_changes(&work_dir, &old, &new).unwrap_err();

    assert!(format!("{error:#}").contains("signing failed"), "{error:#}");
    assert_eq!(git(&repo.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&repo.root, &["diff", "--cached", "--binary"]), staged);
}

#[test]
fn changed_target_ref_refuses_the_prepared_commit() {
    let repo = plan_repo();
    let before = git(&repo.root, &["rev-parse", "HEAD"]);
    let tree = git(&repo.root, &["rev-parse", "HEAD^{tree}"]);
    let prepared = git(
        &repo.root,
        &["commit-tree", &tree, "-p", &before, "-m", "prepared"],
    );
    let moved = commit_file(&repo.root, "operator.txt", "operator\n");
    stage_edit(&repo, "src/x.rs");
    let index = git(&repo.root, &["diff", "--cached", "--binary"]);
    assert!(move_completion_ref(
        &repo.root,
        "refs/heads/main",
        &before,
        &prepared,
        "completion"
    )
    .is_err());
    assert_eq!(git(&repo.root, &["rev-parse", "HEAD"]), moved);
    assert_eq!(git(&repo.root, &["diff", "--cached", "--binary"]), index);
}

#[test]
fn previously_staged_plan_deletion_does_not_leave_the_done_file_staged_as_deleted() {
    let repo = plan_repo();
    std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
    git(&repo.root, &["add", "-u", "--", OLD]);
    let work_dir = WorkDir::new(&repo.root).unwrap();
    commit_post_completion_changes(&work_dir, &repo.root.join(OLD), &repo.root.join(NEW)).unwrap();
    assert_eq!(git(&repo.root, &["diff", "--cached", "--name-only"]), "");
}

#[test]
fn metacharacters_in_plan_paths_are_literal() {
    let repo = plan_repo();
    let old = "doc/plans/IN_PROGRESS-PLAN-[x].md";
    let new = "doc/plans/DONE-PLAN-[x].md";
    commit_file(&repo.root, old, "plan\n");
    commit_file(&repo.root, "doc/plans/DONE-PLAN-x.md", "unrelated\n");
    std::fs::rename(repo.root.join(old), repo.root.join(new)).unwrap();
    std::fs::write(repo.root.join("doc/plans/DONE-PLAN-x.md"), "operator\n").unwrap();
    let work_dir = WorkDir::new(&repo.root).unwrap();
    commit_post_completion_changes(&work_dir, &repo.root.join(old), &repo.root.join(new)).unwrap();
    assert_eq!(
        git(&repo.root, &["show", "HEAD:doc/plans/DONE-PLAN-x.md"]),
        "unrelated"
    );
    assert_eq!(
        std::fs::read_to_string(repo.root.join("doc/plans/DONE-PLAN-x.md")).unwrap(),
        "operator\n"
    );
}

#[test]
fn an_operator_merge_in_progress_is_not_overwritten() {
    let repo = plan_repo();
    let before = git(&repo.root, &["rev-parse", "HEAD"]);
    std::fs::write(repo.root.join(".git/MERGE_HEAD"), format!("{before}\n")).unwrap();
    std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
    let work_dir = WorkDir::new(&repo.root).unwrap();
    assert!(
        commit_post_completion_changes(&work_dir, &repo.root.join(OLD), &repo.root.join(NEW))
            .is_err()
    );
    assert_eq!(git(&repo.root, &["rev-parse", "HEAD"]), before);
}

/// Rename the plan and commit that with `extra` edits, then attest the commit
/// the way the completion path does. `true` when the ledger names it.
fn commit_and_attest(repo: &Repo, extra: &[&str]) -> bool {
    std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
    git(&repo.root, &["add", "-A", "--", OLD, NEW]);
    for path in extra {
        stage_edit(repo, path);
    }
    let from = git(&repo.root, &["rev-parse", "HEAD"]);
    git(&repo.root, &["commit", "-q", "-m", "complete"]);
    let after = git(&repo.root, &["rev-parse", "HEAD"]);
    let changed = git(&repo.root, &["show", "--name-only", "--format=", "HEAD"]);
    for path in extra {
        assert!(changed.lines().any(|line| line == *path), "{changed}");
    }
    let work_dir = WorkDir::new(&repo.root).unwrap();
    let (old, new) = (repo.root.join(OLD), repo.root.join(NEW));
    let reference = "refs/heads/main";
    attest_completion_commit(&work_dir, &repo.root, &old, &new, reference, &from, &after);
    ledger(repo).contains(&format!("attest {from} {after} {reference}\n"))
}

#[test]
fn a_commit_of_only_the_plan_files_and_knowledge_is_attested() {
    let repo = plan_repo();
    assert!(commit_and_attest(&repo, &["doc/loom/knowledge/a.md"]));
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

#[test]
fn a_commit_of_the_plan_files_and_a_source_is_not_attested() {
    let repo = plan_repo();
    assert!(!commit_and_attest(&repo, &["src/x.rs"]));
    assert_eq!(ledger(&repo), "");
}

#[test]
fn staged_knowledge_stays_out_of_the_attested_done_commit() {
    let repo = plan_repo();
    stage_edit(&repo, "doc/loom/knowledge/a.md");

    complete(&repo);

    let after = git(&repo.root, &["rev-parse", "HEAD"]);
    let from = git(&repo.root, &["rev-parse", "HEAD^"]);
    assert!(ledger(&repo).contains(&format!("attest {from} {after} refs/heads/main\n")));
    assert_eq!(
        git(&repo.root, &["diff", "--cached", "--name-only"]),
        "doc/loom/knowledge/a.md"
    );
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

#[test]
fn a_staged_source_is_not_committed() {
    let repo = plan_repo();
    stage_edit(&repo, "src/x.rs");

    complete(&repo);

    assert_eq!(
        git(&repo.root, &["diff", "--cached", "--name-only"]),
        "src/x.rs"
    );
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

#[test]
fn another_staged_plan_is_not_committed() {
    let repo = plan_repo();
    stage_edit(&repo, OTHER_PLAN);

    complete(&repo);

    assert_eq!(
        git(&repo.root, &["diff", "--cached", "--name-only"]),
        OTHER_PLAN
    );
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

#[test]
fn a_commit_on_an_unguarded_branch_is_not_attested() {
    let repo = plan_repo();
    git(&repo.root, &["checkout", "-q", "-b", "scratch"]);

    std::fs::rename(repo.root.join(OLD), repo.root.join(NEW)).unwrap();
    let work_dir = WorkDir::new(&repo.root).unwrap();
    let head = git(&repo.root, &["rev-parse", "HEAD"]);
    assert!(
        commit_post_completion_changes(&work_dir, &repo.root.join(OLD), &repo.root.join(NEW))
            .is_err()
    );
    assert_eq!(git(&repo.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(ledger(&repo), "");
}

#[test]
fn a_commit_of_the_plans_own_review_document_is_attested() {
    let repo = plan_repo();
    configure_plan_x(&repo);
    assert!(commit_and_attest(&repo, &[REVIEW]));
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

#[test]
fn a_commit_of_another_plans_review_document_is_not_attested() {
    let repo = plan_repo();
    configure_plan_x(&repo);
    assert!(!commit_and_attest(&repo, &[OTHER_REVIEW]));
    assert_eq!(ledger(&repo), "");
}

#[test]
fn a_staged_review_document_stays_out_of_the_attested_done_commit() {
    let repo = plan_repo();
    configure_plan_x(&repo);
    stage_edit(&repo, REVIEW);

    complete(&repo);

    let after = git(&repo.root, &["rev-parse", "HEAD"]);
    let from = git(&repo.root, &["rev-parse", "HEAD^"]);
    assert!(ledger(&repo).contains(&format!("attest {from} {after} refs/heads/main\n")));
    assert_eq!(
        git(&repo.root, &["diff", "--cached", "--name-only"]),
        REVIEW
    );
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}

#[test]
fn another_staged_review_document_stays_out_of_the_done_commit() {
    let repo = plan_repo();
    configure_plan_x(&repo);
    stage_edit(&repo, OTHER_REVIEW);

    complete(&repo);

    assert_eq!(
        git(&repo.root, &["diff", "--cached", "--name-only"]),
        OTHER_REVIEW
    );
    assert!(matches!(guard(&repo), GuardState::Clear { .. }));
}
