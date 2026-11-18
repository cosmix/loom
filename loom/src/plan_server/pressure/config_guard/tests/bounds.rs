//! Snapshots stay bounded and fail closed on trees they cannot check.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::super::lookup::{MAX_WORK, SPENT};
use super::super::{changed, snapshot, ConfigSnapshot, MAX_ENTRIES};
use super::put;

/// Far below the 2 MiB pressure thread: recursion per link level would overflow it.
const SMALL_STACK: usize = 256 * 1024;

/// Snapshot `root` on a thread with `SMALL_STACK`, returning the error text on failure.
fn snapshot_on_small_stack(root: &Path) -> Result<ConfigSnapshot, String> {
    snapshot_on_thread(root, false).0
}

/// Snapshot `root` on a thread with `SMALL_STACK`, with the work units it spent; `portable`
/// opens every path component by component, as on macOS, where there is no `openat2`.
fn snapshot_on_thread(root: &Path, portable: bool) -> (Result<ConfigSnapshot, String>, usize) {
    let root = root.to_path_buf();
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(move || {
            if portable {
                crate::fs::safe_fs::force_disable_openat2_for_tests();
            }
            let result = snapshot(&root).map_err(|error| format!("{error:#}"));
            (result, SPENT.with(|spent| spent.get()))
        })
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

/// `.claude/l` serves the deepest of 120 nested directories, each with an `up -> ..` link, the
/// deepest holding 1000 FIFOs: every up link re-walks its parent's subtree under a longer path.
fn nested_up_link_chain(root: &Path) {
    let mut deepest = PathBuf::from("n");
    for _ in 0..120 {
        deepest.push("c");
        fs::create_dir_all(root.join(&deepest)).unwrap();
        symlink("..", root.join(&deepest).join("up")).unwrap();
    }
    for index in 0..1_000 {
        let fifo = root.join(&deepest).join(format!("f{index}"));
        nix::unistd::mkfifo(fifo.as_path(), nix::sys::stat::Mode::S_IRWXU).unwrap();
    }
    fs::create_dir(root.join(".claude")).unwrap();
    symlink(Path::new("..").join(&deepest), root.join(".claude/l")).unwrap();
}

/// Snapshot the nested up-link chain, `portable` as in `snapshot_on_thread`: it must end at
/// the entry cap having spent at most `MAX_CHAIN_WORK`.
fn assert_the_chain_stops_cheaply(portable: bool) {
    let root = tempdir().unwrap();
    nested_up_link_chain(root.path());

    let (result, spent) = snapshot_on_thread(root.path(), portable);

    let error = result.unwrap_err();
    // The whole phrase: "more than {MAX_ENTRIES}" alone is a prefix of the work cap's message.
    assert!(
        error.contains(&format!("more than {MAX_ENTRIES} configuration entries")),
        "{error}"
    );
    assert!(spent <= MAX_CHAIN_WORK, "spent {spent} work units");
}

/// What the chain may cost. Cached, each real directory, link and path prefix is looked up
/// once, so the cost follows the real tree: about 72,500 work units. Uncached, every re-walk
/// pays again for paths about 120 components deep and the snapshot runs into `MAX_WORK`
/// (1,000,000) before the entry cap. A tenth of `MAX_WORK` leaves a third of headroom over the
/// cached cost while staying a tenth of the budget a lost cache exhausts.
const MAX_CHAIN_WORK: usize = MAX_WORK / 10;

#[test]
fn a_nested_up_link_chain_stops_at_the_entry_cap() {
    assert_the_chain_stops_cheaply(false);
}

/// Without `openat2` each lookup opens its path component by component; the re-walked
/// directories and links are looked up once each, so the chain costs the same.
#[test]
fn a_nested_up_link_chain_stops_at_the_entry_cap_without_openat2() {
    assert_the_chain_stops_cheaply(true);
}

/// Links few enough for the entry cap whose targets step through many components each fail
/// closed on the work budget instead of resolving every one.
#[test]
fn links_costly_to_resolve_fail_closed_on_the_work_budget() {
    let root = tempdir().unwrap();
    fs::create_dir_all(root.path().join(".claude/a")).unwrap();
    // 2 components per `a/..`: each target costs about 600 units; 3000 of them pass the budget.
    let target = "a/..".repeat(200).replace("..a", "../a");
    for index in 0..3_000 {
        symlink(&target, root.path().join(format!(".claude/l{index}"))).unwrap();
    }

    let error = snapshot_on_small_stack(root.path()).unwrap_err();

    assert!(
        error.contains(&format!(
            "more than {MAX_WORK} configuration path lookups (symlinks re-serving directories?); \
             session configuration cannot be checked"
        )),
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
