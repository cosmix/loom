//! `loom stage dispute-findings`, `dispute-contract` and `dispute-integrity`
//! (DESIGN D15): a plan v2 stage disputes review findings, a frozen contract
//! or test-integrity events, one request for the whole batch.
//!
//! The client snapshots the disputed findings or events into the request; the
//! daemon checks every id again and records its own snapshot
//! (`daemon/server/dispute_kinds.rs`). The transport is the one
//! `dispute-criteria` uses (`dispute_transport.rs`).

use anyhow::{Context, Result};
use std::path::Path;

use super::dispute_transport::{send, Dispute};
use super::review_status::stage_worktree_and_target;
use crate::models::dispute::{select_events, select_findings, DisputeKind};
use crate::relay::emit::{mode, EnvSnapshot, StdSink};
use crate::verify::integrity::current_events;
use crate::verify::review::store::open_findings;
use crate::verify::transitions::load_stage;

/// What a dispute names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisputeTarget {
    /// Open review finding ids, own or carried.
    Findings(Vec<String>),
    /// A frozen contract's id.
    Contract(String),
    /// Current test-integrity event ids.
    Integrity(Vec<String>),
}

/// One `loom stage dispute-{findings,contract,integrity}` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisputeFiling {
    pub stage_id: String,
    pub target: DisputeTarget,
    pub reason: String,
    pub evidence_commit: Option<String>,
}

/// File `filing` with the daemon.
pub fn file_dispute(filing: DisputeFiling) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    let kind = snapshot(&work_dir, &filing.stage_id, filing.target)?;
    let dispute = Dispute::of_kind(filing.stage_id, kind, filing.reason, filing.evidence_commit);
    let relay_mode = mode(&EnvSnapshot::from_process_env());
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    send(dispute, relay_mode, &cwd, &mut StdSink::default())
}

/// The dispute kind for `target`, with the named findings or events as the
/// stage's records show them now.
fn snapshot(work_dir: &Path, stage_id: &str, target: DisputeTarget) -> Result<DisputeKind> {
    match target {
        DisputeTarget::Findings(finding_ids) => {
            let open = open_findings(work_dir, stage_id)?;
            let evidence = select_findings(&open, &finding_ids)?;
            Ok(DisputeKind::Findings {
                finding_ids,
                evidence,
            })
        }
        DisputeTarget::Contract(contract_id) => Ok(DisputeKind::Contract { contract_id }),
        DisputeTarget::Integrity(event_ids) => {
            let stage = load_stage(stage_id, work_dir)?;
            let (worktree, target) = stage_worktree_and_target(work_dir, &stage)?;
            let events = current_events(&worktree, &target, &stage.ratchet_files)?;
            let evidence = select_events(&events, &event_ids)?;
            Ok(DisputeKind::Integrity {
                event_ids,
                evidence,
            })
        }
    }
}
