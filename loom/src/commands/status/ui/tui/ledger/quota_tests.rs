//! Tests for the quota footer line: its tiers, its content, and how it shares the
//! footer with the legend strip.

use ratatui::{backend::TestBackend, Terminal};

use super::super::tests::{contains, fixture, screen};
use super::super::{render, LedgerView, RenderOutcome};
use super::*;
use crate::commands::status::data::{StageSummary, StatusData};
use crate::commands::status::render::attention_model::attention_entries;
use crate::commands::status::ui::tui::state::TuiActivityLog;
use crate::plan::graph::levels;

const NOW: i64 = 1_788_523_200;

fn window(kind: WindowKind, used_percent: f64, resets_at: i64) -> QuotaWindow {
    QuotaWindow {
        kind,
        used_percent,
        resets_at: Some(resets_at),
    }
}

fn claude() -> ProviderQuota {
    ProviderQuota {
        observed_at: NOW,
        windows: vec![
            window(WindowKind::FiveHour, 48.0, 1_788_531_180),
            window(WindowKind::SevenDay, 31.0, 1_788_876_000),
        ],
        plan: None,
        error: None,
    }
}

fn codex() -> ProviderQuota {
    ProviderQuota {
        observed_at: 1_788_522_960,
        windows: vec![window(WindowKind::SevenDay, 63.0, 1_788_728_400)],
        plan: Some("pro".to_owned()),
        error: None,
    }
}

fn snapshot() -> QuotaSnapshot {
    QuotaSnapshot {
        claude: Some(claude()),
        codex: Some(codex()),
    }
}

fn data_with(quota: QuotaSnapshot) -> StatusData {
    let mut data = fixture();
    data.quota = quota;
    data
}

fn with_view<T>(data: &StatusData, scrollable: bool, draw: impl FnOnce(&LedgerView) -> T) -> T {
    let levels = levels::compute_all_levels(
        &data.stages,
        |stage| stage.id.as_str(),
        |stage| &stage.dependencies,
    );
    let ordered: Vec<&StageSummary> = data.stages.iter().collect();
    let attention = attention_entries(&data.stages);
    let activity = TuiActivityLog::new();
    let view = LedgerView {
        data,
        levels: &levels,
        ordered: &ordered,
        attention: &attention,
        activity: &activity,
        alerts: &[],
        spinner: '⠋',
        scroll_y: 0,
        legend_open: false,
        tick_age_secs: Some(2),
        last_error: None,
        now_epoch: NOW,
        scrollable,
    };
    draw(&view)
}

fn rows(data: &StatusData, width: u16, height: u16, scrollable: bool) -> Vec<String> {
    with_view(data, scrollable, |view| screen(width, height, view))
}

fn viewport_rows(data: &StatusData, width: u16, height: u16) -> u16 {
    with_view(data, false, |view| {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut outcome = RenderOutcome::default();
        terminal
            .draw(|frame| outcome = render(frame, view))
            .unwrap();
        outcome.table_viewport_rows
    })
}

fn quota_row(rows: &[String]) -> Option<&String> {
    rows.iter().find(|row| row.contains("claude 5h"))
}

#[test]
fn full_width_shows_both_providers_with_meters_and_countdowns() {
    let rows = rows(&data_with(snapshot()), 120, 40, false);
    let quota = &rows[rows.len() - 2];
    for needle in [
        "claude", "5h", "48%", "2h13m", "7d", "31%", "4d2h", "│", "codex", "—", "63%", "2d9h", "━",
    ] {
        assert!(quota.contains(needle), "missing {needle} in {quota:?}");
    }
    assert!(rows.last().unwrap().contains("legend"));

    let line = quota_line(&snapshot(), NOW, 120).to_string();
    assert_eq!(
        line,
        " claude 5h ━━━━╌╌╌╌ 48% · 2h13m  7d ━━╌╌╌╌╌╌ 31% · 4d2h │ codex 5h —  7d ━━━━━╌╌╌ 63% · 2d9h"
    );
}

#[test]
fn medium_width_keeps_shorter_bars_without_truncation() {
    let line = quota_line(&snapshot(), NOW, 90);
    assert!(line.width() <= 90);
    assert_eq!(
        line.to_string(),
        " claude 5h ━━━╌╌╌ 48% · 2h13m  7d ━━╌╌╌╌ 31% · 4d2h │ codex 5h —  7d ━━━━╌╌ 63% · 2d9h"
    );
}

#[test]
fn narrow_width_shows_percentages_only() {
    let line = quota_line(&snapshot(), NOW, 64).to_string();
    assert_eq!(line, " claude 5h 48% · 7d 31% │ codex 5h — · 7d 63%");
    assert!(!line.contains('━'));

    let data = data_with(snapshot());
    let rows = rows(&data, 64, 17, false);
    assert_eq!(
        quota_row(&rows).map(String::as_str),
        Some(" claude 5h 48% · 7d 31% │ codex 5h — · 7d 63%")
    );
    assert_eq!(viewport_rows(&data, 64, 17), 4);
}

