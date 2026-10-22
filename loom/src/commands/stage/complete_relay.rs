//! `loom stage complete` inside a session. A session cannot run the confined
//! criteria itself (it cannot nest the sandbox), and the daemon takes no
//! verdict from it: the daemon runs the completion on the host as a host job
//! and completes the stage only from its own passing run. A `Relay` session
//! requests the job and follows it (`host_verify::wait`), then follows the
//! daemon's application of a passed run; a `Legacy` one is refused; the
//! operator's shell completes locally.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::fs::work_dir::WorkDir;
use crate::git::worktree::stage_worktree_path;
use crate::handoff::completion::content::committed_content;
use crate::handoff::generator::load_session_checkpoint;
use crate::handoff::schema::CompletionPhase;
use crate::host_verify::apply::APPLIED_MARKER;
use crate::host_verify::wait::{self, TicketSink, WaitOutcome, WaitSite};
use crate::host_verify::{store, JobKind, JobRecord, JobState};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::relay::emit::{RelayContext, RelayMode};
use crate::relay::{HostJobRequest, RequestKind};
use crate::verify::transitions::load_stage;

const POLL: Duration = Duration::from_secs(1);
const PRIVILEGED_REFUSAL: &str = "sandboxed completion runs verification only; --no-verify, \
                                  --force-unsafe and --assume-merged are refused";
const COMMIT_FIRST: &str = "Commit first: completion verifies committed content only.";

/// What `loom stage complete` was asked, as far as a session's route needs.
pub(crate) struct SessionCall<'a> {
    pub stage_id: &'a str,
    pub session_id: Option<&'a str>,
    pub privileged: bool,
    pub no_cache: bool,
    pub work_dir: &'a Path,
    pub cwd: &'a Path,
}

/// Route the command by where it runs: `None` for the operator's shell,
/// which completes locally; otherwise the exit code of the session's outcome,
/// whose lines are already written to `out` (the first one always
/// `Host job: <state> (complete, <short head>)`).
pub(crate) fn route(
    mode: RelayMode,
    call: &SessionCall<'_>,
    out: &mut dyn Write,
) -> Result<Option<i32>> {
    let code = match mode {
        RelayMode::Operator => return Ok(None),
        RelayMode::Legacy => {
            let message = format!(
                "This session cannot relay completion (no LOOM_SCRATCH_DIR). Run loom stage \
                 complete {stage} from the operator shell, or loom stage retry {stage} for a \
                 session that can.",
                stage = call.stage_id
            );
            refuse(out, call.cwd, &message)?
        }
        RelayMode::Relay(context) => relay(&context, call, out)?,
    };
    Ok(Some(code))
}

/// The stage, the repository and the checkout a completion verifies.
struct Target {
    stage: Stage,
    repo_root: PathBuf,
    checkout: PathBuf,
}

fn relay(context: &RelayContext, call: &SessionCall<'_>, out: &mut dyn Write) -> Result<i32> {
    if call.privileged {
        return refuse(out, call.cwd, PRIVILEGED_REFUSAL);
    }
    let target = match target(context, call) {
        Ok(target) => target,
        Err(error) => return refuse(out, call.cwd, &format!("{error:#}")),
    };
    if let Err(error) = committed_content(&target.stage, &target.repo_root) {
        let message = format!("{COMMIT_FIRST}\n{error:#}");
        return refuse(out, &target.checkout, &message);
    }
    let started = Instant::now();
    let site = WaitSite {
        work_dir: call.work_dir,
        stage_id: call.stage_id,
        session_id: &context.session_id,
        checkout: &target.checkout,
        knowledge: target.stage.stage_type == StageType::Knowledge,
        kind: JobKind::Complete,
        no_cache: call.no_cache,
        criteria: None,
        scratch_dir: &context.scratch_dir,
        budget: wait::budget_for(&target.stage),
    };
    let mut sink = TicketSink::default();
    let mut emit = |request: &HostJobRequest| -> Result<String> {
        let payload = serde_json::to_value(request)?;
        let what = "loom stage complete";
        let line = context.emit(RequestKind::Complete, payload, what, false, &mut sink)?;
        Ok(line.id)
    };
    match wait::wait_or_request(&site, &mut emit, out)? {
        WaitOutcome::Requested => {
            sink.print_line(out)?;
            Ok(0)
        }
        WaitOutcome::Finished(record, _) if record.state == JobState::Passed => {
            let remaining = site.budget.saturating_sub(started.elapsed());
            follow_application(&site, &record, remaining, out)
                .or_else(|error| refuse(out, &target.checkout, &format!("{error:#}")))
        }
        outcome => Ok(wait::exit_code(&outcome)),
    }
}

