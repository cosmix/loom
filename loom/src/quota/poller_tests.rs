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
