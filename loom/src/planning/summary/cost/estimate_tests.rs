use chrono::{DateTime, Duration, Local, TimeZone, Utc};

use super::estimate::{estimate, totals, EstimateBasis};
use super::{CostLine, Level, TokenVector, Trigger};

fn line(id: &str, level: Level, bytes: Option<u64>, fresh_input: Option<u64>) -> CostLine {
    CostLine {
        time: "2026-01-15T00:00:00Z".into(),
        trigger: Trigger::Background,
        summaries: vec![id.into()],
        level,
        model: None,
        source_bytes: bytes,
        tokens: TokenVector {
            fresh_input,
            cache_write: Some(0),
            cache_read: Some(999_999),
            output: Some(0),
        },
    }
}

#[test]
fn estimate_without_history_uses_starting_figures() {
    let pending = [
        ("demo/product/section_01".into(), Level::Document, 100),
        ("overview/demo/product".into(), Level::Overview, 100),
    ];

    let result = estimate(&[], &pending);

    assert_eq!(result.tokens, 190_000);
    assert_eq!(result.basis, EstimateBasis::Starting);
    assert_eq!(
        result.per_summary,
        vec![
            ("demo/product/section_01".into(), 40_000),
            ("overview/demo/product".into(), 150_000)
        ]
    );
}

#[test]
fn estimate_uses_last_complete_individual_record_and_excludes_cache_reads() {
    let mut recent = line("demo/product/section_01", Level::Document, None, Some(20));
    recent.tokens.cache_write = Some(30);
    recent.tokens.output = Some(40);
    let mut batch = line(
        "demo/product/section_01",
        Level::Document,
        Some(1),
        Some(9_000),
    );
    batch.summaries.push("plans/other".into());
    let log = [
        line(
            "demo/product/section_01",
            Level::Document,
            Some(100),
            Some(10),
        ),
        recent,
        batch,
        line("demo/product/section_01", Level::Document, Some(1), None),
    ];

    let result = estimate(
        &log,
        &[("demo/product/section_01".into(), Level::Document, 90_000)],
    );

    assert_eq!(result.tokens, 90);
    assert_eq!(result.basis, EstimateBasis::Last);
}

#[test]
fn estimate_uses_median_of_three_rates_at_the_same_level() {
    let log = [
        line("plans/a", Level::Document, Some(100), Some(900)),
        line("plans/b", Level::Document, Some(200), Some(400)),
        line("plans/c", Level::Document, Some(50), Some(150)),
        line(
            "overview/demo/product",
            Level::Overview,
            Some(1),
            Some(1_000_000),
        ),
    ];
    let pending = [
        ("plans/a".into(), Level::Document, 10_000),
        ("plans/new".into(), Level::Document, 100),
    ];

    let result = estimate(&log, &pending);

    assert_eq!(result.tokens, 1_200);
    assert_eq!(result.basis, EstimateBasis::PerByte);
    assert_eq!(
        result.per_summary,
        vec![("plans/a".into(), 900), ("plans/new".into(), 300)]
    );
}

#[test]
fn estimate_averages_middle_rates_and_rounds_fractional_tokens_up() {
    let log = [
        line("overview/a/product", Level::Overview, Some(2), Some(1)),
        line("overview/b/product", Level::Overview, Some(2), Some(2)),
    ];

    let result = estimate(&log, &[("overview/new/product".into(), Level::Overview, 3)]);

    assert_eq!(result.tokens, 3);
    assert_eq!(result.basis, EstimateBasis::PerByte);
}

#[test]
fn estimate_ignores_unknown_zero_size_and_batch_rates() {
    let mut batch = line("plans/a", Level::Document, Some(1), Some(1));
    batch.summaries.push("plans/b".into());
    let mut incomplete = line("plans/c", Level::Document, Some(1), Some(1));
    incomplete.tokens.output = None;
    let log = [
        batch,
        incomplete,
        line("plans/d", Level::Document, Some(0), Some(1)),
        line("plans/e", Level::Document, None, Some(1)),
        line("plans/f", Level::Document, Some(1), None),
    ];

    let result = estimate(&log, &[("plans/new".into(), Level::Document, 100)]);

    assert_eq!(result.tokens, 40_000);
    assert_eq!(result.basis, EstimateBasis::Starting);
}

