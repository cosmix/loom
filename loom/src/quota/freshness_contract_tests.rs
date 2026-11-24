//! Contract: a reading observed in the future or at an extreme value is
//! never reported as fresh; small clock skew still reads as age zero.

use super::{age_secs, STALE_AFTER_SECS};

const NOW: i64 = 1_000_000;

#[test]
fn a_future_or_extreme_observed_at_is_not_fresh() {
    let extreme = std::panic::catch_unwind(|| age_secs(i64::MIN, NOW));
    let extreme = extreme.expect("age_secs(i64::MIN, now) must not panic");
    assert!(
        extreme >= STALE_AFTER_SECS,
        "an i64::MIN observation has age {extreme}, below the stale limit {STALE_AFTER_SECS}"
    );

    let future = age_secs(NOW + 10_000, NOW);
    assert!(
        future >= STALE_AFTER_SECS,
        "an observation 10000s in the future has age {future}, below the stale limit {STALE_AFTER_SECS}"
    );

    assert_eq!(age_secs(NOW + 60, NOW), 0, "small skew reads as age zero");
    assert_eq!(age_secs(NOW - 1_000, NOW), 1_000);
}
