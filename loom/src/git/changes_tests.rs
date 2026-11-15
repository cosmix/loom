//! Arguments and parsing of the shared change listing; the end-to-end
//! scenarios are in `changes_contract_tests.rs`.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use super::*;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Changes Test")
        .env("GIT_AUTHOR_EMAIL", "changes@example.invalid")
        .env("GIT_COMMITTER_NAME", "Changes Test")
        .env("GIT_COMMITTER_EMAIL", "changes@example.invalid")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository with `a.txt` and `doc/b.md` committed on `main`.
fn repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(root.join("doc")).unwrap();
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    std::fs::write(root.join("doc/b.md"), "b\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "base"]);
    dir
}

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
}

#[test]
fn nul_paths_keeps_spelling_and_refuses_non_utf8() {
    assert_eq!(nul_paths(b" a \0b\n\0\0").unwrap(), set(&[" a ", "b\n"]));
    let error = nul_paths(b"ok\0bad\xff\0").unwrap_err();
    assert!(format!("{error:#}").contains("git returned a non-UTF-8 path"));
}

#[test]
fn merge_base_is_the_fork_point_id() {
    let dir = repo();
    let root = dir.path();
    let git_repo = WorktreeGit::discovered(root);
    let fork = merge_base(&git_repo, "HEAD", "main").unwrap();
    git(root, &["switch", "-q", "-c", "feature"]);
    std::fs::write(root.join("a.txt"), "feature\n").unwrap();
    git(root, &["commit", "-q", "-am", "feature"]);

    assert_eq!(merge_base(&git_repo, "HEAD", "main").unwrap(), fork);
    assert!(matches!(fork.len(), 40 | 64), "{fork:?}");
    assert!(merge_base(&git_repo, "HEAD", "no-such-branch").is_err());
}

#[test]
fn filter_pathspecs_and_commit_range_narrow_the_listing() {
    let dir = repo();
    let root = dir.path();
    let git_repo = WorktreeGit::discovered(root);
    let base = merge_base(&git_repo, "HEAD", "main").unwrap();
    std::fs::write(root.join("doc/c.md"), "c\n").unwrap();
    git(root, &["add", "doc/c.md"]);
    git(root, &["commit", "-q", "-m", "add c"]);
    std::fs::write(root.join("a.txt"), "edited\n").unwrap();
    std::fs::write(root.join("doc/b.md"), "edited\n").unwrap();
    let query = |filter, to, pathspecs| DiffNames {
        from: &base,
        to,
        filter,
        all_submodules: true,
        pathspecs,
    };

    let all = changed_names(&git_repo, &query(None, None, &[])).unwrap();
    let added = changed_names(&git_repo, &query(Some("A"), None, &[])).unwrap();
    let doc = changed_names(&git_repo, &query(None, None, &["doc"])).unwrap();
    let committed = changed_names(&git_repo, &query(None, Some("HEAD"), &[])).unwrap();

    assert_eq!(all, set(&["a.txt", "doc/b.md", "doc/c.md"]));
    assert_eq!(added, set(&["doc/c.md"]));
    assert_eq!(doc, set(&["doc/b.md", "doc/c.md"]));
    assert_eq!(committed, set(&["doc/c.md"]));
}

#[test]
fn untracked_names_skip_ignored_files_and_honour_pathspecs() {
    let dir = repo();
    let root = dir.path();
    std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(root.join("build.log"), "x\n").unwrap();
    std::fs::write(root.join("doc/new.md"), "x\n").unwrap();
    let git_repo = WorktreeGit::discovered(root);

    assert_eq!(
        untracked_names(&git_repo, &[]).unwrap(),
        set(&[".gitignore", "doc/new.md"])
    );
    assert_eq!(
        untracked_names(&git_repo, &["doc"]).unwrap(),
        set(&["doc/new.md"])
    );
}

