use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::stage::Stage;

use super::completion_evidence::{format_evidence_record, pinned_command, verified_evidence};

pub(super) fn emit_verified_evidence(
    stage: &Stage,
    session_id: &str,
    snapshot: &VerificationSnapshot,
) -> Result<()> {
    let loom_bin = std::env::current_exe()
        .context("resolving the loom executable")?
        .canonicalize()
        .context("canonicalizing the loom executable")?;
    let exact_command = pinned_command(&loom_bin, &stage.id);
    let mut evidence = verified_evidence(stage, session_id, snapshot.commit.clone(), exact_command);
    evidence.check_definition_hash = snapshot.policy.clone();
    let record = format_evidence_record(&evidence)?;

    let mut stdout = io::stdout().lock();
    stdout
        .write_all(record.as_bytes())
        .context("writing completion evidence")?;
    stdout.flush().context("flushing completion evidence")
}