#[test]
fn quota_line_yields_to_the_minimum_table_at_minimum_height() {
    // The fixture's attention panel already forces compact mode at sixteen rows;
    // the quota line is the next thing to go so the table keeps its six rows.
    let data = data_with(snapshot());
    let rows = rows(&data, 64, 16, false);
    assert!(quota_row(&rows).is_none());
    assert_eq!(viewport_rows(&data, 64, 16), 4);
    assert!(rows.last().unwrap().contains("q quit"));
}

#[test]
fn four_windows_drop_the_bars_before_truncating() {
    let mut codex = codex();
    codex
        .windows
        .insert(0, window(WindowKind::FiveHour, 75.0, 1_788_529_200));
    let snapshot = QuotaSnapshot {
        claude: Some(claude()),
        codex: Some(codex),
    };

    let medium = quota_line(&snapshot, NOW, 90);
    assert!(medium.width() <= 90);
    let medium = medium.to_string();
    assert!(!medium.contains('━'));
    assert!(medium.contains("75% · 1h40m"));

    let full = quota_line(&snapshot, NOW, 120);
    assert!(full.width() <= 120);
    assert!(full.to_string().contains('━'));
}

#[test]
fn a_single_provider_has_no_separator() {
    let snapshot = QuotaSnapshot {
        claude: Some(claude()),
        codex: None,
    };
    let line = quota_line(&snapshot, NOW, 120).to_string();
    assert!(!line.contains('│'));
    assert!(!line.contains("codex"));
    assert!(line.contains("claude 5h"));
}

#[test]
fn stale_data_is_noted_at_the_far_right() {
    let mut claude = claude();
    claude.observed_at = NOW - 700;
    let snapshot = QuotaSnapshot {
        claude: Some(claude),
        codex: Some(codex()),
    };
    let line = quota_line(&snapshot, NOW, 120);
    assert_eq!(line.width(), 120);
    let text = line.to_string();
    assert!(text.ends_with("· claude 11m old"));
    assert!(text.contains("48% · 2h13m"));
}

#[test]
fn a_poll_error_is_noted_after_the_meters() {
    let mut codex = codex();
    codex.error = Some("rate limited".to_owned());
    let snapshot = QuotaSnapshot {
        claude: Some(claude()),
        codex: Some(codex),
    };
    let line = quota_line(&snapshot, NOW, 120).to_string();
    assert!(line.contains("· codex: rate limited"));
    assert!(line.contains("63%"));
}

#[test]
fn an_overlong_error_is_truncated_into_the_room_left_or_dropped() {
    let mut codex = codex();
    codex.error = Some("x".repeat(60));
    let snapshot = QuotaSnapshot {
        claude: Some(claude()),
        codex: Some(codex),
    };

    let full = quota_line(&snapshot, NOW, 120);
    assert_eq!(full.width(), 120);
    let full = full.to_string();
    assert!(full.contains("· codex: xxx"));
    assert!(full.ends_with('…'));

    // Four cells of room at the medium tier is not enough to say anything useful.
    let medium = quota_line(&snapshot, NOW, 90).to_string();
    assert!(!medium.contains("· codex:"));
    assert!(medium.contains("63% · 2d9h"));
}

#[test]
fn without_quota_the_footer_is_one_line() {
    let data = fixture();
    let rows = rows(&data, 120, 40, false);
    assert!(rows.last().unwrap().contains("q quit"));
    assert!(quota_row(&rows).is_none());

    let empty = StatusData::default();
    let with_quota = StatusData {
        quota: snapshot(),
        ..StatusData::default()
    };
    assert_eq!(viewport_rows(&empty, 120, 40), 31);
    assert_eq!(viewport_rows(&with_quota, 120, 40), 30);
}

#[test]
fn scroll_hint_follows_the_scrollable_flag() {
    let data = data_with(snapshot());
    let fixed = rows(&data, 120, 40, false);
    assert!(!contains(&fixed, "↑↓"));
    let scrollable = rows(&data, 120, 40, true);
    assert!(contains(&scrollable, "↑↓ scroll"));
}

#[test]
fn layout_tiers_follow_the_width() {
    for (width, expected) in [
        (64, NARROW),
        (MEDIUM_WIDTH - 1, NARROW),
        (MEDIUM_WIDTH, MEDIUM),
        (MEDIUM_WIDTH + 1, MEDIUM),
        (119, MEDIUM),
        (120, FULL),
    ] {
        assert_eq!(quota_layout(width), expected, "width {width}");
    }
}

#[test]
fn has_quota_needs_either_provider() {
    assert!(!has_quota(&QuotaSnapshot::default()));
    assert!(has_quota(&QuotaSnapshot {
        claude: None,
        codex: Some(codex()),
    }));
}
