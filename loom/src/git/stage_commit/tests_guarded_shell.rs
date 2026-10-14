//! A guarded-path refusal's commands, followed through a real shell: lifted
//! from the text, checked to spell the path they are for, and run as printed.

use super::*;
use crate::git::signing::tests::isolated_env;

/// What every printed command starts with, up to its source.
const COMMAND: &str = "git --literal-pathspecs restore --source=";

/// What separates a printed command's options from its path word.
const BEFORE_PATH: &str = " -- ";

/// One command lifted from a refusal text.
struct Printed<'a> {
    /// `HEAD`, `MERGE_HEAD` or a tree id.
    source: &'a str,
    /// The path as the shell is meant to read it.
    word: &'a str,
    /// The whole command, through the end of `word`.
    command: &'a str,
}

/// The bytes of a `$'...'` body through its closing quote.
fn ansi_c_len(body: &str) -> usize {
    let mut chars = body.char_indices();
    while let Some((at, ch)) = chars.next() {
        match ch {
            '\\' => {
                chars.next();
            }
            '\'' => return at + 1,
            _ => {}
        }
    }
    panic!("an unterminated $'...' in: {body}");
}

/// The shell word at the start of `text`: single-quoted runs, `$'...'` runs
/// and the `\'` that joins them. The text after it is prose, and must start
/// with whitespace, a `;` or nothing: any other character (`$(`, a quote, a
/// period) would be read by the shell as part of the word or as syntax.
fn word_at(text: &str) -> &str {
    let mut end = 0;
    loop {
        let rest = &text[end..];
        if let Some(body) = rest.strip_prefix("$'") {
            end += 2 + ansi_c_len(body);
        } else if let Some(body) = rest.strip_prefix('\'') {
            end += 1 + body.find('\'').expect(text) + 1;
        } else if rest.starts_with("\\'") {
            end += 2;
        } else {
            let next = rest.chars().next();
            assert!(
                next.is_none_or(|ch| ch.is_whitespace() || ch == ';'),
                "{next:?} abuts the word {:?} in: {text}",
                &text[..end]
            );
            return &text[..end];
        }
    }
}

fn printed_commands(refusal: &str) -> Vec<Printed<'_>> {
    refusal
        .match_indices(COMMAND)
        .map(|(at, _)| {
            let tail = &refusal[at..];
            let source = tail[COMMAND.len()..]
                .split_whitespace()
                .next()
                .expect(refusal);
            let path_at = tail.find(BEFORE_PATH).expect(refusal) + BEFORE_PATH.len();
            let word = word_at(&tail[path_at..]);
            Printed {
                source,
                word,
                command: &tail[..path_at + word.len()],
            }
        })
        .collect()
}

/// Run `script` in `root` as a resolver's shell would. A POSIX `sh` runs what
/// holds no `$'...'`; `bash` runs the rest, because a dash `/bin/sh` reads
/// `$'a'` as a `$` followed by `'a'`.
fn run_shell(root: &Path, script: &str) -> std::process::Output {
    let shell = if script.contains("$'") { "bash" } else { "sh" };
    Command::new(shell)
        .args(["-c", script])
        .current_dir(root)
        .env_clear()
        .envs(isolated_env(root))
        .output()
        .unwrap()
}

/// Run, on `path`, the command the refusal prints for `source`: `HEAD`,
/// `MERGE_HEAD`, or `None` for the merged tree a cleanly merged path is
/// restored from. The command is picked by asking the shell what its path
/// word stands for, and run through a shell as printed.
pub(super) fn follow_printed(root: &Path, refusal: &str, source: Option<&str>, path: &str) {
    let wanted = |printed: &str| match source {
        Some(side) => printed == side,
        None => printed != "HEAD" && printed != "MERGE_HEAD",
    };
    let printed = printed_commands(refusal)
        .into_iter()
        .find(|printed| {
            wanted(printed.source)
                && run_shell(root, &format!("printf %s {}", printed.word)).stdout == path.as_bytes()
        })
        .unwrap_or_else(|| panic!("no command for {path:?} from {source:?} in: {refusal}"));

    let output = run_shell(root, printed.command);

    assert!(
        output.status.success(),
        "{}: {}",
        printed.command,
        String::from_utf8_lossy(&output.stderr)
    );
}
