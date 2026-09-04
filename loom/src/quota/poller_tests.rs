use super::*;

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
