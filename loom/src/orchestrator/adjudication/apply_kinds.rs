//! Applying a verdict by the kind of dispute it answers (DESIGN D15).
//!
//! A criterion dispute keeps `apply.rs`'s rules exactly. The other kinds never
//! send the stage to a human by their own verdict: each records what was
//! decided, tells the agent in `feedback.md`, and re-queues the stage through
//! [`requeue_or_hold_for_remaining_disputes`], so sibling disputes still hold
//! it until every one has a verdict.
//!
//! * findings `rulings`: appended to `reviews/<stage>/rulings.json`; each
//!   `defer` also carries its finding to `reviews/<target>/carried.json`. A
//!   `defer` whose target is missing, completed or not downstream of the
//!   stage, or any `defer` from an integration-verify stage (integration-verify
//!   never defers), turns the verdict into `NeedsMoreEvidence` naming why.
//! * contract `accept`: amends `contracts` when the verdict carries a patch,
//!   then re-freezes the contract's file at its current worktree content;
//!   `reject`: feedback to restore the frozen contract (`apply_contract.rs`).
//! * integrity `accept`: records each event in `reviews/<stage>/integrity.json`
//!   with the counts and hashes the adjudicator judged; `reject`: feedback to
//!   revert the changes behind the events.

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::models::dispute::{
    DisputeKind, DisputeVerdict, DisputeVerdictRecord, FindingRuling, FindingSnapshot,
    IntegritySnapshot,
};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::integrity::AcceptedEvent;
use crate::verify::review::report::single_line;
use crate::verify::review::store::{CarriedFinding, Ruling, RulingKind};
use crate::verify::review::verdict_records;
use crate::verify::transitions::list_all_stages;

use super::apply::{
    apply_accept, apply_needs_more_evidence, apply_reject, requeue_or_hold_for_remaining_disputes,
};
use super::{apply_contract, feedback, read_request};

/// Apply `record` to `stage` by the kind of the dispute it answers.
pub(super) fn apply_by_kind(
    work_dir: &Path,
    stage: &mut Stage,
    record: &DisputeVerdictRecord,
) -> Result<()> {
    let dispute = record.id;
    match &record.verdict {
        DisputeVerdict::NeedsMoreEvidence { questions } => {
            apply_needs_more_evidence(work_dir, stage, questions)
        }
        DisputeVerdict::Rulings { rulings } => apply_rulings(work_dir, stage, dispute, rulings),
        DisputeVerdict::Accept { plan_patch, .. } => match dispute_kind(work_dir, stage, dispute) {
            Some(DisputeKind::Contract { contract_id }) => {
                let accepted =
                    apply_contract::accept(work_dir, stage, dispute, &contract_id, plan_patch)?;
                accepted.map_or(Ok(()), |body| notify_and_requeue(work_dir, stage, &body))
            }
            Some(DisputeKind::Integrity {
                event_ids,
                evidence,
            }) => apply_integrity_accept(work_dir, stage, dispute, &event_ids, &evidence),
            _ => apply_accept(work_dir, stage, plan_patch, dispute),
        },
        DisputeVerdict::Reject {
            reasoning,
            citations,
        } => match dispute_kind(work_dir, stage, dispute) {
            Some(DisputeKind::Contract { contract_id }) => {
                let body = apply_contract::reject_notice(&stage.id, &contract_id, reasoning);
                notify_and_requeue(work_dir, stage, &body)
            }
            Some(DisputeKind::Integrity { event_ids, .. }) => {
                let body = integrity_reject_notice(&stage.id, &event_ids, reasoning);
                notify_and_requeue(work_dir, stage, &body)
            }
            _ => apply_reject(work_dir, stage, dispute, reasoning, citations),
        },
    }
}

/// The kind of the dispute a verdict answers. An unreadable `request.md`
/// keeps the criterion handling, which never needed the request to apply.
fn dispute_kind(work_dir: &Path, stage: &Stage, dispute: u32) -> Option<DisputeKind> {
    match read_request(work_dir, &stage.id, dispute) {
        Ok(request) => Some(request.kind),
        Err(error) => {
            tracing::warn!(
                target: "loom::adjudication",
                stage = %stage.id,
                dispute,
                error = %format!("{error:#}"),
                "dispute request unreadable; applying the verdict as a criterion verdict",
            );
            None
        }
    }
}

