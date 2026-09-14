use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};

use crate::handoff::completion::attestation_key;
use crate::handoff::generator::{
    load_session_checkpoint, merge_session_handoff, HandoffContent, MergeOutcome,
};
use crate::handoff::schema::{
    AcceptedReceipt, CompletionAttemptEvidence, CompletionCheckpoint, HandoffOrigin,
};
use crate::models::session::Session;
use crate::models::stage::Stage;

pub fn record_attempt_handoff(
    session: &Session,
    stage: &Stage,
    evidence: &CompletionAttemptEvidence,
    work_dir: &Path,
) -> Result<(PathBuf, MergeOutcome)> {
    ensure!(
        evidence.stage_id == stage.id,
        "completion evidence stage mismatch"
    );
    ensure!(
        evidence.session_id == session.id,
        "completion evidence session mismatch"
    );
    let key = attestation_key(work_dir)?;
    let mut evidence = evidence.clone();
    evidence.attestation = None;
    evidence.attestation = Some(key.sign_evidence(&evidence));

    let mut checkpoint = CompletionCheckpoint::new(&stage.id, &session.id);
    checkpoint.record_attempt(&evidence)?;
    for observation in &mut checkpoint.observations {
        observation.attestation =
            Some(key.sign_observation(&checkpoint.stage_id, &checkpoint.session_id, observation));
    }
    if let Some(latest) = &mut checkpoint.latest {
        latest.attestation = Some(key.sign_evidence(latest));
    }
    merge_checkpoint(session, stage, checkpoint, work_dir)
}

pub fn record_accepted_handoff(
    session: &Session,
    stage: &Stage,
    mut receipt: AcceptedReceipt,
    work_dir: &Path,
) -> Result<(PathBuf, MergeOutcome)> {
    let key = attestation_key(work_dir)?;
    let mut checkpoint = load_session_checkpoint(&stage.id, &session.id, work_dir)?
        .context("no completion checkpoint exists for this stage session")?;
    receipt.attestation = None;
    receipt.attestation = Some(key.sign_receipt(&stage.id, &session.id, &receipt));
    checkpoint.record_accepted(receipt)?;
    merge_checkpoint(session, stage, checkpoint, work_dir)
}

fn merge_checkpoint(
    session: &Session,
    stage: &Stage,
    checkpoint: CompletionCheckpoint,
    work_dir: &Path,
) -> Result<(PathBuf, MergeOutcome)> {
    let content = HandoffContent::new(session.id.clone(), stage.id.clone())
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint));
    merge_session_handoff(
        session,
        stage,
        Some(HandoffOrigin::CompletionEvidence),
        content,
        work_dir,
    )
}
