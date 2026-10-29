//! Snapshots stay bounded and fail closed on trees they cannot check.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::super::{changed, snapshot, ConfigSnapshot, MAX_ENTRIES};
use super::put;

/// Far below the 2 MiB pressure thread: recursion per link level would overflow it.
const SMALL_STACK: usize = 256 * 1024;

/// Snapshot `root` on a thread with `SMALL_STACK`, returning the error text on failure.
fn snapshot_on_small_stack(root: &Path) -> Result<ConfigSnapshot, String> {
    let root = root.to_path_buf();
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(move || snapshot(&root).map_err(|error| format!("{error:#}")))
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn a_long_sibling_link_chain_fails_closed_without_recursing() {
    let root = tempdir().unwrap();
    let links = 5_000;
    for level in 0..=links {
        fs::create_dir(root.path().join(format!("d{level}"))).unwrap();
    }
    for level in 0..links {
        let next = format!("../d{}", level + 1);
        symlink(next, root.path().join(format!("d{level}/l"))).unwrap();
    }
    fs::create_dir(root.path().join(".claude")).unwrap();
    symlink("../d0", root.path().join(".claude/x")).unwrap();

    let error = snapshot_on_small_stack(root.path()).unwrap_err();

    assert!(error.contains("cannot be checked"), "{error}");
}

#[test]
fn a_nested_up_link_chain_stops_at_the_entry_cap() {
    let root = tempdir().unwrap();
    let mut deepest = PathBuf::from("n");
    for _ in 0..120 {
        deepest.push("c");
        fs::create_dir_all(root.path().join(&deepest)).unwrap();
        symlink("..", root.path().join(&deepest).join("up")).unwrap();
    }
    for index in 0..1_000 {
        let fifo = root.path().join(&deepest).join(format!("f{index}"));
        nix::unistd::mkfifo(fifo.as_path(), nix::sys::stat::Mode::S_IRWXU).unwrap();
    }
    fs::create_dir(root.path().join(".claude")).unwrap();
    symlink(
        Path::new("..").join(&deepest),
        root.path().join(".claude/l"),
    )
    .unwrap();

    let error = snapshot_on_small_stack(root.path()).unwrap_err();

    assert!(
        error.contains(&format!("more than {MAX_ENTRIES}")),
        "{error}"
    );
}

#[test]
fn a_claude_link_serving_the_repository_root_fails_closed() {
    let root = tempdir().unwrap();
    fs::create_dir(root.path().join(".claude")).unwrap();
    symlink("..", root.path().join(".claude/all")).unwrap();

    let error = format!("{:#}", snapshot(root.path()).unwrap_err());

    assert!(
        error.contains(
            ".claude/all serves the repository root; session configuration cannot be checked"
        ),
        "{error}"
    );
}

#[test]
fn a_claude_link_serving_repository_state_fails_closed() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join(".git/refs")).unwrap();
    fs::create_dir(root.path().join(".claude")).unwrap();
    symlink("../.git/refs", root.path().join(".claude/refs")).unwrap();

    let error = format!("{:#}", snapshot(root.path()).unwrap_err());

    assert!(
        error.contains(".claude/refs serves .git/refs inside .git"),
        "{error}"
    );
}

#[test]
fn root_file_links_serving_directories_are_not_walked() {
    let root = tempdir().unwrap();
    put(root.path(), "docs/a.md", "# A\n");
    symlink("docs", root.path().join("CLAUDE.md")).unwrap();
    symlink(".", root.path().join(".mcp.json")).unwrap();
    let before = snapshot(root.path()).unwrap();
    put(root.path(), "docs/a.md", "# A, edited\n");
    put(root.path(), "docs/b.md", "# B\n");

    let after = snapshot(root.path()).unwrap();

    assert_eq!(changed(&before, &after), Vec::<String>::new());
    let paths: Vec<&String> = after.files.keys().collect();
    assert_eq!(paths, [".mcp.json", "CLAUDE.md"]);
    assert!(after.files["CLAUDE.md"].contains(":directory:"));
}
