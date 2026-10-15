use super::{newest_period, quota_inputs, quota_left, FivePoint};
use crate::quota::cache::write_provider;
use crate::quota::{ProviderQuota, QuotaWindow, WindowKind};
use crate::tokens::model::{QuotaLeft, QuotaProvider};
use crate::tokens::pass_memo::PassMemo;
use crate::tokens::tests_fixture::{body, Fixture};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::DirBuilderExt;

const NOW: i64 = 1_792_000_000;
const RESET: i64 = NOW + 133 * 60;

fn point(at: i64, used: f64, resets_at: Option<i64>) -> FivePoint {
    FivePoint {
        at,
        used,
        resets_at,
    }
}

/// A history row with a five-hour window, and a seven-day one when `weekly`.
fn row(at: i64, used: f64, resets_at: Option<i64>, weekly: bool) -> Value {
    let five = json!({"kind": "five-hour", "used_percent": used, "resets_at": resets_at});
    let mut windows = vec![five];
    if weekly {
        let seven = json!({"kind": "seven-day", "used_percent": 20.0, "resets_at": NOW + 400_000});
        windows.push(seven);
    }
    json!({"schema_version": 1, "observed_at": at, "windows": windows, "plan": null})
}

fn write_history(fx: &Fixture, provider: &str, rows: &[Value]) {
    let dir = fx.work_root.join("quota").join("history");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .expect("history dir");
    let lines: Vec<String> = rows.iter().map(Value::to_string).collect();
    fs::write(dir.join(format!("{provider}.jsonl")), body(&lines)).expect("history");
}

fn claude_left(minutes_left: Option<u32>) -> Vec<QuotaLeft> {
    vec![QuotaLeft {
        provider: QuotaProvider::Claude,
        minutes_left,
        lasts_to_reset: false,
    }]
}

#[test]
fn reset_jitter_keeps_one_period() {
    let fx = Fixture::new();
    write_history(
        &fx,
        "claude",
        &[
            row(NOW - 1530, 52.0, Some(RESET - 1), true),
            row(NOW - 630, 57.0, Some(RESET), true),
            row(NOW - 30, 62.0, Some(RESET - 1), true),
        ],
    );

    assert_eq!(quota_left(&fx.work_root, NOW), claude_left(Some(94)));
}

#[test]
fn reset_jump_or_usage_drop_starts_a_new_period() {
    let jump = [
        point(NOW - 1530, 52.0, Some(RESET)),
        point(NOW - 630, 57.0, Some(RESET + 301)),
        point(NOW - 30, 62.0, Some(RESET + 301)),
    ];
    let drop = [
        point(NOW - 1530, 52.0, None),
        point(NOW - 1000, 10.0, None),
        point(NOW - 30, 20.0, Some(RESET)),
    ];
    let jitter = [
        point(NOW - 630, 57.0, Some(RESET)),
        point(NOW - 30, 62.0, Some(RESET + 300)),
    ];

    let newest = vec![(NOW - 630, 57.0), (NOW - 30, 62.0)];
    assert_eq!(newest_period(&jump), (newest.clone(), Some(RESET + 301)));
    assert_eq!(
        newest_period(&drop),
        (vec![(NOW - 1000, 10.0), (NOW - 30, 20.0)], Some(RESET))
    );
    assert_eq!(newest_period(&jitter), (newest, Some(RESET + 300)));
}

#[test]
fn newer_snapshot_reading_is_appended() {
    let fx = Fixture::new();
    write_history(
        &fx,
        "claude",
        &[
            row(NOW - 1530, 52.0, Some(RESET), false),
            row(NOW - 630, 57.0, Some(RESET), false),
        ],
    );
    let window = QuotaWindow {
        kind: WindowKind::FiveHour,
        used_percent: 62.0,
        resets_at: Some(RESET),
    };
    let snapshot = ProviderQuota {
        observed_at: NOW - 30,
        windows: vec![window],
        plan: None,
        error: None,
    };
    write_provider(&fx.work_root, "claude", &snapshot).expect("snapshot");

    assert_eq!(quota_left(&fx.work_root, NOW), claude_left(Some(94)));
}

#[test]
fn provider_without_a_newest_five_hour_window_has_no_entry() {
    let fx = Fixture::new();
    let weekly_only = json!({"schema_version": 1, "observed_at": NOW - 30,
        "windows": [{"kind": "seven-day", "used_percent": 20.0, "resets_at": NOW + 400_000}],
        "plan": "prolite"});
    write_history(
        &fx,
        "codex",
        &[row(NOW - 630, 40.0, Some(RESET), false), weekly_only],
    );

    assert_eq!(quota_left(&fx.work_root, NOW), Vec::new());
}

#[test]
fn short_history_yields_an_entry_without_minutes() {
    let fx = Fixture::new();
    write_history(&fx, "claude", &[row(NOW - 30, 62.0, Some(RESET), false)]);

    assert_eq!(quota_left(&fx.work_root, NOW), claude_left(None));
}

#[test]
fn quota_inputs_are_the_files_the_quota_writers_create() {
    let fx = Fixture::new();
    let snapshot = ProviderQuota {
        observed_at: NOW - 30,
        windows: vec![],
        plan: None,
        error: None,
    };
    for provider in ["claude", "codex"] {
        write_history(&fx, provider, &[row(NOW - 30, 62.0, Some(RESET), false)]);
        write_provider(&fx.work_root, provider, &snapshot).expect("snapshot");
    }

    for path in quota_inputs(&fx.work_root) {
        assert!(path.is_file(), "{} is not a quota file", path.display());
    }
}

#[test]
fn memo_reuses_quota_left_until_a_file_changes_or_30_s_pass() {
    let fx = Fixture::new();
    let steady = [
        row(NOW - 1530, 52.0, Some(RESET), false),
        row(NOW - 30, 62.0, Some(RESET), false),
    ];
    write_history(&fx, "claude", &steady);
    let mut memo = PassMemo::new(&fx.work_root);

    assert_eq!(
        memo.quota_left(&fx.work_root, NOW + 2),
        claude_left(Some(94))
    );
    assert_eq!(
        quota_left(&fx.work_root, NOW + 31),
        claude_left(Some(93)),
        "a fresh estimate at NOW + 31"
    );
    assert_eq!(
        memo.quota_left(&fx.work_root, NOW + 31),
        claude_left(Some(94)),
        "reused 29 s later"
    );
    assert_eq!(
        memo.quota_left(&fx.work_root, NOW + 32),
        claude_left(Some(93)),
        "recomputed 30 s later"
    );

    let steeper = [
        row(NOW - 1530, 52.0, Some(RESET), false),
        row(NOW - 630, 60.0, Some(RESET), false),
        row(NOW - 30, 72.0, Some(RESET), false),
    ];
    write_history(&fx, "claude", &steeper);

    assert_eq!(
        memo.quota_left(&fx.work_root, NOW + 33),
        claude_left(Some(33)),
        "recomputed when the history changed"
    );
}
