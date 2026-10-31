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
//! Applying the job freezes the contracts from those reports only while the
//! binding still holds, and otherwise records the refusal, which the tick
//! step files as a `freeze-refusal` dispute: the relayed request told the
//! writer to end its turn. A run interrupted or stopped by the daemon, or
//! whose reports or binding cannot be read, is no one's fault, and its first
//! problem says so (`INFRASTRUCTURE_PREFIX`). Any other cancel, and a stage
//! already frozen, record nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::apply::Applied;
use super::freeze_memo::run_memoized;
use super::freeze_refuse::{failure_message, REFUSAL_MAX_BYTES};
use super::runner::DAEMON_STOPPED;
use super::{child, digest, store, CriterionOutcome, JobRecord, JobResult, JobState};
use crate::daemon::ContractRunReport;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::{CommandConfinement, Stage};
use crate::relay::sha256_hex;
use crate::stage_service::contracts::{freeze_with_reports, locate, read_files, Site};
use crate::stage_service::Response;
use crate::verify::contracts::freeze::{
    check_site_changes, prepare_contract, run_prepared, PreparedContract,
};
use crate::verify::contracts::refusal::{
    problem_lines, record_daemon_refusal, run_output_line, RefusalSource, INFRASTRUCTURE_PREFIX,
};
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
    let stage = load_stage(&record.stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{}'", record.stage_id))?;
    super::child_policy::ensure_bound_stage(&stage, work_dir)?;
    let site = locate(work_dir, &stage)?;
    let contract_site = ContractSite::resolved(
        work_dir.to_path_buf(),
        stage.clone(),
        site.worktree_root.clone(),
        site.working_dir.clone(),
    );
    check_site_changes(&contract_site, &site.git)?;
    let mut before = ContractReports::capture(&site, &stage)?;
    if let Some(mismatch) = request_mismatch(&before, record) {
        bail!("the tree does not match the request: {mismatch}");
    }
    let confinement = resolve_confinement(
        stage.sandbox.command_confinement,
        plan_confinement(work_dir),
    );
    let reports = run_contracts(work_dir, record, &site, &stage, confinement, outcomes)?;
    outcomes.push(format_outcome(&contract_site, confinement)?);
    let reloaded = load_stage(&record.stage_id, work_dir)?;
    if let Some(change) = before.changes(&ContractReports::capture(&site, &reloaded)?) {
        return Ok(Some(change));
    }
    before.reports = reports;
    let bytes =
        serde_json::to_vec_pretty(&before).context("cannot serialize the contract reports")?;
    store::save_job_file(work_dir, &record.stage_id, &record.id, REPORTS_FILE, &bytes)?;
    Ok(None)
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
/// progress the moment it is known. Returns the red runs' reports.
fn run_contracts(
    work_dir: &Path,
    record: &JobRecord,
    site: &Site,
    stage: &Stage,
    confinement: CommandConfinement,
    outcomes: &mut Vec<CriterionOutcome>,
) -> Result<Vec<ContractRunReport>> {
    let mut reports = Vec::new();
    run_memoized(
        &stage.contracts,
        |contract| prepare_contract(&site.worktree_root, &site.working_dir, contract),
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
        JobState::Failed => refuse(work_dir, record, &failure_message(work_dir, record)),
        JobState::Interrupted => {
            let failure = failure_message(work_dir, record);
            let message = format!("the contract run was interrupted\n{failure}");
            refuse_infrastructure(work_dir, record, &message)
        }
        // The shutdown's cancel leaves no successor job, so no one else would
        // act on the writer's ended turn: refuse like an interrupted run.
        JobState::Cancelled if record.detail.as_deref() == Some(DAEMON_STOPPED) => {
            let message = "the daemon stopped during the contract run; rerun the freeze";
            refuse_infrastructure(work_dir, record, message)
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
    let moved = load_stage(&record.stage_id, work_dir)
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

/// Record the refusal `message`, cut to 4 KiB, for the job's session: one
/// problem per line. The tick step files it while that session is still the
/// stage's contract writer, and drops it otherwise.
fn refuse(work_dir: &Path, record: &JobRecord, message: &str) -> Result<Applied> {
    let message = truncate_to_byte_limit(message, REFUSAL_MAX_BYTES);
    record_daemon_refusal(
        work_dir,
        &record.stage_id,
        &record.session_id,
        &problem_lines(&message),
        RefusalSource::Daemon,
    )?;
    Ok(Applied::FreezeRefused(message))
}

/// [`refuse`] a run that failed through no one's change.
fn refuse_infrastructure(work_dir: &Path, record: &JobRecord, message: &str) -> Result<Applied> {
    refuse(
        work_dir,
        record,
        &format!("{INFRASTRUCTURE_PREFIX}{message}"),
    )
}
