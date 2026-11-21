//! The finished-plan screen drawn on a test backend.

use chrono::{DateTime, Duration, TimeZone, Utc};
use ratatui::{backend::TestBackend, Terminal};

use super::ledger::text::text_width;
use super::plan_overview::{render_plan_overview, Metrics};
use super::plan_overview_format::keep_tail;
use super::plan_overview_timeline::timeline_lines;
use crate::models::stage::phases::{Phase, PhaseSpan};
use crate::plan_stats::{ModelStats, PlanStatsRecord, StageStats};
use crate::tokens::TokenUsage;

/// Plan span of the fixture: 100 minutes from nine o'clock.
const PLAN_MINUTES: i64 = 100;

fn at(minutes: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).unwrap() + Duration::minutes(minutes)
}

fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

fn span(phase: Phase, from: i64, to: i64) -> PhaseSpan {
    PhaseSpan {
        phase,
        started_at: at(from),
        ended_at: Some(at(to)),
    }
}

fn stage(id: &str, from: i64, to: i64, phases: Vec<PhaseSpan>) -> StageStats {
    StageStats {
        id: id.to_string(),
        name: id.to_string(),
        stage_type: "standard".to_string(),
        status: "completed".to_string(),
        started_at: Some(at(from)),
        completed_at: Some(at(to)),
        execution_secs: (to - from) * 60,
        model: Some("opus".to_string()),
        effort: Some("high".to_string()),
        phases,
        tokens: usage(10, 20, 300, 40),
    }
}

fn skipped(id: &str) -> StageStats {
    StageStats {
        status: "skipped".to_string(),
        started_at: None,
        completed_at: None,
        execution_secs: 0,
        phases: Vec::new(),
        ..stage(id, 0, 0, Vec::new())
    }
}

/// Seven named stages, one skipped, one with every phase, plus `extra` filler.
fn record(extra: usize) -> PlanStatsRecord {
    let mut stages = vec![
        stage("alpha", 0, 10, vec![span(Phase::Contract, 0, 10)]),
        stage(
            "beta",
            10,
            60,
            vec![
                span(Phase::Contract, 10, 15),
                span(Phase::Executing, 15, 30),
                span(Phase::Waiting, 30, 35),
                span(Phase::Adjudication, 35, 40),
                span(Phase::Retry, 40, 60),
            ],
        ),
        skipped("gamma"),
        stage("delta", 50, 100, vec![span(Phase::Executing, 50, 100)]),
    ];
    for index in 0..extra {
        let id = format!("filler-{index:02}");
        stages.push(stage(&id, 20, 30, vec![span(Phase::Executing, 20, 30)]));
    }
    PlanStatsRecord {
        schema_version: 1,
        plan_id: "20261121T090000Z-demo-0b8f5d2e".to_string(),
        plan_name: "Open GitHub Issues".to_string(),
        started_at: at(0),
        ended_at: at(PLAN_MINUTES),
        wall_secs: PLAN_MINUTES * 60,
        execution_secs: 9_000,
        totals: usage(1_210_000, 6_470_000, 182_600_000, 2_160_000),
        stages,
        models: vec![
            ModelStats {
                model: "sonnet".to_string(),
                effort: None,
                lane: "claude".to_string(),
                agents: 2,
                tokens: usage(100_000, 200_000, 9_000_000, 300_000),
            },
            ModelStats {
                model: "opus".to_string(),
                effort: Some("high".to_string()),
                lane: "claude".to_string(),
                agents: 5,
                tokens: usage(1_000_000, 6_000_000, 170_000_000, 1_800_000),
            },
        ],
    }
}

fn draw(
    record: &PlanStatsRecord,
    width: u16,
    height: u16,
    scroll_y: u16,
) -> (Vec<String>, Metrics) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut metrics = None;
    terminal
        .draw(|frame| {
            let area = frame.area();
            metrics = Some(render_plan_overview(frame, area, record, scroll_y));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows = buffer
        .content()
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect();
    (rows, metrics.unwrap())
}

fn screen(rows: &[String]) -> String {
    rows.join("\n")
}

fn row_of<'a>(rows: &'a [String], needle: &str) -> &'a str {
    rows.iter()
        .find(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle}:\n{}", screen(rows)))
}

