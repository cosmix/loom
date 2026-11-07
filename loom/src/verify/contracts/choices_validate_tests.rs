//! The rules on one writer value: a test is one identifier to its runner, and
//! a `--cwd` is a real directory in the live worktree.

use super::*;

fn adapter(name: &str) -> Option<&'static dyn TestRunnerAdapter> {
    let adapter = registry::by_name(name);
    assert!(adapter.is_some(), "{name} is an adapter");
    adapter
}

#[test]
fn a_semicolon_in_a_writers_test_is_refused_for_every_adapter() {
    for runner in ["sbt", "vitest", "cargo-test"] {
        let error = check_test("c1", "FooSpec; compile", adapter(runner)).unwrap_err();
        assert!(
            format!("{error:#}").contains("holds ';'"),
            "{runner}: {error:#}"
        );
    }
    assert!(check_test("c1", "FooSpec; compile", None).is_err());
}

#[test]
fn whitespace_is_refused_for_sbt_and_kept_for_full_test_names() {
    let error = check_test("c1", "FooSpec BarSpec", adapter("sbt")).unwrap_err();
    assert!(
        format!("{error:#}").contains("fully qualified suite name"),
        "{error:#}"
    );
    check_test("c1", "com.example.FooSpec", adapter("sbt")).unwrap();
    check_test("c1", "math > adds two numbers", adapter("vitest")).unwrap();
    check_test("c1", "adds two numbers", adapter("jest")).unwrap();
}

#[test]
fn a_cwd_that_is_a_symlink_or_no_directory_in_the_live_worktree_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("pkg/sub")).unwrap();
    std::os::unix::fs::symlink(root.join("pkg"), root.join("link")).unwrap();
    std::fs::write(root.join("file"), "x").unwrap();
    let git = WorktreeGit::discovered(&root);
    let place = Place {
        git: &git,
        root: root.clone(),
        working_dir: &root,
        working: Vec::new(),
        base: "HEAD",
    };
    let dir = |parts: &[&str]| {
        parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
    };

    live_directory(&place, "c1", &dir(&["pkg", "sub"])).unwrap();
    for refused in [&["link"][..], &["link", "sub"], &["file"], &["missing"]] {
        let error = live_directory(&place, "c1", &dir(refused)).unwrap_err();
        assert!(
            format!("{error:#}").contains("is not a directory in the worktree"),
            "{refused:?}: {error:#}"
        );
    }
}
