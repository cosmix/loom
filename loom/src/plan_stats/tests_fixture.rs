//! Shared fixtures of the plan stats tests: a two-lane token total, stages with and
//! without timing, and an initialized work directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, TimeZone, Utc};
use uuid::Uuid;

use super::identity::RunIdentity;
use crate::models::stage::phases::{Phase, PhaseSpan};
use crate::models::stage::{Stage, StageStatus};
use crate::tokens::plan_totals::{ModelUsage, PlanTokens};
use crate::tokens::TokenUsage;
use crate::verify::transitions::create_stage;

pub const RUN_ID: &str = "0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10";
pub const PLAN_ID: &str = "20261121T080000Z-plan-demo-0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10";

/// `[plan]` table as `loom init` writes it, with a complete run identity.
pub const PLAN_TABLE: &str = "[plan]\n\
source_path = \"doc/plans/IN_PROGRESS-PLAN-demo.md\"\n\
plan_id = \"demo\"\n\
plan_name = \"Demo plan\"\n\
base_branch = \"main\"\n\
stats_initialized_at = \"2026-11-21T08:00:00Z\"\n\
stats_source_slug = \"plan-demo\"\n\
stats_run_id = \"0b8f5d2e-4c1a-4a57-9f6e-2d7c1b3a9e10\"\n";

/// Nine o'clock, `secs` seconds on.
pub fn at(secs: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).unwrap() + Duration::seconds(secs)
}

pub fn run() -> RunIdentity {
    RunIdentity::new(at(-3600), "plan-demo", Uuid::parse_str(RUN_ID).unwrap())
}

pub fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

pub fn stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Stage {id}"),
        status,
        updated_at: at(0),
        ..Stage::default()
    }
}

/// A stage that ran from `start` to `end` seconds after nine with one executing span.
pub fn finished(id: &str, start: i64, end: i64, execution_secs: i64) -> Stage {
    Stage {
        started_at: Some(at(start)),
        completed_at: Some(at(end)),
        execution_secs: Some(execution_secs),
        phases: vec![PhaseSpan {
            phase: Phase::Executing,
            started_at: at(start),
            ended_at: Some(at(end)),
        }],
        ..stage(id, StageStatus::Completed)
    }
}

/// Two stages' usage on the claude lane (two agents on opus) and the codex lane (one agent).
pub fn two_lane_tokens() -> PlanTokens {
    let claude = usage(1_000, 2_000, 30_000, 4_000);
    let codex = usage(500, 0, 5_000, 700);
    let mut totals = claude;
    totals.add(&codex);
    PlanTokens {
        totals,
        per_stage: BTreeMap::from([("build".to_string(), claude), ("port".to_string(), codex)]),
        models: vec![
            ModelUsage {
                model: "opus".to_string(),
                effort: Some("high".to_string()),
                lane: "claude".to_string(),
                agents: 2,
                tokens: claude,
            },
            ModelUsage {
                model: "gpt-6".to_string(),
                effort: None,
                lane: "codex".to_string(),
                agents: 1,
                tokens: codex,
            },
        ],
    }
}

/// A state directory holding `PLAN_TABLE` and `stages`.
pub fn work_dir(root: &Path, plan_table: &str, stages: &[Stage]) -> PathBuf {
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("config.toml"), plan_table).unwrap();
    for stage in stages {
        create_stage(stage, &work_dir).unwrap();
    }
    work_dir
}
