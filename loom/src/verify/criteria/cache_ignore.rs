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

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

/// Timeout for the single batched `git check-ignore` call. Mirrors
/// `GIT_READ_TIMEOUT` in `crate::git::runner`, which is not itself `pub`.
const CHECK_IGNORE_TIMEOUT: Duration = Duration::from_secs(15);

/// True when `command` names at least one path-like token that `git`
/// considers ignored under `acceptance_dir`, or the check could not be
/// completed at all (no repository, `git` missing from `PATH`, a spawn or
/// wait failure). Both cases mean "do not cache": an ignored path sits
/// outside the tree digest the cache key is built from, and a failed check
/// cannot rule out that the command depends on one.
///
/// A command with no path-like tokens (e.g. `cargo test`) never invokes
/// `git` at all. Repository and digest capability are checked separately by
/// the input-fingerprint path.
pub(super) fn references_ignored_path(command: &str, acceptance_dir: &Path) -> bool {
    let tokens = path_like_tokens(command, acceptance_dir);
    if tokens.is_empty() {
        return false;
    }
    check_ignore(&tokens, acceptance_dir).unwrap_or(true)
}

/// Extract every whitespace-separated token of `command` that looks like a
/// filesystem path. Existing bare names and names containing a dot are
/// included so `cat ignored.txt` cannot evade the ignored-input guard.
fn path_like_tokens(command: &str, acceptance_dir: &Path) -> Vec<String> {
    command
        .split_whitespace()
        .filter_map(|token| {
            let token = token.trim_matches(is_shell_punctuation);
            let token = strip_quotes(token).trim_matches(is_shell_punctuation);
            if token.starts_with('-')
                || token.starts_with("http://")
                || token.starts_with("https://")
                || token.contains('$')
                || token.is_empty()
            {
                return None;
            }
            let token = token.strip_prefix("./").unwrap_or(token);
            let looks_like_path =
                token.contains('/') || token.contains('.') || acceptance_dir.join(token).exists();
            looks_like_path.then(|| token.to_string())
        })
        .collect()
}

fn is_shell_punctuation(value: char) -> bool {
    matches!(value, ';' | '|' | '&' | '(' | ')' | '<' | '>')
}

/// Strip one layer of matching leading/trailing quotes (`'...'` or
/// `"..."`), if present.
fn strip_quotes(token: &str) -> &str {
    let bytes = token.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if first == last && (first == b'\'' || first == b'"') {
            return &token[1..token.len() - 1];
        }
    }
    token
}

/// Run `git -C <acceptance_dir> check-ignore -q --stdin -z` once, feeding
/// every candidate as a NUL-separated line on stdin. `Ok(true)` means at
/// least one candidate is ignored (`-q` exits 0), `Ok(false)` means none are
/// (exit 1); any other outcome — spawn failure, a write or wait error, exit
/// 128 (no repository, bad arguments) — is `None`.
fn check_ignore(candidates: &[String], acceptance_dir: &Path) -> Option<bool> {
    let mut payload = Vec::new();
    for candidate in candidates {
        payload.extend_from_slice(candidate.as_bytes());
        payload.push(0);
    }

    let mut child = Command::new("git")
        .arg("-C")
        .arg(acceptance_dir)
        .args(["check-ignore", "-q", "--stdin", "-z"])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Write, then drop the handle to close the write end of the pipe:
    // `check-ignore --stdin` reads until EOF before it can decide whether
    // any candidate is ignored.
    let wrote = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(&payload).is_ok());

    let status = match child.wait_timeout(CHECK_IGNORE_TIMEOUT) {
        Ok(Some(status)) => Some(status),
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
        Err(_) => None,
    };

    if !wrote {
        return None;
    }
    match status.and_then(|s| s.code()) {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_tokens_that_are_not_path_like() {
        let temp = tempfile::tempdir().unwrap();
        assert!(path_like_tokens("cargo test --lib", temp.path()).is_empty());
        assert!(path_like_tokens("echo $H/.loom/x", temp.path()).is_empty());
        assert!(path_like_tokens("curl https://example.com/a", temp.path()).is_empty());
    }

    #[test]
    fn extracts_a_relative_path_token() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            path_like_tokens("./target/debug/loom --version", temp.path()),
            vec!["target/debug/loom".to_string()]
        );
    }

    #[test]
    fn extracts_a_quoted_path_token() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            path_like_tokens(r#"cat "src/main.rs""#, temp.path()),
            vec!["src/main.rs".to_string()]
        );
    }
}