fn notify_and_requeue(work_dir: &Path, stage: &mut Stage, body: &str) -> Result<()> {
    feedback::write_notice(work_dir, &stage.id, body)?;
    requeue_or_hold_for_remaining_disputes(work_dir, stage)
}

fn apply_rulings(
    work_dir: &Path,
    stage: &mut Stage,
    dispute: u32,
    rulings: &[FindingRuling],
) -> Result<()> {
    let request = read_request(work_dir, &stage.id, dispute)?;
    let DisputeKind::Findings { evidence, .. } = request.kind else {
        bail!(
            "dispute {dispute} of stage '{}' is not a findings dispute, so it takes no rulings",
            stage.id
        );
    };
    if let Some(question) = refused_defer(work_dir, stage, rulings)? {
        return apply_needs_more_evidence(work_dir, stage, &[question]);
    }
    for (target, carried) in carried_by_target(&stage.id, dispute, rulings, &evidence)? {
        verdict_records::append_carried(work_dir, &target, &carried)?;
    }
    let recorded: Vec<Ruling> = rulings
        .iter()
        .map(|ruling| Ruling {
            finding: ruling.finding.clone(),
            ruling: ruling.ruling,
            target_stage: ruling.target_stage.clone(),
            dispute,
        })
        .collect();
    verdict_records::append_rulings(work_dir, &stage.id, &recorded)?;
    notify_and_requeue(work_dir, stage, &rulings_notice(rulings))
}

/// Every `defer` ruling with its target stage.
fn defers(rulings: &[FindingRuling]) -> impl Iterator<Item = (&FindingRuling, &str)> {
    rulings
        .iter()
        .filter(|ruling| ruling.ruling == RulingKind::Defer)
        .filter_map(|ruling| Some((ruling, ruling.target_stage.as_deref()?)))
}

/// Why a `defer` cannot stand, as the question for the next round, or `None`
/// when every one can.
fn refused_defer(
    work_dir: &Path,
    stage: &Stage,
    rulings: &[FindingRuling],
) -> Result<Option<String>> {
    let Some((first, _)) = defers(rulings).next() else {
        return Ok(None);
    };
    if stage.stage_type == StageType::IntegrationVerify {
        return Ok(Some(format!(
            "The ruling on '{}' defers it, but '{}' is an integration-verify stage and \
             integration-verify never defers: rule the finding uphold or dismiss.",
            first.finding, stage.id
        )));
    }
    let stages = list_all_stages(work_dir)?;
    for (ruling, target) in defers(rulings) {
        if let Some(why) = defer_target_problem(&stages, &stage.id, target) {
            return Ok(Some(format!(
                "The ruling on '{}' cannot defer it to '{target}': {why}. Defer only to a \
                 stage that transitively depends on '{}' and has not completed, or rule the \
                 finding uphold or dismiss.",
                ruling.finding, stage.id
            )));
        }
    }
    Ok(None)
}

fn defer_target_problem(stages: &[Stage], origin: &str, target: &str) -> Option<&'static str> {
    let Some(found) = stages.iter().find(|stage| stage.id == target) else {
        return Some("no such stage exists");
    };
    if found.status == StageStatus::Completed {
        return Some("that stage has already completed");
    }
    if !depends_on(stages, target, origin) {
        return Some("that stage does not depend on this one");
    }
    None
}

/// Whether `stage_id` transitively depends on `ancestor`.
fn depends_on(stages: &[Stage], stage_id: &str, ancestor: &str) -> bool {
    let dependencies: HashMap<&str, &[String]> = stages
        .iter()
        .map(|stage| (stage.id.as_str(), stage.dependencies.as_slice()))
        .collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut queue = vec![stage_id];
    while let Some(current) = queue.pop() {
        for dependency in dependencies.get(current).copied().unwrap_or_default() {
            let dependency = dependency.as_str();
            if dependency == ancestor {
                return true;
            }
            if seen.insert(dependency) {
                queue.push(dependency);
            }
        }
    }
    false
}

