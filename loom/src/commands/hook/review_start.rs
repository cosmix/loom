//! Immutable worktree identity captured when a code reviewer starts.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::review_harvest::{read_input, stage_worktree, work_dir_from_env, HarvestInput};
use super::review_transcript::validate_transcript_identity;
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::validation::validate_id;
use crate::verify::review::fingerprint::{self, ChangeFingerprint};
use crate::verify::transitions::load_stage;

const START_VERSION: u32 = 1;
const MAX_START_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewStart {
    version: u32,
    stage_id: String,
    session_id: String,
    agent_id: String,
    transcript_path: PathBuf,
    fingerprint: ChangeFingerprint,
}

/// Hidden SubagentStart delegate. An unavailable snapshot leaves the later
/// harvest without evidence, so that harvest fails closed.
pub fn review_start() -> Result<()> {
    let input = read_input()?;
    capture(&work_dir_from_env()?, &input)?;
    Ok(())
}

/// Capture the tree before the reviewer begins. The first exact invocation
/// wins: a repeated start cannot rebind the agent to later edits.
pub(super) fn capture(work_dir: &Path, input: &HarvestInput) -> Result<()> {
    validate_id(&input.stage_id).context("invalid stage id")?;
    validate_id(&input.session_id).context("invalid parent session id")?;
    validate_id(&input.agent_id).context("invalid reviewer agent id")?;
    validate_transcript_identity(input)?;
    let stage = load_stage(&input.stage_id, work_dir)?;
    if stage.plan_version != 2 {
        return Ok(());
    }
    let worktree = stage_worktree(work_dir, &stage)?;
    let target = crate::fs::resolve_target_branch_from_config(work_dir, &worktree)?;
    let fingerprint = fingerprint::compute(&worktree, &target)?;
    let evidence = ReviewStart {
        version: START_VERSION,
        stage_id: input.stage_id.clone(),
        session_id: input.session_id.clone(),
        agent_id: input.agent_id.clone(),
        transcript_path: input.transcript_path.clone(),
        fingerprint,
    };
    let root = work_dir
        .canonicalize()
        .context("resolving the work directory")?;
    let dir = root.join("reviews").join(&input.stage_id);
    let relative = start_relative(input);
    locked_dir_update(&dir, || match read_start(&root, &relative) {
        Ok(existing) => {
            ensure!(
                existing == evidence,
                "reviewer start evidence already differs"
            );
            Ok(())
        }
        Err(error) if is_not_found(&error) => {
            let body = serde_json::to_string(&evidence)?;
            atomic_write_locked(&root.join(&relative), &body)
        }
        Err(error) => Err(error),
    })
}

/// Load the exact invocation's start snapshot. Missing, malformed, or
/// mismatched evidence is never replaced by a harvest-time fingerprint.
pub(super) fn load(work_dir: &Path, input: &HarvestInput) -> Result<ChangeFingerprint> {
    validate_id(&input.stage_id).context("invalid stage id")?;
    validate_id(&input.session_id).context("invalid parent session id")?;
    validate_id(&input.agent_id).context("invalid reviewer agent id")?;
    let root = work_dir
        .canonicalize()
        .context("resolving the work directory")?;
    let evidence = read_start(&root, &start_relative(input))
        .context("missing or invalid reviewer start evidence")?;
    ensure!(
        evidence.version == START_VERSION,
        "unsupported reviewer start evidence"
    );
    ensure!(
        evidence.stage_id == input.stage_id
            && evidence.session_id == input.session_id
            && evidence.agent_id == input.agent_id
            && evidence.transcript_path == input.transcript_path,
        "reviewer start evidence does not match this invocation"
    );
    Ok(evidence.fingerprint)
}

fn read_start(root: &Path, relative: &Path) -> Result<ReviewStart> {
    let bytes = read_bounded(root, relative, MAX_START_BYTES)?;
    serde_json::from_slice(&bytes).context("parsing reviewer start evidence")
}

fn start_relative(input: &HarvestInput) -> PathBuf {
    let mut hasher = Sha256::new();
    for part in [&input.stage_id, &input.session_id, &input.agent_id] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    Path::new("reviews")
        .join(&input.stage_id)
        .join(format!("start-{}.json", hex::encode(hasher.finalize())))
}
