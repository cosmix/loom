//! Installing into the directory git really runs hooks from, reporting when
//! loom must not write there, and never writing through a link at a hook path.

use super::*;
use std::os::unix::fs::symlink;
use std::process::Command;
use tempfile::TempDir;

/// `git` with no `-c core.hooksPath` override: what it answers is what a commit
/// in this repository would do.
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn repo_with_hooks_path(configured: &str) -> TempDir {
    let repo = TempDir::new().unwrap();
    git(repo.path(), &["init", "-q", "-b", "main"]);
    git(repo.path(), &["config", "core.hooksPath", configured]);
    repo
}

/// Where git itself looks for hooks in `root`.
fn gits_hooks_dir(root: &Path) -> std::path::PathBuf {
    root.join(git(root, &["rev-parse", "--git-path", "hooks"]))
}

fn no_hooks_in(dir: &Path) -> bool {
    !dir.join("pre-commit").exists() && !dir.join("reference-transaction").exists()
}

#[test]
fn a_dangling_symlink_at_the_pre_commit_path_is_not_written_through() {
    let temp = TempDir::new().unwrap();
    let hooks = temp.path().join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let outside = temp.path().join("outside-target");
    symlink(&outside, hooks.join("pre-commit")).unwrap();

    let error = install_pre_commit_hook(temp.path()).unwrap_err();

    assert!(error.to_string().contains("pre-commit"), "{error:#}");
    assert!(!outside.exists(), "the link's target was created");
    assert!(fs::symlink_metadata(hooks.join("pre-commit"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!is_pre_commit_hook_installed(temp.path()));
}

#[test]
fn a_dangling_symlink_at_the_reference_transaction_path_is_foreign() {
    let temp = TempDir::new().unwrap();
    let hooks = temp.path().join(".git/hooks");
    fs::create_dir_all(&hooks).unwrap();
    let outside = temp.path().join("outside-target");
    let hook = hooks.join("reference-transaction");
    symlink(&outside, &hook).unwrap();

    let installed = install_reference_transaction_hook(temp.path()).unwrap();

    assert_eq!(installed, HookInstall::ForeignHookPresent);
    assert!(!outside.exists());
    assert_eq!(fs::read_link(&hook).unwrap(), outside);
    assert!(!is_reference_transaction_hook_installed(temp.path()));
}

#[test]
fn hooks_go_where_a_hooks_path_inside_the_repository_sends_git() {
    for configured in [".git/custom-hooks", "local-hooks"] {
        let repo = repo_with_hooks_path(configured);
        let root = repo.path();

        assert!(install_pre_commit_hook(root).unwrap(), "{configured}");
        let installed = install_reference_transaction_hook(root).unwrap();

        assert_eq!(installed, HookInstall::Installed, "{configured}");
        assert!(is_pre_commit_hook_installed(root), "{configured}");
        assert!(
            is_reference_transaction_hook_installed(root),
            "{configured}"
        );
        let dir = gits_hooks_dir(root);
        assert_eq!(dir, root.join(configured));
        assert!(dir.join("pre-commit").is_file() && dir.join("reference-transaction").is_file());
        assert!(no_hooks_in(&root.join(".git/hooks")), "{configured}");
        assert!(
            !install_pre_commit_hook(root).unwrap(),
            "{configured}: second install"
        );
    }
}

#[test]
fn an_absolute_hooks_path_inside_the_repository_is_followed() {
    let repo = repo_with_hooks_path("placeholder");
    let root = repo.path();
    let dir = root.join(".git/absolute-hooks");
    git(root, &["config", "core.hooksPath", dir.to_str().unwrap()]);

    assert!(install_pre_commit_hook(root).unwrap());

    assert!(dir.join("pre-commit").is_file());
    assert!(is_pre_commit_hook_installed(root));
}

#[test]
fn a_foreign_hook_in_the_configured_directory_is_kept() {
    let repo = repo_with_hooks_path(".git/custom-hooks");
    let root = repo.path();
    let dir = root.join(".git/custom-hooks");
    fs::create_dir_all(&dir).unwrap();
    let foreign = "#!/bin/sh\nexit 0\n";
    for name in ["pre-commit", "reference-transaction"] {
        fs::write(dir.join(name), foreign).unwrap();
        make_executable(&dir.join(name)).unwrap();
    }

    assert!(install_pre_commit_hook(root).unwrap());
    let installed = install_reference_transaction_hook(root).unwrap();

    assert_eq!(installed, HookInstall::ForeignHookPresent);
    assert_eq!(
        fs::read_to_string(dir.join("reference-transaction")).unwrap(),
        foreign
    );
    assert_eq!(
        fs::read_to_string(dir.join(FOREIGN_HOOK_NAME)).unwrap(),
        foreign
    );
    assert!(is_pre_commit_hook_installed(root));
    assert!(!is_reference_transaction_hook_installed(root));
}

/// Git runs nothing from `.git/hooks` here, and the directory it does use may
/// serve other repositories.
#[test]
fn a_hooks_path_outside_the_repository_is_reported_and_left_alone() {
    let shared = TempDir::new().unwrap();
    let repo = repo_with_hooks_path(shared.path().to_str().unwrap());
    let root = repo.path();

    let pre_commit = format!("{:#}", install_pre_commit_hook(root).unwrap_err());
    let reference = format!(
        "{:#}",
        install_reference_transaction_hook(root).unwrap_err()
    );

    for message in [pre_commit, reference] {
        assert!(message.contains("core.hooksPath"), "{message}");
        assert!(message.contains("outside this repository"), "{message}");
        assert!(
            message.contains(shared.path().to_str().unwrap()),
            "{message}"
        );
    }
    assert!(no_hooks_in(shared.path()));
    assert!(no_hooks_in(&root.join(".git/hooks")));
    assert!(!is_pre_commit_hook_installed(root));
    assert!(!is_reference_transaction_hook_installed(root));
}

#[test]
fn a_hook_already_in_an_outside_directory_counts_as_installed() {
    let shared = TempDir::new().unwrap();
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    install_pre_commit_hook(root).unwrap();
    install_reference_transaction_hook(root).unwrap();
    for name in ["pre-commit", "reference-transaction"] {
        fs::rename(root.join(".git/hooks").join(name), shared.path().join(name)).unwrap();
    }
    git(
        root,
        &["config", "core.hooksPath", shared.path().to_str().unwrap()],
    );

    assert!(is_pre_commit_hook_installed(root));
    assert!(is_reference_transaction_hook_installed(root));
    assert!(!install_pre_commit_hook(root).unwrap());
    assert_eq!(
        install_reference_transaction_hook(root).unwrap(),
        HookInstall::UpToDate
    );
}

#[test]
fn hooks_turned_off_with_dev_null_are_not_installed() {
    let repo = repo_with_hooks_path("/dev/null");
    let root = repo.path();

    assert!(install_pre_commit_hook(root).is_err());
    assert!(install_reference_transaction_hook(root).is_err());
    assert!(!is_pre_commit_hook_installed(root));
    assert!(!is_reference_transaction_hook_installed(root));
}

/// Git expands `~/`; read literally, it would name a directory in the checkout.
#[test]
fn a_home_relative_hooks_path_is_not_a_directory_in_the_checkout() {
    let repo = repo_with_hooks_path("~/loom-test-shared-hooks");
    let root = repo.path();

    assert!(install_pre_commit_hook(root).is_err());

    assert!(!root.join("~").exists());
}

/// A hook loom changed there would show as the operator's own tracked change,
/// and the target guard holds a commit of it.
#[test]
fn a_tracked_hooks_directory_is_reported_and_left_alone() {
    let repo = repo_with_hooks_path(".githooks");
    let root = repo.path();
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    let tracked = root.join(".githooks/pre-commit");
    fs::create_dir_all(tracked.parent().unwrap()).unwrap();
    fs::write(&tracked, "#!/bin/sh\nexit 0\n").unwrap();
    make_executable(&tracked).unwrap();
    git(
        root,
        &["-c", "core.hooksPath=/dev/null", "add", ".githooks"],
    );
    git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "-m",
            "hooks",
        ],
    );

    let message = format!("{:#}", install_pre_commit_hook(root).unwrap_err());
    let other = format!(
        "{:#}",
        install_reference_transaction_hook(root).unwrap_err()
    );

    assert!(message.contains("tracked"), "{message}");
    assert!(other.contains("tracked"), "{other}");
    assert_eq!(git(root, &["status", "--porcelain"]), "");
    assert!(!is_pre_commit_hook_installed(root));
}

#[test]
fn a_symlink_on_the_way_to_the_hooks_path_is_reported() {
    let elsewhere = TempDir::new().unwrap();
    let repo = repo_with_hooks_path("linked/hooks");
    let root = repo.path();
    symlink(elsewhere.path(), root.join("linked")).unwrap();

    assert!(install_pre_commit_hook(root).is_err());

    assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 0);
}
