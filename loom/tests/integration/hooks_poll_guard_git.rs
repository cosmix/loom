//! Pathless-vs-path-scoped `git show`/`git diff` cases (rule 4) for `hooks_poll_guard.rs`.
//!
//! Split out purely for size: sharing the parent's harness (hook installation, `Session`,
//! `run_bash_hook`, `warn_context`) via `use super::*` - read the parent's module docs first.

use super::*;

// 6. Pathless `git show`/`git diff` warns to run --stat first; the same commands with a path
//    (`--`, a bare path argument anywhere - `git diff HEAD src/main.rs`, `git diff --cached
//    loom/src/lib.rs` - or `--stat`) do not warn.
#[test]
fn pathless_git_show_diff_warns_scoped_forms_do_not() {
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new();

    for cmd in ["git show abc123", "git diff HEAD~1..HEAD"] {
        let out = run_bash_hook(&hook, cmd, &session, None);
        assert_eq!(out.code, 0, "{cmd}: stderr={}", out.stderr);
        assert!(
            warn_context(&out.stdout).contains("--stat first"),
            "{cmd}: {}",
            out.stdout
        );
    }

    for cmd in [
        "git show abc123 -- path/to/file",
        "git diff --stat HEAD~1..HEAD",
        "git diff src/main.rs",
        "git diff HEAD src/main.rs",
        "git diff --cached loom/src/lib.rs",
    ] {
        let out = run_bash_hook(&hook, cmd, &session, None);
        assert_eq!(out.code, 0, "{cmd}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "{cmd} must not warn: {}",
            out.stdout
        );
    }
}
