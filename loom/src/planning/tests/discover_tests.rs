use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use crate::planning::discover::{discover, read_doc, walk_doc, DocDirState, WalkScope};
use crate::planning::kinds::DocId;

fn put(root: &Path, rel: &str, content: &str) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

#[test]
fn missing_doc_is_an_empty_tree() {
    let dir = tempfile::tempdir().unwrap();
    let tree = discover(dir.path()).unwrap();
    assert_eq!(tree.doc_dir, DocDirState::Missing);
    assert!(tree.docs.is_empty());
}

#[test]
fn symlinked_doc_is_an_empty_tree() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "real/a.md", "x\n");
    symlink(dir.path().join("real"), dir.path().join("doc")).unwrap();
    let tree = discover(dir.path()).unwrap();
    assert_eq!(tree.doc_dir, DocDirState::Symlinked);
    assert!(tree.docs.is_empty());
    assert!(read_doc(dir.path(), Path::new("doc/a.md")).is_err());
}

#[test]
fn a_fifo_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "doc/a.md", "x\n");
    let fifo = dir.path().join("doc/pipe.md");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).unwrap();
    let tree = discover(dir.path()).unwrap();
    let paths: Vec<_> = tree.docs.iter().map(|d| d.path.clone()).collect();
    assert_eq!(paths, [Path::new("doc/a.md")]);
}

#[test]
fn depth_beyond_the_bound_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let deep = vec!["d"; 17].join("/");
    put(dir.path(), &format!("doc/{deep}/a.md"), "x\n");
    let error = discover(dir.path()).unwrap_err().to_string();
    assert!(error.contains("16"), "{error}");
    let ok = vec!["d"; 15].join("/");
    let other = tempfile::tempdir().unwrap();
    put(other.path(), &format!("doc/{ok}/a.md"), "x\n");
    assert_eq!(discover(other.path()).unwrap().docs.len(), 1);
}

#[test]
fn summaries_are_walked_only_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "doc/loom/summaries/s.md", "x\n");
    put(dir.path(), "doc/a.md", "x\n");
    let (_, docs) = walk_doc(dir.path(), WalkScope::Documents).unwrap();
    assert_eq!(docs.len(), 1);
    let (_, all) = walk_doc(dir.path(), WalkScope::IncludeSummaries).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn read_doc_follows_an_allowed_link_and_refuses_escapes() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "doc/a.md", "alpha\n");
    put(dir.path(), "secret", "no\n");
    symlink("a.md", dir.path().join("doc/in.md")).unwrap();
    symlink("../secret", dir.path().join("doc/out.md")).unwrap();
    assert_eq!(
        read_doc(dir.path(), Path::new("doc/in.md")).unwrap(),
        "alpha\n"
    );
    assert!(read_doc(dir.path(), Path::new("doc/out.md")).is_err());
    assert!(read_doc(dir.path(), Path::new("doc/../secret")).is_err());
    assert!(read_doc(dir.path(), Path::new("secret")).is_err());
}

#[test]
fn by_id_resolves_a_plan_to_its_winning_copy() {
    let dir = tempfile::tempdir().unwrap();
    for rel in [
        "doc/plans/IN_PROGRESS-PLAN-foo.md",
        "doc/plans/PLAN-foo.md",
        "doc/plans/archive/DONE-PLAN-foo.md",
        "doc/product/index.md",
    ] {
        put(dir.path(), rel, "# T\n");
    }
    let tree = discover(dir.path()).unwrap();
    let winner = Path::new("doc/plans/archive/DONE-PLAN-foo.md");
    assert_eq!(tree.plan("foo").unwrap().path, winner);
    let found = tree.by_id("plan/foo").unwrap();
    assert_eq!(found.path, winner);
    assert_eq!(found.id, DocId("plan/foo".into()));
    assert_eq!(
        tree.by_id("product/index").unwrap().path,
        Path::new("doc/product/index.md")
    );
    assert!(tree.by_id("plan/bar").is_none());
}

#[test]
fn has_dir_is_true_only_for_directories_holding_documents() {
    let dir = tempfile::tempdir().unwrap();
    for rel in ["doc/a/b.md", "doc/a-b/x.md", "doc/c.md", "doc/d/e/f.md"] {
        put(dir.path(), rel, "x\n");
    }
    let tree = discover(dir.path()).unwrap();
    for yes in ["doc", "doc/a", "doc/a-b", "doc/d", "doc/d/e", "doc/d/e/"] {
        assert!(tree.has_dir(Path::new(yes)), "{yes}");
    }
    for no in [
        "doc/a/b.md",
        "doc/c.md",
        "doc/c",
        "doc/ab",
        "doc/a/b",
        "doc/d/e/f.md/g",
        "doc/typo",
        "other",
    ] {
        assert!(!tree.has_dir(Path::new(no)), "{no}");
    }
}

#[test]
fn a_root_below_a_symlinked_directory_still_discovers_and_reads_links() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "real/repo/doc/a.md", "alpha\n");
    symlink("a.md", dir.path().join("real/repo/doc/in.md")).unwrap();
    symlink(dir.path().join("real"), dir.path().join("alias")).unwrap();
    let root = dir.path().join("alias/repo");
    let tree = discover(&root).unwrap();
    let linked = tree.get(Path::new("doc/in.md")).unwrap();
    assert!(linked.via_symlink);
    assert_eq!(read_doc(&root, Path::new("doc/in.md")).unwrap(), "alpha\n");
}