#[test]
fn estimate_reports_weakest_basis_in_a_mixed_batch() {
    let log = [line("plans/a", Level::Document, Some(10), Some(20))];
    let pending = [
        ("plans/a".into(), Level::Document, 10),
        ("plans/b".into(), Level::Document, 10),
        ("overview/c/product".into(), Level::Overview, 10),
    ];

    let result = estimate(&log, &pending);

    assert_eq!(result.tokens, 150_040);
    assert_eq!(result.basis, EstimateBasis::Starting);
}

#[test]
fn estimate_empty_batch_has_no_cost() {
    let result = estimate(&[], &[]);

    assert_eq!(result.tokens, 0);
    assert!(result.per_summary.is_empty());
    assert_eq!(result.basis, EstimateBasis::Last);
}

fn local_now() -> DateTime<Local> {
    Local
        .with_ymd_and_hms(2026, 1, 15, 0, 5, 0)
        .single()
        .unwrap()
}

fn timed_line(time: DateTime<Local>, input: u64) -> CostLine {
    let mut record = line("plans/a", Level::Document, None, Some(input));
    // Use UTC in the log so the totals must convert back to the local date.
    record.time = time.with_timezone(&Utc).to_rfc3339();
    record
}

#[test]
fn totals_use_local_midnight_and_include_all_token_components() {
    let now = local_now();
    let midnight = now - Duration::minutes(5);
    let mut today = timed_line(midnight, 2);
    today.tokens.cache_write = Some(3);
    today.tokens.cache_read = Some(5);
    today.tokens.output = Some(7);
    let log = [timed_line(midnight - Duration::seconds(1), 11), today];

    let result = totals(&log, now);

    assert_eq!(result.today.fresh_input, Some(2));
    assert_eq!(result.today.cache_write, Some(3));
    assert_eq!(result.today.cache_read, Some(5));
    assert_eq!(result.today.output, Some(7));
    assert_eq!(result.week.fresh_input, Some(13));
}

#[test]
fn totals_use_a_trailing_168_hour_window_and_exclude_future_and_invalid_times() {
    let now = local_now();
    let start = now - Duration::hours(168);
    let mut invalid = timed_line(now, 1_000);
    invalid.time = "invalid".into();
    let log = [
        timed_line(start - Duration::seconds(1), 1_000),
        timed_line(start, 2),
        timed_line(start + Duration::seconds(1), 3),
        timed_line(now, 5),
        timed_line(now + Duration::seconds(1), 1_000),
        invalid,
    ];

    let result = totals(&log, now);

    assert_eq!(result.week.fresh_input, Some(10));
    assert_eq!(result.today.fresh_input, Some(5));
}

#[test]
fn totals_preserve_unknown_components_and_sum_known_components() {
    let now = local_now();
    let mut unknown = timed_line(now, 1);
    unknown.tokens = TokenVector {
        fresh_input: None,
        cache_write: None,
        cache_read: None,
        output: None,
    };
    let mut partial = timed_line(now, 2);
    partial.tokens.cache_write = None;
    partial.tokens.cache_read = None;
    partial.tokens.output = Some(0);

    let result = totals(&[unknown, partial], now);

    assert_eq!(result.today.fresh_input, Some(2));
    assert_eq!(result.today.cache_write, None);
    assert_eq!(result.today.cache_read, None);
    assert_eq!(result.today.output, Some(0));
}

#[test]
fn totals_without_records_keep_all_components_unknown() {
    let result = totals(&[], local_now());

    assert_eq!(result.today.fresh_input, None);
    assert_eq!(result.today.cache_write, None);
    assert_eq!(result.today.cache_read, None);
    assert_eq!(result.today.output, None);
    assert_eq!(result.week.fresh_input, None);
    assert_eq!(result.week.cache_write, None);
    assert_eq!(result.week.cache_read, None);
    assert_eq!(result.week.output, None);
}

#[test]
fn token_arithmetic_saturates_on_overflow() {
    let now = local_now();
    let mut first = timed_line(now, u64::MAX);
    first.tokens.cache_write = Some(1);
    first.tokens.output = Some(1);
    let mut second = timed_line(now, 1);
    second.summaries = vec!["plans/b".into()];
    let log = [first, second];
    let pending = [
        ("plans/a".into(), Level::Document, 1),
        ("plans/b".into(), Level::Document, 1),
    ];

    let usage = totals(&log, now);
    let prediction = estimate(&log, &pending);

    assert_eq!(usage.today.fresh_input, Some(u64::MAX));
    assert_eq!(usage.week.fresh_input, Some(u64::MAX));
    assert_eq!(prediction.tokens, u64::MAX);
    assert_eq!(prediction.per_summary[0].1, u64::MAX);
}
