//! The inline plan report is dropped, and nothing else, when it would push a
//! status frame past `MAX_RESPONSE_BYTES`.

use super::*;
use crate::models::stage::phases::{Phase, PhaseSpan};
use crate::plan_stats::{ModelStats, PlanStatsRecord, StageStats};
use crate::status::data::{StageSummary, StatusData};
use crate::tokens::TokenUsage;
use chrono::{TimeZone, Utc};
use std::io::Cursor;

fn record() -> PlanStatsRecord {
    let start = Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).unwrap();
    let usage = TokenUsage {
        input: 10,
        cache_write: 20,
        cache_read: 30,
        output: 40,
    };
    PlanStatsRecord {
        schema_version: 1,
        plan_id: "20261121T080000Z-plan-demo-0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10".to_string(),
        plan_name: "Demo plan".to_string(),
        started_at: start,
        ended_at: start + chrono::Duration::seconds(60),
        wall_secs: 60,
        execution_secs: 50,
        totals: usage,
        stages: vec![StageStats {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            stage_type: "standard".to_string(),
            status: "completed".to_string(),
            started_at: Some(start),
            completed_at: Some(start + chrono::Duration::seconds(60)),
            execution_secs: 50,
            model: Some("opus".to_string()),
            effort: Some("high".to_string()),
            phases: vec![PhaseSpan {
                phase: Phase::Executing,
                started_at: start,
                ended_at: Some(start + chrono::Duration::seconds(60)),
            }],
            tokens: usage,
        }],
        models: vec![ModelStats {
            model: "opus".to_string(),
            effort: Some("high".to_string()),
            lane: "claude".to_string(),
            agents: 1,
            tokens: usage,
        }],
    }
}

/// A status update whose one stage name is `padding` bytes long.
fn update(padding: usize, plan_stats: Option<PlanStatsRecord>) -> Response {
    Response::StatusUpdate {
        data: Box::new(StatusData {
            stages: vec![StageSummary {
                id: "alpha".to_string(),
                name: "n".repeat(padding),
                ..Default::default()
            }],
            plan_name: Some("Demo plan".to_string()),
            plan_stats,
            ..Default::default()
        }),
    }
}

fn json_len(response: &Response) -> usize {
    serde_json::to_vec(response).unwrap().len()
}

fn decode(frame: &[u8]) -> StatusData {
    match Response::read_wire(&mut Cursor::new(frame)).unwrap() {
        Response::StatusUpdate { data } => *data,
        other => panic!("expected a status update, got {other:?}"),
    }
}

/// The padding that makes the full response exactly `target` bytes of JSON.
fn padding_for(target: usize) -> usize {
    target - json_len(&update(0, Some(record())))
}

#[test]
fn a_frame_of_exactly_the_limit_keeps_the_inline_report() {
    let response = update(padding_for(MAX_RESPONSE_BYTES), Some(record()));
    assert_eq!(json_len(&response), MAX_RESPONSE_BYTES);

    let data = decode(&encode_response_frame(&response).unwrap());

    assert_eq!(data.plan_stats, Some(record()));
}

#[test]
fn a_frame_one_byte_over_the_limit_drops_only_the_inline_report() {
    let padding = padding_for(MAX_RESPONSE_BYTES + 1);
    let response = update(padding, Some(record()));
    assert_eq!(json_len(&response), MAX_RESPONSE_BYTES + 1);

    let frame = encode_response_frame(&response).unwrap();
    let data = decode(&frame);

    assert!(data.plan_stats.is_none());
    assert_eq!(data.stages.len(), 1);
    assert_eq!(data.stages[0].id, "alpha");
    assert_eq!(data.stages[0].name.len(), padding);
    assert_eq!(data.plan_name.as_deref(), Some("Demo plan"));
    assert!(frame.len() - 4 <= MAX_RESPONSE_BYTES);
}

#[test]
fn a_record_alone_larger_than_the_frame_still_lets_the_status_flow() {
    let mut huge = record();
    huge.plan_name = "p".repeat(MAX_RESPONSE_BYTES);

    let data = decode(&encode_response_frame(&update(10, Some(huge))).unwrap());

    assert!(data.plan_stats.is_none());
    assert_eq!(data.stages[0].name.len(), 10);
}

#[test]
fn a_frame_that_overflows_without_the_report_is_still_refused() {
    let response = update(MAX_RESPONSE_BYTES, Some(record()));

    let error = encode_response_frame(&response).unwrap_err();

    assert!(error.to_string().contains("exceeds"), "{error:#}");
}

#[test]
fn an_oversized_update_without_a_report_is_refused_as_before() {
    assert!(encode_response_frame(&update(MAX_RESPONSE_BYTES, None)).is_err());
}
