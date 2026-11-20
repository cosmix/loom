//! `loom stage dispute-findings`, `dispute-contract`, `dispute-integrity` and
//! `dispute-criteria --field impact-tests` (DESIGN D15): a plan v2 stage
//! disputes review findings, frozen contracts, test-integrity events or
//! failing impact-selected tests, one request for the whole batch.
//!
//! The client snapshots the disputed findings or events into the request; the
//! daemon checks every id again and records its own snapshot
//! (`stage_service/dispute_kinds.rs`). An impact-test dispute carries the ids
//! alone: the daemon copies each failure from the record loom's own gate run
//! wrote (`verify::impact_tests::observed`). The transport is the one
//! `dispute-criteria` uses (`dispute_transport.rs`).

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::dispute_transport::{send, Dispute};
use super::review_status::stage_worktree_and_target;
use crate::fs::safe_read::read_bounded;
use crate::fs::stage_request::StageRequest;
use crate::models::dispute::{
    select_events, select_findings, DisputeKind, FileReplacement, MAX_REPLACEMENT_BYTES,
};
use crate::relay::emit::{mode, EnvSnapshot, StdSink};
use crate::relay::{new_request_id, RequestKind, Ticket, MAX_TICKET_BYTES};
use crate::verify::contracts::normalize;
use crate::verify::contracts::site::stage_site;
use crate::verify::integrity::current_events;
use crate::verify::review::store::open_findings;
use crate::verify::transitions::load_stage;

/// What a dispute names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisputeTarget {
    /// Open review finding ids, own or carried.
    Findings(Vec<String>),
    /// Frozen contract ids, and `<frozen-file>=<path>` values naming the file
    /// proposed in place of each replaced frozen file.
    Contract {
        contract_ids: Vec<String>,
        replacements: Vec<String>,
    },
    /// Current test-integrity event ids.
    Integrity(Vec<String>),
    /// Ids of failing impact-selected tests, `IT-<runner>:<package>`.
    ImpactTests(Vec<String>),
}

/// One `loom stage dispute-{findings,contract,integrity}` or
/// `dispute-criteria --field impact-tests` invocation.
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
    ensure_fits_ticket(&kind, &filing.reason, filing.evidence_commit.as_ref())?;
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
        DisputeTarget::Contract {
            contract_ids,
            replacements,
        } => Ok(DisputeKind::Contract {
            contract_ids,
            replacements: read_replacements(work_dir, stage_id, &replacements)?,
        }),
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
        DisputeTarget::ImpactTests(test_ids) => Ok(DisputeKind::ImpactTests {
            test_ids,
            evidence: Vec::new(),
        }),
    }
}

/// The replacements `values` name, each `<frozen-file>=<path>`. A value
/// without `=`, with an empty side, or naming a frozen file a second time is
/// refused. The frozen file is kept in the freeze record's path form.
fn read_replacements(
    work_dir: &Path,
    stage_id: &str,
    values: &[String],
) -> Result<Vec<FileReplacement>> {
    let mut named = HashSet::new();
    let mut parsed = Vec::with_capacity(values.len());
    for value in values {
        let (file, path) = parse_replacement(value)?;
        if !named.insert(file.clone()) {
            bail!(
                "--replacement names the frozen file '{file}' twice: file one replacement per file"
            );
        }
        parsed.push((file, Path::new(path)));
    }
    if parsed.is_empty() {
        return Ok(Vec::new());
    }
    let worktree_root = stage_worktree_root(work_dir, stage_id)?;
    parsed
        .into_iter()
        .map(|(file, path)| {
            Ok(FileReplacement {
                content: read_replacement(&worktree_root, path)?,
                file,
                base_sha256: None,
            })
        })
        .collect()
}

