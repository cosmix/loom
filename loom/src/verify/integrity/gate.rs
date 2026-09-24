//! The test-integrity gate at stage completion (DESIGN D13).

use anyhow::{bail, Result};
use std::path::Path;

use super::{load_accepted, scan, Accepted, EventKind, IntegrityEvent};
use crate::models::stage::{Stage, StageType};
use crate::verify::review::report::single_line;

/// Fail, listing every blocking event, unless `reviews/<stage>/integrity.json`
/// accepts every integrity event of the worktree against `target_branch` and
/// none got worse. Only v2 `standard` and `integration-verify` stages are
/// checked; any other stage passes.
pub fn check(stage: &Stage, work_dir: &Path, worktree: &Path, target_branch: &str) -> Result<()> {
    let covered = matches!(
        stage.stage_type,
        StageType::Standard | StageType::IntegrationVerify
    );
    if stage.plan_version != 2 || !covered {
        return Ok(());
    }
    let scan = scan(worktree, target_branch, &stage.ratchet_files)?;
    for path in &scan.unprofiled {
        println!(
            "Test integrity: no language profile covers {}; its tests are not counted.",
            single_line(path)
        );
    }
    let accepted = load_accepted(work_dir, &stage.id)?;
    let blocking: Vec<String> = scan
        .events
        .iter()
        .filter_map(|event| {
            shortfall(event, &accepted).map(|why| format!("{}: {why}", describe(event)))
        })
        .collect();
    if blocking.is_empty() {
        return Ok(());
    }
    bail!(
        "test-integrity gate failed for stage '{id}':\n  - {}\n\
         Revert the change behind each event, or dispute it.\n\
         Run `loom stage review integrity {id}` for each event's detail.",
        blocking.join("\n  - "),
        id = stage.id
    )
}

/// Why `accepted` does not cover `event`, or `None` when it does. A count
/// event stays covered while its total is at least the accepted one, a file
/// event while the file keeps the accepted content.
pub fn shortfall(event: &IntegrityEvent, accepted: &Accepted) -> Option<String> {
    let Some(record) = accepted
        .accepted
        .iter()
        .find(|record| record.event == event.id)
    else {
        return Some("not accepted".to_string());
    };
    match event.kind {
        EventKind::DeclTotal | EventKind::AssertTotal => {
            match (event.current, record.accepted_current) {
                (Some(current), Some(floor)) => (current < floor)
                    .then(|| format!("worse than accepted: {current} now, {floor} accepted")),
                _ => Some("the accepted record has no count".to_string()),
            }
        }
        EventKind::AssertionEdit | EventKind::Ratchet => {
            let changed = event.current_sha256 != record.accepted_sha256;
            changed.then(|| "worse than accepted: the file changed since it was accepted".into())
        }
    }
}

/// One line naming the event and what it measured.
pub fn describe(event: &IntegrityEvent) -> String {
    let id = single_line(&event.id);
    let counted = |measure: &str| {
        let base = event.base.unwrap_or_default();
        let current = event.current.unwrap_or_default();
        format!("{id} ({measure} {base} at base, {current} now)")
    };
    match event.kind {
        EventKind::DeclTotal => counted("test declarations"),
        EventKind::AssertTotal => counted("assertions"),
        EventKind::AssertionEdit => format!(
            "{id} ({} assertion line(s) removed or changed)",
            event.detail.len()
        ),
        EventKind::Ratchet => format!("{id} (ratchet file differs from base)"),
    }
}
