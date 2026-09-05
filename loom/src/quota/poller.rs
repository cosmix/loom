//! Daemon-side background thread that keeps the on-disk quota cache warm.
//!
//! One thread polls both providers on independent schedules: each starts at
//! `POLL_INTERVAL` and backs off (doubling, capped at `MAX_BACKOFF`) on
//! consecutive failures, resetting to `POLL_INTERVAL` on success. A 429
//! from Claude uses the server's own `Retry-After` instead of the doubling
//! backoff. Nothing here is persisted across a daemon restart.

use super::cache;
use super::claude::{self, RateLimited};
use super::codex;
use super::credentials;
use crate::codex::find_codex_path;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_secs(180);
const MAX_BACKOFF: Duration = Duration::from_secs(900);
const RATE_LIMIT_MIN_BACKOFF: Duration = Duration::from_secs(300);
const CODEX_DEADLINE: Duration = Duration::from_secs(15);
const SLEEP_SLICE: Duration = Duration::from_millis(250);

/// Spawn the quota-polling thread.
///
/// `work_root` is the `.loom/work` directory (the same path
/// `collect_status_data` resolves a `WorkDir` from); results are written
/// under `work_root/quota/`. The thread notices `shutdown` within one
/// `SLEEP_SLICE` between polls; a Claude request already in flight can
/// hold it for up to the HTTP request timeout (10s), a codex exchange for up
/// to `RECV_SLICE` plus the 2-second child grace, after which the daemon's
/// 5-second join abandons the thread and exits anyway.
pub fn spawn_quota_poller(work_root: PathBuf, shutdown: Arc<AtomicBool>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("quota-poller".to_string())
        .spawn(move || run_quota_poller(&work_root, &shutdown))
        .expect("failed to spawn quota-poller thread")
}

/// Per-provider scheduling and backoff state. Lives only in the poller
/// thread's stack, so a daemon restart starts every provider's backoff over
/// from `POLL_INTERVAL`.
struct ProviderState {
    next_due: Instant,
    interval: Duration,
    last_error: Option<String>,
}

impl ProviderState {
    fn new() -> Self {
        Self {
            next_due: Instant::now(),
            interval: POLL_INTERVAL,
            last_error: None,
        }
    }
}

fn run_quota_poller(work_root: &Path, shutdown: &AtomicBool) {
    let mut claude_state = ProviderState::new();
    let mut codex_state = ProviderState::new();

    while !shutdown.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now >= claude_state.next_due {
            poll_claude(work_root, &mut claude_state);
        }
        if now >= codex_state.next_due {
            poll_codex(work_root, &mut codex_state, shutdown);
        }
        thread::sleep(SLEEP_SLICE);
    }
}

fn epoch_now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The next poll interval given the current one and whether this poll
/// failed: unchanged on success (resets to `POLL_INTERVAL`), doubled and
/// capped at `MAX_BACKOFF` on failure.
fn next_interval(current: Duration, failed: bool) -> Duration {
    if !failed {
        return POLL_INTERVAL;
    }
    current.saturating_mul(2).min(MAX_BACKOFF)
}

/// The backoff to apply after a 429: the server's own `Retry-After` when it
/// gave one, floored at [`RATE_LIMIT_MIN_BACKOFF`] either way.
fn rate_limit_backoff(retry_after_secs: Option<u64>) -> Duration {
    retry_after_secs
        .map(Duration::from_secs)
        .unwrap_or(RATE_LIMIT_MIN_BACKOFF)
        .max(RATE_LIMIT_MIN_BACKOFF)
}

fn record_success(provider: &str, state: &mut ProviderState) {
    if state.last_error.is_some() {
        eprintln!("quota: {provider}: recovered");
    }
    state.last_error = None;
    state.interval = next_interval(state.interval, false);
    state.next_due = Instant::now() + state.interval;
}

fn record_error(provider: &str, state: &mut ProviderState, message: String, backoff: Duration) {
    if state.last_error.as_deref() != Some(message.as_str()) {
        eprintln!("quota: {provider}: {message}");
    }
    state.last_error = Some(message);
    state.interval = backoff;
    state.next_due = Instant::now() + state.interval;
}

fn poll_claude(work_root: &Path, state: &mut ProviderState) {
    let Some(home) = dirs::home_dir() else {
        state.next_due = Instant::now() + state.interval;
        return;
    };
    // No claude.ai login is a common, expected state (e.g. codex-only use) -
    // skip silently rather than treat it as a poll failure worth logging.
    let Ok(token) = credentials::access_token(&home) else {
        state.next_due = Instant::now() + state.interval;
        return;
    };
    let client = match claude::build_client() {
        Ok(client) => client,
        Err(e) => {
            record_error(
                "claude",
                state,
                e.to_string(),
                next_interval(state.interval, true),
            );
            return;
        }
    };

    match claude::fetch(&client, &token, epoch_now()) {
        Ok(quota) => finish_poll("claude", work_root, state, quota),
        Err(e) => {
            if let Some(rate_limited) = e.downcast_ref::<RateLimited>() {
                let backoff = rate_limit_backoff(rate_limited.retry_after_secs);
                let _ = cache::record_failure(work_root, "claude", "rate limited");
                record_error("claude", state, "rate limited".to_string(), backoff);
            } else {
                let message = e.to_string();
                let _ = cache::record_failure(work_root, "claude", &message);
                record_error(
                    "claude",
                    state,
                    message,
                    next_interval(state.interval, true),
                );
            }
        }
    }
}

/// Cache a successful poll and reset the provider's backoff, or - on a cache
/// write failure - treat it as a poll failure so a bad disk state still
/// backs off rather than spinning at `POLL_INTERVAL`.
fn finish_poll(
    provider: &str,
    work_root: &Path,
    state: &mut ProviderState,
    quota: crate::quota::model::ProviderQuota,
) {
    if cache::write_provider(work_root, provider, &quota).is_ok() {
        record_success(provider, state);
    } else {
        record_error(
            provider,
            state,
            "failed to write quota cache".to_string(),
            next_interval(state.interval, true),
        );
    }
}

fn poll_codex(work_root: &Path, state: &mut ProviderState, shutdown: &AtomicBool) {
    let Ok(codex_bin) = find_codex_path() else {
        state.next_due = Instant::now() + state.interval;
        return;
    };

    match codex::poll_once(&codex_bin, CODEX_DEADLINE, shutdown, epoch_now()) {
        Ok(quota) => finish_poll("codex", work_root, state, quota),
        Err(e) => {
            let message = e.to_string();
            let _ = cache::record_failure(work_root, "codex", &message);
            record_error("codex", state, message, next_interval(state.interval, true));
        }
    }
}

#[cfg(test)]
#[path = "poller_tests.rs"]
mod tests;
