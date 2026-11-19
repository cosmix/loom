//! The grant files of a stored kickoff target: which exist, and deleting exactly those.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::delete::*;

const PLAN: &str = "doc/plans/PLAN-demo-work.md";
const BUNDLE: &str = "doc/plans/briefs/demo-work";
const FEATURE: &str = "doc/product/demo-work/index.md";

fn project() -> TempDir {
    tempfile::tempdir().expect("a scratch project")
}

fn touch(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "# file\n").unwrap();
    path
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "git {args:?}: {stderr}");
}

/// Every file of the tree under `root`, relative, sorted.
fn tree(root: &Path) -> Vec<String> {
    let mut all = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap().file_type();
            if kind.is_dir() {
                pending.push(path.clone());
            }
            all.push(path.strip_prefix(root).unwrap().display().to_string());
        }
    }
    all.sort();
    all
}

#[test]
fn written_lists_plan_document_then_bundle() {
    let dir = project();
    touch(dir.path(), PLAN);
    touch(dir.path(), "doc/plans/briefs/demo-work/a.md");
    let found = written(dir.path(), PLAN).unwrap();
    assert_eq!(found.paths, [PLAN, BUNDLE]);
    assert!(!found.committed, "no repository holds them");
}

#[test]
fn written_lists_only_what_exists() {
    let dir = project();
    assert!(written(dir.path(), PLAN).unwrap().paths.is_empty());
    touch(dir.path(), "doc/plans/briefs/demo-work/a.md");
    assert_eq!(written(dir.path(), PLAN).unwrap().paths, [BUNDLE]);
    touch(dir.path(), FEATURE);
    assert_eq!(written(dir.path(), FEATURE).unwrap().paths, [FEATURE]);
}

#[test]
fn committed_reads_head() {
    let dir = project();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    touch(dir.path(), PLAN);
    assert!(!written(dir.path(), PLAN).unwrap().committed, "no HEAD");
    git(dir.path(), &["add", "--", PLAN]);
    git(dir.path(), &["commit", "-q", "-m", "plan"]);
    assert!(written(dir.path(), PLAN).unwrap().committed);
    touch(dir.path(), "doc/plans/briefs/demo-work/a.md");
    assert!(
        written(dir.path(), PLAN).unwrap().committed,
        "any path held"
    );
}

#[test]
fn committed_is_false_when_head_lacks_the_files() {
    let dir = project();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    touch(dir.path(), "README.md");
    git(dir.path(), &["add", "--", "README.md"]);
    git(dir.path(), &["commit", "-q", "-m", "readme"]);
    touch(dir.path(), PLAN);
    assert!(!written(dir.path(), PLAN).unwrap().committed);
}

#[test]
fn remove_deletes_plan_and_bundle_only() {
    let dir = project();
    let plan = touch(dir.path(), PLAN);
    touch(dir.path(), "doc/plans/briefs/demo-work/a.md");
    touch(dir.path(), "doc/plans/briefs/demo-work/deep/b.md");
    let other_plan = touch(dir.path(), "doc/plans/PLAN-other.md");
    let other_bundle = touch(dir.path(), "doc/plans/briefs/other/b.md");
    let longer = touch(dir.path(), "doc/plans/PLAN-demo-work-2.md");

    assert_eq!(remove_written(dir.path(), PLAN).unwrap(), [PLAN, BUNDLE]);

    assert!(!plan.exists());
    assert!(!dir.path().join(BUNDLE).exists());
    assert!(other_plan.exists() && other_bundle.exists() && longer.exists());
    assert!(
        dir.path().join("doc/plans/briefs").is_dir(),
        "briefs/ stays"
    );
}

#[test]
fn remove_with_nothing_written_removes_nothing() {
    let dir = project();
    let other = touch(dir.path(), "doc/plans/PLAN-other.md");
    assert!(remove_written(dir.path(), PLAN).unwrap().is_empty());
    assert!(remove_written(dir.path(), FEATURE).unwrap().is_empty());
    assert!(other.exists());
}

#[test]
fn remove_feature_drops_its_directory_when_empty() {
    let dir = project();
    touch(dir.path(), FEATURE);
    let sibling = touch(dir.path(), "doc/product/other/index.md");
    let removed = remove_written(dir.path(), FEATURE).unwrap();
    assert_eq!(removed, [FEATURE, "doc/product/demo-work"]);
    assert!(!dir.path().join("doc/product/demo-work").exists());
    assert!(sibling.exists());
}

#[test]
fn remove_feature_keeps_a_directory_that_holds_more() {
    let dir = project();
    touch(dir.path(), FEATURE);
    let notes = touch(dir.path(), "doc/product/demo-work/notes.md");
    assert_eq!(remove_written(dir.path(), FEATURE).unwrap(), [FEATURE]);
    assert!(
        notes.exists(),
        "only the document of a feature is the grant's"
    );
}

