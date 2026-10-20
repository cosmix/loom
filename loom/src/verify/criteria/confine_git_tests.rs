use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{git_dirs, NetworkAccess};
use crate::git::target_guard::test_support::{git, repo, Repo};
use crate::models::stage::CommandConfinement;
use crate::verify::criteria::{run_spec_in_optional_tree, CommandSpec};

/// A repository with linked worktrees `a` and `b` under `.worktrees/`, a hook,
/// a FETCH_HEAD, and the main checkout's index.
fn linked_worktrees() -> Repo {
    let repo = repo();
    for name in ["a", "b"] {
        let path = format!(".worktrees/{name}");
        git(&repo.root, &["worktree", "add", "-q", "-b", name, &path]);
    }
    let common = repo.root.join(".git");
    std::fs::create_dir_all(common.join("hooks")).unwrap();
    std::fs::write(common.join("hooks/pre-commit"), "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::write(common.join("FETCH_HEAD"), "fetched\n").unwrap();
    repo
}

/// Directories, then files, a command confined to worktree `a` must not read.
fn hidden(common: &Path) -> ([PathBuf; 2], [PathBuf; 2]) {
    (
        ["hooks", "worktrees/b"].map(|entry| common.join(entry)),
        ["index", "FETCH_HEAD"].map(|entry| common.join(entry)),
    )
}

/// The read-only roots a command denied the network gets in `tree`.
fn roots(tree: &Path) -> Vec<PathBuf> {
    git_dirs(tree, NetworkAccess::Denied)
        .map(|git| git.roots)
        .unwrap_or_default()
}

#[test]
fn a_linked_worktree_exposes_its_own_metadata_and_the_shared_store_only() {
    let repo = linked_worktrees();
    let common = repo.root.join(".git");
    let roots = roots(&repo.root.join(".worktrees/a"));
    for expected in ["worktrees/a", "config", "objects", "refs", "HEAD", "info"] {
        assert!(
            roots.contains(&common.join(expected)),
            "{expected}: {roots:?}"
        );
    }
    let (directories, files) = hidden(&common);
    for path in directories.iter().chain(&files) {
        assert!(path.exists(), "fixture lacks {}", path.display());
        assert!(
            !roots.iter().any(|root| path.starts_with(root)),
            "{} exposed: {roots:?}",
            path.display()
        );
    }
}

#[test]
fn a_main_checkout_or_a_plain_directory_needs_no_git_roots() {
    let repo = repo();
    let main = git_dirs(&repo.root, NetworkAccess::Denied).unwrap();
    assert_eq!(main.admin, repo.root.join(".git"));
    assert!(main.roots.is_empty());
    let plain = tempfile::tempdir().unwrap();
    let plain = plain.path().canonicalize().unwrap();
    assert_eq!(git_dirs(&plain, NetworkAccess::Denied), None);
}

/// Whoever writes the tree can point its `.git` file at a directory outside
/// the repository, at one in the tree whose `config` links out, or at another
/// worktree's directory; none of them binds.
#[test]
fn a_crafted_git_file_binds_nothing() {
    let repo = linked_worktrees();
    let tree = repo.root.join(".worktrees/a");
    let outside = tempfile::tempdir().unwrap();
    let outside = outside.path().canonicalize().unwrap();
    std::fs::write(outside.join("config"), "secret\n").unwrap();
    std::fs::create_dir(tree.join("evil")).unwrap();
    std::os::unix::fs::symlink(&outside, tree.join("evil/config")).unwrap();
    let other = repo.root.join(".git/worktrees/b");
    for named in [outside.as_path(), Path::new("evil"), other.as_path()] {
        std::fs::write(tree.join(".git"), format!("gitdir: {}\n", named.display())).unwrap();
        assert_eq!(git_dirs(&tree, NetworkAccess::Denied), None, "{named:?}");
    }
}

#[test]
fn a_common_entry_linking_out_of_the_common_directory_is_not_bound() {
    let repo = linked_worktrees();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().canonicalize().unwrap().join("secret");
    std::fs::write(&secret, "secret\n").unwrap();
    let shallow = repo.root.join(".git/shallow");
    std::os::unix::fs::symlink(&secret, &shallow).unwrap();
    let roots = roots(&repo.root.join(".worktrees/a"));
    assert!(roots.contains(&repo.root.join(".git/objects")), "{roots:?}");
    assert!(
        !roots.iter().any(|root| *root == shallow || *root == secret),
        "{roots:?}"
    );
}

/// A FIFO with no writer would block a plain read forever; the lookup runs on
/// another thread so a regression fails the test instead of hanging it.
#[test]
fn a_fifo_at_commondir_neither_blocks_nor_binds() {
    let repo = linked_worktrees();
    let commondir = repo.root.join(".git/worktrees/a/commondir");
    std::fs::remove_file(&commondir).unwrap();
    nix::unistd::mkfifo(&commondir, nix::sys::stat::Mode::S_IRWXU).unwrap();
    let tree = repo.root.join(".worktrees/a");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(git_dirs(&tree, NetworkAccess::Denied));
    });
    let dirs = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("reading commondir blocked on a FIFO");
    assert_eq!(dirs, None);
}

#[test]
fn provisioning_binds_no_git_metadata() {
    let repo = linked_worktrees();
    let tree = repo.root.join(".worktrees/a");
    assert_eq!(git_dirs(&tree, NetworkAccess::Provision), None);
    if crate::process::sandbox_probe::skip_unless(
        super::super::launcher().is_some(),
        "provisioning_binds_no_git_metadata",
        "OS sandbox launcher absent",
    ) {
        return;
    }
    let admin = repo.root.join(".git/worktrees/a");
    let mut original = std::process::Command::new("/bin/sh");
    original.current_dir(&tree);
    let (wrapped, _scratch, git) =
        super::super::wrap(&original, Some(&tree), NetworkAccess::Provision).unwrap();
    assert_eq!(git, None);
    let admin = admin.to_string_lossy();
    assert!(
        !wrapped
            .get_args()
            .any(|arg| arg.to_string_lossy().contains(&*admin)),
        "provisioning names {admin}"
    );
}

#[test]
fn confined_git_reads_a_linked_worktree_and_writes_nothing() {
    if crate::process::sandbox_probe::skip_unless(
        crate::verify::criteria::host_launcher().is_some() && which::which("git").is_ok(),
        "confined_git_reads_a_linked_worktree_and_writes_nothing",
        "OS sandbox launcher or git absent",
    ) {
        return;
    }
    let repo = linked_worktrees();
    let common = repo.root.join(".git");
    let tree = repo.root.join(".worktrees/a");
    let tip = git(&tree, &["rev-parse", "HEAD"]);
    let config = std::fs::read(common.join("config")).unwrap();
    let (directories, files) = hidden(&common);
    let unreadable = directories
        .iter()
        .map(|path| format!("! ls '{}'", path.display()))
        .chain(
            files
                .iter()
                .map(|path| format!("! cat '{}'", path.display())),
        )
        .collect::<Vec<_>>()
        .join(" && ");
    let line = format!("git ls-files && {unreadable} && ! git commit -q --allow-empty -m confined");
    let result = run_spec_in_optional_tree(
        &CommandSpec::shell(line),
        Some(&tree),
        Some(&tree),
        Duration::from_secs(30),
        CommandConfinement::Confined,
    )
    .unwrap();
    assert!(
        result.success,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(result.stdout.contains("README.md"), "{}", result.stdout);
    assert_eq!(git(&tree, &["rev-parse", "HEAD"]), tip);
    assert_eq!(std::fs::read(common.join("config")).unwrap(), config);
}
