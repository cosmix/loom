//! What a record holds: totals, cache hit, durations, agents and option fidelity.

use super::build::assemble;
use super::record::{PlanStatsRecord, MAX_RECORD_BYTES};
use super::tests_fixture::*;
use crate::models::stage::phases::{Phase, PhaseSpan};
use crate::models::stage::StageStatus;
use crate::tokens::plan_totals::PlanTokens;
use crate::tokens::share_percent;

fn plan_record() -> PlanStatsRecord {
    let mut build = finished("build", 0, 600, 500);
    build.model = Some("opus".to_string());
    build.reasoning_effort = Some("high".to_string());
    let port = finished("port", 100, 1_200, 900);
    let mut docs = stage("docs", StageStatus::Skipped);
    docs.updated_at = at(1_300);
    assemble(
        &run(),
        "Demo plan".to_string(),
        vec![docs, port, build],
        &two_lane_tokens(),
    )
    .unwrap()
}

#[test]
fn the_totals_are_the_two_lanes_summed() {
    let record = plan_record();
    assert_eq!(record.totals, usage(1_500, 2_000, 35_000, 4_700));
    assert_eq!(record.totals.fresh(), 8_200);
    assert_eq!(super::all_tokens(&record.totals), 43_200);
    let by_stage = record
        .stages
        .iter()
        .fold(usage(0, 0, 0, 0), |mut sum, stage| {
            sum.add(&stage.tokens);
            sum
        });
    assert_eq!(by_stage, record.totals);
    let by_model = record
        .models
        .iter()
        .fold(usage(0, 0, 0, 0), |mut sum, model| {
            sum.add(&model.tokens);
            sum
        });
    assert_eq!(by_model, record.totals);
}

#[test]
fn the_cache_hit_is_the_ledgers_input_side_share() {
    assert_eq!(plan_record().totals.cache_hit_percent(), Some(91));
    assert_eq!(
        usage(1, 0, 1, 9).cache_hit_percent(),
        Some(50),
        "half rounds up"
    );
    assert_eq!(usage(2, 0, 1, 0).cache_hit_percent(), Some(33));
    assert_eq!(usage(1, 0, 2, 0).cache_hit_percent(), Some(67));
    assert_eq!(
        usage(0, 0, 0, 77).cache_hit_percent(),
        None,
        "no input side, no figure"
    );
}

#[test]
fn a_share_rounds_half_up_and_has_no_figure_for_an_empty_whole() {
    assert_eq!(share_percent(1, 3), Some(33));
    assert_eq!(share_percent(2, 3), Some(67));
    assert_eq!(share_percent(1, 2), Some(50));
    assert_eq!(share_percent(5, 0), None);
}

#[test]
fn wall_time_runs_from_the_first_start_to_the_last_end_skips_included() {
    let record = plan_record();
    assert_eq!(record.started_at, at(0));
    assert_eq!(
        record.ended_at,
        at(1_300),
        "the skip transition ends the plan"
    );
    assert_eq!(record.wall_secs, 1_300);
    assert_eq!(
        record.execution_secs, 1_400,
        "500 + 900 + a skipped stage's 0"
    );
}

#[test]
fn wall_time_ends_at_a_merge_wait_that_outlasts_every_completion() {
    let mut held = finished("held", 0, 600, 500);
    held.phases.push(PhaseSpan {
        phase: Phase::Waiting,
        started_at: at(601),
        ended_at: Some(at(5_000)),
    });
    let record = assemble(&run(), "Held".to_string(), vec![held], &two_lane_tokens()).unwrap();
    assert_eq!(record.ended_at, at(5_000));
    assert_eq!(record.wall_secs, 5_000);
}

#[test]
fn stages_keep_their_own_times_and_options_as_persisted() {
    let record = plan_record();
    let ids: Vec<&str> = record
        .stages
        .iter()
        .map(|stage| stage.id.as_str())
        .collect();
    assert_eq!(ids, ["build", "port", "docs"], "by start, unstarted last");
    let build = &record.stages[0];
    assert_eq!(
        (build.started_at, build.completed_at),
        (Some(at(0)), Some(at(600)))
    );
    assert_eq!(
        (build.execution_secs, build.status.as_str()),
        (500, "completed")
    );
    assert_eq!(build.stage_type, "standard");
    assert_eq!(build.model.as_deref(), Some("opus"));
    assert_eq!(build.effort.as_deref(), Some("high"));
    assert_eq!(build.phases.len(), 1);
    assert_eq!(build.tokens, usage(1_000, 2_000, 30_000, 4_000));
    let port = &record.stages[1];
    assert_eq!((port.model.clone(), port.effort.clone()), (None, None));
    assert_eq!(port.tokens, usage(500, 0, 5_000, 700));
    let docs = &record.stages[2];
    assert_eq!(docs.status, "skipped");
    assert_eq!((docs.started_at, docs.completed_at), (None, None));
    assert_eq!(
        docs.tokens,
        usage(0, 0, 0, 0),
        "a stage without usage reads zero"
    );
}

