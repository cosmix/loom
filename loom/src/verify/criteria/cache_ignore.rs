//! Detection of git-ignored paths referenced by an acceptance command.
//!
//! A command whose text names a path that `.gitignore` excludes — most
//! commonly a build artifact like `./target/debug/loom` — can pass today
//! and fail tomorrow with no change to the tracked tree: `cargo clean`, a
//! fresh worktree, or a cache-miss rebuild can remove the artifact without
//! moving `compute_cache_key`'s digest, since that digest is built entirely
//! from the tracked tree and `git status`'s view of it, which by definition
//! excludes ignored paths. [`references_ignored_path`] is the guard
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
/// `git` at all, so it is never rejected here on account of running outside
/// a repository — that case is instead caught downstream, when
/// `compute_cache_key` itself returns `None`.
pub(super) fn references_ignored_path(command: &str, acceptance_dir: &Path) -> bool {
    let tokens = path_like_tokens(command);
    if tokens.is_empty() {
        return false;
    }
    check_ignore(&tokens, acceptance_dir).unwrap_or(true)
}

/// Extract every whitespace-separated token of `command` that looks like a
/// filesystem path: contains a `/` once surrounding quotes and a leading
/// `./` are stripped. Flags (`-x`), URLs (`http://`, `https://`), and
/// anything naming a shell variable (`$`) are never candidates — the first
/// two are not paths, and the third cannot be resolved without a shell.
fn path_like_tokens(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .filter_map(|token| {
            let token = strip_quotes(token);
            if token.starts_with('-')
                || token.starts_with("http://")
                || token.starts_with("https://")
                || token.contains('$')
            {
                return None;
            }
            let token = token.strip_prefix("./").unwrap_or(token);
            token.contains('/').then(|| token.to_string())
        })
        .collect()
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
        .map(|mut stdin| stdin.write_all(&payload).is_ok())
        .unwrap_or(false);

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
        assert!(path_like_tokens("cargo test --lib").is_empty());
        assert!(path_like_tokens("echo $H/.loom/x").is_empty());
        assert!(path_like_tokens("curl https://example.com/a").is_empty());
    }

    #[test]
    fn extracts_a_relative_path_token() {
        assert_eq!(
            path_like_tokens("./target/debug/loom --version"),
            vec!["target/debug/loom".to_string()]
        );
    }

    #[test]
    fn extracts_a_quoted_path_token() {
        assert_eq!(
            path_like_tokens(r#"cat "src/main.rs""#),
            vec!["src/main.rs".to_string()]
        );
    }
}
