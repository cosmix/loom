//! `config.worktree` guards: the admin directory listing and its split by
//! session, and the refusal of a pinned run under a file git would act on.

use super::*;
use crate::git::worktree::WorktreeGit;
use crate::verify::contracts::test_support::contract_worktree;
use tempfile::TempDir;

/// A repository at `<tmp>/repo` with the stage worktree `.worktrees/s1`.
fn repository() -> (TempDir, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    (tmp, repo, worktree)
}

fn git(dir: &Path, args: &[&str]) {
    let output = run_git(args, dir).unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

fn admin_dir(repo: &Path) -> PathBuf {
    repo.join(".git/worktrees/s1")
}

/// Write `config.worktree` for `s1`, plus what makes `git status` run a clean
/// filter that creates the returned marker.
fn plant_filter(repo: &Path, worktree: &Path) -> PathBuf {
    // An index entry without stat data makes `git status` hash the file.
    git(worktree, &["read-tree", "HEAD"]);
    let marker = repo.parent().unwrap().join("filter-ran");
    let config = format!(
        "[filter \"x\"]\n\tclean = touch '{}'; cat\n",
        marker.display()
    );
    std::fs::write(admin_dir(repo).join("config.worktree"), config).unwrap();
    std::fs::write(worktree.join(".gitattributes"), "* filter=x\n").unwrap();
    std::fs::write(worktree.join("README.md"), "changed\n").unwrap();
    marker
}

fn enable_extension(repo: &Path) {
    git(repo, &["config", "extensions.worktreeConfig", "true"]);
}

fn status(repo: &Path, worktree: &Path) -> Result<std::process::Output> {
    WorktreeGit::pinned(repo, worktree)?.run(&["status", "--porcelain"])
}

#[test]
fn every_admin_directory_is_listed_and_symlinks_are_not() {
    let (_tmp, repo, _worktree) = repository();
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "loom/s2", ".worktrees/s2"],
    );
    std::os::unix::fs::symlink("s1", repo.join(".git/worktrees/link")).unwrap();

    let dirs = worktree_admin_dirs(&repo).unwrap();

    let common = repo.join(".git").canonicalize().unwrap().join("worktrees");
    assert_eq!(dirs, vec![common.join("s1"), common.join("s2")]);
}

#[test]
fn a_directory_without_git_has_no_admin_directories() {
    let tmp = TempDir::new().unwrap();
    assert!(worktree_admin_dirs(tmp.path()).unwrap().is_empty());
}

/// [`repository`] plus the registered worktree `.worktrees/s2`, and the
/// canonical `<common dir>/worktrees`.
fn two_worktrees() -> (TempDir, PathBuf, PathBuf) {
    let (tmp, repo, _worktree) = repository();
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "loom/s2", ".worktrees/s2"],
    );
    let registry = repo.join(".git").canonicalize().unwrap().join("worktrees");
    (tmp, repo, registry)
}

#[test]
fn the_session_worktree_is_its_own_and_every_other_is_a_sibling() {
    let (_tmp, repo, registry) = two_worktrees();

    let stage = partition_admin_dirs(&repo, Some(&repo.join(".worktrees/s1"))).unwrap();
    let checkout = partition_admin_dirs(&repo, None).unwrap();

    assert_eq!(stage.own, Some(registry.join("s1")));
    assert_eq!(stage.siblings, vec![registry.join("s2")]);
    assert_eq!(checkout.own, None);
    assert_eq!(
        checkout.siblings,
        vec![registry.join("s1"), registry.join("s2")]
    );
}

#[test]
fn the_own_directory_is_found_through_a_symlinked_path() {
    let (tmp, repo, registry) = two_worktrees();
    let links = TempDir::new().unwrap();
    let alias = links.path().join("alias");
    std::os::unix::fs::symlink(tmp.path(), &alias).unwrap();

    let dirs = partition_admin_dirs(&repo, Some(&alias.join("repo/.worktrees/s1"))).unwrap();

    assert_eq!(dirs.own, Some(registry.join("s1")));
    assert_eq!(dirs.siblings, vec![registry.join("s2")]);
}

