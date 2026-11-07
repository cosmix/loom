//! The `Freeze` host job (common.md 4.3) and its application (4.6).
//!
//! A contract writer's freeze request queues this job for the HEAD and tree
//! the writer saw. The child checks the changes with git pinned to the
//! stage's registered git directory, binds the run to HEAD, the tree digest,
//! the hashes of the files the freeze will record and the stage's contract
//! definitions, runs every contract and the format gate on the host, and
//! checks the binding again: anything that moved during the run is an error.
//! A passing run leaves `contract-reports.json` in its job directory.
//!
//! The job and its application overlay the stage's contracts with the
//! writer's saved choices (`choices::load_effective_stage`), so the contracts
//! run, bound and frozen are the ones the writer chose. The job checks its
//! policy binding on the stage record before the overlay, as the runner
//! computed it.
//!
//! Applying the job freezes the contracts from those reports only while the
//! binding still holds. The writer's freeze command waits for the job and its
//! application: a failed job whose only red outcome is the static gate is
//! returned to the waiting writer (`refuse_failed`), and so is a run
//! interrupted or stopped by the daemon (`return_infrastructure`), until the
//! session's third such refusal. Any other refusal is recorded for the tick
//! step to file as a `freeze-refusal` dispute. A run interrupted or stopped by
//! the daemon, or whose reports or binding cannot be read, is no one's fault,
//! and its first problem says so (`INFRASTRUCTURE_PREFIX`). Any other cancel,
//! and a stage already frozen, record nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::apply::Applied;
use super::freeze_memo::run_memoized;
use super::freeze_prepare::prepare_planned;
use super::freeze_refuse::{
    failure_message, refuse, refuse_failed, refuse_infrastructure, return_infrastructure,
};
use super::runner::DAEMON_STOPPED;
use super::{child, digest, store, CriterionOutcome, JobRecord, JobResult, JobState};
use crate::daemon::ContractRunReport;
use crate::models::stage::{CommandConfinement, Stage};
use crate::plan::schema::ContractSpec;
use crate::relay::sha256_hex;
use crate::stage_service::contracts::{
    freeze_with_reports, locate, read_files, revalidate_saved, Site,
};
use crate::stage_service::Response;
use crate::verify::contracts::choices::{effective_contracts, load_effective_stage};
use crate::verify::contracts::freeze::{
    check_site_changes, run_prepared, written_paths, PreparedContract, BUILD_FAILED, UNVERIFIED,
};
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::contracts::site::ContractSite;
use crate::verify::contracts::store::load_freeze;
use crate::verify::criteria::{plan_confinement, resolve_confinement};
use crate::verify::transitions::load_stage;

#[path = "freeze_format.rs"]
mod format;
use format::{format_outcome, outcome, verdict_line};

/// The job file a passing run leaves for its application.
pub(crate) const REPORTS_FILE: &str = "contract-reports.json";

/// One file the run hashed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedFile {
    path: String,
    sha256: String,
}

/// `<job dir>/contract-reports.json`: what the run was bound to (HEAD, the
/// tree digest, the stage's contract definitions and the files the freeze
/// will record, in path order), and its reports. A binding captured from the
/// worktree has no reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractReports {
    head: String,
    tree_digest: String,
    contracts_digest: String,
    files: Vec<VerifiedFile>,
    reports: Vec<ContractRunReport>,
}

impl ContractReports {
    /// The binding of the stage's worktree as it stands now, read through
    /// the site's pinned git (the child and the daemon run outside the
    /// stage's sandbox).
    fn capture(site: &Site, stage: &Stage) -> Result<Self> {
        let files = read_files(site, stage)?
            .iter()
            .map(|(path, bytes)| VerifiedFile {
                path: path.clone(),
                sha256: sha256_hex(bytes),
            })
            .collect();
        Ok(Self {
            head: digest::head_in(&site.git)?,
            tree_digest: digest::tree_digest_in(&site.git)?,
            contracts_digest: contracts_digest(stage)?,
            files,
            reports: Vec::new(),
        })
    }

    /// How the binding `now` differs from this one, naming each part that
    /// moved. Reports are not part of a binding.
    fn changes(&self, now: &Self) -> Option<String> {
        let mut found = Vec::new();
        if self.head != now.head {
            found.push(format!("HEAD moved from {} to {}", self.head, now.head));
        }
        if self.tree_digest != now.tree_digest {
            found.push(format!(
                "the tree digest changed from {} to {}",
                self.tree_digest, now.tree_digest
            ));
        }
        let (was, is) = (hashes(&self.files), hashes(&now.files));
        let paths: BTreeSet<&str> = was.keys().chain(is.keys()).copied().collect();
        let changed: Vec<&str> = paths
            .into_iter()
            .filter(|path| was.get(path) != is.get(path))
            .collect();
        if !changed.is_empty() {
            let changed = changed.join(", ");
            found.push(format!("contract or harness files changed: {changed}"));
        }
        if self.contracts_digest != now.contracts_digest {
            found.push("the stage's contract definitions changed".to_string());
        }
        (!found.is_empty()).then(|| found.join("; "))
    }
}

