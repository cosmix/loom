//! The two on-disk daemon credentials, and how they are read and compared.
//!
//! Split from `client.rs` so that connection handling and credential storage
//! are separately readable: everything here is about the FILES, and nothing
//! here decides what a credential entitles a caller to. That decision is
//! `client::authorize_preface` and `self_service`.

use super::storage::{publish_private_file, publish_search_exclusions};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Filename for the user-tier token (mode 0o600, lives under `.loom/work/`).
pub(super) const USER_TOKEN_FILE: &str = "user.token";

/// Filename for the admin token (mode 0o600). Lives under the per-project
/// `.loom/work/` directory alongside `user.token`. It is owner-only so a
/// stage-confined agent cannot read it, and being per-project means
/// concurrent daemons for different projects never share — let alone
/// clobber or delete — each other's token.
pub(super) const ADMIN_TOKEN_FILE: &str = "admin.token";

/// Path to the per-project admin token: `<work_dir>/admin.token`.
///
/// Mode 0o600 (owner-only rw). Kept per-project rather than in a shared
/// runtime directory so two daemons (different projects, or a restart)
/// can never overwrite or delete one another's token.
pub fn admin_token_path(work_dir: &Path) -> PathBuf {
    work_dir.join(ADMIN_TOKEN_FILE)
}

/// Generate a 64-character hex token from 32 cryptographically-strong bytes.
///
/// Uses `OsRng` (getrandom on Linux, SecRandomCopyBytes on macOS) instead of
/// `Uuid::new_v4` so token entropy is the full 256 bits the format implies.
fn generate_token_hex() -> Result<String> {
    let mut bytes = [0u8; 32];
    let mut f = fs::File::open("/dev/urandom").context("Failed to open /dev/urandom")?;
    use std::io::Read;
    f.read_exact(&mut bytes)
        .context("Failed to read 32 random bytes")?;
    let mut s = String::with_capacity(64);
    for b in &bytes {
        s.push_str(&format!("{b:02x}"));
    }
    Ok(s)
}

/// Publish the search-exclusion files, then fresh admin and user tokens.
///
/// - admin.token (mode 0o600): required for privileged ops (Stop and the
///   verification-bypass flags `--no-verify`, `--force-unsafe`,
///   `--assume-merged`). Owner-only so a stage-confined agent cannot
///   read it.
/// - user.token  (mode 0o600): used for Ping / Subscribe / Unsubscribe /
///   DisputeCriteria. Owner-only so another local user cannot read it
///   and exercise User-capability RPCs (S-8a).
///
/// Both are 32-byte / 256-bit random hex from /dev/urandom
/// (OsRng-equivalent). The exclusion files are published first so there is
/// never a window where a token exists without them, keeping a sandboxed
/// agent's `rg`/`fd`/`ag` from opening either token file and tripping the
/// sandbox's own deny rule, which would otherwise stall auto mode on an
/// operator prompt.
pub(super) fn publish_fresh_tokens(work_dir: &Path) -> Result<()> {
    publish_search_exclusions(work_dir).context("Failed to publish search-exclusion files")?;
    let admin_token = generate_token_hex()?;
    let user_token = generate_token_hex()?;
    publish_private_file(
        work_dir,
        Path::new(ADMIN_TOKEN_FILE),
        admin_token.as_bytes(),
    )
    .context("Failed to publish admin token file")?;
    publish_private_file(work_dir, Path::new(USER_TOKEN_FILE), user_token.as_bytes())
        .context("Failed to publish user token file")?;
    Ok(())
}

fn read_token_file(work_dir: &Path, relative: &Path) -> Option<String> {
    crate::fs::safe_read::read_to_string_bounded(work_dir, relative, 4096)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Read the user-tier auth token (Ping / Subscribe / Unsubscribe).
pub fn read_user_token(work_dir: &Path) -> Option<String> {
    read_token_file(work_dir, Path::new(USER_TOKEN_FILE))
}

/// Back-compat shim used by status UI helpers — returns the user token.
///
/// Kept on the public surface because TUI code reads it for `Ping` /
/// `SubscribeStatus`. Never use this for `Stop`; that path must call
/// the admin-proof verifier.
pub fn read_auth_token(work_dir: &Path) -> Option<String> {
    read_user_token(work_dir)
}

/// Constant-time comparison of two strings.
fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.as_bytes()
            .iter()
            .zip(b.as_bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

pub(super) fn verify_user_token(work_dir: &Path, provided_token: &str) -> bool {
    let Some(expected) = read_user_token(work_dir) else {
        return false;
    };
    ct_eq(&expected, provided_token)
}