/// The canonical root of the stage's worktree. A session reaches the state
/// directory through its worktree's `.loom/work` symlink; the repository root
/// is the one the resolved path names.
fn stage_worktree_root(work_dir: &Path, stage_id: &str) -> Result<PathBuf> {
    let stage = load_stage(stage_id, work_dir)?;
    let state_dir = work_dir
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", work_dir.display()))?;
    Ok(stage_site(&state_dir, &stage)?.0)
}

/// `<frozen-file>=<path>` split at its first `=`, the frozen file normalized.
fn parse_replacement(value: &str) -> Result<(String, &str)> {
    let Some((file, path)) = value.split_once('=') else {
        bail!("--replacement '{value}' is not `<frozen-file>=<path>`");
    };
    let file = normalize(file.trim());
    if file.is_empty() || path.is_empty() {
        bail!(
            "--replacement '{value}' needs both a frozen file and a path: `<frozen-file>=<path>`"
        );
    }
    Ok((file, path))
}

/// The replacement file at `path`: UTF-8 text of at most
/// [`MAX_REPLACEMENT_BYTES`], outside the stage worktree, read without
/// following a symlink at its last component.
///
/// A path that canonicalizes into the worktree is refused whatever it names
/// (`./` spellings and symlinks resolve first): the replacement is the daemon's
/// to write there on an accept, and the agent never edits a frozen file.
fn read_replacement(worktree_root: &Path, path: &Path) -> Result<String> {
    let resolved = path
        .canonicalize()
        .with_context(|| format!("cannot read the replacement file {}", path.display()))?;
    if resolved.starts_with(worktree_root) {
        bail!(
            "the replacement {} lies inside the stage worktree: copy the frozen file to \
             $LOOM_SCRATCH_DIR, edit the copy there and name that copy",
            path.display()
        );
    }
    let Some(name) = path.file_name() else {
        bail!("the replacement {} does not name a file", path.display());
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .with_context(|| format!("cannot read the replacement file {}", path.display()))?;
    let bytes =
        read_bounded(&parent, Path::new(name), MAX_REPLACEMENT_BYTES).with_context(|| {
            format!(
            "cannot read the replacement {} (a regular file of at most {MAX_REPLACEMENT_BYTES} \
             bytes; a symlink is refused)",
            path.display()
        )
        })?;
    String::from_utf8(bytes)
        .with_context(|| format!("the replacement {} is not valid UTF-8", path.display()))
}

/// Refuse a dispute whose replacements each fit [`MAX_REPLACEMENT_BYTES`] but
/// whose encoded ticket does not: several replacements share one ticket, and
/// JSON escaping expands a control character to six bytes. Measures the bytes
/// `RelayContext::emit` checks for the whole kind, before any ticket is
/// written.
fn ensure_fits_ticket(
    kind: &DisputeKind,
    reason: &str,
    evidence_commit: Option<&String>,
) -> Result<()> {
    let DisputeKind::Contract { replacements, .. } = kind else {
        return Ok(());
    };
    if replacements.is_empty() {
        return Ok(());
    }
    let request = StageRequest::FileDispute {
        kind: kind.clone(),
        reason: reason.to_string(),
        evidence_commit: evidence_commit.cloned(),
    };
    let ticket = Ticket {
        v: 1,
        id: new_request_id(),
        kind: RequestKind::FileDispute,
        created_at: chrono::Utc::now(),
        payload: serde_json::to_value(&request).context("failed to serialize the dispute")?,
    };
    let encoded = ticket.encode().len();
    if encoded > MAX_TICKET_BYTES {
        let total: usize = replacements.iter().map(|r| r.content.len()).sum();
        bail!(
            "the replacements are {total} bytes in all (limit {MAX_REPLACEMENT_BYTES} each), but \
             the dispute encodes to {encoded} bytes, over the {MAX_TICKET_BYTES}-byte request \
             limit (control characters and quotes expand when encoded): file the replacements \
             in separate filings, one file each, or shrink the replacement"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "dispute_replacement_tests.rs"]
mod replacement_tests;
