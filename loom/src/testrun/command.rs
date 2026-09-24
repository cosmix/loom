//! Shell command building every adapter shares.
//!
//! - Quoting: [`shell_quote`] and [`shell_word`] turn a test name or file into
//!   one `sh` word; [`shell_words()`] joins several.
//! - Regex filters: [`regex_literal`] makes a runner's regex filter match a
//!   test name literally.
//! - Selection: [`distinct`] and [`distinct_files`] name each target once.
//! - Launchers: [`marker_choice`] and [`package_runner`] pick a command by the
//!   marker files in the package directory.

use std::borrow::Cow;
use std::path::Path;

use super::TestTarget;

/// The regex metacharacters JavaScript, Ruby, PCRE, Go RE2 and CMake share.
const REGEX_SPECIAL: &str = r"\.^$|?*+()[]{}";

/// Lockfiles that mark a Bun package.
const BUN_LOCKFILES: &[&str] = &["bun.lock", "bun.lockb"];

/// Whether `sh` reads `c` literally outside quotes.
fn is_plain(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_./:=@%+,-".contains(c)
}

/// `value` as one single-quoted `sh` word, each `'` written as `'\''`.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// `value` unchanged when it is non-empty and every character is in
/// `[A-Za-z0-9_./:=@%+,-]`, else [`shell_quote`]d.
pub fn shell_word(value: &str) -> Cow<'_, str> {
    if !value.is_empty() && value.chars().all(is_plain) {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(shell_quote(value))
    }
}

/// Each of `values` as a [`shell_word`], separated by spaces.
pub fn shell_words<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    let words: Vec<Cow<'a, str>> = values.into_iter().map(shell_word).collect();
    words.join(" ")
}

/// `value` with `\ . ^ $ | ? * + ( ) [ ] { }` backslash-escaped, so a
/// runner's regex filter matches it literally. Other punctuation stays as
/// written: not every runner's regex dialect accepts an escaped `#`, `-` or `~`.
pub fn regex_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        if REGEX_SPECIAL.contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// The items of `items` without repeats, in first-seen order.
pub fn distinct<T: PartialEq>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut found = Vec::new();
    for item in items {
        if !found.contains(&item) {
            found.push(item);
        }
    }
    found
}

/// Each target's file once, in first-seen order.
pub fn distinct_files(targets: &[TestTarget]) -> Vec<&str> {
    distinct(targets.iter().map(|target| target.file.as_str()))
}

/// `present` when any of `markers` is a file in `package_dir`, else `absent`.
pub fn marker_choice(
    package_dir: &Path,
    markers: &[&str],
    present: &'static str,
    absent: &'static str,
) -> &'static str {
    if markers
        .iter()
        .any(|marker| package_dir.join(marker).is_file())
    {
        present
    } else {
        absent
    }
}

/// The package runner that starts a JavaScript test runner: `bunx` in a Bun
/// package (`bun.lock` or `bun.lockb`), else `npx`.
pub fn package_runner(package_dir: &Path) -> &'static str {
    marker_choice(package_dir, BUN_LOCKFILES, "bunx", "npx")
}

/// A test name that naive quoting breaks: an apostrophe, a command separator
/// and a command substitution.
#[cfg(test)]
pub(crate) const HOSTILE_NAME: &str = "doesn't crash; $(x)";

/// Asserts that `command`, lexed as `sh` splits it, holds `word` as one whole
/// word and neither substitutes a command nor separates one with `;`.
#[cfg(test)]
pub(crate) fn assert_one_word(command: &str, word: &str) {
    use crate::plan::schema::validation::shell_lex::{lex, Token};

    let tokens = lex(command);
    let words: Vec<_> = tokens
        .iter()
        .filter_map(|token| match token {
            Token::Word(lexed) => Some(lexed),
            _ => None,
        })
        .collect();
    assert!(
        words.iter().any(|lexed| lexed.value == word),
        "{command:?} does not hold {word:?} as one word"
    );
    assert!(
        words.iter().all(|lexed| lexed.substitutions.is_empty()),
        "{command:?} substitutes a command"
    );
    assert!(
        !tokens.contains(&Token::Control(";")),
        "{command:?} separates a command"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(file: &str) -> TestTarget {
        TestTarget {
            file: file.to_string(),
            name: None,
        }
    }

    #[test]
    fn shell_quote_escapes_apostrophes() {
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_one_word(&format!("echo {}", shell_quote(HOSTILE_NAME)), HOSTILE_NAME);
    }

    #[test]
    fn shell_word_quotes_only_when_needed() {
        for plain in ["a::b", "src/a_test.go", "-Dx=1", "a@b%c+d,e"] {
            assert!(matches!(shell_word(plain), Cow::Borrowed(_)), "{plain}");
        }
        for special in ["", "a b", "~x", "#x", "a*", "$x", "a'b"] {
            assert_eq!(shell_word(special), shell_quote(special), "{special:?}");
        }
        assert_eq!(shell_words(["a.rs", "b c.rs"]), "a.rs 'b c.rs'");
        assert_one_word(
            &format!("cargo test {}", shell_word(HOSTILE_NAME)),
            HOSTILE_NAME,
        );
    }

    #[test]
    fn regex_literal_escapes_the_shared_metacharacters() {
        assert_eq!(
            regex_literal(r"a.b*c+d?e(f)[g]{h}|i^j$k\l"),
            r"a\.b\*c\+d\?e\(f\)\[g\]\{h\}\|i\^j\$k\\l"
        );
        assert_eq!(regex_literal("adds #1 - ~/x & y"), "adds #1 - ~/x & y");
    }

    #[test]
    fn distinct_files_keep_first_seen_order() {
        let targets = [target("b.rs"), target("a.rs"), target("b.rs")];
        assert_eq!(distinct_files(&targets), ["b.rs", "a.rs"]);
        assert_eq!(distinct([2, 1, 2, 3, 1]), [2, 1, 3]);
    }

    #[test]
    fn marker_choice_checks_each_marker_file() {
        let dir = tempfile::tempdir().expect("temporary package directory");
        let choose = || marker_choice(dir.path(), &["a.lock", "b.lock"], "yes", "no");
        assert_eq!(choose(), "no");
        std::fs::write(dir.path().join("b.lock"), "").expect("marker file");
        assert_eq!(choose(), "yes");
        assert_eq!(package_runner(dir.path()), "npx");
        std::fs::write(dir.path().join("bun.lockb"), "").expect("bun lockfile");
        assert_eq!(package_runner(dir.path()), "bunx");
    }
}
