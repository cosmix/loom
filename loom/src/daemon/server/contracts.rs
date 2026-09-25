//! Server-side handler for `Request::FreezeContracts` (DESIGN D8).
//!
//! Trust boundary: the contract session asks, the daemon decides. Before it
//! writes anything the handler checks, itself:
//!
//! 1. the caller is the stage's running `Contract` session;
//! 2. the stage is a v2 `standard` stage with contracts and no freeze yet,
//!    executing or waiting for input (parked on the writer's refused freeze,
//!    `verify::contracts::refusal`);
//! 3. every path changed since the stage base is a contract file or matches a
//!    `harness` glob (D8 step 1, re-run with read-only git in the stage
//!    worktree, pinned to the stage's registered git directory so the
//!    worktree's `.git` file cannot choose git's configuration), and the
//!    worktree holds no FIFO, socket or device node
//!    outside git-ignored directories: git lists none, and one planted where
//!    the implementer will write blocks its first `open()`;
//! 4. every contract file exists (D8 step 2, while reading the files).
//!
//! It does not repeat D8 step 3, the red run. Running a contract executes
//! test code the agent wrote, and the daemon runs outside the sandbox, so it
//! never runs one. The outcomes are taken from the sandboxed CLI's reports;
//! the handler checks only their shape: one report per contract, a red
//! outcome, a known adapter name. Before the stage can complete, the
//! completion check (D9) confirms every frozen file is unchanged and re-runs
//! every contract.
//!
//! Once the checks pass, it copies the contract and harness files under
//! `.loom/work/contracts/<stage>/files/` and writes `freeze.json` last; its
//! appearance is what ends the contract phase. A freeze is the writer at
//! work, so a stage still waiting on an earlier refusal goes back to
//! `Executing`, where the handover to the implementer looks for it. The
//! operator's typing and the writer's first tool call can reach the daemon
//! in either order, so the freeze cannot rely on the monitor having resumed
//! the stage first.
//!
//! A refusal is a `Response::Error`, never an `Err`: the spool drain retries
//! an `Err` on every tick, and a request refused once is refused forever.
//! `Err` is left for a failure to write the freeze itself.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::self_service::session_owns_stage_as;
use crate::daemon::protocol::{ContractRunReport, Response};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::git::worktree::WorktreeGit;
use crate::models::session::SessionType;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::relay::sha256_hex;
use crate::testrun::registry;
use crate::verify::contracts::refusal::end_wait;
use crate::verify::contracts::store::{
    self, FreezeRecord, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION, MAX_FROZEN_FILE_BYTES,
};
use crate::verify::contracts::{
    changes, is_contract_or_harness, normalize, site::stage_site, RED_OUTCOMES,
};
use crate::verify::transitions::{load_stage, update_stage};

/// Bounds on what one freeze copies into `.loom/work`: the harness globs
/// come from the plan, but the files they match are the agent's.
const MAX_FROZEN_FILES: usize = 500;
const MAX_FROZEN_TOTAL_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn handle_freeze_contracts(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    reports: &[ContractRunReport],
) -> Result<Response> {
    if let Err(error) = crate::validation::validate_id(stage_id) {
        return Ok(refused(format!("invalid stage_id: {error}")));
    }
    let work_dir = work_dir
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize work_dir {}", work_dir.display()))?;
    let (record, files) = match prepare(&work_dir, stage_id, session_id, reports) {
        Ok(prepared) => prepared,
        Err(error) => return Ok(refused(format!("{error:#}"))),
    };
    store::write_freeze(&work_dir, &record, &files)?;
    if let Err(error) = end_refusal_wait(&work_dir, stage_id, session_id) {
        // The freeze stands; only the resume is missing, and the operator
        // has `loom stage resume` for it.
        tracing::warn!(
            stage_id = %stage_id,
            error = %format!("{error:#}"),
            "Froze the contracts, but the stage stays waiting for input"
        );
    }
    Ok(Response::ContractsFrozen { files: files.len() })
}

fn refused(message: String) -> Response {
    Response::Error { message }
}

/// Take a stage parked on its writer's refused freeze back to `Executing`
/// once the same writer has frozen.
fn end_refusal_wait(work_dir: &Path, stage_id: &str, session_id: &str) -> Result<()> {
    update_stage(stage_id, work_dir, |stage| {
        let parked = stage.status == StageStatus::WaitingForInput
            && stage.session.as_deref() == Some(session_id);
        if parked {
            end_wait(stage)?;
        }
        Ok(())
    })?;
    Ok(())
}

/// Where the stage's files are: git pinned to the stage worktree and,
/// beneath the worktree, the working directory contract paths are relative
/// to.
struct Site {
    git: WorktreeGit,
    working_dir: PathBuf,
}

type Prepared = (FreezeRecord, Vec<(String, Vec<u8>)>);