/// A tool artifact is left out only while untracked; one tracked at the base
/// stays a change.
#[test]
fn changes_since_drops_only_untracked_tool_artifacts() {
    let dir = repo();
    let root = dir.path();
    std::fs::write(root.join(".profile"), "").unwrap();
    git(root, &["add", "-f", ".profile"]);
    git(root, &["commit", "-q", "-m", "tracked profile"]);
    let git_repo = WorktreeGit::discovered(root);
    let base = merge_base(&git_repo, "HEAD", "main").unwrap();
    std::fs::write(root.join(".profile"), "export A=1\n").unwrap();
    std::fs::write(root.join(".gitconfig"), "").unwrap();

    assert_eq!(
        changes_since(&git_repo, &base, None).unwrap(),
        set(&[".profile"])
    );
}

/// The filter limits the tracked changes only: a modified file drops out of an
/// `A` listing, an untracked file stays in it.
#[test]
fn changes_since_filter_limits_tracked_changes_not_untracked_files() {
    let dir = repo();
    let root = dir.path();
    let git_repo = WorktreeGit::discovered(root);
    let base = merge_base(&git_repo, "HEAD", "main").unwrap();
    std::fs::write(root.join("a.txt"), "changed\n").unwrap();
    std::fs::write(root.join("doc/staged.md"), "s\n").unwrap();
    git(root, &["add", "doc/staged.md"]);
    std::fs::write(root.join("new.txt"), "n\n").unwrap();

    assert_eq!(
        changes_since(&git_repo, &base, Some("A")).unwrap(),
        set(&["doc/staged.md", "new.txt"])
    );
    assert_eq!(
        changes_since(&git_repo, &base, None).unwrap(),
        set(&["a.txt", "doc/staged.md", "new.txt"])
    );
}

/// The listing reads a private copy of the index: a stat-dirty file is no
/// change, a staged new file is one, and the real index is not rewritten,
/// through pinned git (`contract_worktree`). The same through discovered git
/// is covered in `changes_contract_tests.rs`.
#[test]
fn pinned_listing_sees_the_index_and_writes_none() {
    let tmp = TempDir::new().unwrap();
    let worktree =
        crate::verify::contracts::test_support::contract_worktree(&tmp.path().join("repo"), "s1");
    let git_repo = crate::verify::contracts::test_support::pinned(&worktree);
    let file = std::fs::File::options()
        .write(true)
        .open(worktree.join("README.md"))
        .unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(3600))
        .unwrap();
    drop(file);
    std::fs::write(worktree.join("staged.rs"), "fn s() {}\n").unwrap();
    git(&worktree, &["add", "staged.rs"]);
    let index = git_repo.git_dir_path().unwrap().join("index");
    let before = std::fs::read(&index).unwrap();
    let query = DiffNames {
        from: "HEAD",
        to: None,
        filter: None,
        all_submodules: false,
        pathspecs: &[],
    };

    let listed = changed_names(&git_repo, &query).unwrap();

    assert_eq!(listed, set(&["staged.rs"]));
    assert!(
        std::fs::read(&index).unwrap() == before,
        "the index was rewritten"
    );
}

/// The private copy keeps the index's modification time. A file whose
/// content changed without changing size or modification time is told apart
/// from the index entry only while its time is not older than the index's
/// (a racily clean entry, which git compares by content); a copy stamped with
/// the copy time would make the entry look clean.
#[test]
fn private_index_keeps_the_index_modification_time() {
    let dir = repo();
    let root = dir.path();
    let stamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    let touch = |path: &Path| {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
    };
    git(root, &["config", "core.trustctime", "false"]);
    std::fs::write(root.join("a.txt"), "x\n").unwrap();
    touch(&root.join("a.txt"));
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-q", "-m", "x"]);
    touch(&root.join(".git/index"));
    std::fs::write(root.join("a.txt"), "y\n").unwrap();
    touch(&root.join("a.txt"));
    let query = DiffNames {
        from: "HEAD",
        to: None,
        filter: None,
        all_submodules: false,
        pathspecs: &[],
    };

    let listed = changed_names(&WorktreeGit::discovered(root), &query).unwrap();

    assert_eq!(listed, set(&["a.txt"]));
}
