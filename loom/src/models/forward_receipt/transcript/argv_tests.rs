use std::path::{Path, PathBuf};

use super::*;

const HOME: &str = "/home/op";
const DEFAULT_HOOKS: &str = "/home/op/.claude/hooks/loom";
const RELOCATED_HOOKS: &str = "/opt/agents/claude/hooks/loom";
const ARGS: &str = "task 'work' --model gpt-5.6-terra --effort xhigh --write";

fn accepted() -> Option<(String, String)> {
    Some(("gpt-5.6-terra".to_owned(), "xhigh".to_owned()))
}

/// `program` followed by [`ARGS`], parsed against `hooks_dir` under [`HOME`].
fn argv(program: &str, hooks_dir: &str) -> Option<(String, String)> {
    let command = format!("{program} {ARGS}");
    exact_forward_argv_in(&command, Some(Path::new(hooks_dir)), Some(Path::new(HOME)))
}

#[test]
fn production_argv_with_quoted_metacharacters_is_accepted() {
    let command = r#"~/.claude/hooks/loom/codex-forward.sh task 'prompt containing '\'' and
$ ( *' --model gpt-5.6-terra --effort xhigh --write"#;

    let parsed = exact_forward_argv_in(
        command,
        Some(Path::new(DEFAULT_HOOKS)),
        Some(Path::new(HOME)),
    );
    assert_eq!(parsed, accepted());
}

#[test]
fn the_default_root_accepts_the_bare_tilde_and_its_absolute_path() {
    assert_eq!(argv(DEFAULT_FORWARD_WRAPPER, DEFAULT_HOOKS), accepted());
    assert_eq!(
        argv(&format!("  {DEFAULT_FORWARD_WRAPPER}"), DEFAULT_HOOKS),
        accepted()
    );
    let absolute = format!("{DEFAULT_HOOKS}/codex-forward.sh");
    assert_eq!(argv(&absolute, DEFAULT_HOOKS), accepted());
}

/// bash leaves a quoted or escaped `~` literal and would run
/// `./~/.claude/hooks/loom/codex-forward.sh` from the writable working
/// directory, so none of these spellings is the wrapper.
#[test]
fn a_quoted_or_escaped_tilde_is_not_the_wrapper() {
    for program in [
        "'~/.claude/hooks/loom/codex-forward.sh'",
        "\"~/.claude/hooks/loom/codex-forward.sh\"",
        "\\~/.claude/hooks/loom/codex-forward.sh",
        "'~'/.claude/hooks/loom/codex-forward.sh",
        "~/.claude/hooks/loom/codex-forward\\.sh",
        "~/.claude/hooks/loom/codex-forward.sh''",
        "''~/.claude/hooks/loom/codex-forward.sh",
    ] {
        assert_eq!(argv(program, DEFAULT_HOOKS), None, "{program}");
    }
}

#[test]
fn the_wrapper_in_a_relocated_hooks_dir_is_accepted_only_by_its_absolute_path() {
    let absolute = format!("{RELOCATED_HOOKS}/codex-forward.sh");
    for spelling in [
        absolute.clone(),
        format!("'{absolute}'"),
        format!("\"{absolute}\""),
        "'/opt/agents/claude/hooks'/loom/codex-forward.sh".to_string(),
    ] {
        assert_eq!(argv(&spelling, RELOCATED_HOOKS), accepted(), "{spelling}");
    }
    // The `~` spelling names `<home>/.claude/hooks/loom`, which is not where a
    // relocated root installed the wrapper.
    assert_eq!(argv(DEFAULT_FORWARD_WRAPPER, RELOCATED_HOOKS), None);
    for foreign in [
        "/tmp/codex-forward.sh",
        "/opt/agents/claude/hooks/loom/../codex-forward.sh",
        "/opt/agents/claude/hooks/loom/x/codex-forward.sh",
        "/home/op/.claude/hooks/loom/codex-forward.sh",
        "codex-forward.sh",
    ] {
        assert_eq!(argv(foreign, RELOCATED_HOOKS), None, "{foreign}");
    }
}

#[test]
fn no_resolved_hooks_dir_accepts_nothing() {
    let command = format!("{DEFAULT_FORWARD_WRAPPER} {ARGS}");
    assert_eq!(
        exact_forward_argv_in(&command, None, Some(Path::new(HOME))),
        None
    );
    let parsed = exact_forward_argv_in(&command, Some(Path::new(DEFAULT_HOOKS)), None);
    assert_eq!(parsed, None);
}

/// A home reached through a symlink still names the default root: the
/// comparison is canonical, as `default_paths` resolves the hooks directory.
#[cfg(unix)]
#[test]
fn a_symlinked_home_still_accepts_the_tilde_spelling() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().canonicalize().unwrap().join("real-home");
    let hooks: PathBuf = real.join(".claude/hooks/loom");
    std::fs::create_dir_all(&hooks).unwrap();
    let link = temp.path().join("home-link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let command = format!("{DEFAULT_FORWARD_WRAPPER} {ARGS}");
    assert_eq!(
        exact_forward_argv_in(&command, Some(&hooks), Some(&link)),
        accepted()
    );
}

#[test]
fn forwarding_argv_with_redirection_is_rejected() {
    let command = format!("{DEFAULT_FORWARD_WRAPPER} {ARGS} 2>&1");
    let parsed = exact_forward_argv_in(
        &command,
        Some(Path::new(DEFAULT_HOOKS)),
        Some(Path::new(HOME)),
    );
    assert_eq!(parsed, None);
}

#[test]
fn forwarding_argv_with_double_quoted_variable_is_rejected() {
    let command = "~/.claude/hooks/loom/codex-forward.sh task \"$var\" --model gpt-5.6-terra --effort xhigh --write";
    let parsed = exact_forward_argv_in(
        command,
        Some(Path::new(DEFAULT_HOOKS)),
        Some(Path::new(HOME)),
    );
    assert_eq!(parsed, None);
}

/// The shell guard refuses any word 0 holding a `$`, quoted or not; so does
/// the parser, even where the literal word would equal the wrapper's path.
#[test]
fn a_dollar_in_the_wrapper_word_is_refused_in_every_quoting() {
    for program in [
        "'$HOME/.claude/hooks/loom/codex-forward.sh'",
        "'$HOME'/.claude/hooks/loom/codex-forward.sh",
        "\\$HOME/.claude/hooks/loom/codex-forward.sh",
    ] {
        assert_eq!(argv(program, DEFAULT_HOOKS), None, "{program}");
    }
    // Unquoted and double-quoted `$` fail the shell-syntax check first.
    for program in [
        "$HOME/.claude/hooks/loom/codex-forward.sh",
        "\"$HOME/.claude/hooks/loom/codex-forward.sh\"",
    ] {
        assert_eq!(argv(program, DEFAULT_HOOKS), None, "{program}");
    }
    let dollar_dir = "/opt/a$b/hooks/loom";
    assert_eq!(
        argv("'/opt/a$b/hooks/loom/codex-forward.sh'", dollar_dir),
        None
    );
}