#[test]
fn models_carry_the_distinct_agent_count_and_lane() {
    let record = plan_record();
    let rows: Vec<(&str, Option<&str>, &str, u32)> = record
        .models
        .iter()
        .map(|m| {
            (
                m.model.as_str(),
                m.effort.as_deref(),
                m.lane.as_str(),
                m.agents,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("opus", Some("high"), "claude", 2),
            ("gpt-6", None, "codex", 1)
        ]
    );
}

#[test]
fn nullable_fields_serialize_as_explicit_null() {
    let value = serde_json::to_value(plan_record()).unwrap();
    let docs = &value["stages"][2];
    assert!(docs["started_at"].is_null() && docs["completed_at"].is_null());
    assert!(docs["model"].is_null() && docs["effort"].is_null());
    assert!(value["models"][1]["effort"].is_null());
    assert_eq!(docs["phases"], serde_json::json!([]));
}

#[test]
fn a_plan_where_nothing_ran_has_the_init_time_and_no_duration() {
    let stages = vec![
        stage("a", StageStatus::Skipped),
        stage("b", StageStatus::Skipped),
    ];
    let record = assemble(
        &run(),
        "All skipped".to_string(),
        stages,
        &PlanTokens::default(),
    )
    .unwrap();
    assert_eq!((record.started_at, record.ended_at), (at(-3600), at(-3600)));
    assert_eq!((record.wall_secs, record.execution_secs), (0, 0));
    assert_eq!(record.totals, usage(0, 0, 0, 0));
    assert!(record.models.is_empty());
}

#[test]
fn an_incomplete_plan_reports_what_ran_and_the_stage_state() {
    let mut running = stage("run", StageStatus::Executing);
    running.started_at = Some(at(50));
    running.phases = vec![PhaseSpan {
        phase: Phase::Executing,
        started_at: at(50),
        ended_at: None,
    }];
    let done = finished("done", 0, 40, 40);
    let record = assemble(
        &run(),
        "Half".to_string(),
        vec![running, done],
        &PlanTokens::default(),
    )
    .unwrap();
    assert_eq!((record.started_at, record.ended_at), (at(0), at(40)));
    assert_eq!(record.stages[1].status, "executing");
    assert_eq!(record.stages[1].phases[0].ended_at, None);
}

#[test]
fn missing_timing_data_never_breaks_the_record() {
    let mut odd = finished("odd", 0, 10, -5);
    odd.execution_secs = Some(-5);
    odd.completed_at = Some(at(-100));
    let late = stage("late", StageStatus::Completed);
    let record = assemble(
        &run(),
        "Odd".to_string(),
        vec![late, odd],
        &PlanTokens::default(),
    )
    .unwrap();
    assert!(record.wall_secs >= 0 && record.ended_at >= record.started_at);
    assert_eq!(record.execution_secs, 0, "a negative counter reads zero");
    assert_eq!(record.stages[0].id, "odd", "the unstarted stage sorts last");
}

#[test]
fn a_record_over_the_bound_loses_phase_spans_not_totals() {
    let spans: Vec<PhaseSpan> = (0..512)
        .map(|n| PhaseSpan {
            phase: Phase::Waiting,
            started_at: at(n),
            ended_at: Some(at(n + 1)),
        })
        .collect();
    let stages: Vec<_> = (0..16)
        .map(|n| {
            let mut stage = finished(&format!("s{n:02}"), 0, 10, 1);
            stage.phases = spans.clone();
            stage
        })
        .collect();
    let record = assemble(&run(), "Big".to_string(), stages, &two_lane_tokens()).unwrap();
    assert!(record.to_json().unwrap().len() <= MAX_RECORD_BYTES);
    assert!(record.stages.iter().any(|stage| stage.phases.is_empty()));
    assert_eq!(record.totals, two_lane_tokens().totals);
    assert_eq!(record.stages.len(), 16);
}

#[test]
fn unknown_keys_and_other_schema_versions_are_refused() {
    let record = plan_record();
    let mut value = serde_json::to_value(&record).unwrap();
    assert_eq!(
        PlanStatsRecord::from_json(&serde_json::to_vec(&value).unwrap()).unwrap(),
        record
    );
    value["schema_version"] = 2.into();
    assert!(PlanStatsRecord::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    value["schema_version"] = 1.into();
    value["extra"] = true.into();
    assert!(PlanStatsRecord::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
}
