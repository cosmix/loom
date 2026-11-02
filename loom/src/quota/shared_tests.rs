use super::*;
use crate::quota::claude::RateLimited;
use crate::quota::model::{QuotaWindow, WindowKind};
use std::cell::Cell;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::sync::mpsc;
use tempfile::tempdir;

fn quota(observed_at: i64) -> ProviderQuota {
    ProviderQuota {
        observed_at,
        windows: vec![QuotaWindow {
            kind: WindowKind::FiveHour,
            used_percent: 10.0,
            resets_at: Some(9_000),
        }],
        plan: None,
        error: None,
    }
}

#[test]
fn state_reader_refuses_fifo_oversize_and_symlinked_parent() {
    use std::os::unix::ffi::OsStrExt;
    let root = tempdir().unwrap();
    let dir = root.path().join("quota");
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("state.json");
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated pathname.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    assert_eq!(read_state(&path), AccountState::default());
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, vec![b' '; MAX_STATE_BYTES + 1]).unwrap();
    assert_eq!(read_state(&path), AccountState::default());
    let link = root.path().join("link");
    std::os::unix::fs::symlink(&dir, &link).unwrap();
    assert_eq!(
        read_state(&link.join("state.json")),
        AccountState::default()
    );
}

/// A 429 with a Retry-After must back off across pollers: the state written
/// through `dir` has to read back through the same `dir`.
#[cfg(unix)]
fn assert_retry_after_is_honoured(dir: &Path) {
    let limited = poll_shared(dir, "aa", 1_000, || {
        Err(RateLimited {
            retry_after_secs: Some(600),
        }
        .into())
    })
    .unwrap();
    assert_eq!(limited.next_poll_at, 1_600);
    let adopted = poll_shared(dir, "aa", 1_100, || panic!("fetched during the backoff")).unwrap();
    assert_eq!(adopted, limited);
}

#[cfg(unix)]
#[test]
fn state_reads_back_under_a_symlinked_home() {
    let root = tempdir().unwrap();
    let real_home = root.path().join("real-home");
    std::fs::create_dir(&real_home).unwrap();
    let home = root.path().join("home");
    std::os::unix::fs::symlink(&real_home, &home).unwrap();
    assert_retry_after_is_honoured(&shared_dir(&home));
}

#[cfg(unix)]
#[test]
fn state_reads_back_under_a_symlinked_loom_directory() {
    let root = tempdir().unwrap();
    let dotfiles = root.path().join("dotfiles-loom");
    std::fs::create_dir(&dotfiles).unwrap();
    let home = root.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::os::unix::fs::symlink(&dotfiles, home.join(".loom")).unwrap();
    assert_retry_after_is_honoured(&shared_dir(&home));
}

#[cfg(unix)]
#[test]
fn state_reader_still_refuses_a_symlinked_quota_directory_under_an_alias() {
    let root = tempdir().unwrap();
    let real_home = root.path().join("real-home");
    let elsewhere = root.path().join("elsewhere");
    std::fs::create_dir_all(real_home.join(".loom")).unwrap();
    std::fs::create_dir(&elsewhere).unwrap();
    let body = serde_json::to_string(&AccountState {
        next_poll_at: 42,
        ..AccountState::default()
    })
    .unwrap();
    std::fs::write(elsewhere.join("claude-aa.json"), body).unwrap();
    std::os::unix::fs::symlink(&elsewhere, real_home.join(".loom/quota")).unwrap();
    let home = root.path().join("home");
    std::os::unix::fs::symlink(&real_home, &home).unwrap();
    assert_eq!(
        read_state(&shared_dir(&home).join("claude-aa.json")),
        AccountState::default()
    );
}

fn limited(state: AccountState, times: u32) -> AccountState {
    (0..times).fold(state, |s, _| apply_rate_limited(s, None, 1_000))
}