fn hashes(files: &[VerifiedFile]) -> BTreeMap<&str, &str> {
    let pairs = files.iter();
    pairs
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect()
}

/// sha256 of the stage's effective contract definitions.
fn contracts_digest(stage: &Stage) -> Result<String> {
    let bytes = serde_json::to_vec(&stage.contracts).context("cannot serialize the contracts")?;
    Ok(sha256_hex(&bytes))
}

/// Run the freeze job `record` and write its result. A failure of the run is
/// the result's `error`; `Err` means the result itself could not be written.
pub fn run_freeze_job(work_dir: &Path, record: &JobRecord) -> Result<JobResult> {
    let mut outcomes = Vec::new();
    let checked = run(work_dir, record, &mut outcomes);
    let result = child::finish(record, outcomes, checked, None);
    store::save_result(work_dir, &record.stage_id, &result)?;
    Ok(result)
}

/// The freeze pipeline. `Ok(Some(change))` names what moved during the run.
fn run(
    work_dir: &Path,
    record: &JobRecord,
    outcomes: &mut Vec<CriterionOutcome>,
) -> Result<Option<String>> {
    let mut stage = load_stage(&record.stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{}'", record.stage_id))?;
    // The runner bound the job to the stage record's policy, not the overlay.
    super::child_policy::ensure_bound_stage(&stage, work_dir)?;
    let planned = stage.contracts.clone();
    stage.contracts = effective_contracts(work_dir, &stage)?;
    let site = locate(work_dir, &stage)?;
    let contract_site = contract_site_of(work_dir, &stage, &site);
    check_site_changes(&contract_site, &site.git)?;
    let mut before = ContractReports::capture(&site, &stage)?;
    if let Some(mismatch) = request_mismatch(&before, record) {
        bail!("the tree does not match the request: {mismatch}");
    }
    // Admission judged the worktree as it stood then; the tree the job runs on
    // is the request's, so the writer's choices must pass on it too.
    revalidate_saved(work_dir, &record.stage_id, &planned, &site)?;
    let confinement = resolve_confinement(
        stage.sandbox.command_confinement,
        plan_confinement(work_dir),
    );
    let reports = run_contracts(
        work_dir,
        record,
        &site,
        &stage,
        &planned,
        confinement,
        outcomes,
    )?;
    let gate = static_outcome(&contract_site, &site, confinement, &reports)?;
    outcomes.push(gate);
    let reloaded = load_effective_stage(&record.stage_id, work_dir)?;
    if let Some(change) = before.changes(&ContractReports::capture(&site, &reloaded)?) {
        return Ok(Some(change));
    }
    before.reports = reports;
    save_reports(work_dir, record, &before)?;
    Ok(None)
}

fn contract_site_of(work_dir: &Path, stage: &Stage, site: &Site) -> ContractSite {
    let (root, working) = (site.worktree_root.clone(), site.working_dir.clone());
    ContractSite::resolved(work_dir.to_path_buf(), stage.clone(), root, working)
}

fn save_reports(work_dir: &Path, record: &JobRecord, bound: &ContractReports) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(bound).context("cannot serialize the contract reports")?;
    store::save_job_file(work_dir, &record.stage_id, &record.id, REPORTS_FILE, &bytes)
}

/// The static gate's outcome over the files the run wrote; the build must
/// have compiled for it to be judged.
fn static_outcome(
    contract_site: &ContractSite,
    site: &Site,
    confinement: CommandConfinement,
    reports: &[ContractRunReport],
) -> Result<CriterionOutcome> {
    let written = written_paths(contract_site, &site.git)?;
    let compiles = reports
        .iter()
        .all(|r| r.outcome != BUILD_FAILED && r.outcome != UNVERIFIED);
    format_outcome(contract_site, &site.git, confinement, &written, compiles)
}

fn request_mismatch(binding: &ContractReports, record: &JobRecord) -> Option<String> {
    let mut found = Vec::new();
    if binding.head != record.head {
        found.push(format!(
            "HEAD is {}, the request named {}",
            binding.head, record.head
        ));
    }
    if binding.tree_digest != record.tree_digest {
        found.push(format!(
            "tree digest is {}, the request saw {}",
            binding.tree_digest, record.tree_digest
        ));
    }
    (!found.is_empty()).then(|| found.join("; "))
}

