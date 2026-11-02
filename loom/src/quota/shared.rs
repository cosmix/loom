//! Machine-wide, per-account Claude quota state: `~/.loom/quota/claude-<key>.json`.
//!
//! Every daemon on a machine polls with the same claude.ai OAuth token, and
//! the usage endpoint rate-limits per token. Daemons sharing a token therefore
//! share one state file, guarded by an exclusive `flock` on a sibling lock
//! file: only the daemon that takes the lock after `next_poll_at` fetches, the
//! rest adopt its result. `<key>` is a truncated SHA-256 of the token, so
//! accounts stay separate and the token itself is never written anywhere.

use super::cache;
use super::model::ProviderQuota;
use super::poller::{next_interval, rate_limit_backoff, POLL_INTERVAL};
use crate::context::untrusted::inline_safe;
use anyhow::{Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Consecutive 429s for one account before `rate limited` is shown.
pub(super) const RATE_LIMIT_CONFIRMATIONS: u32 = 3;

/// State files for keys other than the current one are pruned once this old.
const STALE_KEY_AGE: Duration = Duration::from_secs(24 * 60 * 60);

const RATE_LIMITED: &str = "rate limited";
const MAX_STATE_BYTES: usize = 64 * 1024;

/// One account's shared poll state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct AccountState {
    /// Last successful reading; its `error` is always `None`.
    pub quota: Option<ProviderQuota>,
    /// Epoch seconds; nobody fetches before this.
    pub next_poll_at: i64,
    /// Current backoff interval.
    pub interval_secs: u64,
    pub consecutive_rate_limits: u32,
    /// Most recent attempt's error; `None` after a success.
    pub last_error: Option<String>,
}

impl Default for AccountState {
    fn default() -> Self {
        Self {
            quota: None,
            next_poll_at: 0,
            interval_secs: POLL_INTERVAL.as_secs(),
            consecutive_rate_limits: 0,
            last_error: None,
        }
    }
}

pub(super) fn apply_success(state: AccountState, quota: ProviderQuota, now: i64) -> AccountState {
    let mut quota = quota;
    quota.error = None;
    scheduled(
        AccountState {
            quota: Some(quota),
            consecutive_rate_limits: 0,
            last_error: None,
            ..state
        },
        POLL_INTERVAL,
        now,
    )
}

pub(super) fn apply_rate_limited(
    state: AccountState,
    retry_after_secs: Option<u64>,
    now: i64,
) -> AccountState {
    scheduled(
        AccountState {
            consecutive_rate_limits: state.consecutive_rate_limits.saturating_add(1),
            last_error: Some(RATE_LIMITED.to_string()),
            ..state
        },
        rate_limit_backoff(retry_after_secs),
        now,
    )
}

pub(super) fn apply_error(state: AccountState, message: &str, now: i64) -> AccountState {
    let interval = next_interval(Duration::from_secs(state.interval_secs), true);
    scheduled(
        AccountState {
            consecutive_rate_limits: 0,
            last_error: Some(inline_safe(message)),
            ..state
        },
        interval,
        now,
    )
}

fn scheduled(state: AccountState, interval: Duration, now: i64) -> AccountState {
    AccountState {
        interval_secs: interval.as_secs(),
        next_poll_at: now.saturating_add(interval.as_secs() as i64),
        ..state
    }
}

/// The error to show for this account: a streak of 429s stays hidden until
/// [`RATE_LIMIT_CONFIRMATIONS`] in a row, any other error shows at once.
pub(super) fn display_error(state: &AccountState) -> Option<String> {
    if state.consecutive_rate_limits > 0 {
        return (state.consecutive_rate_limits >= RATE_LIMIT_CONFIRMATIONS)
            .then(|| RATE_LIMITED.to_string());
    }
    state.last_error.clone()
}

/// The shared directory under `home`.
pub(super) fn shared_dir(home: &Path) -> PathBuf {
    home.join(".loom").join("quota")
}

/// First 16 hex chars of the token's SHA-256; never reveals the token.
pub(super) fn token_key(token: &str) -> String {
    hex::encode(&Sha256::digest(token.as_bytes())[..8])
}

/// Take the account's lock, then either adopt the state another daemon left
/// (before `next_poll_at`) or run `fetch` and record its outcome.
pub(super) fn poll_shared(
    dir: &Path,
    key: &str,
    now: i64,
    fetch: impl FnOnce() -> Result<ProviderQuota>,
) -> Result<AccountState> {
    crate::fs::cache_io::create_dir(dir)?;
    let lock = open_lock(&dir.join(format!("claude-{key}.lock")))?;
    lock.lock_exclusive()
        .context("failed to lock quota state")?;

    let path = dir.join(format!("claude-{key}.json"));
    let state = read_state(&path);
    if now < state.next_poll_at {
        return Ok(state);
    }

    let next = match fetch() {
        Ok(quota) => apply_success(state, quota, now),
        Err(e) => match e.downcast_ref::<super::claude::RateLimited>() {
            Some(limited) => apply_rate_limited(state, limited.retry_after_secs, now),
            None => apply_error(state, &e.to_string(), now),
        },
    };
    crate::fs::cache_io::reject_symlink(&path)?;
    let body = serde_json::to_string(&next).context("failed to serialize quota state")?;
    crate::fs::cache_io::atomic_write(&path, &body)?;
    prune_stale(dir, key);
    Ok(next)
}

/// Missing, unreadable, symlinked, or unparsable state is the default (poll now).
fn read_state(path: &Path) -> AccountState {
    // Anchor above the quota directory so a replacement is refused as well. The
    // anchor is `~/.loom`, which the user may reach through a symlinked $HOME or
    // keep behind a symlink, so it is resolved before the no-follow walk.
    let Some(anchor) = path.parent().and_then(Path::parent) else {
        return AccountState::default();
    };
    let Ok(relative) = path.strip_prefix(anchor) else {
        return AccountState::default();
    };
    let Ok(root) = anchor.canonicalize() else {
        return AccountState::default();
    };
    let Ok(body) = crate::fs::safe_read::read_to_string_bounded(&root, relative, MAX_STATE_BYTES)
    else {
        return AccountState::default();
    };
    let mut state: AccountState = serde_json::from_str(&body).unwrap_or_default();
    state.quota = state.quota.map(cache::sanitize);
    state
}

fn open_lock(path: &Path) -> Result<File> {
    crate::fs::cache_io::reject_symlink(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options
        .open(path)
        .with_context(|| format!("failed to open quota lock: {}", path.display()))
}

/// Best effort: token rotation leaves other keys' state behind, so drop
/// `claude-*.json` of other keys untouched for a day. Lock files must keep
/// their inode: a process may still hold or be waiting on an open descriptor.
fn prune_stale(dir: &Path, key: &str) {
    let own = format!("claude-{key}.");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_state =
            name.starts_with("claude-") && name.ends_with(".json") && !name.starts_with(&own);
        let age = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mtime| SystemTime::now().duration_since(mtime).ok());
        if is_state && age.is_some_and(|age| age > STALE_KEY_AGE) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
#[path = "shared_tests.rs"]
mod tests;