#[test]
fn success_resets_streak_and_error() {
    let state = limited(AccountState::default(), 2);
    let state = apply_success(state, quota(1_000), 1_000);
    assert_eq!(state.consecutive_rate_limits, 0);
    assert_eq!(state.last_error, None);
    assert_eq!(state.interval_secs, POLL_INTERVAL.as_secs());
    assert_eq!(state.next_poll_at, 1_000 + POLL_INTERVAL.as_secs() as i64);
    assert_eq!(display_error(&state), None);
}

#[test]
fn rate_limit_is_shown_only_on_the_third_consecutive_429() {
    let state = apply_success(AccountState::default(), quota(1_000), 1_000);
    assert_eq!(display_error(&limited(state.clone(), 1)), None);
    assert_eq!(display_error(&limited(state.clone(), 2)), None);
    let third = limited(state, 3);
    assert_eq!(display_error(&third), Some("rate limited".to_string()));
    assert!(third.quota.is_some());
    assert_eq!(
        display_error(&apply_success(third, quota(2_000), 2_000)),
        None
    );
}

#[test]
fn a_non_429_error_shows_immediately_and_resets_the_streak() {
    let state = limited(AccountState::default(), 2);
    let state = apply_error(state, "boom", 1_000);
    assert_eq!(state.consecutive_rate_limits, 0);
    assert_eq!(display_error(&state), Some("boom".to_string()));
    assert_eq!(state.interval_secs, 600);
}

#[test]
fn rate_limit_backoff_honours_retry_after_with_a_300s_floor() {
    let long = apply_rate_limited(AccountState::default(), Some(600), 1_000);
    assert_eq!(long.next_poll_at, 1_600);
    let short = apply_rate_limited(AccountState::default(), Some(30), 1_000);
    assert_eq!(short.next_poll_at, 1_300);
}

#[test]
fn token_key_is_16_hex_chars_and_differs_per_token() {
    let key = token_key("secret-token");
    assert_eq!(key.len(), 16);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(!key.contains("secret"));
    assert_ne!(key, token_key("other-token"));
}