/// Run every contract; each is an outcome `contract:<id>` that passes when
/// the run is red, as the freeze requires, and is appended to the job's
/// progress the moment it is known. Returns the red runs' reports. `planned`
/// is the stage's contracts before the writer's choices.
fn run_contracts(
    work_dir: &Path,
    record: &JobRecord,
    site: &Site,
    stage: &Stage,
    planned: &[ContractSpec],
    confinement: CommandConfinement,
    outcomes: &mut Vec<CriterionOutcome>,
) -> Result<Vec<ContractRunReport>> {
    let mut reports = Vec::new();
    run_memoized(
        &stage.contracts,
        |contract| prepare_planned(&site.worktree_root, &site.working_dir, planned, contract),
        PreparedContract::build_key,
        |contract, prepared| {
            run_prepared(
                &site.worktree_root,
                contract,
                prepared,
                confinement,
                |line| println!("{line}"),
            )
        },
        |contract, verdict| {
            let (passed, summary) = match verdict.judged {
                Ok(report) => {
                    let line = verdict_line(&report, verdict.shared_from.as_deref());
                    reports.push(report);
                    (true, line)
                }
                Err(problem) => (false, problem),
            };
            let label = format!("contract:{}", contract.id);
            let done = outcome(label, passed, &summary);
            // Progress is a courtesy to the waiting writer: the final result
            // carries every outcome, so a failed append is logged only.
            if let Err(error) =
                store::append_progress(work_dir, &record.stage_id, &record.id, &done)
            {
                let job = &record.id;
                let error = format!("{error:#}");
                tracing::warn!(job = %job, error = %error, "cannot record contract progress");
            }
            outcomes.push(done);
        },
    )?;
    Ok(reports)
}

/// Apply a finished freeze job: freeze from a passing run whose binding
/// still holds, record the refusal for anything else that ran, and leave a
/// cancelled job or a frozen stage alone. `Err` is an I/O failure a later
/// attempt may not hit, a refusal that could not be recorded included.
pub fn apply_freeze_result(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    if load_freeze(work_dir, &record.stage_id)?.is_some() {
        return Ok(Applied::Ignored("already frozen".to_string()));
    }
    match record.state {
        JobState::Passed => apply_passed(work_dir, record),
        JobState::Failed => refuse_failed(work_dir, record),
        JobState::Interrupted => {
            let failure = failure_message(work_dir, record);
            let message = format!("the contract run was interrupted\n{failure}");
            return_infrastructure(work_dir, record, &message)
        }
        // The shutdown's cancel leaves no successor job: the waiting writer
        // reads `Host job: stopped` and runs the freeze again, so the run is
        // returned to it like an interrupted one.
        JobState::Cancelled if record.detail.as_deref() == Some(DAEMON_STOPPED) => {
            let message = "the daemon stopped during the contract run; rerun the freeze";
            return_infrastructure(work_dir, record, message)
        }
        JobState::Cancelled => Ok(Applied::Ignored(format!(
            "cancelled: {}",
            record.detail.as_deref().unwrap_or("no reason recorded")
        ))),
        JobState::Queued | JobState::Running => Ok(Applied::Ignored(
            "the contract run has not finished".to_string(),
        )),
    }
}

fn apply_passed(work_dir: &Path, record: &JobRecord) -> Result<Applied> {
    let reports = match load_reports(work_dir, record) {
        Ok(reports) => reports,
        Err(error) => {
            let message = format!("the contract run's reports cannot be read: {error:#}");
            return refuse_infrastructure(work_dir, record, &message);
        }
    };
    let moved = load_effective_stage(&record.stage_id, work_dir)
        .and_then(|stage| ContractReports::capture(&locate(work_dir, &stage)?, &stage))
        .map(|now| reports.changes(&now));
    match moved {
        Ok(None) => {}
        Ok(Some(change)) => {
            let message = format!("the stage changed after its contract run: {change}");
            return refuse(work_dir, record, &message);
        }
        Err(error) => {
            // The error can name worktree files and quote git: one quoted line.
            let why = run_output_line(&format!("{error:#}"));
            let message = format!("the stage cannot be checked against its contract run\n{why}");
            return refuse_infrastructure(work_dir, record, &message);
        }
    }
    let verified: Vec<(String, String)> = reports
        .files
        .into_iter()
        .map(|file| (file.path, file.sha256))
        .collect();
    let response = freeze_with_reports(
        work_dir,
        &record.stage_id,
        &record.session_id,
        &reports.reports,
        &verified,
    )?;
    match response {
        Response::ContractsFrozen { files } => Ok(Applied::FreezeAccepted { files }),
        Response::Error { message } => refuse(work_dir, record, &message),
        other => refuse(
            work_dir,
            record,
            &format!("unexpected freeze answer: {other:?}"),
        ),
    }
}

fn load_reports(work_dir: &Path, record: &JobRecord) -> Result<ContractReports> {
    let bytes = store::load_job_file(work_dir, &record.stage_id, &record.id, REPORTS_FILE)?
        .with_context(|| format!("{REPORTS_FILE} is missing"))?;
    serde_json::from_slice(&bytes).with_context(|| format!("{REPORTS_FILE} is malformed"))
}

#[cfg(test)]
#[path = "tests_freeze_static.rs"]
mod static_tests;
