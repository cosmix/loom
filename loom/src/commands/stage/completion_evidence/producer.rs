use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};

use crate::handoff::worktree_head_commit;
use crate::models::stage::Stage;

use super::completion_evidence::{format_evidence_record, pinned_command, verified_evidence};

pub(super) fn emit_verified_evidence(
    stage: &Stage,
    session_id: &str,
    checkout: &Path,
) -> Result<()> {
    let commit = worktree_head_commit(checkout)?;
    let loom_bin = std::env::current_exe()
        .context("resolving the loom executable")?
        .canonicalize()
        .context("canonicalizing the loom executable")?;
    let exact_command = pinned_command(&loom_bin, &stage.id);
    let evidence = verified_evidence(stage, session_id, commit, exact_command);
    let record = format_evidence_record(&evidence)?;

    let mut stdout = io::stdout().lock();
    stdout
        .write_all(record.as_bytes())
        .context("writing completion evidence")?;
    stdout.flush().context("flushing completion evidence")
}
