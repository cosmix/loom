//! Repair of torn journal tails for records whose supervisor is proven gone.

use std::collections::HashSet;
use std::path::Path;

use chrono::Utc;

use crate::native_worker::{index, store};

use super::{read_claim, supervisor_gone, tombstone, Notes, WarnKind, Warning, CLAIM_GRACE};

/// Reclaims the unterminated last line of every record in the stage whose
/// supervisor is proven gone, by the same proof `decide` uses. True when any
/// record was repaired, so the caller reloads the stage. A held record, one whose
/// journal has no torn tail, and one whose supervisor may still run are not touched:
/// the proof can write a tombstone, and a live supervisor could append while the
/// file is truncated.
pub(super) fn repair_lost_tails(
    work_dir: &Path,
    stage_id: &str,
    held: &HashSet<String>,
    found: &mut Vec<Warning>,
) -> bool {
    let mut repaired = false;
    for entry in index::list_invocations(&store::stage_dir(work_dir, stage_id)) {
        let Ok(entry) = entry else { continue };
        let dir = entry.path();
        let invocation_id = entry.file_name().to_string_lossy().into_owned();
        if held.contains(&invocation_id) || !store::has_torn_tail(&dir) {
            continue;
        }
        let mut notes = Notes::new();
        let gone = supervisor_is_gone(&dir, &mut notes);
        found.extend(
            notes
                .into_iter()
                .map(|(kind, detail)| Warning::new(stage_id, Some(&invocation_id), kind, detail)),
        );
        if !gone {
            continue;
        }
        match store::repair_torn_tail(&dir) {
            Ok(true) => {
                repaired = true;
                tracing::warn!(stage_id, %invocation_id,
                    "dropped an unterminated events.jsonl line into events.jsonl.torn-*");
            }
            Ok(false) => {}
            Err(error) => found.push(Warning::new(
                stage_id,
                Some(&invocation_id),
                WarnKind::CannotSettle,
                format!("{error:#}"),
            )),
        }
    }
    repaired
}

/// Mirrors `decide`: a tombstone, a dead claimed process, or an old record with
/// no claim that the daemon can tombstone.
fn supervisor_is_gone(dir: &Path, notes: &mut Notes) -> bool {
    match read_claim(dir) {
        Ok(Some(claim)) if claim.pid <= 0 => true,
        Ok(Some(claim)) => supervisor_gone(dir, &claim, notes),
        Ok(None) => {
            let Ok(manifest) = store::read_manifest(dir) else {
                return false;
            };
            let age = Utc::now().signed_duration_since(manifest.created_at);
            age.to_std().is_ok_and(|age| age >= CLAIM_GRACE) && tombstone(dir)
        }
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "runner_repair_tests.rs"]
mod tests;