#[test]
fn remove_refuses_a_locked_spelling() {
    for locked in [
        "doc/plans/IN_PROGRESS-PLAN-demo-work.md",
        "doc/plans/DONE-PLAN-demo-work.md",
        "doc/plans/RETIRED-PLAN-demo-work.md",
        "doc/plans/archive/PLAN-demo-work.md",
        "doc/plans/archive/DONE-PLAN-demo-work.md",
    ] {
        let dir = project();
        let plan = touch(dir.path(), PLAN);
        let bundle = touch(dir.path(), "doc/plans/briefs/demo-work/a.md");
        touch(dir.path(), locked);
        let error = remove_written(dir.path(), PLAN).unwrap_err();
        let expected = format!("{locked} has gone into execution; it cannot be deleted here.");
        assert_eq!(error, expected);
        assert!(
            plan.exists() && bundle.exists(),
            "{locked}: nothing touched"
        );
    }
}

/// Targets a hand-edited draft file could hold. Each is refused by both calls and the file it
/// names, created where a path can be, survives.
#[test]
fn edited_targets_outside_the_grant_are_refused() {
    let edited = [
        "doc/loom/knowledge/INDEX.md",
        "doc/plans/PLAN-a/../PLAN-demo-work.md",
        "doc/product/../plans/PLAN-demo-work.md",
        "doc/plans/PLAN-demo-work.md/",
        "doc/product/demo-work/index.md/",
        "doc/plans/PLAN-Demo-work.md",
        "doc/plans/PLAN-d\u{0435}mo-work.md",
        "doc/product/a/b/index.md",
        "doc/product/archive/index.md",
        "doc/plans/PLAN-.md",
        "./doc/plans/PLAN-demo-work.md",
        "doc/plans//PLAN-demo-work.md",
        "/doc/plans/PLAN-demo-work.md",
        "doc/plans/briefs/demo-work",
        "doc/plans/IN_PROGRESS-PLAN-demo-work.md",
        "",
    ];
    for target in edited {
        let dir = project();
        touch(dir.path(), PLAN);
        touch(dir.path(), "doc/loom/knowledge/INDEX.md");
        touch(dir.path(), "doc/plans/PLAN-Demo-work.md");
        touch(dir.path(), "doc/plans/briefs/demo-work/a.md");
        let before = tree(dir.path());
        assert!(written(dir.path(), target).is_err(), "{target:?} listed");
        assert!(
            remove_written(dir.path(), target).is_err(),
            "{target:?} removed"
        );
        assert_eq!(tree(dir.path()), before, "{target:?} touched the tree");
    }
}

#[test]
fn symlinked_bundle_is_unlinked_never_followed() {
    let dir = project();
    let outside = project();
    let keep = touch(outside.path(), "keep.md");
    touch(dir.path(), PLAN);
    fs::create_dir_all(dir.path().join("doc/plans/briefs")).unwrap();
    symlink(outside.path(), dir.path().join(BUNDLE)).unwrap();

    assert_eq!(remove_written(dir.path(), PLAN).unwrap(), [PLAN, BUNDLE]);

    assert!(
        keep.exists(),
        "the delete followed the link out of the project"
    );
    assert!(fs::symlink_metadata(dir.path().join(BUNDLE)).is_err());
}

/// A symlinked `doc/`, `doc/plans/` or `doc/plans/briefs/` is refused before anything goes.
#[test]
fn symlinked_directory_on_the_way_is_refused() {
    for linked in ["doc", "doc/plans", "doc/plans/briefs"] {
        let dir = project();
        let outside = project();
        let real = outside.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let target_rel = Path::new(PLAN);
        let bundle_rel = Path::new(BUNDLE).join("a.md");
        let link = dir.path().join(linked);
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        // The real tree behind the link holds the same files the grant names.
        for rel in [target_rel, bundle_rel.as_path()] {
            if let Ok(below) = rel.strip_prefix(linked) {
                let path = real.join(below);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, "# outside\n").unwrap();
            }
        }
        symlink(&real, &link).unwrap();
        if linked == "doc/plans/briefs" {
            touch(dir.path(), PLAN);
        }
        let before = tree(outside.path());

        assert!(written(dir.path(), PLAN).is_err(), "{linked}: listed");
        assert!(
            remove_written(dir.path(), PLAN).is_err(),
            "{linked}: removed"
        );
        assert_eq!(tree(outside.path()), before, "{linked}: followed");
        assert!(dir.path().join(PLAN).exists(), "{linked}: nothing goes");
    }
}

#[test]
fn symlinked_feature_directory_is_refused() {
    let dir = project();
    let outside = project();
    let index = touch(outside.path(), "index.md");
    fs::create_dir_all(dir.path().join("doc/product")).unwrap();
    symlink(outside.path(), dir.path().join("doc/product/demo-work")).unwrap();

    assert!(written(dir.path(), FEATURE).is_err());
    assert!(remove_written(dir.path(), FEATURE).is_err());
    assert!(index.exists());
}

#[test]
fn symlinked_document_is_unlinked_as_a_link() {
    let dir = project();
    let outside = project();
    let keep = touch(outside.path(), "keep.md");
    fs::create_dir_all(dir.path().join("doc/plans")).unwrap();
    symlink(&keep, dir.path().join(PLAN)).unwrap();

    assert_eq!(remove_written(dir.path(), PLAN).unwrap(), [PLAN]);
    assert!(keep.exists());
    assert!(fs::symlink_metadata(dir.path().join(PLAN)).is_err());
}
