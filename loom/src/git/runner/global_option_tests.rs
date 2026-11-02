//! The subcommand behind global options, and links planted in a checkout.

use super::tests::planted_gitlink;
use super::*;

/// A global option before the subcommand hides nothing: the same comparisons
/// with every global option the host passes. Each prefix's control is git
/// outside the runner, which does run the filter with that prefix.
#[test]
fn a_global_option_before_the_subcommand_does_not_restore_gitlink_recursion() {
    let prefixes: [&[&str]; 6] = [
        &["--no-optional-locks"],
        &["--literal-pathspecs"],
        &["--no-pager"],
        &["-c", "core.quotePath=false"],
        &["--no-optional-locks", "-c", "core.fsmonitor=false"],
        &["-C", "."],
    ];
    for prefix in prefixes {
        let (_tmp, repo, marker) = planted_gitlink();
        for subcommand in [&["status", "--porcelain"][..], &["diff", "--name-only"][..]] {
            let args = [prefix, subcommand].concat();
            run_git(&args, &repo).unwrap();
            assert!(!marker.exists(), "git {args:?} ran the gitlink's filter");
        }
        let control = std::process::Command::new("git")
            .args(prefix)
            .args(["status", "--porcelain"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(control.status.success(), "{control:?}");
        assert!(
            marker.exists(),
            "{prefix:?}: the fixture's filter never runs"
        );
    }
}

#[test]
fn the_subcommand_is_found_behind_every_global_option_form() {
    let cases: [(&[&str], Option<&str>); 7] = [
        (&["status"], Some("status")),
        (&["--no-optional-locks", "diff"], Some("diff")),
        (&["-c", "a=b", "-c", "c=d", "merge"], Some("merge")),
        (
            &["-C", "dir", "--git-dir", "x", "--work-tree", "y", "log"],
            Some("log"),
        ),
        (&["--git-dir=x", "--namespace", "n", "show"], Some("show")),
        (&["--version"], None),
        (&[], None),
    ];
    for (args, expected) in cases {
        assert_eq!(subcommand(args), expected, "{args:?}");
    }
}

#[test]
fn the_deadline_follows_the_subcommand_behind_a_global_option() {
    assert_eq!(
        git_timeout(&["-c", "a=b", "commit-tree"]),
        GIT_MUTATION_TIMEOUT
    );
    assert_eq!(git_timeout(&["--no-pager", "fetch"]), GIT_NETWORK_TIMEOUT);
    assert_eq!(
        git_timeout(&["--no-optional-locks", "status"]),
        GIT_READ_TIMEOUT
    );
}

/// A checkout's own git directory: git writes through a planted link there, so
/// the runner refuses to start it. A flipping writer can still win the moment
/// between this look and git's open; only `WorktreeGit::pinned_main` closes that.
#[test]
fn a_planted_link_in_the_checkouts_git_directory_stops_the_command() {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let plain = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid"])
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success(), "{args:?}: {output:?}");
    };
    plain(&["init", "-q", "-b", "main"]);
    plain(&["commit", "-q", "--allow-empty", "-m", "base"]);
    let victim = tmp.path().join("victim");
    std::fs::write(&victim, "orig\n").unwrap();
    let link = repo.join(".git/COMMIT_EDITMSG");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&victim, &link).unwrap();

    let args = [
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@example.invalid",
        "commit",
    ];
    let args = [&args[..], &["-q", "--allow-empty", "-m", "through"]].concat();
    let error = run_git(&args, &repo).unwrap_err();

    assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "orig\n");

    plain(&["commit", "-q", "--allow-empty", "-m", "through"]);
    let reached = std::fs::read_to_string(&victim).unwrap();
    assert!(
        reached.contains("through"),
        "git did not follow it: {reached}"
    );
}
