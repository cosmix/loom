//! Applying an integrity dispute's verdict: an accept records the events as
//! judged, a reject tells the agent to revert the changes behind them.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::dispute::IntegritySnapshot;
use crate::models::stage::Stage;
use crate::verify::integrity::AcceptedEvent;
use crate::verify::review::verdict_records;

use super::notify_and_hold;

pub(super) fn apply_integrity_accept(
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
    notify_and_hold(work_dir, stage, &body)
}

pub(super) fn integrity_reject_notice(
    stage_id: &str,
    event_ids: &[String],
    reasoning: &str,
) -> String {
    format!(
        "The adjudicator rejected your integrity dispute: {} stand.\n\n### Reasoning\n\n{}\n\n\
         Action: revert the changes behind these events (`loom stage review integrity \
         {stage_id}` lists what each one lost), then complete the stage again.\n",
        event_ids.join(", "),
        reasoning.trim()
    )
}
