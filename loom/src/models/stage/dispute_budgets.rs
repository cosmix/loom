//! Per-kind dispute budgets (DESIGN D15): each kind of dispute a stage files
//! draws on its own counter, and a stage may file at most
//! [`MAX_DISPUTES_PER_KIND`] of each. An exhausted budget escalates the stage
//! to `NeedsHumanReview` instead of filing.

use serde::{Deserialize, Serialize};

use super::types::Stage;
use crate::models::dispute::DisputeKind;

/// Disputes of one kind a single stage may file before further ones are
/// refused.
pub const MAX_DISPUTES_PER_KIND: u32 = 3;

/// The adjudication counters a stage keeps beside `dispute_count`: rounds
/// of evidence answered, amendments applied, and the plan v2 disputes filed
/// per kind. Flattened into `Stage`, so each stays a top-level key of the
/// stage file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisputeTally {
    /// Number of evidence-loop rounds (NeedsMoreEvidence -> Executing -> NeedsAdjudication).
    #[serde(default)]
    pub evidence_rounds: u32,
    /// Number of accepted plan amendments applied for this stage.
    #[serde(default)]
    pub amendments_applied: u32,
    /// Plan v2 findings, contract and integrity disputes filed, each capped by
    /// `dispute_budgets`.
    #[serde(default)]
    pub finding_disputes: u32,
    #[serde(default)]
    pub contract_disputes: u32,
    #[serde(default)]
    pub integrity_disputes: u32,
}

impl Stage {
    /// Maximum number of criterion disputes a stage may file before further
    /// dispute requests are refused. See `dispute_budget_exhausted`.
    pub fn max_disputes_per_stage(&self) -> u32 {
        MAX_DISPUTES_PER_KIND
    }

    /// True when `dispute_count` has reached `max_disputes_per_stage`.
    pub fn dispute_budget_exhausted(&self) -> bool {
        self.dispute_count >= self.max_disputes_per_stage()
    }
}

/// Disputes of `kind`'s kind the stage has filed.
pub fn filed(stage: &Stage, kind: &DisputeKind) -> u32 {
    match kind {
        DisputeKind::Criterion { .. } => stage.dispute_count,
        DisputeKind::Findings { .. } => stage.tally.finding_disputes,
        DisputeKind::Contract { .. } => stage.tally.contract_disputes,
        DisputeKind::Integrity { .. } => stage.tally.integrity_disputes,
    }
}

/// True when the stage has filed [`MAX_DISPUTES_PER_KIND`] disputes of
/// `kind`'s kind.
pub fn exhausted(stage: &Stage, kind: &DisputeKind) -> bool {
    filed(stage, kind) >= MAX_DISPUTES_PER_KIND
}

/// Count one more dispute of `kind`'s kind against the stage.
pub fn spend(stage: &mut Stage, kind: &DisputeKind) {
    let counter = match kind {
        DisputeKind::Criterion { .. } => &mut stage.dispute_count,
        DisputeKind::Findings { .. } => &mut stage.tally.finding_disputes,
        DisputeKind::Contract { .. } => &mut stage.tally.contract_disputes,
        DisputeKind::Integrity { .. } => &mut stage.tally.integrity_disputes,
    };
    *counter = counter.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> DisputeKind {
        DisputeKind::Contract {
            contract_id: "rejects-x".to_string(),
        }
    }

    #[test]
    fn each_kind_spends_only_its_own_budget() {
        let mut stage = Stage::default();
        for _ in 0..MAX_DISPUTES_PER_KIND {
            assert!(!exhausted(&stage, &contract()));
            spend(&mut stage, &contract());
        }
        assert!(exhausted(&stage, &contract()));
        assert_eq!(stage.tally.contract_disputes, MAX_DISPUTES_PER_KIND);
        let criterion = DisputeKind::Criterion { criterion_index: 0 };
        assert!(!exhausted(&stage, &criterion));
        assert!(!stage.dispute_budget_exhausted());
        assert_eq!(
            (stage.tally.finding_disputes, stage.tally.integrity_disputes),
            (0, 0)
        );
    }

    #[test]
    fn a_criterion_dispute_spends_the_dispute_count() {
        let mut stage = Stage::default();
        spend(&mut stage, &DisputeKind::Criterion { criterion_index: 1 });
        assert_eq!(stage.dispute_count, 1);
    }

    /// `#[serde(flatten)]` must keep each tally counter a top-level key of the
    /// stage file's YAML frontmatter (the same `serde_yaml::to_string` call
    /// `serialize_stage_to_markdown` uses), not nested under a `tally:` map —
    /// existing stage files on disk have no such wrapper key.
    #[test]
    fn tally_keys_stay_top_level_in_the_stage_file() {
        let stage = Stage {
            tally: DisputeTally {
                evidence_rounds: 2,
                finding_disputes: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&stage).expect("stage serializes");
        assert!(
            yaml.lines().any(|l| l == "finding_disputes: 1"),
            "finding_disputes must be a top-level key:\n{yaml}",
        );
        assert!(
            yaml.lines().any(|l| l == "evidence_rounds: 2"),
            "evidence_rounds must be a top-level key:\n{yaml}",
        );

        let reloaded: Stage = serde_yaml::from_str(&yaml).expect("stage deserializes");
        assert_eq!(reloaded.tally, stage.tally);
    }
}
