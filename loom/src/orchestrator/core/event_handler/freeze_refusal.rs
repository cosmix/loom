//! Filing a contract writer's refused freeze for adjudication.
//!
//! A refused freeze never waits for the operator: loom files it as a
//! `freeze-refusal` dispute, an adjudicator decides it, and a fresh contract
//! writer starts with the verdict. [`Orchestrator::dispute_refused_freezes`]
//! is the one filer. It runs every tick, manual mode included, for each
//! `Executing` v2 standard stage with contracts, nothing frozen, and a current
//! `Contract` session:
//!
//! - a refusal the daemon recorded (`contracts/<stage>/refused.json`, written
//!   by the inbox drain and the freeze host job's application) is filed at
//!   once when it names the current writer, and dropped as stale otherwise;
//! - a refusal the writer's CLI left in its scratch directory is recorded and
//!   filed once the file and the session's latest heartbeat are both
//!   [`REFUSAL_QUIET_SECS`] old and the session runs no subagent: the writer
//!   may still fix and retry in the same turn. Its problems are the writer's
//!   own text and are filed with [`WRITER_PREFIX`];
//! - once the stage's freeze-refusal budget is spent, the writer is retired and
//!   the stage goes to `NeedsHumanReview`.
//!
//! A filed or escalated refusal's record is removed; a declined one is marked
//! and never filed again. A filing that fails is retried on the next tick
//! under the same request key, so it writes one `request.md`. A fresh writer
//! has a new session id, so neither evidence source can re-file for it.

use std::path::Path;
use std::time::{Duration, SystemTime};

use anyhow::Result;

use crate::fs::session_files::load_session_exact;
use crate::models::dispute::DisputeKind;
use crate::models::session::SessionType;
use crate::models::stage::{dispute_budgets, Stage, StageStatus, StageType};
use crate::orchestrator::monitor::heartbeat::{heartbeat_path, read_heartbeat};
use crate::relay::scratch_root_from_env;
use crate::stage_service::{escalate_freeze_refusal, file_freeze_refusal, FreezeRefusalFiling};
use crate::subagent_lifecycle::store::{replay, ChildDisposition};
use crate::verify::contracts::refusal::{
    daemon_refusal, local_refusal, mark_declined, record_daemon_refusal, take_daemon_refusal,
    DaemonRefusal, RefusalSource, WRITER_PREFIX,
};
use crate::verify::contracts::store::load_freeze;

use super::super::{
    clear_status_line, orchestrator::unretired_detail, persistence::Persistence, Orchestrator,
};

/// How long a writer's own refusal, and the writer, must be quiet before loom
/// files it: the writer may fix the problems and freeze again in the same turn.
pub(in crate::orchestrator::core) const REFUSAL_QUIET_SECS: u64 = 300;

impl Orchestrator {
    /// File every refused freeze that stands for a stage's current contract
    /// writer. Called by `run_tick` right before the frozen-contract handover.
    pub(in crate::orchestrator::core) fn dispute_refused_freezes(&mut self) {
        // No scratch root leaves only the writers' own refusals unread; the
        // inbox drain already reports that it is missing.
        let scratch_root = scratch_root_from_env().ok();
        self.dispute_refused_freezes_at(scratch_root.as_deref(), SystemTime::now());
    }

    /// [`Self::dispute_refused_freezes`] with the scratch root and the time
    /// the quiet rule measures against.
    fn dispute_refused_freezes_at(&mut self, scratch_root: Option<&Path>, now: SystemTime) {
        let executing: Vec<String> = self
            .graph
            .all_nodes()
            .into_iter()
            .filter(|node| node.status == StageStatus::Executing)
            .map(|node| node.id.clone())
            .collect();
        for stage_id in executing {
            let key = format!("freeze-refusal:{stage_id}");
            match self.dispute_refused_freeze(&stage_id, scratch_root, now) {
                Ok(()) => {
                    self.spool_drain_error_logged.remove(&key);
                }
                Err(error) => {
                    if self.spool_drain_error_logged.insert(key) {
                        clear_status_line();
                        tracing::warn!(
                            stage_id = %stage_id,
                            error = %format!("{error:#}"),
                            "Could not file a refused contract freeze; retrying every tick"
                        );
                    }
                }
            }
        }
    }

