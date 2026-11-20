//! What a criterion narrowing rests on and what it hands off, kept in
//! `.loom/work/reviews/<stage>/`: `base-evidence.json` holds loom's own runs
//! of a stage's criteria at the stage's base (`loom check --at-base`), and
//! `handoffs.json` the tests an accepted amendment left out of the stage's
//! check, with where they run instead.
//!
//! Both files are written through the locked read-modify-write of
//! [`super::verdict_records`]; a re-applied verdict records nothing twice.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::store::{check_version, load_optional, RECORD_VERSION};
use super::verdict_records::update;

/// `base-evidence.json` in the stage's `reviews/` directory.
pub const BASE_EVIDENCE_FILE: &str = "base-evidence.json";
/// `handoffs.json` in the stage's `reviews/` directory.
pub const HANDOFFS_FILE: &str = "handoffs.json";

/// One run of a stage's criterion at the stage's base, as loom observed it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BaseEvidence {
    /// The criterion's index in the stage's `acceptance`.
    pub index: usize,
    /// The command that ran; evidence applies only while the criterion is unchanged.
    pub command: String,
    /// The base commit the run checked out; evidence applies only while it
    /// is still the stage's base, so a run before the stage took in a newer
    /// base says nothing about the current one.
    pub base: String,
    pub passed: bool,
    /// Tests the run executed, when the runner's summary was parsed.
    pub executed: Option<u64>,
    /// Tests that failed, when the runner's summary was parsed.
    pub failed: Option<u64>,
    pub build_failed: bool,
    pub timed_out: bool,
}

/// Tests an accepted amendment left out of a stage's check.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    pub stage: String,
    /// The amended plan field (`acceptance`, `contracts`).
    pub field: String,
    pub index: usize,
    pub tests: Vec<String>,
    /// Where the tests run instead: `integration-verify`, `ci`, or `nowhere`.
    pub runs_at: String,
    pub reason: String,
    pub dispute: String,
    /// True when the base run counted the failing tests; false for a
    /// timed-out or unparsed base run, or a deleted contract.
    pub verified: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct EvidenceRecord {
    version: u32,
    runs: Vec<BaseEvidence>,
}

impl Default for EvidenceRecord {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            runs: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct HandoffRecord {
    version: u32,
    handoffs: Vec<Handoff>,
}

impl Default for HandoffRecord {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            handoffs: Vec::new(),
        }
    }
}

fn load_evidence_record(work_dir: &Path, stage_id: &str) -> Result<EvidenceRecord> {
    let record: EvidenceRecord =
        load_optional(work_dir, stage_id, BASE_EVIDENCE_FILE)?.unwrap_or_default();
    check_version(
        record.version,
        &format!("{BASE_EVIDENCE_FILE} of stage '{stage_id}'"),
    )?;
    Ok(record)
}

fn load_handoff_record(work_dir: &Path, stage_id: &str) -> Result<HandoffRecord> {
    let record: HandoffRecord =
        load_optional(work_dir, stage_id, HANDOFFS_FILE)?.unwrap_or_default();
    check_version(
        record.version,
        &format!("{HANDOFFS_FILE} of stage '{stage_id}'"),
    )?;
    Ok(record)
}

/// The stage's recorded base runs; an absent file loads as none.
pub fn load_base_evidence(work_dir: &Path, stage_id: &str) -> Result<Vec<BaseEvidence>> {
    Ok(load_evidence_record(work_dir, stage_id)?.runs)
}

/// Record one base run, replacing an earlier run of the same criterion
/// (`index` and `command`): the latest run is the one that counts.
pub fn save_base_evidence(work_dir: &Path, stage_id: &str, evidence: &BaseEvidence) -> Result<()> {
    update(work_dir, stage_id, BASE_EVIDENCE_FILE, || {
        let mut record = load_evidence_record(work_dir, stage_id)?;
        record
            .runs
            .retain(|run| run.index != evidence.index || run.command != evidence.command);
        record.runs.push(evidence.clone());
        Ok(record)
    })
}

/// The stage's recorded hand-offs; an absent file loads as none.
pub fn load_handoffs(work_dir: &Path, stage_id: &str) -> Result<Vec<Handoff>> {
    Ok(load_handoff_record(work_dir, stage_id)?.handoffs)
}

