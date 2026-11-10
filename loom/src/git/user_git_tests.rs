//! `IndexBefore` against real repositories with the user's hooks and unusual index contents.

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use super::*;

/// A repository whose hooks live in `hooks/` beside the work tree files.
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    let hooks = dir.path().join("hooks");
    fs::create_dir(&hooks).unwrap();
    for (key, value) in [
        ("user.name", "Index Tests"),
        ("user.email", "index@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", hooks.to_str().unwrap()),
    ] {
        git(dir.path(), &["config", key, value]);
    }
    dir
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let out = user_git(root).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn hook(root: &Path, name: &str, script: &str) {
    let path = root.join("hooks").join(name);
    fs::write(&path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// What the planner's commit does to `a.md`: read the index, stage, commit `a.md` alone, reset.
fn commit_a(root: &Path) -> IndexReset {
    let index = IndexBefore::read(root).unwrap();
    git(root, &["add", "--", "a.md"]);
    git(
        root,
        &["commit", "-q", "-m", "edit", "--", ":(literal)a.md"],
    );
    index.reset(root, &["a.md"], "HEAD")
}

#[test]
fn a_staged_path_that_is_not_utf8_neither_fails_the_commit_nor_loses_its_staging() {
    for born in [true, false] {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("a.md"), "one\n").unwrap();
        if born {
            git(root, &["add", "--", "a.md"]);
            git(root, &["commit", "-q", "-m", "initial"]);
            fs::write(root.join("a.md"), "two\n").unwrap();
        }
        let odd = OsStr::from_bytes(b"caf\xe9.txt");
        fs::write(root.join(odd), "odd\n").unwrap();
        let mut add = user_git(root);
        add.args(["add", "--"]).arg(odd);
        assert!(add.status().unwrap().success());

        let reset = commit_a(root);

        assert!(reset.failed.is_none(), "{:?}", reset.failed);
        assert!(reset.kept.is_empty());
        let cached = git(root, &["diff", "--cached", "--name-only", "-z"]);
        assert_eq!(
            cached.split(|byte| *byte == 0).next().unwrap(),
            b"caf\xe9.txt"
        );
    }
}

#[test]
fn a_commit_a_hook_makes_on_top_of_the_first_commit_does_not_hide_its_files_from_the_reset() {
    let dir = repo();
    let root = dir.path();
    fs::write(root.join("a.md"), "one\n").unwrap();
    hook(
        root,
        "pre-commit",
        "#!/bin/sh\nprintf 'generated\\n' > G\ngit add G\n",
    );
    hook(
        root,
        "post-commit",
        "#!/bin/sh\n[ -e done ] && exit 0\ntouch done\nprintf 'x\\n' > X\ngit add X\n\
         git commit -q -m second -- X\n",
    );

    let reset = commit_a(root);

    assert!(reset.failed.is_none(), "{:?}", reset.failed);
    assert_eq!(git(root, &["rev-list", "--count", "HEAD"]), b"2\n");
    assert_eq!(git(root, &["show", "HEAD:G"]), b"generated\n");
    assert_eq!(git(root, &["diff", "--cached", "--name-only"]), b"");
}