/// Everything the relay guards require, then the stage and its checkout: the
/// stage worktree, or the repository root for a knowledge stage.
fn target(context: &RelayContext, call: &SessionCall<'_>) -> Result<Target> {
    if let Some(requested) = call.session_id.filter(|id| *id != context.session_id) {
        bail!(
            "--session {requested} is not this session ({})",
            context.session_id
        );
    }
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    context.check(RequestKind::Complete, Some(call.stage_id), call.cwd, uid)?;
    let stage = load_stage(call.stage_id, call.work_dir)
        .with_context(|| format!("Failed to load stage '{}'", call.stage_id))?;
    let repo_root = WorkDir::new(call.work_dir)?
        .main_project_root()
        .context("the repository root is unavailable")?;
    let checkout = if stage.stage_type == StageType::Knowledge {
        repo_root.clone()
    } else {
        stage_worktree_path(&stage, &repo_root)?
    };
    Ok(Target {
        stage,
        repo_root,
        checkout,
    })
}

/// How the daemon applied a passed completion job, once it has.
enum Application {
    Completed,
    Rejected(String),
    /// Applied without completing the stage or recording a rejection.
    Unapplied(String),
}

/// Wait up to `budget` for the daemon to apply the passed job `record`, and
/// report it: completed (exit 0), rejected or not applied (exit 1), or still
/// pending (exit 0, the request kept so the next call follows the same job).
/// The request is released before the lines print, so a failure to release
/// still ends in one refusal first line.
fn follow_application(
    site: &WaitSite<'_>,
    record: &JobRecord,
    budget: Duration,
    out: &mut dyn Write,
) -> Result<i32> {
    let end = Instant::now() + budget;
    let heading = |state: &str| wait::heading(JobKind::Complete, state, &record.head);
    let (lines, code) = loop {
        match application(site, record)? {
            Some(Application::Completed) => {
                let done = format!(
                    "Stage '{}' was completed by the daemon. End the session now.",
                    site.stage_id
                );
                break ([heading("completed"), done], 0);
            }
            Some(Application::Rejected(reason)) => break ([heading("rejected"), reason], 1),
            Some(Application::Unapplied(reason)) => break ([heading("refused"), reason], 1),
            None if Instant::now() >= end => {
                let pending = format!(
                    "{}: the run passed and the daemon has not applied it yet. Run the same \
                     command again to keep waiting.",
                    heading("pending")
                );
                writeln!(out, "{pending}")?;
                out.flush()?;
                return Ok(0);
            }
            None => std::thread::sleep(POLL.min(end.saturating_duration_since(Instant::now()))),
        }
    };
    wait::release(site, record)?;
    for line in lines {
        writeln!(out, "{line}")?;
    }
    out.flush()?;
    Ok(code)
}

/// The stage completed, a rejection recorded for this job (read without the
/// attestation key, which a session cannot read), or the job's applied
/// marker with neither; `None` while the daemon has not applied it.
///
/// The daemon writes the transition, then the checkpoint, then the marker
/// last, so the marker is read first: once it exists, the stage and the
/// checkpoint read after it are final. Reading it last could pair a marker
/// with a stage or checkpoint read before the daemon wrote them. Without a
/// marker only a completed stage counts, since the transition precedes it.
fn application(site: &WaitSite<'_>, record: &JobRecord) -> Result<Option<Application>> {
    let marker = store::load_job_file(site.work_dir, site.stage_id, &record.id, APPLIED_MARKER)?;
    let stage = load_stage(site.stage_id, site.work_dir)?;
    if stage.status == StageStatus::Completed {
        return Ok(Some(Application::Completed));
    }
    let Some(marker) = marker else {
        return Ok(None);
    };
    let checkpoint = load_session_checkpoint(site.stage_id, site.session_id, site.work_dir)?;
    let rejection = checkpoint
        .and_then(|checkpoint| checkpoint.latest)
        .filter(|attempt| {
            attempt.phase == CompletionPhase::DaemonRejected && attempt.evidence_nonce == record.id
        });
    if let Some(attempt) = rejection {
        let reason = attempt
            .diagnostic_first_line
            .unwrap_or_else(|| "the daemon refused to complete the stage".to_string());
        return Ok(Some(Application::Rejected(reason)));
    }
    let applied = String::from_utf8_lossy(&marker).trim().to_string();
    Ok(Some(Application::Unapplied(format!(
        "the daemon applied the run without completing the stage: {applied}"
    ))))
}

/// The refusal's first line names HEAD of the checkout at `dir`.
fn refuse(out: &mut dyn Write, dir: &Path, message: &str) -> Result<i32> {
    wait::refusal(out, JobKind::Complete, &wait::head_or_empty(dir), message)
}
