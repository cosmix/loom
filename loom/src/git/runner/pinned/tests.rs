use super::*;
use crate::git::runner::run_git_checked;
use crate::git::runner::tests::{isolated_git, isolated_git_ok};
use std::io::{Seek, Write};
use std::path::PathBuf;

/// A repository whose `core.fsmonitor` command touches the returned marker
/// file (absent on return). The fsmonitor state is already recorded in its
/// index, so every later index read runs the command.
fn repo_with_marker_fsmonitor() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    isolated_git_ok(&root, &["init", "-b", "main"]);
    std::fs::write(root.join("seed.txt"), "seed").unwrap();
    isolated_git_ok(&root, &["add", "seed.txt"]);
    let marker = root.join(".git").join("fsmonitor-ran");
    let command = format!("touch '{}'; false", marker.display());
    isolated_git_ok(&root, &["config", "core.fsmonitor", &command]);
    // The first index refresh after the key is set records the fsmonitor
    // extension; plain `ls-files` runs the command only from then on.
    isolated_git_ok(&root, &["status", "--porcelain"]);
    let _ = std::fs::remove_file(&marker);
    (temp, root, marker)
}

/// Positive control: plain git reading the index runs the fixture command, so
/// the absence asserted below comes from the runner and not a broken fixture.
#[test]
fn plain_git_reading_the_index_runs_the_fsmonitor_command() {
    let (_temp, root, marker) = repo_with_marker_fsmonitor();

    let output = isolated_git(&root, &["ls-files", "-s", "-z"]);

    assert!(output.status.success(), "{output:?}");
    assert!(
        marker.exists(),
        "the fixture fsmonitor command must be live"
    );
}

#[test]
fn runner_calls_never_run_the_repository_fsmonitor_command() {
    let (_temp, root, marker) = repo_with_marker_fsmonitor();

    run_git_checked(&["ls-files", "-s", "-z"], &root).expect("ls-files");
    run_git_checked(&["status", "--porcelain"], &root).expect("status");
    run_git_pinned_checked(&["ls-files", "-s", "-z"], &root).expect("pinned ls-files");
    let mut names = tempfile::tempfile().unwrap();
    names.write_all(b"seed.txt\0").unwrap();
    names.rewind().unwrap();
    let output = run_git_pinned(
        &["check-attr", "--stdin", "-z", "linguist-vendored"],
        Some(names),
        &root,
    )
    .expect("pinned check-attr");

    assert!(output.status.success(), "{output:?}");
    assert!(
        !marker.exists(),
        "a call routed through the runner ran the repository's core.fsmonitor command"
    );
}

#[test]
fn a_stdin_file_reaches_git_as_its_standard_input() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    isolated_git_ok(root, &["init", "-b", "main"]);
    std::fs::write(root.join(".gitattributes"), "a.txt linguist-vendored\n").unwrap();
    let mut names = tempfile::tempfile().unwrap();
    names.write_all(b"a.txt\0b.txt\0").unwrap();
    names.rewind().unwrap();

    let output = run_git_pinned(
        &["check-attr", "--stdin", "-z", "linguist-vendored"],
        Some(names),
        root,
    )
    .expect("check-attr");

    assert_eq!(
        output.stdout, b"a.txt\0linguist-vendored\0set\0b.txt\0linguist-vendored\0unspecified\0",
        "{output:?}"
    );
}

#[test]
fn pinned_commands_drop_every_variable_that_redirects_git() {
    let removed = |command: &Command| -> Vec<String> {
        command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect()
    };

    let pinned = removed(&pinned_command("git", &[], None, Path::new(".")));
    let plain = removed(&git_command("git", &[], &[], Path::new(".")));

    for variable in REPO_REDIRECT_ENV {
        assert!(pinned.iter().any(|name| name == variable), "{variable}");
        assert!(plain.iter().any(|name| name == variable), "{variable}");
    }
}
