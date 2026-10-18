//! Applying the verdict on an impact-test dispute (DESIGN D14, D15).
//!
//! `accept` records each disputed failure in `reviews/<stage>/impact.json` as
//! the dispute's evidence showed it, so the gate notes it while it gets no
//! worse; `reject` tells the agent the failures stand. Both re-queue the stage
//! like the other plan v2 kinds (`apply_kinds.rs`). The evidence is the
//! daemon's copy of loom's observed record (`stage_service/dispute_kinds.rs`),
//! so nothing the filing agent wrote reaches `impact.json`.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::dispute::ImpactSnapshot;
use crate::models::stage::Stage;
use crate::verify::impact_tests::AcceptedFailure;
use crate::verify::review::verdict_records;

/// Record the failures `test_ids` names, as `evidence` snapshots them, as
/// accepted by dispute `dispute`; returns the notice for the agent.
pub(super) fn accept(
    work_dir: &Path,
    stage: &Stage,
    dispute: u32,
    test_ids: &[String],
    evidence: &[ImpactSnapshot],
) -> Result<String> {
    let accepted = test_ids
        .iter()
        .map(|id| {
            evidence
                .iter()
                .find(|failure| failure.id == *id)
                .map(|failure| AcceptedFailure::of(failure, dispute))
                .with_context(|| format!("dispute {dispute} holds no evidence for test '{id}'"))
        })
        .collect::<Result<Vec<_>>>()?;
    verdict_records::append_impact_acceptance(work_dir, &stage.id, &accepted)?;
    Ok(format!(
        "The adjudicator accepted your impact-test dispute: {} accepted as judged. \
         `loom stage complete` notes each while it fails exactly as accepted, over the same \
         tests; any other failure of it needs a fix or a new dispute.\n",
        test_ids.join(", ")
    ))
}

/// The notice for a rejected impact-test dispute.
pub(super) fn reject_notice(test_ids: &[String], reasoning: &str) -> String {
    format!(
        "The adjudicator rejected your impact-test dispute: {} stand.\n\n### Reasoning\n\n{}\n\n\
         Action: the stage's changes cause these failures; fix them, then complete the stage \
         again.\n",
        test_ids.join(", "),
        reasoning.trim()
    )
}

#[cfg(test)]
#[path = "apply_impact_tests.rs"]
mod tests;
