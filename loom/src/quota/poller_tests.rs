use super::*;
use tempfile::tempdir;

#[test]
fn next_interval_resets_to_the_poll_interval_on_success() {
    assert_eq!(next_interval(MAX_BACKOFF, false), POLL_INTERVAL);
}

#[test]
fn next_interval_doubles_on_failure() {
    assert_eq!(next_interval(POLL_INTERVAL, true), POLL_INTERVAL * 2);
}

#[test]
fn next_interval_caps_at_the_max_backoff() {
    let almost_max = MAX_BACKOFF - Duration::from_secs(1);
    assert_eq!(next_interval(almost_max, true), MAX_BACKOFF);
    assert_eq!(next_interval(MAX_BACKOFF, true), MAX_BACKOFF);
}

#[test]
fn rate_limit_backoff_uses_the_servers_retry_after() {
    assert_eq!(rate_limit_backoff(Some(600)), Duration::from_secs(600));
}

#[test]
fn rate_limit_backoff_floors_a_short_retry_after_at_the_minimum() {
    assert_eq!(rate_limit_backoff(Some(30)), RATE_LIMIT_MIN_BACKOFF);
}

#[test]
fn rate_limit_backoff_defaults_to_the_minimum_without_a_retry_after() {
    assert_eq!(rate_limit_backoff(None), RATE_LIMIT_MIN_BACKOFF);
}

fn successful_quota(observed_at: i64) -> crate::quota::model::ProviderQuota {
    crate::quota::model::ProviderQuota {
        observed_at,
        windows: vec![crate::quota::model::QuotaWindow {
            kind: crate::quota::model::WindowKind::FiveHour,
            used_percent: 42.0,
            resets_at: Some(2_000),
        }],
        plan: None,
        error: None,
    }
}

#[test]
fn finish_poll_appends_history_only_after_the_snapshot_write_succeeds() {
    let dir = tempdir().unwrap();
    let mut state = ProviderState::new();

    finish_poll(
        "claude",
        dir.path(),
        &mut state,
        successful_quota(chrono::Utc::now().timestamp()),
    );

    let history = crate::quota::read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.points.len(), 1);
}

#[test]
fn finish_poll_does_not_append_history_when_the_snapshot_write_fails() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.json");
    std::fs::write(&target, "unchanged").unwrap();
    std::fs::create_dir_all(cache::quota_dir(dir.path())).unwrap();
    std::os::unix::fs::symlink(&target, cache::provider_path(dir.path(), "claude")).unwrap();
    let mut state = ProviderState::new();

    finish_poll("claude", dir.path(), &mut state, successful_quota(1_000));

    let history = crate::quota::read_history(dir.path(), "claude", 0, None);
    assert_eq!(
        history.diagnostics.source,
        crate::quota::HistorySourceState::Missing
    );
}

fn account_after(limits: u32) -> AccountState {
    let state = shared::apply_success(AccountState::default(), successful_quota(1_000), 1_000);
    (0..limits).fold(state, |s, _| shared::apply_rate_limited(s, None, 2_000))
}

#[test]
fn a_second_account_without_a_reading_clears_the_first_accounts_quota() {
    let dir = tempdir().unwrap();
    let mut state = ProviderState::new();
    project_account(dir.path(), &mut state, &account_after(0), 1_000);
    assert!(cache::read_provider(dir.path(), "claude").is_some());

    let second = AccountState {
        consecutive_rate_limits: 1,
        ..AccountState::default()
    };
    project_account(dir.path(), &mut state, &second, 2_000);

    assert!(cache::read_provider(dir.path(), "claude").is_none());
}

#[test]
fn a_failed_cache_write_keeps_its_backoff() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.json");
    std::fs::write(&target, "unchanged").unwrap();
    std::fs::create_dir_all(cache::quota_dir(dir.path())).unwrap();
    std::os::unix::fs::symlink(&target, cache::provider_path(dir.path(), "claude")).unwrap();

    let mut state = ProviderState::new();
    let expected_backoff = next_interval(state.interval, true);
    let before = Instant::now();
    project_account(dir.path(), &mut state, &account_after(0), 1_000);

    assert_eq!(state.interval, expected_backoff);
    assert!(state.next_due >= before + expected_backoff);
}

#[test]
fn an_unconfirmed_429_keeps_the_last_reading_without_an_error() {
    let dir = tempdir().unwrap();
    let mut state = ProviderState::new();
    project_account(dir.path(), &mut state, &account_after(0), 1_000);
    project_account(dir.path(), &mut state, &account_after(1), 2_000);

    let cached = cache::read_provider(dir.path(), "claude").unwrap();
    assert_eq!(cached.error, None);
    assert_eq!(cached.windows, successful_quota(1_000).windows);
    assert_eq!(cached.observed_at, 1_000);
}

#[test]
fn the_third_consecutive_429_writes_rate_limited_and_keeps_the_windows() {
    let dir = tempdir().unwrap();
    let mut state = ProviderState::new();
    project_account(dir.path(), &mut state, &account_after(0), 1_000);
    project_account(dir.path(), &mut state, &account_after(3), 2_000);

    let cached = cache::read_provider(dir.path(), "claude").unwrap();
    assert_eq!(cached.error.as_deref(), Some("rate limited"));
    assert_eq!(cached.windows.len(), 1);
}

#[test]
fn projection_schedules_the_next_poll_from_the_account_state() {
    let dir = tempdir().unwrap();
    let mut state = ProviderState::new();
    let before = Instant::now();
    project_account(dir.path(), &mut state, &account_after(1), 2_000);

    assert_eq!(state.interval, RATE_LIMIT_MIN_BACKOFF);
    assert!(state.next_due >= before + RATE_LIMIT_MIN_BACKOFF);
}