    fn dispute_refused_freeze(
        &mut self,
        stage_id: &str,
        scratch_root: Option<&Path>,
        now: SystemTime,
    ) -> Result<()> {
        let Some((stage, session_id)) = self.refusing_writer(stage_id)? else {
            return Ok(());
        };
        let Some(refusal) = self.standing_refusal(&stage, &session_id, scratch_root, now)? else {
            return Ok(());
        };
        if refusal.declined.is_some() {
            return Ok(());
        }
        self.file_refusal(&stage, &refusal)
    }

    /// `stage_id` and its current contract writer's session id, while its
    /// contract phase is open and a refusal of it could be disputed.
    fn refusing_writer(&self, stage_id: &str) -> Result<Option<(Stage, String)>> {
        let stage = self.load_stage(stage_id)?;
        let open = stage.status == StageStatus::Executing
            && stage.plan_version == 2
            && stage.stage_type == StageType::Standard
            && !stage.contracts.is_empty();
        let Some(session_id) = stage.session.clone().filter(|_| open) else {
            return Ok(None);
        };
        let work_dir = &self.config.work_dir;
        let writer = load_session_exact(work_dir, &session_id)?.filter(|session| {
            session.session_type == SessionType::Contract
                && session.stage_id.as_deref() == Some(stage_id)
        });
        if writer.is_none() || load_freeze(work_dir, stage_id)?.is_some() {
            return Ok(None);
        }
        Ok(Some((stage, session_id)))
    }

    /// The refusal that stands for writer `session_id`: the daemon's record,
    /// else the writer's own once it has gone quiet, recorded first so the
    /// filing below has one record to file and take.
    fn standing_refusal(
        &mut self,
        stage: &Stage,
        session_id: &str,
        scratch_root: Option<&Path>,
        now: SystemTime,
    ) -> Result<Option<DaemonRefusal>> {
        let work_dir = self.config.work_dir.clone();
        match self.daemon_record(&stage.id)? {
            Some(record) if record.session_id == session_id => return Ok(Some(record)),
            Some(stale) => {
                take_daemon_refusal(&work_dir, &stage.id)?;
                tracing::info!(
                    stage_id = %stage.id,
                    session_id = %stale.session_id,
                    "Dropped the refused freeze of a previous contract writer"
                );
            }
            None => {}
        }
        let Some(root) = scratch_root else {
            return Ok(None);
        };
        let Some((problems, written)) = local_refusal(root, session_id)? else {
            return Ok(None);
        };
        if !self.writer_is_quiet(&stage.id, session_id, written, now) {
            return Ok(None);
        }
        record_daemon_refusal(
            &work_dir,
            &stage.id,
            session_id,
            &problems,
            RefusalSource::Writer,
        )?;
        daemon_refusal(&work_dir, &stage.id)
    }

    /// The daemon's record for `stage_id`. Only the daemon writes it, so one
    /// that cannot be parsed is damage: it is dropped, and said so.
    fn daemon_record(&self, stage_id: &str) -> Result<Option<DaemonRefusal>> {
        match daemon_refusal(&self.config.work_dir, stage_id) {
            Ok(record) => Ok(record),
            Err(error) => {
                take_daemon_refusal(&self.config.work_dir, stage_id)?;
                clear_status_line();
                eprintln!(
                    "Dropped the unreadable refused-freeze record of stage '{stage_id}': \
                     {error:#}"
                );
                Ok(None)
            }
        }
    }

    /// Whether writer `session_id` is done with the refusal it wrote at
    /// `written`: the refusal and its latest heartbeat (one from another
    /// session, or none, counts as old) are [`REFUSAL_QUIET_SECS`] old and it
    /// runs no subagent. A child journal that cannot be read waits.
    fn writer_is_quiet(
        &mut self,
        stage_id: &str,
        session_id: &str,
        written: SystemTime,
        now: SystemTime,
    ) -> bool {
        let quiet = Duration::from_secs(REFUSAL_QUIET_SECS);
        let aged = |at: SystemTime| now.duration_since(at).is_ok_and(|age| age >= quiet);
        let work_dir = &self.config.work_dir;
        let heartbeat = read_heartbeat(&heartbeat_path(work_dir, stage_id))
            .ok()
            .filter(|heartbeat| heartbeat.session_id == session_id);
        if !aged(written) || heartbeat.is_some_and(|hb| !aged(SystemTime::from(hb.timestamp))) {
            return false;
        }
        let children = replay(work_dir)
            .map(|index| index.session_child_disposition(stage_id, session_id))
            .unwrap_or_else(|error| ChildDisposition::Unknown(format!("{error:#}")));
        self.note_child_disposition(stage_id, session_id, &children);
        children == ChildDisposition::NoChildren
    }

