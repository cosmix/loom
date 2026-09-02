//! Per-thread test redirect for [`super::UserConfig::load`]/
//! [`super::UserConfig::load_strict`]/[`super::set`].
//!
//! Exists only in the lib test binary (this module is `#[cfg(test)]` from
//! `mod.rs`). One thread-local backs both the read side and the write side,
//! so a test that installs [`redirect_user_config`] can do a real set/read
//! round trip against a temp file without ever touching the operator's
//! `~/.loom/config.toml`. The Rust test harness runs each test on its own
//! thread, so the redirect is hermetic (a test that installs none sees "no
//! user config") and parallel-safe (no `#[serial]`, no process-global `$HOME`
//! mutation, no `unsafe`).

use std::cell::RefCell;
use std::path::PathBuf;

thread_local! {
    static TEST_CONFIG_PATH: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// The current thread's redirect, or `None` when unset — "no user config".
pub(super) fn test_redirect() -> Option<PathBuf> {
    TEST_CONFIG_PATH.with(|cell| cell.borrow().clone())
}

/// Point [`super::UserConfig::load`]/[`super::UserConfig::load_strict`]/
/// [`super::set`] at `path` for the current thread until the returned guard
/// drops.
///
/// Restores the previous value on drop, so a panicking test body cannot leak
/// the redirect into another test on a reused harness thread.
pub(crate) fn redirect_user_config(path: PathBuf) -> UserConfigRedirect {
    let previous = TEST_CONFIG_PATH.with(|cell| cell.replace(Some(path)));
    UserConfigRedirect { previous }
}

/// Guard returned by [`redirect_user_config`]; restores the prior redirect on
/// drop.
pub(crate) struct UserConfigRedirect {
    previous: Option<PathBuf>,
}

impl Drop for UserConfigRedirect {
    fn drop(&mut self) {
        TEST_CONFIG_PATH.with(|cell| *cell.borrow_mut() = self.previous.take());
    }
}
