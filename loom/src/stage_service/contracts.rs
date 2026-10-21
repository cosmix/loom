//! The daemon's side of the contract freeze (DESIGN D8).
//!
//! Trust boundary: the contract session asks, the daemon decides, and the
//! daemon runs the contracts itself. [`handle_freeze_contracts`] admits a
//! request (the caller is the stage's running `Contract` session; the stage
//! is a v2 `standard` stage with contracts and no freeze yet, executing or
//! waiting for input, parked on the writer's refused freeze,
//! `verify::contracts::refusal`) and queues a `Freeze` host job for the HEAD
//! and tree the session saw. The job runs every contract on the host under
//! loom's own confinement (`host_verify::freeze`); applying a passing job
//! calls [`freeze_with_reports`] with the run's reports and the hashes of the
//! files it verified.
//!
//! Before it writes anything, [`freeze_with_reports`] checks, itself:
//!
//! 1. the caller is still the stage's running contract session, and the stage
//!    still takes a freeze;
//! 2. one red report per contract, each naming a known adapter if any;
//! 3. every path changed since the stage base is a contract file or matches a
//!    `harness` glob (D8 step 1, re-run with read-only git in the stage
//!    worktree, pinned to the stage's registered git directory so the
//!    worktree's `.git` file cannot choose git's configuration), and the
//!    worktree holds no FIFO, socket or device node
//!    outside git-ignored directories: git lists none, and one planted where
//!    the implementer will write blocks its first `open()`;
//! 4. every contract file exists (D8 step 2, while reading the files), and the
//!    files it reads are exactly the ones the run hashed, byte for byte, so the
//!    record holds what the run proved red.
//!
//! Once the checks pass, it copies the contract and harness files under
//! `.loom/work/contracts/<stage>/files/` and writes `freeze.json` last; its
//! appearance is what ends the contract phase. A freeze is the writer at
//! work, so a stage still waiting on an earlier refusal goes back to
//! `Executing`, where the handover to the implementer looks for it. The
//! operator's typing and the job's application can reach the stage in either
//! order, so the freeze cannot rely on the monitor having resumed it first.
//!
//! A refusal is a `Response::Error`, never an `Err`: the spool drain retries
//! an `Err` on every tick, and a request refused once is refused forever.
//! `Err` is left for a failure to queue the job or write the freeze itself.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::ownership::session_owns_stage_as;
use super::{ContractRunReport, Response};
use crate::git::worktree::WorktreeGit;
use crate::host_verify::submit::{submit, Submission, SubmitRequest};
use crate::host_verify::JobKind;
use crate::models::session::SessionType;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::relay::sha256_hex;
use crate::testrun::registry;
use crate::verify::contracts::frozen_set::read_frozen_set;
use crate::verify::contracts::refusal::end_wait;
use crate::verify::contracts::run_dir::resolve_run;
use crate::verify::contracts::store::{
    self, FreezeRecord, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use crate::verify::contracts::{
    changes, is_contract_or_harness, normalize, resolve_adapter, site::stage_site, RED_OUTCOMES,
};
use crate::verify::transitions::{load_stage, update_stage};

/// Queue the host contract run of `stage_id` for its contract writer
/// `session_id`, against the `head` and `tree_digest` the writer saw. A job
/// already running for the same tree is joined; one for another tree is
/// superseded. `Response::Ok` once queued or joined.
pub(crate) fn handle_freeze_contracts(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    head: &str,
    tree_digest: &str,
    request_id: &str,
) -> Result<Response> {
    if let Err(error) = crate::validation::validate_id(stage_id) {
        return Ok(refused(format!("invalid stage_id: {error}")));
    }
    let work_dir = canonical(work_dir)?;
    if let Err(error) = admit(&work_dir, stage_id, session_id) {
        return Ok(refused(format!("{error:#}")));
    }
    let request = SubmitRequest {
        stage_id,
        session_id,
        request_id,
        kind: JobKind::Freeze,
        head,
        tree_digest,
        no_cache: false,
        criteria: None,
    };
    Ok(match submit(&work_dir, &request, Utc::now())? {
        Submission::Queued(_) | Submission::Joined(_) | Submission::Superseded { .. } => {
            Response::Ok
        }
        Submission::Refused(message) => refused(message),
    })
}

/// Freeze the contracts of `stage_id` from a host run's `reports`. `verified`
/// is the `(path, sha256)` list of the files the run hashed; the freeze is
/// refused unless the files read now are exactly those.
pub(crate) fn freeze_with_reports(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    reports: &[ContractRunReport],
    verified: &[(String, String)],
) -> Result<Response> {
    if let Err(error) = crate::validation::validate_id(stage_id) {
        return Ok(refused(format!("invalid stage_id: {error}")));
    }
    let work_dir = canonical(work_dir)?;
    let (record, files) = match prepare(&work_dir, stage_id, session_id, reports, verified) {
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

fn canonical(work_dir: &Path) -> Result<PathBuf> {
    work_dir
        .canonicalize()
        .with_context(|| format!("Failed to canonicalize work_dir {}", work_dir.display()))
}

/// The stage, once `session_id` is shown to be its running contract session
/// and the stage still takes a freeze.
fn admit(work_dir: &Path, stage_id: &str, session_id: &str) -> Result<Stage> {
    session_owns_stage_as(work_dir, stage_id, session_id, &[SessionType::Contract])
        .context("only the stage's running contract session may freeze its contracts")?;
    let stage = load_stage(stage_id, work_dir)?;
    check_stage(work_dir, &stage)?;
    Ok(stage)
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

/// Where the stage's files are: git pinned to the stage worktree, its root
/// and, beneath it, the working directory contract paths are relative to.
pub(crate) struct Site {
    pub(crate) git: WorktreeGit,
    pub(crate) worktree_root: PathBuf,
    pub(crate) working_dir: PathBuf,
}

type Prepared = (FreezeRecord, Vec<(String, Vec<u8>)>);

fn prepare(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    reports: &[ContractRunReport],
    verified: &[(String, String)],
) -> Result<Prepared> {
    let stage = admit(work_dir, stage_id, session_id)?;
    check_reports(&stage, reports)?;
    let site = locate(work_dir, &stage)?;
    let base = changes::stage_base(&site.git, work_dir)?;
    check_changes(&site, &stage, &base)?;
    let files = read_files(&site, &stage)?;
    check_verified(&files, verified)?;
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
        contracts: frozen_contracts(&site, &stage, reports)?,
    };
    Ok((record, files))
}

/// Each contract's run report with its file and the directory it runs from,
/// which the completion check then runs it from too while the contract names
/// that file. The directory is resolved here as the host run resolved it.
fn frozen_contracts(
    site: &Site,
    stage: &Stage,
    reports: &[ContractRunReport],
) -> Result<Vec<FrozenContract>> {
    let mut frozen = Vec::with_capacity(stage.contracts.len());
    for contract in &stage.contracts {
        let Some(report) = reports.iter().find(|r| r.contract_id == contract.id) else {
            continue;
        };
        let adapter = resolve_adapter(contract, &site.working_dir);
        let run = resolve_run(
            contract,
            adapter,
            &site.worktree_root,
            &site.working_dir,
            None,
        )?;
        frozen.push(FrozenContract {
            file: Some(normalize(&contract.file)),
            cwd: Some(run.cwd),
            ..FrozenContract::from(report)
        });
    }
    Ok(frozen)
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

pub(crate) fn locate(work_dir: &Path, stage: &Stage) -> Result<Site> {
    let (worktree_root, working_dir) = stage_site(work_dir, stage)?;
    Ok(Site {
        git: WorktreeGit::pinned_in_project_of(work_dir, &worktree_root)?,
        worktree_root,
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

/// Every contract file (each must exist) and every existing harness match
/// (`frozen_set::read_frozen_set`).
pub(crate) fn read_files(site: &Site, stage: &Stage) -> Result<Vec<(String, Vec<u8>)>> {
    let harness = changes::harness_files(&site.git, &site.working_dir, &stage.harness)?;
    read_frozen_set(&site.working_dir, &stage.contracts, &harness, &[])
}

/// The files read now must be the ones the run hashed: the same paths, each
/// with the same sha256.
fn check_verified(files: &[(String, Vec<u8>)], verified: &[(String, String)]) -> Result<()> {
    let read: BTreeMap<&str, String> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), sha256_hex(bytes)))
        .collect();
    let run: BTreeMap<&str, &str> = verified
        .iter()
        .map(|(path, sha256)| (path.as_str(), sha256.as_str()))
        .collect();
    let paths: BTreeSet<&str> = read.keys().chain(run.keys()).copied().collect();
    let differing: Vec<&str> = paths
        .into_iter()
        .filter(|path| read.get(path).map(String::as_str) != run.get(path).copied())
        .collect();
    if !differing.is_empty() {
        bail!(
            "the contract and harness files differ from the ones the contract run verified: {}",
            differing.join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "../daemon/server/contracts_tests.rs"]
mod tests;
