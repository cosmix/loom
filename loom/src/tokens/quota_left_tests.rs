use super::{estimate, QuotaEstimate};

const NOW: i64 = 10_000;

fn expected(minutes_left: Option<u32>, lasts_to_reset: bool) -> QuotaEstimate {
    QuotaEstimate {
        minutes_left,
        lasts_to_reset,
    }
}

#[test]
fn rising_ten_percentage_points_over_thirty_minutes() {
    let points = [(NOW - 1800, 52.0), (NOW - 900, 57.0), (NOW, 62.0)];

    let result = estimate(&points, Some(NOW + 133 * 60), NOW);

    assert_eq!(result, expected(Some(114), false));
}

#[test]
fn filters_old_point_and_subtracts_elapsed_half_minute() {
    let points = [
        (NOW - 3000, 10.0),
        (NOW - 1530, 52.0),
        (NOW - 630, 57.0),
        (NOW - 30, 62.0),
    ];

    let result = estimate(&points, Some(NOW + 133 * 60), NOW);

    assert_eq!(result, expected(Some(94), false));
}

#[test]
fn quota_lasts_until_reset_in_sixty_minutes() {
    let points = [
        (NOW - 3000, 10.0),
        (NOW - 1530, 52.0),
        (NOW - 630, 57.0),
        (NOW - 30, 62.0),
    ];

    let result = estimate(&points, Some(NOW + 60 * 60), NOW);

    assert_eq!(result, expected(Some(94), true));
}

#[test]
fn flat_usage_lasts_to_reset_even_without_reset_time() {
    let result = estimate(&[(NOW - 360, 62.0), (NOW, 62.0)], None, NOW);

    assert_eq!(result, expected(None, true));
}

#[test]
fn decreasing_usage_lasts_to_reset() {
    let result = estimate(&[(NOW - 360, 62.0), (NOW, 52.0)], None, NOW);

    assert_eq!(result, expected(None, true));
}

#[test]
fn one_point_has_no_estimate() {
    let result = estimate(&[(NOW, 62.0)], Some(NOW), NOW);

    assert_eq!(result, expected(None, false));
}

#[test]
fn empty_history_has_no_estimate() {
    assert_eq!(estimate(&[], None, NOW), expected(None, false));
}

#[test]
fn two_points_only_two_minutes_apart_have_no_estimate() {
    let result = estimate(&[(NOW - 120, 52.0), (NOW, 62.0)], None, NOW);

    assert_eq!(result, expected(None, false));
}

#[test]
fn points_older_than_thirty_minutes_are_ignored() {
    let points = [(NOW - 2400, 42.0), (NOW - 1801, 52.0), (NOW, 62.0)];

    assert_eq!(estimate(&points, None, NOW), expected(None, false));
}

#[test]
fn usage_above_one_hundred_has_zero_minutes_left() {
    let result = estimate(&[(NOW - 360, 90.0), (NOW, 110.0)], None, NOW);

    assert_eq!(result, expected(Some(0), false));
}

#[test]
fn non_finite_points_do_not_affect_endpoints() {
    let points = [
        (NOW - 1800, f64::NEG_INFINITY),
        (NOW - 1530, 52.0),
        (NOW - 900, f64::NAN),
        (NOW - 30, 62.0),
        (NOW, f64::INFINITY),
    ];

    assert_eq!(estimate(&points, None, NOW), expected(Some(94), false));
}

#[test]
fn invalid_points_cannot_supply_minimum_span() {
    let points = [(NOW - 360, f64::NAN), (NOW - 120, 52.0), (NOW, 62.0)];

    assert_eq!(estimate(&points, None, NOW), expected(None, false));
}

#[test]
fn minimum_span_and_exact_reset_boundary_are_inclusive() {
    let points = [(NOW - 360, 50.0), (NOW, 56.0)];

    let result = estimate(&points, Some(NOW + 44 * 60), NOW);

    assert_eq!(result, expected(Some(44), true));
}

#[test]
fn span_one_second_short_has_no_estimate() {
    let result = estimate(&[(NOW - 359, 50.0), (NOW, 56.0)], None, NOW);

    assert_eq!(result, expected(None, false));
}

#[test]
fn elapsed_time_can_exhaust_remaining_quota() {
    let points = [(NOW - 1800, 90.0), (NOW - 600, 99.0)];

    assert_eq!(estimate(&points, None, NOW), expected(Some(0), false));
}

#[test]
fn tiny_positive_slope_clamps_to_u32_max() {
    let points = [(NOW - 360, 0.0), (NOW, 0.000_000_001)];

    assert_eq!(
        estimate(&points, None, NOW),
        expected(Some(u32::MAX), false)
    );
}

#[test]
fn extreme_timestamp_does_not_overflow_reset_comparison() {
    let now = i64::MAX;
    let points = [(now - 360, 50.0), (now, 56.0)];

    assert_eq!(estimate(&points, Some(now), now), expected(Some(44), true));
}
