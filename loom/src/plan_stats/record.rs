//! The finished-plan record: what the plan overview shows and the archive keeps.
//!
//! The field names are the wire contract of `web/src/api/schema.ts`
//! (`planStatsSchema`): every field is public, nullable values serialize as an
//! explicit `null`, and an unknown key is refused on read.

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::stage::phases::PhaseSpan;
use crate::tokens::TokenUsage;

/// The `schema_version` this build writes and reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Largest encoded record a read accepts and a publish writes. The status frame
/// that carries a record is capped at 2 MiB (`daemon/wire.rs`), so a record of
/// this size always fits a frame on its own.
pub const MAX_RECORD_BYTES: usize = 1024 * 1024;

/// One finished plan: wall and execution time, token totals, one row per stage
/// and one per (model, effort, lane).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStatsRecord {
    pub schema_version: u32,
    /// `<init UTC %Y%m%dT%H%M%SZ>-<source slug>-<run uuid>`, fixed when the plan
    /// is initialized; the archive file is named after it.
    pub plan_id: String,
    pub plan_name: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub wall_secs: i64,
    /// Sum of the stages' execution counters; above `wall_secs` when stages ran
    /// in parallel.
    pub execution_secs: i64,
    pub totals: TokenUsage,
    pub stages: Vec<StageStats>,
    pub models: Vec<ModelStats>,
}

/// One stage of the plan, as its persisted state left it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageStats {
    pub id: String,
    pub name: String,
    /// The stage type's serialized name, e.g. `standard`.
    pub stage_type: String,
    /// The stage status's serialized name, e.g. `completed` or `skipped`.
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub execution_secs: i64,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub phases: Vec<PhaseSpan>,
    pub tokens: TokenUsage,
}

/// Usage of one model at one reasoning effort on one lane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelStats {
    pub model: String,
    pub effort: Option<String>,
    /// `claude` or `codex`.
    pub lane: String,
    /// Distinct agents that used tokens here.
    pub agents: u32,
    pub tokens: TokenUsage,
}

impl PlanStatsRecord {
    /// The record as the pretty JSON the archive and the current copy hold.
    pub(super) fn to_json(&self) -> anyhow::Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).context("cannot encode the plan stats record")
    }

    /// Parse a stored record, refusing a schema version this build does not know.
    pub(super) fn from_json(bytes: &[u8]) -> anyhow::Result<Self> {
        let record: Self =
            serde_json::from_slice(bytes).context("cannot parse the plan stats record")?;
        anyhow::ensure!(
            record.schema_version == SCHEMA_VERSION,
            "plan stats schema version {} is not {SCHEMA_VERSION}",
            record.schema_version
        );
        Ok(record)
    }

    /// Stages whose persisted status is `completed`.
    pub fn completed_count(&self) -> usize {
        self.stages
            .iter()
            .filter(|s| s.status == "completed")
            .count()
    }

    /// Stages whose persisted status is `skipped`.
    pub fn skipped_count(&self) -> usize {
        self.stages.iter().filter(|s| s.status == "skipped").count()
    }
}

/// Fresh plus cache-read tokens, saturating: the "all tokens" figure.
pub fn all_tokens(usage: &TokenUsage) -> u64 {
    usage.fresh().saturating_add(usage.cache_read)
}
