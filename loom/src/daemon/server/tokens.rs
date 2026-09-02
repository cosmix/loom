//! The two on-disk daemon credentials, and how they are read and compared.
//!
//! Split from `client.rs` so that connection handling and credential storage
//! are separately readable: everything here is about the FILES, and nothing
//! here decides what a credential entitles a caller to. That decision is
//! `client::authorize_preface` and `self_service`.

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
