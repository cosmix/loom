//! The plan overview's share of the wire fixtures: stage phases and a completion
//! time in `snapshot.json`, and a snapshot carrying the finished plan's report
//! in `plan-stats-snapshot.json`, both built from serde output.

use super::{fixture_snapshot, WebSnapshot};
use crate::commands::status::data::StageSummary;
use crate::commands::status::web::feed::Feed;
use crate::models::stage::phases::{Phase, PhaseSpan};
use crate::plan_stats::{ModelStats, PlanStatsRecord, StageStats};
use crate::tokens::TokenUsage;
use chrono::{DateTime, Utc};
use std::time::Instant;

const PLAN_STATS_FIXTURE: &str =
    include_str!("../../../../../web/src/api/fixtures/plan-stats-snapshot.json");

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

fn span(phase: Phase, started: &str, ended: &str) -> PhaseSpan {
    PhaseSpan {
        phase,
        started_at: at(started),
        ended_at: Some(at(ended)),
    }
}

/// Gives the fixture stage two phase spans and the time it finished.
pub(super) fn complete_with_phases(stage: &mut StageSummary) {
    assert_eq!(stage.id, "client", "the fixture stage that carries phases");
    stage.phases = vec![
        span(
            Phase::Executing,
            "2026-09-04T11:48:20Z",
            "2026-09-04T11:56:00Z",
        ),
        span(Phase::Retry, "2026-09-04T11:56:00Z", "2026-09-04T12:00:00Z"),
    ];
    stage.completed_at = Some(at("2026-09-04T12:00:00Z"));
}

fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

fn skipped_stage() -> StageStats {
    StageStats {
        id: "docs".to_owned(),
        name: "Documentation".to_owned(),
        stage_type: "standard".to_owned(),
        status: "skipped".to_owned(),
        started_at: None,
        completed_at: None,
        execution_secs: 0,
        model: None,
        effort: None,
        phases: Vec::new(),
        tokens: usage(0, 0, 0, 0),
    }
}

fn fixture_record() -> PlanStatsRecord {
    let server = usage(1_000, 3_000, 50_000, 700);
    PlanStatsRecord {
        schema_version: 1,
        plan_id: "20260904T110000Z-web-dashboard-fixture-0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10"
            .to_owned(),
        plan_name: "Web Dashboard Fixture".to_owned(),
        started_at: at("2026-09-04T11:00:00Z"),
        ended_at: at("2026-09-04T12:00:00Z"),
        wall_secs: 3_600,
        execution_secs: 2_400,
        totals: server,
        stages: vec![
            StageStats {
                id: "server".to_owned(),
                name: "Rust server".to_owned(),
                stage_type: "standard".to_owned(),
                status: "completed".to_owned(),
                started_at: Some(at("2026-09-04T11:00:00Z")),
                completed_at: Some(at("2026-09-04T11:40:00Z")),
                execution_secs: 2_400,
                model: Some("opus".to_owned()),
                effort: Some("high".to_owned()),
                phases: vec![
                    span(
                        Phase::Contract,
                        "2026-09-04T11:00:00Z",
                        "2026-09-04T11:10:00Z",
                    ),
                    span(
                        Phase::Executing,
                        "2026-09-04T11:10:00Z",
                        "2026-09-04T11:40:00Z",
                    ),
                ],
                tokens: server,
            },
            skipped_stage(),
        ],
        models: vec![ModelStats {
            model: "opus".to_owned(),
            effort: Some("high".to_owned()),
            lane: "claude".to_owned(),
            agents: 2,
            tokens: server,
        }],
    }
}

fn with_report() -> WebSnapshot {
    let mut snapshot = fixture_snapshot();
    snapshot.status.plan_stats = Some(fixture_record());
    snapshot
}

fn kinds(frames: &[std::sync::Arc<String>]) -> Vec<String> {
    frames
        .iter()
        .map(|frame| {
            let value: serde_json::Value = serde_json::from_str(frame).unwrap();
            value["t"].as_str().unwrap().to_owned()
        })
        .collect()
}

#[test]
fn plan_stats_fixture_matches_serde_output() {
    let actual = serde_json::to_value(with_report()).unwrap();
    let expected = serde_json::from_str::<serde_json::Value>(PLAN_STATS_FIXTURE).unwrap();
    assert_eq!(
        actual,
        expected,
        "fixture out of date; expected:\n{}",
        serde_json::to_string_pretty(&actual).unwrap()
    );
}

#[test]
fn plan_stats_fixture_deserializes_and_reserializes_unchanged() {
    let snapshot = serde_json::from_str::<WebSnapshot>(PLAN_STATS_FIXTURE).unwrap();
    assert_eq!(snapshot.status.plan_stats, Some(fixture_record()));
    let again = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(
        again,
        serde_json::from_str::<serde_json::Value>(PLAN_STATS_FIXTURE).unwrap()
    );
}

#[test]
fn the_report_reaches_a_full_frame_whole() {
    let mut feed = Feed::default();
    let frames = feed.accept(with_report(), Instant::now()).unwrap();
    assert_eq!(kinds(&frames), ["full"]);
    let frame: serde_json::Value = serde_json::from_str(&frames[0]).unwrap();
    assert_eq!(
        frame["snapshot"],
        serde_json::from_str::<serde_json::Value>(PLAN_STATS_FIXTURE).unwrap()
    );
}

#[test]
fn the_report_appearing_or_going_away_is_sent_as_a_full_frame() {
    let now = Instant::now();
    let mut feed = Feed::default();
    feed.accept(fixture_snapshot(), now).unwrap();

    let appeared = feed.accept(with_report(), now).unwrap();
    assert_eq!(kinds(&appeared), ["full"]);

    let steady = feed.accept(with_report(), now).unwrap();
    assert!(!kinds(&steady).contains(&"full".to_owned()), "{steady:?}");

    let removed = feed.accept(fixture_snapshot(), now).unwrap();
    assert_eq!(kinds(&removed), ["full"]);
}
