//! Claude.ai OAuth token lookup for the quota poller.
//!
//! The token is read into memory here and nowhere else: it is passed to
//! exactly one HTTP request by [`super::claude::fetch`] and must never be
//! logged, cached, or included in an error message.

use anyhow::{anyhow, Result};
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Wall-clock bound on the macOS Keychain lookup.
const KEYCHAIN_DEADLINE: Duration = Duration::from_secs(5);

/// Cached credential files are never trusted past this size.
const MAX_CREDENTIALS_FILE_BYTES: u64 = 64 * 1024;

/// Pure builder for the macOS Keychain lookup argv, asserted directly by the
/// exactness test below so the real command can never drift from what is
/// tested. `-w` is required here (unlike `remote_control::keychain_probe_argv`,
/// which deliberately omits it) because this lookup needs the stored secret,
/// not just proof the entry exists.
pub fn keychain_argv() -> (&'static str, [&'static str; 4]) {
    (
        "security",
        [
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ],
    )
}

/// Resolve a claude.ai OAuth access token.
///
/// On macOS, tries the Keychain first, then falls back (on every OS) to
/// `~/.claude/.credentials.json`. Returns `Err` when neither source yields a
/// usable token - never partial information, and never the token itself in
/// the error.
pub fn access_token(home: &Path) -> Result<String> {
    if cfg!(target_os = "macos") {
        if let Some(token) = keychain_token() {
            return Ok(token);
        }
    }
    file_token(home).ok_or_else(|| anyhow!("no claude.ai login"))
}

fn keychain_token() -> Option<String> {
    let (program, args) = keychain_argv();
    let mut command = Command::new(program);
    command.args(args);
    let output = crate::process::run_bounded(&mut command, KEYCHAIN_DEADLINE)
        .ok()?
        .completed()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    extract_access_token(stdout.trim())
}

fn file_token(home: &Path) -> Option<String> {
    let path = home.join(".claude/.credentials.json");
    let file = std::fs::File::open(path).ok()?;
    let mut body = String::new();
    file.take(MAX_CREDENTIALS_FILE_BYTES)
        .read_to_string(&mut body)
        .ok()?;
    extract_access_token(&body)
}

fn extract_access_token(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let token = value.get("claudeAiOauth")?.get("accessToken")?.as_str()?;
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
