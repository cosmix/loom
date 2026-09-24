//! What an applied dispute verdict records in `.loom/work/reviews/<stage>/`
//! (DESIGN D12, D13, D15): the rulings of a findings dispute, the findings it
//! deferred to a later stage, and the integrity events a dispute accepted.
//!
//! Each write is a locked read-modify-write of the whole file, replaced
//! atomically. The daemon re-applies a verdict whose apply crashed half way,
//! so every write is idempotent: an entry already recorded is not added again.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use super::store::{self, CarriedFinding, Ruling};
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::verify::contracts::store::canonical_work_dir;
use crate::verify::integrity::{self, AcceptedEvent, ACCEPTED_FILE};

/// Append `rulings` to `rulings.json` of `stage_id`.
pub fn append_rulings(work_dir: &Path, stage_id: &str, rulings: &[Ruling]) -> Result<()> {
    update(work_dir, stage_id, store::RULINGS_FILE, || {
        let mut record = store::load_rulings(work_dir, stage_id)?;
        for ruling in rulings {
            if !record.rulings.contains(ruling) {
                record.rulings.push(ruling.clone());
            }
        }
        Ok(record)
    })
}

/// Append `findings` to `carried.json` of `target_stage`; a carried id already
/// listed is kept as it is.
pub fn append_carried(
    work_dir: &Path,
    target_stage: &str,
    findings: &[CarriedFinding],
) -> Result<()> {
    update(work_dir, target_stage, store::CARRIED_FILE, || {
        let mut record = store::load_carried(work_dir, target_stage)?;
        for finding in findings {
            if !record.carried.iter().any(|known| known.id == finding.id) {
                record.carried.push(finding.clone());
            }
        }
        Ok(record)
    })
}

/// Record `events` in `integrity.json` of `stage_id`. A later acceptance of
/// the same event replaces the earlier one: the gate reads one record per
/// event, and the latest dispute judged the latest counts and content.
pub fn append_integrity_acceptance(
    work_dir: &Path,
    stage_id: &str,
    events: &[AcceptedEvent],
) -> Result<()> {
    update(work_dir, stage_id, ACCEPTED_FILE, || {
        let mut record = integrity::load_accepted(work_dir, stage_id)?;
        for event in events {
            record.accepted.retain(|known| known.event != event.event);
            record.accepted.push(event.clone());
        }
        Ok(record)
    })
}

/// Under the stage's `reviews/` directory lock, build the new content of
/// `file` and replace it atomically.
fn update<T: Serialize>(
    work_dir: &Path,
    stage_id: &str,
    file: &str,
    build: impl FnOnce() -> Result<T>,
) -> Result<()> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let dir = store::stage_dir(&root, stage_id);
    locked_dir_update(&dir, || {
        let json = serde_json::to_string_pretty(&build()?)?;
        atomic_write_locked(&dir.join(file), &json)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::integrity::EventKind;
    use crate::verify::review::store::RulingKind;

    fn accepted(current: u64, dispute: u32) -> AcceptedEvent {
        AcceptedEvent {
            event: "TI-assert-rust".to_string(),
            kind: EventKind::AssertTotal,
            language: Some("rust".to_string()),
            path: None,
            base: Some(120),
            accepted_current: Some(current),
            accepted_sha256: None,
            dispute,
        }
    }

    #[test]
    fn a_reapplied_ruling_is_recorded_once() {
        let tmp = tempfile::tempdir().unwrap();
        let ruling = Ruling {
            finding: "F-1-1".to_string(),
            ruling: RulingKind::Dismiss,
            target_stage: None,
            dispute: 2,
        };
        append_rulings(tmp.path(), "s1", std::slice::from_ref(&ruling)).unwrap();
        append_rulings(tmp.path(), "s1", std::slice::from_ref(&ruling)).unwrap();
        let recorded = store::load_rulings(tmp.path(), "s1").unwrap();
        assert_eq!(recorded.rulings, vec![ruling]);
    }

    #[test]
    fn a_later_integrity_acceptance_replaces_the_earlier_one() {
        let tmp = tempfile::tempdir().unwrap();
        append_integrity_acceptance(tmp.path(), "s1", &[accepted(118, 1)]).unwrap();
        append_integrity_acceptance(tmp.path(), "s1", &[accepted(110, 2)]).unwrap();
        let recorded = integrity::load_accepted(tmp.path(), "s1").unwrap();
        assert_eq!(recorded.accepted, vec![accepted(110, 2)]);
    }
}