fn assert_full_report(record: &PlanStatsRecord, width: u16, height: u16) {
    let (rows, metrics) = draw(record, width, height, 0);
    let text = screen(&rows);
    for needle in [
        "PLAN COMPLETE",
        "Open GitHub Issues",
        "wall 1h40m",
        "exec 2h30m",
        "TIMELINE",
        "contract",
        "adjudication",
        "retry",
        "waiting",
        "MODELS",
        "opus/high",
        "sonnet",
        "stats",
        "loom init: next plan · loom clean --state: clear it · q quits",
    ] {
        assert!(
            text.contains(needle),
            "{needle} missing at {width}x{height}:\n{text}"
        );
    }
    let counts = format!("{} completed · 1 skipped", record.completed_count());
    assert!(text.contains(&counts), "{counts} missing:\n{text}");
    let hit = format!("hit {}%", record.totals.cache_hit_percent().unwrap());
    assert!(text.contains(&hit), "{hit} missing:\n{text}");
    for stage in &record.stages {
        assert!(
            text.contains(stage.id.as_str()),
            "{} missing at {width}x{height}:\n{text}",
            stage.id
        );
    }
    assert!(
        metrics.total_lines <= metrics.viewport,
        "nothing should need scrolling at {width}x{height}: {metrics:?}"
    );
}

#[test]
fn the_report_fits_a_wide_terminal() {
    assert_full_report(&record(10), 120, 40);
}

#[test]
fn the_report_fits_a_narrow_terminal() {
    assert_full_report(&record(0), 70, 24);
}

#[test]
fn a_skipped_stage_shows_skipped_and_no_bar() {
    let (rows, _) = draw(&record(0), 120, 40, 0);
    let row = row_of(&rows, "gamma");
    assert!(row.contains("┄ skipped"), "{row}");
    assert!(!row.contains('█') && !row.contains('░'), "{row}");
}

#[test]
fn every_phase_has_its_glyph_and_gaps_read_as_waiting() {
    let (rows, _) = draw(&record(0), 120, 40, 0);
    let beta = row_of(&rows, "beta");
    for glyph in ['▓', '█', '▒', '░'] {
        assert!(beta.contains(glyph), "{glyph} missing: {beta}");
    }
    let alpha = row_of(&rows, "alpha");
    assert!(alpha.contains('▓') && !alpha.contains('█'), "{alpha}");
}

#[test]
fn bars_are_scaled_over_the_plan_span() {
    let lines: Vec<String> = timeline_lines(&record(0), 120)
        .iter()
        .map(ToString::to_string)
        .collect();
    let bar_of = |id: &str| {
        let line = lines.iter().find(|line| line.contains(id)).unwrap();
        let start = line.chars().position(|c| "▓█▒░".contains(c)).unwrap();
        (start, line.chars().filter(|c| "▓█▒░".contains(*c)).count())
    };
    let (alpha_at, alpha_cells) = bar_of("alpha");
    let (delta_at, delta_cells) = bar_of("delta");
    // Delta runs the second half of the plan, to the end of the bar.
    let bar = delta_at - alpha_at + delta_cells;
    assert!(delta_at - alpha_at + 1 >= bar / 2 && delta_at - alpha_at <= bar / 2 + 1);
    // Alpha runs the first tenth.
    assert!(
        alpha_cells * 10 + 10 >= bar && alpha_cells * 10 <= bar + 10,
        "{alpha_cells} of {bar}"
    );
}

#[test]
fn bars_and_rows_stay_inside_the_area() {
    let record = record(3);
    for width in 30..=140u16 {
        for line in timeline_lines(&record, width) {
            assert!(
                line.width() <= usize::from(width),
                "{} cells wide at {width}: {line}",
                line.width()
            );
        }
        let (rows, _) = draw(&record, width, 30, 0);
        assert!(rows.iter().all(|row| text_width(row) <= usize::from(width)));
    }
}