/// The deferred findings as their targets' carried findings. A finding the
/// stage raised itself gets the id `<stage>/F-<round>-<k>`; one it carried
/// from an earlier stage keeps that stage as its origin, and its id.
fn carried_by_target(
    stage_id: &str,
    dispute: u32,
    rulings: &[FindingRuling],
    evidence: &[FindingSnapshot],
) -> Result<BTreeMap<String, Vec<CarriedFinding>>> {
    let mut by_target: BTreeMap<String, Vec<CarriedFinding>> = BTreeMap::new();
    for (ruling, target) in defers(rulings) {
        let snapshot = evidence
            .iter()
            .find(|snapshot| snapshot.id == ruling.finding)
            .with_context(|| {
                format!(
                    "dispute {dispute} holds no evidence for finding '{}'",
                    ruling.finding
                )
            })?;
        let (id, origin_stage) = match &snapshot.origin_stage {
            Some(origin) => (snapshot.id.clone(), origin.clone()),
            None => (format!("{stage_id}/{}", snapshot.id), stage_id.to_string()),
        };
        by_target
            .entry(target.to_string())
            .or_default()
            .push(CarriedFinding {
                id,
                origin_stage,
                finding: snapshot.finding.clone(),
                dispute,
            });
    }
    Ok(by_target)
}

/// Feedback listing the upheld findings to fix, then the closed ones.
fn rulings_notice(rulings: &[FindingRuling]) -> String {
    let mut body = String::from("The adjudicator ruled on your disputed review findings.\n\n");
    let (upheld, closed): (Vec<_>, Vec<_>) = rulings.iter().partition(|r| !r.ruling.closes());
    let action = if upheld.is_empty() {
        "Action: none of the disputed findings needs a change.\n"
    } else {
        "Action: fix every upheld finding and have the fix re-reviewed.\n"
    };
    for (heading, group) in [
        ("Upheld: fix these", upheld),
        ("Closed: no change needed", closed),
    ] {
        if group.is_empty() {
            continue;
        }
        body.push_str(&format!("### {heading}\n\n"));
        for ruling in group {
            let outcome = match ruling.ruling {
                RulingKind::Uphold => "upheld".to_string(),
                RulingKind::Dismiss => "dismissed".to_string(),
                RulingKind::Defer => format!(
                    "deferred to {}",
                    ruling.target_stage.as_deref().unwrap_or_default()
                ),
            };
            let reasoning = single_line(&ruling.reasoning);
            body.push_str(&format!("- {} {outcome}: {reasoning}\n", ruling.finding));
        }
        body.push('\n');
    }
    body.push_str(action);
    body
}

fn apply_integrity_accept(
    work_dir: &Path,
    stage: &mut Stage,
    dispute: u32,
    event_ids: &[String],
    evidence: &[IntegritySnapshot],
) -> Result<()> {
    let accepted = event_ids
        .iter()
        .map(|id| {
            let event = evidence
                .iter()
                .find(|event| event.id == *id)
                .with_context(|| format!("dispute {dispute} holds no evidence for event '{id}'"))?;
            Ok(AcceptedEvent {
                event: event.id.clone(),
                kind: event.kind,
                language: event.language.clone(),
                path: event.path.clone(),
                base: event.base,
                accepted_current: event.current,
                accepted_sha256: event.current_sha256.clone(),
                dispute,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    verdict_records::append_integrity_acceptance(work_dir, &stage.id, &accepted)?;
    let body = format!(
        "The adjudicator accepted your integrity dispute: {} accepted as judged. A later \
         change that makes one of them worse needs a new dispute.\n",
        event_ids.join(", ")
    );
    notify_and_requeue(work_dir, stage, &body)
}

fn integrity_reject_notice(stage_id: &str, event_ids: &[String], reasoning: &str) -> String {
    format!(
        "The adjudicator rejected your integrity dispute: {} stand.\n\n### Reasoning\n\n{}\n\n\
         Action: revert the changes behind these events (`loom stage review integrity \
         {stage_id}` lists what each one lost), then complete the stage again.\n",
        event_ids.join(", "),
        reasoning.trim()
    )
}