fn prepare(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    reports: &[ContractRunReport],
) -> Result<Prepared> {
    session_owns_stage_as(work_dir, stage_id, session_id, &[SessionType::Contract])
        .context("only the stage's running contract session may freeze its contracts")?;
    let stage = load_stage(stage_id, work_dir)?;
    check_stage(work_dir, &stage)?;
    check_reports(&stage, reports)?;
    let site = locate(work_dir, &stage)?;
    let base = changes::stage_base(&site.git, work_dir)?;
    check_changes(&site, &stage, &base)?;
    let files = read_files(&site, &stage)?;
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        frozen_at: Utc::now(),
        base,
        files: files
            .iter()
            .map(|(path, bytes)| FrozenFile {
                path: path.clone(),
                sha256: sha256_hex(bytes),
            })
            .collect(),
        contracts: stage
            .contracts
            .iter()
            .filter_map(|contract| reports.iter().find(|r| r.contract_id == contract.id))
            .map(FrozenContract::from)
            .collect(),
    };
    Ok((record, files))
}

fn check_stage(work_dir: &Path, stage: &Stage) -> Result<()> {
    if !matches!(
        stage.status,
        StageStatus::Executing | StageStatus::WaitingForInput
    ) {
        bail!("stage '{}' is {}, not executing", stage.id, stage.status);
    }
    if stage.plan_version != 2
        || stage.stage_type != StageType::Standard
        || stage.contracts.is_empty()
    {
        bail!("stage '{}' has no contracts to freeze", stage.id);
    }
    if store::load_freeze(work_dir, &stage.id)?.is_some() {
        bail!("the contracts of stage '{}' are already frozen", stage.id);
    }
    Ok(())
}

/// One report per contract, each red, each naming a known adapter if any.
fn check_reports(stage: &Stage, reports: &[ContractRunReport]) -> Result<()> {
    for contract in &stage.contracts {
        let mut matching = reports.iter().filter(|r| r.contract_id == contract.id);
        let (Some(report), None) = (matching.next(), matching.next()) else {
            bail!("contract '{}' needs exactly one run report", contract.id);
        };
        if !RED_OUTCOMES.contains(&report.outcome.as_str()) {
            bail!(
                "contract '{}' is reported '{}'; a contract is frozen only while it fails",
                contract.id,
                report.outcome
            );
        }
        if let Some(adapter) = report.adapter.as_deref() {
            if registry::by_name(adapter).is_none() {
                bail!(
                    "contract '{}' names unknown adapter '{adapter}'",
                    contract.id
                );
            }
        }
    }
    if let Some(stray) = reports
        .iter()
        .find(|r| !stage.contracts.iter().any(|c| c.id == r.contract_id))
    {
        bail!(
            "stage '{}' has no contract '{}'",
            stage.id,
            stray.contract_id
        );
    }
    Ok(())
}

fn locate(work_dir: &Path, stage: &Stage) -> Result<Site> {
    let (worktree_root, working_dir) = stage_site(work_dir, stage)?;
    Ok(Site {
        git: WorktreeGit::pinned_in_project_of(work_dir, &worktree_root)?,
        working_dir,
    })
}

/// DESIGN D8 step 1, re-run by the daemon, and the special files git cannot
/// show it (`changes::special_files`).
fn check_changes(site: &Site, stage: &Stage, base: &str) -> Result<()> {
    let special = changes::special_files(&site.git, &site.working_dir)?;
    if !special.is_empty() {
        bail!(
            "the contract session may not leave a FIFO, socket or device node; found: {}",
            special.join(", ")
        );
    }
    let changed = changes::changed_paths(&site.git, &site.working_dir, base)?;
    let outside: Vec<String> = changed
        .into_iter()
        .filter(|path| !is_contract_or_harness(path, &stage.contracts, &stage.harness))
        .collect();
    if !outside.is_empty() {
        bail!(
            "the contract session may change only contract and harness files; also changed: {}",
            outside.join(", ")
        );
    }
    Ok(())
}

/// Every contract file (each must exist) and every existing harness match,
/// read without following a symlink anywhere beneath the working directory.
fn read_files(site: &Site, stage: &Stage) -> Result<Vec<(String, Vec<u8>)>> {
    let contract_files: BTreeSet<String> =
        stage.contracts.iter().map(|c| normalize(&c.file)).collect();
    let harness = changes::harness_files(&site.git, &site.working_dir, &stage.harness)?;
    let paths: BTreeSet<&String> = contract_files.iter().chain(&harness).collect();
    if paths.len() > MAX_FROZEN_FILES {
        bail!(
            "{} files match the contracts and harness; at most {MAX_FROZEN_FILES} can be frozen",
            paths.len()
        );
    }
    let mut files = Vec::with_capacity(paths.len());
    let mut total = 0usize;
    for path in paths {
        let bytes = match read_bounded(&site.working_dir, Path::new(path), MAX_FROZEN_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if is_not_found(&error) && !contract_files.contains(path) => continue,
            Err(error) => return Err(error.context(format!("cannot freeze '{path}'"))),
        };
        total += bytes.len();
        if total > MAX_FROZEN_TOTAL_BYTES {
            bail!("the frozen files exceed {MAX_FROZEN_TOTAL_BYTES} bytes together");
        }
        files.push((path.clone(), bytes));
    }
    Ok(files)
}

#[cfg(test)]
#[path = "contracts_tests.rs"]
mod tests;
