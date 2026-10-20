use std::path::Path;

use super::*;

const DEFAULT_HOOKS: &str = "/home/op/.claude/hooks/loom";
const ARGS: &str = "task 'work' --model gpt-5.6-terra --effort xhigh --write";

fn accepted() -> Option<(String, String)> {
    Some(("gpt-5.6-terra".to_owned(), "xhigh".to_owned()))
}

/// `program` followed by [`ARGS`], parsed against `hooks_dir`.
fn argv(program: &str, hooks_dir: &str) -> Option<(String, String)> {
    let command = format!("{program} {ARGS}");
    exact_forward_argv_in(&command, Some(Path::new(hooks_dir)))
}

#[test]
fn production_argv_with_quoted_metacharacters_is_accepted() {
    let command = r#"~/.claude/hooks/loom/codex-forward.sh task 'prompt containing '\'' and
$ ( *' --model gpt-5.6-terra --effort xhigh --write"#;

    let parsed = exact_forward_argv_in(command, Some(Path::new(DEFAULT_HOOKS)));
    assert_eq!(parsed, accepted());
}

#[test]
fn the_bare_tilde_and_the_absolute_path_are_accepted() {
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

/// The absolute path needs no shell expansion, so any quoting names the
/// wrapper; no other path ending in `codex-forward.sh` does.
#[test]
fn the_absolute_path_is_accepted_in_any_quoting_and_nothing_else_is() {
    let absolute = format!("{DEFAULT_HOOKS}/codex-forward.sh");
    for spelling in [
        absolute.clone(),
        format!("'{absolute}'"),
        format!("\"{absolute}\""),
        "'/home/op/.claude/hooks'/loom/codex-forward.sh".to_string(),
    ] {
        assert_eq!(argv(&spelling, DEFAULT_HOOKS), accepted(), "{spelling}");
    }
    for foreign in [
        "/tmp/codex-forward.sh",
        "/home/op/.claude/hooks/loom/../codex-forward.sh",
        "/home/op/.claude/hooks/loom/x/codex-forward.sh",
        "/opt/agents/claude/hooks/loom/codex-forward.sh",
        "codex-forward.sh",
    ] {
        assert_eq!(argv(foreign, DEFAULT_HOOKS), None, "{foreign}");
    }
}

#[test]
fn no_resolved_hooks_dir_accepts_nothing() {
    let command = format!("{DEFAULT_FORWARD_WRAPPER} {ARGS}");
    assert_eq!(exact_forward_argv_in(&command, None), None);
}

#[test]
fn forwarding_argv_with_redirection_is_rejected() {
    let command = format!("{DEFAULT_FORWARD_WRAPPER} {ARGS} 2>&1");
    let parsed = exact_forward_argv_in(&command, Some(Path::new(DEFAULT_HOOKS)));
    assert_eq!(parsed, None);
}

#[test]
fn forwarding_argv_with_double_quoted_variable_is_rejected() {
    let command = "~/.claude/hooks/loom/codex-forward.sh task \"$var\" --model gpt-5.6-terra --effort xhigh --write";
    let parsed = exact_forward_argv_in(command, Some(Path::new(DEFAULT_HOOKS)));
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
