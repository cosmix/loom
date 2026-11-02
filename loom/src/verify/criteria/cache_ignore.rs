//! Detection of git-ignored paths referenced by an acceptance command.
//!
//! A command whose text names a path that `.gitignore` excludes — most
//! commonly a build artifact like `./target/debug/loom` — can pass today
//! and fail tomorrow with no change to the tracked tree: `cargo clean`, a
//! fresh worktree, or a cache-miss rebuild can remove the artifact without
//! moving the input/context digest, since that digest is built from the
//! tracked tree and `git status`'s view of it, which by definition excludes
//! ignored paths. [`references_ignored_path`] is the guard
//! `is_cacheable` applies before ever storing a pass for such a command.

use std::path::Path;

use crate::plan::schema::validation::shell_lex::{lex, Token};
use crate::verify::review::fingerprint::checkout_git;

/// True when `command` names at least one path-like token that `git`
/// considers ignored under `acceptance_dir`, or the check could not be
/// completed at all (no repository, `git` missing from `PATH`, a spawn or
/// wait failure). Both cases mean "do not cache": an ignored path sits
/// outside the tree digest the cache key is built from, and a failed check
/// cannot rule out that the command depends on one.
///
/// Every static bare word is queried, including a path absent at the first
/// run. Repository and digest capability are checked separately by the
/// input-fingerprint path.
pub(super) fn references_ignored_path(command: &str, acceptance_dir: &Path) -> bool {
    let Some(tokens) = path_like_tokens(command) else {
        return true;
    };
    if tokens.is_empty() {
        return false;
    }
    check_ignore(&tokens, acceptance_dir).unwrap_or(true)
}

/// Use the shared shell lexer so quoted and escaped spaces remain one path.
/// Check bare words too: an ignored file need not exist when the command runs.
/// Unknown expansions, heredocs and option-attached paths are uncacheable.
fn path_like_tokens(command: &str) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    for token in lex(command) {
        match token {
            Token::Redirect("<<" | "<<-" | "<<<") => return None,
            Token::Word(word) => {
                if word.expands || !word.substitutions.is_empty() {
                    return None;
                }
                let value = word.value;
                // A glob can read an ignored match omitted from the manifest.
                if value.chars().any(|ch| matches!(ch, '*' | '?' | '[' | ']')) {
                    return None;
                }
                if value.is_empty() || value.starts_with("http://") || value.starts_with("https://")
                {
                    continue;
                }
                if value.starts_with('-') {
                    if value.contains('/')
                        || value.contains('.')
                        || value.contains('=')
                        || (!value.starts_with("--") && value.len() > 2)
                    {
                        return None;
                    }
                    continue;
                }
                let path = value.strip_prefix("./").unwrap_or(&value);
                paths.push(path.to_string());
            }
            _ => {}
        }
    }
    Some(paths)
}

/// Run `git check-ignore` once over every candidate, through the git of the
/// checkout holding `acceptance_dir` ([`checkout_git`]): the host child runs
/// this outside the stage's sandbox, where a `.git` the stage controls would
/// otherwise choose the repository, and with it the exclude rules, that
/// answer. Git runs at the checkout's root, so each candidate is made
/// relative to it. `Some(true)` means at least one candidate is ignored
/// (exit 0), `Some(false)` means none are (exit 1); any other outcome (no
/// checkout, a non-UTF-8 path, a spawn or deadline failure, exit 128 for a
/// path outside the repository) is `None`.
fn check_ignore(candidates: &[String], acceptance_dir: &Path) -> Option<bool> {
    let acceptance_dir = acceptance_dir.canonicalize().ok()?;
    let git = checkout_git(&acceptance_dir).ok()?;
    let prefix = acceptance_dir.strip_prefix(git.work_tree()).ok()?;
    let paths = candidates
        .iter()
        .map(|candidate| prefix.join(candidate).to_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    let mut args = vec!["check-ignore", "--"];
    args.extend(paths.iter().map(String::as_str));
    match git.run(&args).ok()?.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
#[path = "cache_ignore_tests.rs"]
mod checkout_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_static_words_and_rejects_expansion() {
        assert_eq!(
            path_like_tokens("cargo test --lib"),
            Some(vec!["cargo".into(), "test".into()])
        );
        assert!(path_like_tokens("echo $H/.loom/x").is_none());
        assert_eq!(
            path_like_tokens("curl https://example.com/a"),
            Some(vec!["curl".into()])
        );
    }

    #[test]
    fn extracts_a_relative_path_token() {
        assert_eq!(
            path_like_tokens("./target/debug/loom --version"),
            Some(vec!["target/debug/loom".to_string()])
        );
    }

    #[test]
    fn extracts_a_quoted_path_token() {
        assert_eq!(
            path_like_tokens(r#"cat "src/main.rs""#),
            Some(vec!["cat".into(), "src/main.rs".to_string()])
        );
    }

    #[test]
    fn decodes_quoted_and_escaped_paths_and_keeps_operators_distinct() {
        for command in ["cat 'ignored data.txt'", "cat ignored\\ data.txt"] {
            assert_eq!(
                path_like_tokens(command),
                Some(vec!["cat".into(), "ignored data.txt".into()])
            );
        }
        assert_eq!(
            path_like_tokens("printf 'a; b'"),
            Some(vec!["printf".into(), "a; b".into()])
        );
    }
}
