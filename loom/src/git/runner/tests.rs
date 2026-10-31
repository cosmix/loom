use super::*;

#[test]
fn git_deadlines_are_operation_specific() {
    assert_eq!(git_timeout(&["status"]), GIT_READ_TIMEOUT);
    assert_eq!(git_timeout(&["merge"]), GIT_MUTATION_TIMEOUT);
    for sub in ["merge-tree", "commit-tree", "update-ref", "stash"] {
        assert_eq!(git_timeout(&[sub]), GIT_MUTATION_TIMEOUT);
    }
    assert_eq!(git_timeout(&["fetch"]), GIT_NETWORK_TIMEOUT);
    assert!(GIT_READ_TIMEOUT < GIT_MUTATION_TIMEOUT);
    assert!(GIT_MUTATION_TIMEOUT < GIT_NETWORK_TIMEOUT);
}

#[test]
fn git_runner_returns_structured_timeout() {
    let repo = tempfile::tempdir().unwrap();
    let error = run_git_program(
        "sh",
        &["-c", "sleep 60"],
        &[],
        "git -c",
        repo.path(),
        Duration::from_millis(100),
    )
    .expect_err("fake git command must time out");

    let timeout = error
        .downcast_ref::<crate::process::ProcessTimeoutError>()
        .expect("caller must be able to classify a timeout");
    assert_eq!(timeout.operation(), "git -c");
}

#[test]
fn repository_selectors_are_removed_before_explicit_overrides() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for root in [first.path(), second.path()] {
        isolated_git_ok(root, &["init"]);
    }
    std::fs::write(first.path().join("first.txt"), "first").unwrap();
    std::fs::write(second.path().join("second.txt"), "second").unwrap();

    let mut command = Command::new("git");
    command
        .env("GIT_DIR", second.path().join(".git"))
        .env("GIT_WORK_TREE", second.path())
        .env("GIT_INDEX_FILE", second.path().join(".git/index"));
    harden(
        &mut command,
        &global_args(&["add", "first.txt"]),
        first.path(),
    );
    assert!(command.output().unwrap().status.success());
    assert_eq!(
        run_git_checked(&["ls-files"], first.path()).unwrap(),
        "first.txt"
    );
    assert_eq!(run_git_checked(&["ls-files"], second.path()).unwrap(), "");

    let alternate_index = first.path().join("alternate-index");
    let env = [("GIT_INDEX_FILE", alternate_index.as_os_str())];
    let output = run_git_with_env(&["add", "first.txt"], &env, first.path()).unwrap();
    assert!(output.status.success());
    assert!(alternate_index.exists());
    let output = run_git_with_env(&["ls-files"], &env, first.path()).unwrap();
    assert_eq!(output.stdout, b"first.txt\n");
}

/// Every hook name this fixture wires up. Each script just touches a
/// marker file under the repo's `markers` directory when it runs.
const HOOK_NAMES: [&str; 5] = [
    "pre-commit",
    "commit-msg",
    "pre-merge-commit",
    "post-merge",
    "post-checkout",
];

/// Set up isolated from ambient host git config (never mutates
/// process-wide environment; each command carries its own overrides).
pub(super) fn isolated_git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

pub(super) fn isolated_git_ok(root: &Path, args: &[&str]) {
    let output = isolated_git(root, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository whose `core.hooksPath` (set via local config, so it is
/// live for any plain `git` call) points at a fixture directory where
/// every hook in [`HOOK_NAMES`] touches a marker file under
/// `<root>/markers` when it runs. Also carries a `feature` branch ready
/// to merge or check out, and leaves `main` checked out.
#[cfg(unix)]
fn repo_with_marker_hooks() -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    isolated_git_ok(&root, &["init", "-b", "main"]);
    isolated_git_ok(&root, &["config", "user.email", "runner-test@example.com"]);
    isolated_git_ok(&root, &["config", "user.name", "Runner Test"]);
    std::fs::write(root.join("seed.txt"), "seed").unwrap();
    isolated_git_ok(&root, &["add", "seed.txt"]);
    isolated_git_ok(&root, &["commit", "-m", "seed"]);

    // A branch to merge and check out, created outside the runner and
    // BEFORE the hook fixture is installed below - otherwise these
    // plain setup commands would themselves trigger the fixture hooks
    // (pre-commit, commit-msg, post-checkout) and pre-populate the
    // markers the tests assert on.
    isolated_git_ok(&root, &["checkout", "-b", "feature"]);
    std::fs::write(root.join("feature.txt"), "feature").unwrap();
    isolated_git_ok(&root, &["add", "feature.txt"]);
    isolated_git_ok(&root, &["commit", "-m", "feature commit"]);
    isolated_git_ok(&root, &["checkout", "main"]);

    let hooks_dir = root.join("hooks-fixture");
    let markers_dir = root.join("markers");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    std::fs::create_dir_all(&markers_dir).unwrap();
    for hook in HOOK_NAMES {
        let script_path = hooks_dir.join(hook);
        let marker_path = markers_dir.join(hook);
        std::fs::write(
            &script_path,
            format!("#!/bin/sh\ntouch \"{}\"\n", marker_path.display()),
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();
    }
    isolated_git_ok(
        &root,
        &["config", "core.hooksPath", hooks_dir.to_str().unwrap()],
    );

    (temp, root)
}

#[cfg(unix)]
fn assert_no_hooks_fired(markers_dir: &Path) {
    for hook in HOOK_NAMES {
        assert!(
            !markers_dir.join(hook).exists(),
            "hook '{hook}' fired through a call routed via the runner"
        );
    }
}

#[cfg(unix)]
#[test]
fn run_git_checked_disables_repository_hooks_for_commit_merge_and_checkout() {
    let (_temp, root) = repo_with_marker_hooks();
    let markers_dir = root.join("markers");

    run_git_checked(&["commit", "--allow-empty", "-m", "runner commit"], &root)
        .expect("runner commit must succeed with hooks disabled");
    assert_no_hooks_fired(&markers_dir);

    run_git_checked(
        &["merge", "--no-ff", "feature", "-m", "runner merge"],
        &root,
    )
    .expect("runner merge must succeed with hooks disabled");
    assert_no_hooks_fired(&markers_dir);

    run_git_checked(&["checkout", "feature"], &root)
        .expect("runner checkout must succeed with hooks disabled");
    assert_no_hooks_fired(&markers_dir);
}

/// Positive control: the same commit, run with plain `git` bypassing the
/// runner entirely, must fire the fixture hooks. This proves the
/// absence asserted above comes from the runner's
/// `-c core.hooksPath=/dev/null`, not from a broken fixture.
#[cfg(unix)]
#[test]
fn plain_git_commit_proves_the_fixture_hooks_are_live() {
    let (_temp, root) = repo_with_marker_hooks();
    let markers_dir = root.join("markers");

    isolated_git_ok(&root, &["commit", "--allow-empty", "-m", "control commit"]);

    assert!(
        markers_dir.join("pre-commit").exists(),
        "fixture pre-commit hook must be live"
    );
    assert!(
        markers_dir.join("commit-msg").exists(),
        "fixture commit-msg hook must be live"
    );
}