#[test]
fn a_long_stage_id_is_cut_with_an_ellipsis_on_a_narrow_terminal() {
    let mut record = record(0);
    record.stages[0].id = "an-extraordinarily-long-stage-identifier".to_string();
    let (rows, _) = draw(&record, 50, 30, 0);
    let row = rows.iter().find(|row| row.contains("an-extra")).unwrap();
    assert!(row.contains('…'), "{row}");
    assert!(!row.contains("identifier"), "{row}");
}

#[test]
fn overflow_rows_are_reachable_by_scrolling() {
    let record = record(40);
    let (first, metrics) = draw(&record, 70, 24, 0);
    assert!(metrics.total_lines > metrics.viewport);
    let last = &record.stages.last().unwrap().id;
    assert!(!screen(&first).contains(last.as_str()));
    assert!(screen(&first).contains("rows 1-"), "{}", screen(&first));

    let end = metrics.total_lines - metrics.viewport;
    let (last_rows, scrolled) = draw(&record, 70, 24, end);
    assert_eq!(scrolled.scroll_y, end);
    assert!(screen(&last_rows).contains(last.as_str()));
    assert!(screen(&last_rows).contains("MODELS"));
    // The footer stays on screen at every offset.
    assert!(screen(&last_rows).contains("q quits"));
    assert!(screen(&first).contains("q quits"));

    let (past, clamped) = draw(&record, 70, 24, u16::MAX);
    assert_eq!(clamped.scroll_y, end);
    assert_eq!(past, last_rows);
}

#[test]
fn below_the_pinned_height_the_footer_ends_the_scrolling_body() {
    let record = record(5);
    let (first, metrics) = draw(&record, 120, 7, 0);
    assert!(!screen(&first).contains("q quits"));
    assert_eq!(metrics.viewport, 7);
    let end = metrics.total_lines - metrics.viewport;
    let (last, scrolled) = draw(&record, 120, 7, u16::MAX);
    assert_eq!(scrolled.scroll_y, end);
    let text = screen(&last);
    assert!(
        text.contains("saved ") && text.contains("q quits"),
        "{text}"
    );
}

#[test]
fn a_tiny_terminal_draws_without_a_pinned_footer_and_does_not_panic() {
    let record = record(5);
    for (width, height) in [(1, 1), (10, 3), (30, 7), (20, 8), (200, 2)] {
        let (_, metrics) = draw(&record, width, height, 3);
        assert!(metrics.scroll_y <= metrics.total_lines);
    }
}

#[test]
fn a_plan_without_models_or_skips_omits_those_parts() {
    let mut record = record(0);
    record.models.clear();
    record.stages.retain(|stage| stage.status != "skipped");
    let (rows, _) = draw(&record, 100, 30, 0);
    let text = screen(&rows);
    assert!(!text.contains("MODELS"));
    assert!(text.contains("3 completed") && !text.contains("skipped"));
}

#[test]
fn models_are_ordered_by_fresh_tokens_with_their_share() {
    let (rows, _) = draw(&record(0), 120, 40, 0);
    let opus = rows
        .iter()
        .position(|row| row.contains("opus/high"))
        .unwrap();
    let sonnet = rows.iter().position(|row| row.contains("sonnet")).unwrap();
    assert!(opus < sonnet);
    assert!(
        rows[opus].contains('▇') && rows[opus].contains("94%"),
        "{}",
        rows[opus]
    );
    assert!(
        rows[opus].contains("170.00M") && rows[opus].contains("  5 "),
        "{}",
        rows[opus]
    );
}

#[test]
fn kept_tails_read_compactly() {
    assert_eq!(keep_tail("abcdefgh", 5), "…efgh");
    assert_eq!(keep_tail("abc", 5), "abc");
    assert_eq!(keep_tail("abc", 0), "");
}
