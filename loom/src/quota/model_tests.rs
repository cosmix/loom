use super::*;
use crate::orchestrator::monitor::ContextHealth;

#[test]
fn window_kind_labels_are_short() {
    assert_eq!(WindowKind::FiveHour.label(), "5h");
    assert_eq!(WindowKind::SevenDay.label(), "7d");
}

#[test]
fn quota_health_follows_the_context_health_bands() {
    assert!(matches!(quota_health(0.0), ContextHealth::Green));
    assert!(matches!(quota_health(59.0), ContextHealth::Green));
    assert!(matches!(quota_health(60.0), ContextHealth::Yellow));
    assert!(matches!(quota_health(89.0), ContextHealth::Yellow));
    assert!(matches!(quota_health(90.0), ContextHealth::Red));
    assert!(matches!(quota_health(100.0), ContextHealth::Red));
}

#[test]
fn format_reset_uses_short_units_below_a_day() {
    assert_eq!(format_reset(0), "0s");
    assert_eq!(format_reset(45), "45s");
    assert_eq!(format_reset(133), "2m13s");
    assert_eq!(format_reset(7_980), "2h13m");
}

#[test]
fn format_reset_switches_to_days_and_hours_at_a_day() {
    assert_eq!(format_reset(86_400), "1d0h");
    assert_eq!(format_reset(352_800), "4d2h");
}

#[test]
fn format_reset_clamps_negative_input_to_zero() {
    assert_eq!(format_reset(-30), "0s");
}

#[test]
fn reset_text_is_none_without_a_reset_time() {
    assert_eq!(reset_text(None, 1000), None);
}

#[test]
fn reset_text_reads_now_once_the_reset_time_has_passed() {
    assert_eq!(reset_text(Some(999), 1000), Some("now".to_string()));
    assert_eq!(reset_text(Some(1000), 1000), Some("now".to_string()));
}

#[test]
fn reset_text_formats_a_future_reset_time() {
    assert_eq!(reset_text(Some(1_133), 1_000), Some("2m13s".to_string()));
}

#[test]
fn age_secs_clamps_a_future_observed_at_to_zero() {
    assert_eq!(age_secs(1_000, 1_500), 500);
    assert_eq!(age_secs(1_500, 1_000), 0);
}

#[test]
fn clamp_percent_drops_non_finite_values() {
    assert_eq!(clamp_percent(None), None);
    assert_eq!(clamp_percent(Some(f64::NAN)), None);
    assert_eq!(clamp_percent(Some(f64::INFINITY)), None);
    assert_eq!(clamp_percent(Some(f64::NEG_INFINITY)), None);
}

#[test]
fn clamp_percent_clamps_out_of_range_values() {
    assert_eq!(clamp_percent(Some(-5.0)), Some(0.0));
    assert_eq!(clamp_percent(Some(151.0)), Some(100.0));
    assert_eq!(clamp_percent(Some(48.5)), Some(48.5));
}

#[test]
fn normalize_epoch_passes_seconds_through_unchanged() {
    assert_eq!(normalize_epoch(1_788_531_180), 1_788_531_180);
}

#[test]
fn normalize_epoch_divides_milliseconds_down_to_seconds() {
    assert_eq!(normalize_epoch(1_788_728_400_000), 1_788_728_400);
}
