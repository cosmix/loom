//! A path as one shell word, for the commands a refusal prints.

use crate::context::untrusted::flatten_char;
use crate::testrun::command::shell_quote;

/// `path` as one shell word that reads back as exactly `path`, on a single
/// line. A name whose characters all print as themselves is single-quoted for
/// any POSIX shell. One with a character that does not is written in ANSI-C
/// `$'...'` quoting, which bash and zsh read: `\n`, `\t`, `\\` and `\'` by name,
/// `!` and every such character as the `\xHH` of each of its UTF-8 bytes,
/// everything else as it is. A single-quoted newline would start a new log
/// line, and `\n` inside single quotes would stay a backslash and an `n`.
pub(super) fn shell_path(path: &str) -> String {
    if path.chars().any(is_hazard) {
        ansi_c_quote(path)
    } else {
        shell_quote(path)
    }
}

/// Whether `ch` would print as something other than itself: a control
/// character (newline, tab, DEL, C1), or one `flatten_char` replaces, which is
/// the repo's list of what a log or terminal shows as something else (a
/// bidirectional override that reverses the rest of the line, a zero-width or
/// other format character, a non-ASCII space, U+2028 and U+2029). Such a name
/// would look like its ASCII sibling in the refusal. A control character is
/// checked on its own as well: the one-line guarantee must not depend on
/// `flatten_char`'s policy.
fn is_hazard(ch: char) -> bool {
    ch.is_control() || flatten_char(ch) != ch
}

fn ansi_c_quote(path: &str) -> String {
    let mut quoted = String::with_capacity(path.len() + 3);
    quoted.push_str("$'");
    for ch in path.chars() {
        match ch {
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            '\\' => quoted.push_str("\\\\"),
            '\'' => quoted.push_str("\\'"),
            // An interactive shell's history lexer would otherwise have to
            // parse the `$'...'` to know whether this `!` is quoted.
            '!' => quoted.push_str("\\x21"),
            ch if is_hazard(ch) => {
                let mut utf8 = [0u8; 4];
                for byte in ch.encode_utf8(&mut utf8).bytes() {
                    quoted.push_str(&format!("\\x{byte:02x}"));
                }
            }
            ch => quoted.push(ch),
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::shell_path;
    use crate::process::sandbox_probe::skip_unless;
    use std::process::Command;

    #[test]
    fn a_plain_name_is_single_quoted() {
        assert_eq!(shell_path("doc/specs/a.md"), "'doc/specs/a.md'");
    }

    #[test]
    fn single_quotes_are_closed_escaped_and_reopened() {
        assert_eq!(shell_path("doc/it's.md"), r"'doc/it'\''s.md'");
    }

    #[test]
    fn shell_metacharacters_stay_inside_single_quotes() {
        assert_eq!(
            shell_path("doc/$(touch x) `y` !z.md"),
            "'doc/$(touch x) `y` !z.md'"
        );
    }

    #[test]
    fn a_control_character_switches_to_ansi_c_quoting() {
        assert_eq!(shell_path("a\nb\tc"), r"$'a\nb\tc'");
        assert_eq!(shell_path("a\u{7f}b\rc\u{1b}"), r"$'a\x7fb\x0dc\x1b'");
    }

    #[test]
    fn ansi_c_quoting_escapes_backslash_and_quote() {
        assert_eq!(shell_path("it's\\\n"), r"$'it\'s\\\n'");
    }

    #[test]
    fn ansi_c_quoting_writes_a_bang_as_hex() {
        assert_eq!(shell_path("a!b\n"), r"$'a\x21b\n'");
    }

    #[test]
    fn a_c1_control_is_written_as_its_utf8_bytes() {
        assert_eq!(shell_path("a\u{85}b"), r"$'a\xc2\x85b'");
    }

    #[test]
    fn a_character_that_displays_as_something_else_is_written_as_its_utf8_bytes() {
        assert_eq!(shell_path("\u{202e}x"), r"$'\xe2\x80\xaex'");
        assert_eq!(shell_path("a\u{a0}b"), r"$'a\xc2\xa0b'");
        assert_eq!(shell_path("\u{200b}"), r"$'\xe2\x80\x8b'");
        assert_eq!(shell_path("\u{2028}"), r"$'\xe2\x80\xa8'");
        assert_eq!(shell_path("\u{feff}a"), r"$'\xef\xbb\xbfa'");
        assert_eq!(shell_path("a\u{ad}"), r"$'a\xc2\xad'");
    }

    #[test]
    fn combining_marks_and_other_non_ascii_stay_literal() {
        assert_eq!(shell_path("cafe\u{301}.md"), "'cafe\u{301}.md'");
        assert_eq!(shell_path("\u{e9}\n"), "$'\u{e9}\\n'");
    }

    /// What `shell` prints for `printf %s <word>`.
    fn printed_by(shell: &str, flags: &[&str], word: &str) -> Vec<u8> {
        let output = Command::new(shell)
            .args(flags)
            .arg(format!("printf %s {word}"))
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{shell} {word}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    #[test]
    fn the_word_reads_back_as_the_name_in_bash_and_zsh() {
        let names = [
            "doc/specs/a.md",
            "a\\\n",
            "a\tb",
            "a\r",
            "it's\n",
            "\u{7f}",
            "\u{1b}",
            "\u{85}",
            "\u{202e}x",
            "a\u{a0}b",
            "\u{200b}",
            "\u{2028}",
            "x$",
            "$'x",
            "-n",
            "a!b",
            "a!b\n",
            "$(touch x) `y` \"z\"",
            "cafe\u{301}.md",
        ];
        for name in names {
            let word = shell_path(name);
            assert_eq!(
                printed_by("bash", &["-c"], &word),
                name.as_bytes(),
                "{word}"
            );
        }
        let zsh_runs = Command::new("zsh").arg("--version").output().is_ok();
        if skip_unless(
            zsh_runs,
            "git::stage_commit::checks::guarded::quote::tests::the_word_reads_back_as_the_name_in_bash_and_zsh",
            "zsh is not on PATH, so its round trips did not run",
        ) {
            return;
        }
        for name in names {
            let word = shell_path(name);
            let printed = printed_by("zsh", &["-f", "-c"], &word);
            assert_eq!(printed, name.as_bytes(), "zsh: {word}");
        }
    }
}
