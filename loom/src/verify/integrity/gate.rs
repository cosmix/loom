//! The test-integrity gate at stage completion (DESIGN D13).

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::{load_accepted, scan_changes, Accepted, EventKind, IntegrityEvent};
use crate::git::worktree::WorktreeGit;
use crate::models::stage::Stage;
use crate::verify::review::fingerprint::{self, ChangeFingerprint};
use crate::verify::review::gate::covers;
use crate::verify::review::report::single_line;

/// [`check_changes`] against the worktree's fingerprint from the loom daemon
/// that owns it (`fingerprint::compute`), for a stage the review gate covers
/// (`covers`); any other stage passes. This process reads the worktree
/// through `fingerprint::local_git`.
pub fn check(stage: &Stage, work_dir: &Path, worktree: &Path, target_branch: &str) -> Result<()> {
    if !covers(stage) {
        return Ok(());
    }
    let changes = fingerprint::compute(worktree, target_branch)
        .context("failed to list the worktree's changes for the test-integrity check")?;
    check_changes(
        stage,
        work_dir,
        &fingerprint::local_git(worktree)?,
        &changes,
    )
}

/// Fail, listing every blocking event, unless `reviews/<stage>/integrity.json`
/// accepts every integrity event of `changes`, the change fingerprint of
/// `repo`'s worktree, and none got worse. `changes` must come from the
/// observer that derived the accepted events: the daemon.
pub fn check_changes(
    stage: &Stage,
    work_dir: &Path,
    repo: &WorktreeGit,
    changes: &ChangeFingerprint,
) -> Result<()> {
    let scan = scan_changes(repo, changes, &stage.ratchet_files)?;
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
    bail!("{}", failure_message(&stage.id, &blocking))
}

/// The gate's failure: every blocking event, and the two ways out of each.
fn failure_message(stage_id: &str, blocking: &[String]) -> String {
    format!(
        "test-integrity gate failed for stage '{stage_id}':\n  - {}\n\
         Revert the change behind each event, or dispute it with \
         `loom stage dispute-integrity {stage_id} --event <id> ... --reason ...`.\n\
         Run `loom stage review integrity {stage_id}` for each event's detail.",
        blocking.join("\n  - ")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_names_the_dispute_command() {
        let blocking =
            ["TI-decl-rust (test declarations 3 at base, 2 now): not accepted".to_string()];
        let message = failure_message("s1", &blocking);
        assert!(
            message.contains("\n  - TI-decl-rust (test declarations"),
            "{message}"
        );
        assert!(
            message.contains("loom stage dispute-integrity s1 --event <id> ... --reason ..."),
            "{message}"
        );
        assert!(
            message.contains("loom stage review integrity s1"),
            "{message}"
        );
    }
}