    /// Say once per writer, until its children are known again, that their
    /// ownership is uncertain and its refusal waits: a tick loop would repeat
    /// it every tick.
    fn note_child_disposition(
        &mut self,
        stage_id: &str,
        session_id: &str,
        children: &ChildDisposition,
    ) {
        let key = format!("freeze-refusal-children:{stage_id}:{session_id}");
        let ChildDisposition::Unknown(reason) = children else {
            self.spool_drain_error_logged.remove(&key);
            return;
        };
        if self.spool_drain_error_logged.insert(key) {
            clear_status_line();
            tracing::warn!(
                stage_id,
                session_id,
                %reason,
                "Child ownership is uncertain; the refusal waits"
            );
        }
    }

    /// File `refusal` as the stage's next freeze-refusal dispute, or, with
    /// the budget spent, retire the writer and escalate.
    fn file_refusal(&mut self, stage: &Stage, refusal: &DaemonRefusal) -> Result<()> {
        let work_dir = self.config.work_dir.clone();
        let problems = filed_problems(refusal);
        let kind = DisputeKind::FreezeRefusal {
            problems: Vec::new(),
        };
        if dispute_budgets::exhausted(stage, &kind) {
            return self.escalate_refused_freezes(&stage.id, &problems);
        }
        let key = format!(
            "freeze-refusal-{}-{}",
            refusal.session_id,
            refusal.at.to_rfc3339()
        );
        let filing = file_freeze_refusal(&work_dir, &stage.id, problems, &key)?;
        settle_filing(&work_dir, &stage.id, filing)
    }

    /// The writer's refusals have spent the budget: retire it, then send the
    /// stage to a human, naming any survivor of the retirement.
    fn escalate_refused_freezes(&mut self, stage_id: &str, problems: &[String]) -> Result<()> {
        let unretired =
            unretired_detail(self.retire_stage_agents(stage_id, StageStatus::Executing));
        let escalated = escalate_freeze_refusal(
            &self.config.work_dir,
            stage_id,
            problems,
            unretired.as_deref(),
        );
        if let Err(error) = &escalated {
            // A retirement that took every agent down released the stage's
            // session, so no later tick finds a writer to retry for: the
            // caller's "retrying every tick" is then untrue.
            tracing::warn!(
                stage_id,
                retired = unretired.is_none(),
                error = %format!("{error:#}"),
                "Could not hand a stage with a spent freeze-refusal budget to a human"
            );
        }
        escalated?;
        take_daemon_refusal(&self.config.work_dir, stage_id)?;
        clear_status_line();
        eprintln!(
            "REVIEW NEEDED: stage '{stage_id}' had its contract freeze refused more often than \
             adjudication allows. Next: loom stage human-review {stage_id}"
        );
        Ok(())
    }
}

/// Act on what the filing of `stage_id`'s refusal did: a filed refusal's
/// record is taken, a declined one is marked so it is never filed again.
fn settle_filing(work_dir: &Path, stage_id: &str, filing: FreezeRefusalFiling) -> Result<()> {
    match filing {
        FreezeRefusalFiling::Filed(note) => {
            take_daemon_refusal(work_dir, stage_id)?;
            clear_status_line();
            println!("  Contract freeze refused: {stage_id}: {note} for adjudication");
            tracing::info!(stage_id, %note, "Filed a refused contract freeze");
        }
        FreezeRefusalFiling::Declined(reason) => {
            mark_declined(work_dir, stage_id, &reason)?;
            clear_status_line();
            eprintln!(
                "Loom declined to file the refused contract freeze of stage '{stage_id}': \
                 {reason}"
            );
            tracing::warn!(stage_id, %reason, "Declined a refused contract freeze");
        }
    }
    Ok(())
}

/// The problems a filing carries. A writer's refusal is its own text; once the
/// record is removed the prefix is the only mark of that source.
fn filed_problems(refusal: &DaemonRefusal) -> Vec<String> {
    match refusal.source {
        RefusalSource::Daemon => refusal.problems.clone(),
        RefusalSource::Writer => refusal
            .problems
            .iter()
            .map(|problem| format!("{WRITER_PREFIX}{problem}"))
            .collect(),
    }
}

#[cfg(test)]
#[path = "freeze_refusal_tests.rs"]
mod tests;