/// Append `handoffs`; one already recorded for the same `(dispute, field,
/// index)` is kept as it is, so a re-applied verdict appends nothing.
pub fn record_handoffs(work_dir: &Path, stage_id: &str, handoffs: &[Handoff]) -> Result<()> {
    if handoffs.is_empty() {
        return Ok(());
    }
    update(work_dir, stage_id, HANDOFFS_FILE, || {
        let mut record = load_handoff_record(work_dir, stage_id)?;
        for handoff in handoffs {
            if !record.handoffs.iter().any(|known| same_key(known, handoff)) {
                record.handoffs.push(handoff.clone());
            }
        }
        Ok(record)
    })
}

/// Remove the hand-offs recorded for the same `(dispute, field, index)` as
/// one of `handoffs`: a verdict whose amendment the cap refused leaves no
/// test out, so it hands none off.
pub fn forget_handoffs(work_dir: &Path, stage_id: &str, handoffs: &[Handoff]) -> Result<()> {
    if handoffs.is_empty() {
        return Ok(());
    }
    update(work_dir, stage_id, HANDOFFS_FILE, || {
        let mut record = load_handoff_record(work_dir, stage_id)?;
        record
            .handoffs
            .retain(|known| !handoffs.iter().any(|handoff| same_key(known, handoff)));
        Ok(record)
    })
}

/// Whether `a` and `b` hand off the same entry of the same verdict.
fn same_key(a: &Handoff, b: &Handoff) -> bool {
    a.dispute == b.dispute && a.field == b.field && a.index == b.index
}

#[cfg(test)]
#[path = "handoffs_test_support.rs"]
pub(crate) mod test_support;

#[cfg(test)]
mod tests {
    use super::*;

    fn run(index: usize, command: &str, passed: bool) -> BaseEvidence {
        BaseEvidence {
            index,
            command: command.to_string(),
            base: "b1".to_string(),
            passed,
            executed: Some(10),
            failed: Some(if passed { 0 } else { 1 }),
            build_failed: false,
            timed_out: false,
        }
    }

    fn handoff(dispute: &str, index: usize, reason: &str) -> Handoff {
        Handoff {
            stage: "s1".to_string(),
            field: "acceptance".to_string(),
            index,
            tests: vec!["net_fetch".to_string()],
            runs_at: "ci".to_string(),
            reason: reason.to_string(),
            dispute: dispute.to_string(),
            verified: true,
        }
    }

    #[test]
    fn absent_files_load_as_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load_base_evidence(tmp.path(), "s1").unwrap().is_empty());
        assert!(load_handoffs(tmp.path(), "s1").unwrap().is_empty());
    }

    #[test]
    fn a_later_base_run_of_the_same_criterion_replaces_the_earlier_one() {
        let tmp = tempfile::tempdir().unwrap();
        save_base_evidence(tmp.path(), "s1", &run(0, "cargo test", true)).unwrap();
        save_base_evidence(tmp.path(), "s1", &run(1, "cargo test", true)).unwrap();
        save_base_evidence(tmp.path(), "s1", &run(0, "cargo test", false)).unwrap();
        let runs = load_base_evidence(tmp.path(), "s1").unwrap();
        assert_eq!(
            runs,
            vec![run(1, "cargo test", true), run(0, "cargo test", false)]
        );
    }

    #[test]
    fn a_reapplied_handoff_is_recorded_once() {
        let tmp = tempfile::tempdir().unwrap();
        let first = handoff("d-1", 0, "needs network");
        record_handoffs(tmp.path(), "s1", std::slice::from_ref(&first)).unwrap();
        record_handoffs(tmp.path(), "s1", &[handoff("d-1", 0, "other text")]).unwrap();
        record_handoffs(tmp.path(), "s1", &[handoff("d-2", 0, "needs network")]).unwrap();
        let recorded = load_handoffs(tmp.path(), "s1").unwrap();
        assert_eq!(recorded, vec![first, handoff("d-2", 0, "needs network")]);
    }

    #[test]
    fn forgetting_removes_only_the_named_handoffs() {
        let tmp = tempfile::tempdir().unwrap();
        let kept = handoff("d-2", 0, "needs network");
        record_handoffs(tmp.path(), "s1", &[handoff("d-1", 0, "x"), kept.clone()]).unwrap();
        forget_handoffs(tmp.path(), "s1", &[handoff("d-1", 0, "other text")]).unwrap();
        assert_eq!(load_handoffs(tmp.path(), "s1").unwrap(), vec![kept]);
    }
}
