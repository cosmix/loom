//! Merge resolver records that outlived what started them: a daemon that died
//! between launch and publication, or a publication that failed.

use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::session_registry::record_may_be_running;
use crate::orchestrator::signals::publish_merge_session;
use crate::orchestrator::terminal::native::{
    read_pid_entry, session_process_status, NativeBackend, SessionProcessStatus,
};

use super::review_route::awaits_merge;

impl Orchestrator {
    /// Publish the resolver `spawned`, whose process runs. A failure leaves
    /// its write-ahead record and PID file discoverable, and the attempt is
    /// spent: the caller keeps tracking it rather than losing a live writer.
    pub(super) fn publish_spawned_resolver(&self, spawned: &Session) {
        if let Err(error) = publish_merge_session(spawned, &self.config.work_dir) {
            tracing::warn!(
                session_id = %spawned.id,
                error = %format!("{error:#}"),
                "Could not publish a running merge resolver; keeping it tracked"
            );
        }
    }

    /// Settle a merge record of `stage` found at daemon start. The stage does
    /// not name its resolver, so `current_session_is_orphan` never sees one.
    /// One that runs is tracked again; a `Spawning` one no process can back is
    /// retired, or left for discovery while it is too young to judge. A dead
    /// `Running` one stays for the monitor, whose exit path lands its work.
    pub(in crate::orchestrator::core) fn recover_merge_record(
        &mut self,
        stage: &Stage,
        session: &Session,
    ) {
        if !matches!(
            session.status,
            SessionStatus::Spawning | SessionStatus::Running
        ) {
            return;
        }
        let work_dir = self.config.work_dir.clone();
        match session_process_status(&work_dir, session) {
            SessionProcessStatus::VerifiedAlive | SessionProcessStatus::Unverifiable => {
                self.track_surviving_resolver(stage, session);
            }
            SessionProcessStatus::Dead | SessionProcessStatus::Missing
                if session.status == SessionStatus::Spawning
                    && !record_may_be_running(&work_dir, session) =>
            {
                let reason = SessionExitReason::Crashed;
                self.finish_stale_merge_retirement(&stage.id, session, reason);
            }
            SessionProcessStatus::Dead | SessionProcessStatus::Missing => {}
        }
    }

    fn track_surviving_resolver(&mut self, stage: &Stage, session: &Session) {
        if !awaits_merge(&stage.status) || self.active_sessions.contains_key(&stage.id) {
            return;
        }
        let mut tracked = session.clone();
        if tracked.status == SessionStatus::Spawning {
            if let Err(error) = self.publish_recovered_launch(&stage.id, &mut tracked) {
                tracing::warn!(session_id = %session.id, %error, "Cannot adopt a merge resolver");
                return;
            }
        }
        tracing::info!(
            stage_id = %stage.id,
            session_id = %session.id,
            "Tracking a merge resolver that outlived the previous daemon"
        );
        self.active_sessions.insert(stage.id.clone(), tracked);
    }

    /// Fill in what the launch's lost answer would have published.
    fn publish_recovered_launch(
        &self,
        stage_id: &str,
        session: &mut Session,
    ) -> anyhow::Result<()> {
        let identity = NativeBackend::window_title_and_pid_key(session)
            .and_then(|(_, key)| read_pid_entry(&self.config.work_dir, &key));
        if let Some(identity) = identity {
            session.set_pid(identity.pid);
        }
        session.set_worktree_path(Worktree::worktree_path(&self.config.repo_root, stage_id));
        session.try_mark_running()?;
        self.publish_spawned_resolver(session);
        Ok(())
    }
}

#[cfg(test)]
#[path = "resolver_recovery_tests.rs"]
mod tests;
