use super::*;

const NOW: i64 = 1_788_523_200;

#[test]
fn the_bucket_shape_parses_both_windows() {
    let body = r#"{
        "five_hour": { "utilization": 33.0, "resets_at": "2026-04-11T07:00:00+00:00" },
        "seven_day": { "utilization": 61.0, "resets_at": "2026-04-17T07:00:00+00:00" }
    }"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows.len(), 2);
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
    assert_eq!(quota.windows[0].used_percent, 33.0);
    assert_eq!(quota.windows[1].kind, WindowKind::SevenDay);
    assert_eq!(quota.windows[1].used_percent, 61.0);
}

#[test]
fn a_missing_or_null_bucket_yields_no_window_for_it() {
    let body = r#"{
        "five_hour": { "utilization": 33.0, "resets_at": null },
        "seven_day": null
    }"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows.len(), 1);
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
}

#[test]
fn the_limits_array_shape_maps_kinds_to_windows() {
    let body = r#"{
        "limits": [
            { "kind": "session", "percent": 42.0, "resets_at": "2026-04-11T07:00:00Z" },
            { "kind": "weekly_all", "percent": 18.0, "resets_at": "2026-04-17T07:00:00Z" }
        ]
    }"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows.len(), 2);
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
    assert_eq!(quota.windows[0].used_percent, 42.0);
    assert_eq!(quota.windows[1].kind, WindowKind::SevenDay);
    assert_eq!(quota.windows[1].used_percent, 18.0);
}

#[test]
fn buckets_win_when_both_shapes_are_present() {
    let body = r#"{
        "five_hour": { "utilization": 5.0, "resets_at": null },
        "limits": [ { "kind": "session", "percent": 90.0, "resets_at": null } ]
    }"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows.len(), 1);
    assert_eq!(quota.windows[0].used_percent, 5.0);
}

#[test]
fn neither_shape_present_yields_zero_windows_without_erroring() {
    let quota = parse_response(r#"{"unrelated": true}"#, NOW).unwrap();
    assert!(quota.windows.is_empty());
}

#[test]
fn an_unknown_limit_kind_is_ignored() {
    let body = r#"{"limits": [{ "kind": "other", "percent": 50.0, "resets_at": null }]}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert!(quota.windows.is_empty());
}

#[test]
fn a_placeholder_limit_is_ignored() {
    let body = r#"{"limits": [{ "kind": "session", "percent": 0, "resets_at": null }]}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert!(quota.windows.is_empty());
}

#[test]
fn a_fractional_second_timestamp_parses() {
    let body = r#"{"five_hour": {"utilization": 10.0, "resets_at": "2026-04-11T07:00:00.250Z"}}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows[0].resets_at, Some(1_775_890_800));
}

#[test]
fn a_plus_two_offset_timestamp_parses() {
    let body = r#"{"five_hour": {"utilization": 10.0, "resets_at": "2026-04-11T09:00:00+02:00"}}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows[0].resets_at, Some(1_775_890_800));
}

#[test]
fn a_millisecond_epoch_resets_at_is_normalized_to_seconds() {
    let body =
        r#"{"limits": [{ "kind": "session", "percent": 10.0, "resets_at": 1788728400000 }]}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows[0].resets_at, Some(1_788_728_400));
}

#[test]
fn a_percentage_above_one_hundred_clamps_to_one_hundred() {
    let body = r#"{"five_hour": {"utilization": 150.0, "resets_at": null}}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows[0].used_percent, 100.0);
}

#[test]
fn a_missing_percent_yields_no_window_but_the_other_window_still_parses() {
    // Two failure modes for an invalid percentage: a JSON `null` (handled
    // entirely inside this crate's parsing) and a number so large the JSON
    // text itself is rejected by the parser (handled by the next test) -
    // together these cover "yields no window" from either path.
    let body = r#"{"limits": [
        { "kind": "session", "percent": null, "resets_at": "2026-04-11T07:00:00Z" },
        { "kind": "weekly_all", "percent": 22.0, "resets_at": null }
    ]}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows.len(), 1);
    assert_eq!(quota.windows[0].kind, WindowKind::SevenDay);
    assert_eq!(quota.windows[0].used_percent, 22.0);
}

#[test]
fn a_number_outside_f64_range_fails_the_whole_parse_rather_than_silently_using_it() {
    let body = r#"{"limits": [{ "kind": "session", "percent": 1e999, "resets_at": null }]}"#;
    assert!(parse_response(body, NOW).is_err());
}

#[test]
fn an_unparsable_timestamp_becomes_none_without_dropping_the_window() {
    let body = r#"{"five_hour": {"utilization": 12.0, "resets_at": "not-a-timestamp"}}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows.len(), 1);
    assert_eq!(quota.windows[0].used_percent, 12.0);
    assert_eq!(quota.windows[0].resets_at, None);
}

#[test]
fn a_duplicate_kind_in_the_limits_array_keeps_the_first() {
    let body = r#"{"limits": [
        { "kind": "session", "percent": 10.0, "resets_at": null },
        { "kind": "session", "percent": 90.0, "resets_at": null }
    ]}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows.len(), 1);
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
    assert_eq!(quota.windows[0].used_percent, 10.0);
}

#[test]
fn five_hour_is_ordered_first_even_when_the_input_lists_weekly_first() {
    let body = r#"{"limits": [
        { "kind": "weekly_all", "percent": 5.0, "resets_at": null },
        { "kind": "session", "percent": 6.0, "resets_at": null }
    ]}"#;
    let quota = parse_response(body, NOW).unwrap();
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
    assert_eq!(quota.windows[1].kind, WindowKind::SevenDay);
}