#[test]
fn an_unregistered_worktree_beside_registered_ones_is_refused() {
    let (_tmp, repo, _registry) = two_worktrees();
    let ghost = repo.join(".worktrees/ghost");
    std::fs::create_dir_all(&ghost).unwrap();

    let error = partition_admin_dirs(&repo, Some(&ghost)).unwrap_err();

    let message = format!("{error:#}");
    assert!(
        message.contains("is not a registered worktree"),
        "{message}"
    );
    assert!(message.contains("ghost"), "{message}");
}

#[test]
fn two_directories_claiming_one_worktree_are_refused() {
    let (_tmp, repo, registry) = two_worktrees();
    let claim = std::fs::read(registry.join("s1/gitdir")).unwrap();
    std::fs::write(registry.join("s2/gitdir"), claim).unwrap();

    let error = partition_admin_dirs(&repo, Some(&repo.join(".worktrees/s1"))).unwrap_err();

    assert!(
        format!("{error:#}").contains("more than one worktree"),
        "{error:#}"
    );
}

#[test]
fn a_repository_without_worktrees_partitions_to_nothing() {
    let tmp = TempDir::new().unwrap();
    let absent = tmp.path().join(".worktrees/s1");

    let dirs = partition_admin_dirs(tmp.path(), Some(&absent)).unwrap();

    assert_eq!(dirs, AdminDirs::default());
}

#[test]
fn a_hostile_file_is_ignored_by_git_and_allowed_while_the_extension_is_off() {
    let (_tmp, repo, worktree) = repository();
    let marker = plant_filter(&repo, &worktree);

    let output = status(&repo, &worktree).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(
        !marker.exists(),
        "git read config.worktree with the extension off"
    );
}

#[test]
fn sparse_checkout_keys_alone_are_allowed_with_the_extension_on() {
    let (_tmp, repo, worktree) = repository();
    enable_extension(&repo);
    let config =
        "[core]\n\tsparseCheckout = true\n\tsparseCheckoutCone = true\n[index]\n\tsparse = true\n";
    std::fs::write(admin_dir(&repo).join("config.worktree"), config).unwrap();

    let output = status(&repo, &worktree).unwrap();

    assert!(output.status.success(), "{output:?}");
}

/// The positive control runs last: git that is not pinned does run the
/// filter, so its absence before comes from the refusal.
#[test]
fn a_filter_in_config_worktree_is_refused_before_any_command_runs() {
    let (_tmp, repo, worktree) = repository();
    enable_extension(&repo);
    let marker = plant_filter(&repo, &worktree);

    let error = status(&repo, &worktree).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("filter.x.clean"), "{message}");
    assert!(message.contains("will not run git"), "{message}");
    assert!(!marker.exists(), "a command ran before the refusal");

    WorktreeGit::discovered(&worktree)
        .run(&["status", "--porcelain"])
        .unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}

#[test]
fn an_empty_regular_file_is_allowed_with_the_extension_on() {
    let (_tmp, repo, worktree) = repository();
    enable_extension(&repo);
    std::fs::write(admin_dir(&repo).join("config.worktree"), "").unwrap();

    let output = status(&repo, &worktree).unwrap();

    assert!(output.status.success(), "{output:?}");
    check_worktree_config(&repo, &admin_dir(&repo)).unwrap();
}

#[test]
fn a_symlinked_config_worktree_still_goes_through_the_full_check() {
    let (_tmp, repo, worktree) = repository();
    enable_extension(&repo);
    let hostile = repo.parent().unwrap().join("hostile.config");
    std::fs::write(&hostile, "[core]\n\tpager = touch /nonexistent\n").unwrap();
    std::os::unix::fs::symlink(&hostile, admin_dir(&repo).join("config.worktree")).unwrap();

    let error = status(&repo, &worktree).unwrap_err();

    assert!(format!("{error:#}").contains("core.pager"), "{error:#}");
}

#[test]
fn a_config_worktree_that_does_not_parse_is_refused() {
    let (_tmp, repo, worktree) = repository();
    enable_extension(&repo);
    std::fs::write(
        admin_dir(&repo).join("config.worktree"),
        "[core\n\tbroken\n",
    )
    .unwrap();

    let error = status(&repo, &worktree).unwrap_err();

    assert!(
        format!("{error:#}").contains("cannot be parsed"),
        "{error:#}"
    );
}
