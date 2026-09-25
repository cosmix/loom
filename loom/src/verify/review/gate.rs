//! The review gate at stage completion (DESIGN D12).
//!
//! A v2 `standard` or `integration-verify` stage completes only when a
//! well-formed review round is recorded, the latest one saw exactly the
//! worktree's current changes, and no finding, own or carried, is open.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::fingerprint::{self, ChangeFingerprint};
use super::report::single_line;
use super::store::{self, OpenFinding, ReviewRound};
use crate::models::stage::{Stage, StageType};

/// Whether the review gate and the test-integrity gate cover `stage`: a v2
/// `standard` or `integration-verify` stage.
pub fn covers(stage: &Stage) -> bool {
    stage.plan_version == 2
        && matches!(
            stage.stage_type,
            StageType::Standard | StageType::IntegrationVerify
        )
}

/// [`check`] against the worktree's fingerprint from the loom daemon that
/// owns it ([`fingerprint::compute`]), for a stage the gate [`covers`]; any
/// other stage passes. A stage the gate covers must have a worktree.
pub fn check_at_completion(
    stage: &Stage,
    work_dir: &Path,
    worktree_root: Option<&Path>,
    target_branch: &str,
) -> Result<()> {
    if !covers(stage) {
        return Ok(());
    }
    let Some(worktree_root) = worktree_root else {
        bail!(
            "Stage '{}': no worktree to check the review against",
            stage.id
        );
    };
    let current = fingerprint::compute(worktree_root, target_branch)
        .context("failed to compute the worktree's change fingerprint for the review gate")?;
    check(stage, work_dir, &current)
}

/// Fail, listing every problem at once, unless the latest well-formed review
/// round saw exactly the `current` changes and no finding is open. `current`
/// must come from the same observer as the rounds' fingerprints: the daemon.
pub fn check(stage: &Stage, work_dir: &Path, current: &ChangeFingerprint) -> Result<()> {
    let rounds = store::load_rounds(work_dir, &stage.id)?;
    let rulings = store::load_rulings(work_dir, &stage.id)?;
    let carried = store::load_carried(work_dir, &stage.id)?;
    let mut problems = Vec::new();
    match rounds.iter().rev().find(|round| round.is_well_formed()) {
        None => problems.push("no well-formed review round is recorded".to_string()),
        Some(latest) => problems.extend(stale_review(latest, current)),
    }
    let open = store::open_among(&rounds, &rulings, &carried);
    problems.extend(open.iter().map(describe_open));
    if problems.is_empty() {
        return Ok(());
    }
    bail!(
        "{}",
        failure_message(&stage.id, &problems, !open.is_empty(), rounds.last())
    )
}

/// Why `latest` no longer covers the `current` changes, if it does not.
fn stale_review(latest: &ReviewRound, current: &ChangeFingerprint) -> Option<String> {
    if current.value == latest.fingerprint {
        return None;
    }
    let changed = fingerprint::changed_since(&latest.files, &current.files);
    let since = if changed.is_empty() {
        "the base commit changed".to_string()
    } else {
        let paths: Vec<String> = changed.iter().map(|path| single_line(path)).collect();
        format!("changed since: {}", paths.join(", "))
    };
    Some(format!(
        "review round {} saw {}, but the worktree is now at {} ({since})",
        latest.round, latest.fingerprint, current.value
    ))
}

fn describe_open(open: &OpenFinding) -> String {
    let finding = &open.finding;
    format!(
        "open finding {} ({}) {}:{}: {}",
        open.id,
        single_line(&finding.severity),
        single_line(&finding.file),
        finding.line,
        single_line(&finding.claim)
    )
}

fn failure_message(
    stage_id: &str,
    problems: &[String],
    has_open: bool,
    last_round: Option<&ReviewRound>,
) -> String {
    let mut message = format!(
        "review gate failed for stage '{stage_id}':\n  - {}",
        problems.join("\n  - ")
    );
    let malformed = last_round.and_then(|round| Some((round.round, round.malformed.as_ref()?)));
    if let Some((round, reason)) = malformed {
        message.push_str(&format!(
            "\nThe latest review round ({round}) is malformed: {:?}",
            single_line(reason)
        ));
    }
    if has_open {
        message.push_str(&format!(
            "\nOpen findings: fix them and run a re-review, or dispute them together with \
             `loom stage dispute-findings {stage_id} --finding <id> ... --reason ...`."
        ));
    }
    message.push_str(&format!(
        "\nRun `loom stage review status {stage_id}` for the rounds, the open findings and \
         the files changed since the latest review."
    ));
    message
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
