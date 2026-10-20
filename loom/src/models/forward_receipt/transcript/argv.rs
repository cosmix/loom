//! Whether a transcript's Bash command is exactly loom's forwarding wrapper
//! call, and the model and effort it names.

use std::path::Path;

/// The wrapper's default spelling, the one the shipped agent definitions use.
const DEFAULT_FORWARD_WRAPPER: &str = "~/.claude/hooks/loom/codex-forward.sh";

/// `exact_forward_argv_in` against the installed hooks directory.
pub fn exact_forward_argv(command: &str) -> Option<(String, String)> {
    let hooks_dir = crate::assets::install::claude_hooks_dir();
    exact_forward_argv_in(command, hooks_dir.as_deref())
}

/// The `(model, effort)` of `command` when it is exactly
/// `<wrapper> task <prompt> --model <m> --effort <e> --write`, the wrapper
/// named as [`is_forward_wrapper`] accepts it; `None` for anything else, and
/// for every command when no hooks directory resolves.
pub(super) fn exact_forward_argv_in(
    command: &str,
    hooks_dir: Option<&Path>,
) -> Option<(String, String)> {
    if !safe_shell_syntax(command) {
        return None;
    }
    let words = shell_words::split(command).ok()?;
    if words.len() != 8
        || !is_forward_wrapper(command, &words[0], hooks_dir?)
        || words[1] != "task"
        || words[2].is_empty()
        || words[3] != "--model"
        || words[5] != "--effort"
        || words[7] != "--write"
        || !bounded_diagnostic(&words[4])
        || !bounded_diagnostic(&words[6])
    {
        return None;
    }
    Some((words[4].clone(), words[6].clone()))
}

/// Whether `program`, word 0 of `command`, names loom's forwarding wrapper
/// exactly: the absolute `<hooks_dir>/codex-forward.sh`, or the
/// `~/.claude/hooks/loom/codex-forward.sh` spelling when `command` wrote that
/// word bare. bash tilde-expands only an unquoted, unescaped leading `~`: a
/// quoted or escaped one runs `./~/.claude/...` from the session's writable
/// working directory.
/// A `$` anywhere in the word is refused, quoted or not, as the shell guard
/// refuses it: a literal `$HOME/...` never names the wrapper. No other path
/// ending in `codex-forward.sh` counts.
fn is_forward_wrapper(command: &str, program: &str, hooks_dir: &Path) -> bool {
    if program.contains('$') {
        return false;
    }
    if hooks_dir.join("codex-forward.sh").to_str() == Some(program) {
        return true;
    }
    // `safe_shell_syntax` refuses every unquoted blank but a space, so the
    // first space-separated run is word 0 as written.
    let written = command.trim_start_matches(' ').split(' ').next();
    program == DEFAULT_FORWARD_WRAPPER && written == Some(DEFAULT_FORWARD_WRAPPER)
}

fn safe_shell_syntax(input: &str) -> bool {
    #[derive(Clone, Copy)]
    enum State {
        Plain,
        Single,
        Double,
        Escape,
        DoubleEscape,
    }
    let mut state = State::Plain;
    for character in input.chars() {
        state = match (state, character) {
            (State::Plain, '\'') => State::Single,
            (State::Plain, '"') => State::Double,
            (State::Plain, '\\') => State::Escape,
            (State::Plain, c) if "\n\r\t\u{b}\u{c};|&<>`$()#*?[]{}".contains(c) => return false,
            (State::Plain, _) => State::Plain,
            (State::Single, '\'') => State::Plain,
            (State::Single, _) => State::Single,
            (State::Double, '"') => State::Plain,
            (State::Double, '\\') => State::DoubleEscape,
            (State::Double, c) if "\n\r\t\u{b}\u{c}$`".contains(c) => return false,
            (State::Double, _) => State::Double,
            (State::Escape, c) if !c.is_control() => State::Plain,
            (State::DoubleEscape, '"' | '\\') => State::Double,
            _ => return false,
        };
    }
    matches!(state, State::Plain)
}

fn bounded_diagnostic(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
#[path = "argv_tests.rs"]
mod tests;