#[test]
fn a_second_daemon_within_the_interval_adopts_the_first_result() {
    let dir = tempdir().unwrap();
    let calls = Cell::new(0);
    let fetch = || {
        calls.set(calls.get() + 1);
        Ok(quota(1_000))
    };
    let first = poll_shared(dir.path(), "aa", 1_000, fetch).unwrap();
    let second = poll_shared(dir.path(), "aa", 1_010, || {
        calls.set(calls.get() + 1);
        Ok(quota(1_010))
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(second.quota, first.quota);
}

#[test]
fn different_accounts_each_fetch() {
    let dir = tempdir().unwrap();
    let calls = Cell::new(0);
    for key in ["aa", "bb"] {
        poll_shared(dir.path(), key, 1_000, || {
            calls.set(calls.get() + 1);
            Ok(quota(1_000))
        })
        .unwrap();
    }
    assert_eq!(calls.get(), 2);
}

#[test]
fn fetches_again_once_next_poll_at_has_passed() {
    let dir = tempdir().unwrap();
    let calls = Cell::new(0);
    for now in [1_000, 1_000 + POLL_INTERVAL.as_secs() as i64] {
        poll_shared(dir.path(), "aa", now, || {
            calls.set(calls.get() + 1);
            Ok(quota(now))
        })
        .unwrap();
    }
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_429_from_the_fetch_is_counted_and_keeps_the_last_reading() {
    let dir = tempdir().unwrap();
    poll_shared(dir.path(), "aa", 1_000, || Ok(quota(1_000))).unwrap();
    let state = poll_shared(dir.path(), "aa", 5_000, || {
        Err(RateLimited {
            retry_after_secs: None,
        }
        .into())
    })
    .unwrap();
    assert_eq!(state.consecutive_rate_limits, 1);
    assert_eq!(state.quota.unwrap().observed_at, 1_000);
    assert_eq!(state.next_poll_at, 5_300);
}

#[test]
fn a_corrupt_state_file_is_treated_as_default_and_overwritten() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("claude-aa.json"), "{not json").unwrap();
    let state = poll_shared(dir.path(), "aa", 1_000, || Ok(quota(1_000))).unwrap();
    assert_eq!(state.quota.unwrap().observed_at, 1_000);
    let body = std::fs::read_to_string(dir.path().join("claude-aa.json")).unwrap();
    assert!(serde_json::from_str::<AccountState>(&body).is_ok());
}

#[test]
fn stale_state_of_other_keys_is_pruned_but_locks_and_fresh_state_stay() {
    let dir = tempdir().unwrap();
    let old = dir.path().join("claude-bb.json");
    std::fs::write(&old, "{}").unwrap();
    let day_and_more = SystemTime::now() - Duration::from_secs(25 * 60 * 60);
    File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(day_and_more)
        .unwrap();
    let fresh = dir.path().join("claude-cc.json");
    std::fs::write(&fresh, "{}").unwrap();
    let lock_path = dir.path().join("claude-bb.lock");
    let lock = open_lock(&lock_path).unwrap();
    lock.set_modified(day_and_more).unwrap();

    poll_shared(dir.path(), "aa", 1_000, || Ok(quota(1_000))).unwrap();

    assert!(!old.exists());
    assert!(lock_path.exists());
    assert!(fresh.exists());
    assert!(dir.path().join("claude-aa.json").exists());
}

#[cfg(unix)]
#[test]
fn pruning_keeps_a_held_old_lock_and_serializes_the_next_fetch() {
    let dir = tempdir().unwrap();
    poll_shared(dir.path(), "aa", 1_000, || Ok(quota(1_000))).unwrap();
    let lock_path = dir.path().join("claude-aa.lock");
    let held = open_lock(&lock_path).unwrap();
    held.set_modified(SystemTime::now() - STALE_KEY_AGE - Duration::from_secs(60))
        .unwrap();
    held.lock_exclusive().unwrap();
    let original = held.metadata().unwrap();

    poll_shared(dir.path(), "bb", 1_000, || Ok(quota(1_000))).unwrap();
    let after = lock_path.metadata().unwrap();
    assert_eq!((after.dev(), after.ino()), (original.dev(), original.ino()));
    assert!(dir.path().join("claude-aa.json").exists());

    let (entered_tx, entered_rx) = mpsc::channel();
    let path = dir.path().to_path_buf();
    let next_poll = std::thread::spawn(move || {
        poll_shared(&path, "aa", 5_000, || {
            entered_tx.send(()).unwrap();
            Ok(quota(5_000))
        })
    });
    let entered_while_held = entered_rx.recv_timeout(Duration::from_millis(100)).is_ok();
    FileExt::unlock(&held).unwrap();
    assert!(
        !entered_while_held,
        "fetch entered while the old lock was held"
    );
    assert_eq!(
        next_poll
            .join()
            .unwrap()
            .unwrap()
            .quota
            .unwrap()
            .observed_at,
        5_000
    );
}

#[cfg(unix)]
#[test]
fn pruning_keeps_the_inode_for_a_lock_opened_before_cleanup() {
    let dir = tempdir().unwrap();
    poll_shared(dir.path(), "aa", 1_000, || Ok(quota(1_000))).unwrap();
    let lock_path = dir.path().join("claude-aa.lock");
    let held = open_lock(&lock_path).unwrap();
    held.set_modified(SystemTime::now() - STALE_KEY_AGE - Duration::from_secs(60))
        .unwrap();
    held.lock_exclusive().unwrap();
    let waiting = open_lock(&lock_path).unwrap();

    poll_shared(dir.path(), "bb", 1_000, || Ok(quota(1_000))).unwrap();
    let after = lock_path.metadata().unwrap();
    let opened = waiting.metadata().unwrap();
    assert_eq!((after.dev(), after.ino()), (opened.dev(), opened.ino()));

    FileExt::unlock(&held).unwrap();
    waiting.lock_exclusive().unwrap();
    let newcomer = open_lock(&lock_path).unwrap();
    assert!(FileExt::try_lock_exclusive(&newcomer).is_err());
}
