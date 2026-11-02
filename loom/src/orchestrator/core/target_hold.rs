//! The daemon's side of the target guard ([`crate::git::target_guard`]): it
//! runs the guard at startup and every tick, remembers whether the target is
//! held, and keeps held work away from the target: merges wait, merge
//! resolvers do not spawn, knowledge stages (which commit to the target's
//! checkout) do not start, and merged-ness is judged against the tip loom
//! accepted, never against a target an agent moved. It fails closed: a check
//! that finds the merge lock taken judges the target without writing, and a
//! check that errors leaves the target held.

use chrono::Utc;

use crate::git::branch::{branch_ref, resolve_target_branch};
use crate::git::hooks::{install_reference_transaction_hook, HookInstall};
use crate::git::merge::rev_parse;
use crate::git::target_guard::{self, target_key, AttestationMode, GuardState, Hold, HoldReason};
use crate::models::stage::{Stage, StageType};
use crate::orchestrator::scheduling_report::BlockReason;

use super::{clear_status_line, Orchestrator};

/// Print `line` for the operator and log it as a warning.
fn warn_operator(line: &str) {
    clear_status_line();
    eprintln!("{line}");
    tracing::warn!("{line}");
}

/// Whether two holds are for the same tip and the same reasons.
fn same_hold(a: &Hold, b: &Hold) -> bool {
    a.observed == b.observed && a.reasons == b.reasons
}

/// The startup line for a hook install: `Ok` is only logged, `Err` warns
/// the operator.
fn hook_install_line(installed: &anyhow::Result<HookInstall>) -> Result<&'static str, String> {
    match installed {
        Ok(HookInstall::Installed) => Ok("target guard: reference-transaction hook installed"),
        Ok(HookInstall::UpToDate) => Ok("target guard: reference-transaction hook up to date"),
        Ok(HookInstall::ForeignHookPresent) => Err(
            "target guard: the reference-transaction hook git runs here belongs to another \
             tool and was left in place"
                .to_string(),
        ),
        Err(error) => Err(format!(
            "target guard: could not install the reference-transaction hook: {error:#}"
        )),
    }
}

/// The startup line while attestation is off for `reason`: what the guard
/// holds without it, given the record's latch for the target.
fn attestation_off_line(reason: &str, latched: &anyhow::Result<bool>) -> String {
    match latched {
        Ok(false) => format!(
            "target guard: attestation off ({reason}); only control-path changes, \
             rewrites of the target and unmerged stage work are held"
        ),
        Ok(true) => format!(
            "target guard: attestation off ({reason}), but this run recorded it on; \
             every move without a ledger line holds until loom target accept"
        ),
        Err(error) => format!(
            "target guard: attestation off ({reason}); the guard record could not be \
             read: {error:#}"
        ),
    }
}

impl Orchestrator {
    /// The configured target branch.
    fn guard_target(&self) -> String {
        resolve_target_branch(&self.config.base_branch, &self.config.repo_root)
    }

    /// Run the target guard for the configured target and remember the
    /// result: the hold, or `None` while the target is clear.
    ///
    /// While another owner holds the merge lock, the target is judged
    /// without writing ([`target_guard::pending_hold`]), so a session that
    /// takes the lock cannot keep a move unjudged. A hold judged that way
    /// gates everything a hold gates but is not printed: a merge holding the
    /// lock may be between moving the target and recording the advance. A
    /// check that errors keeps the target held until a later check answers;
    /// the error is logged once per distinct message. A hold is printed when
    /// its tip or its reasons are new.
    pub(in crate::orchestrator::core) fn check_target_guard(&mut self) -> Option<Hold> {
        let target = self.guard_target();
        let checked = target_guard::check(&self.config.repo_root, &self.config.work_dir, &target);
        match checked {
            Ok(Some(GuardState::Clear { .. })) => self.clear_target_hold(),
            Ok(Some(GuardState::Held(hold))) => self.remember_hold(&target, hold),
            Ok(None) => self.judge_without_lock(&target),
            Err(error) => self.fail_closed(&target, &error),
        }
        self.target_hold.clone()
    }

    fn clear_target_hold(&mut self) {
        self.target_hold = None;
        self.target_hold_announced = false;
        self.target_guard_error = None;
    }

    /// Keep `hold` from a check that took the merge lock, printing it unless
    /// it was already printed.
    fn remember_hold(&mut self, target: &str, hold: Hold) {
        if self.hold_is_news(&hold) {
            warn_operator(&target_guard::hold_alert(target, &hold));
        }
        self.target_guard_error = None;
        self.target_hold_announced = true;
        self.target_hold = Some(hold);
    }

    /// Whether `hold` was not printed yet: no hold was printed, or the one
    /// printed is for another tip or other reasons.
    fn hold_is_news(&self, hold: &Hold) -> bool {
        let printed = self.target_hold_announced
            && self
                .target_hold
                .as_ref()
                .is_some_and(|previous| same_hold(previous, hold));
        !printed
    }

    /// Judge `target` read-only while the merge lock is taken: a hold is
    /// kept unprinted, no hold clears the target, an error holds it.
    fn judge_without_lock(&mut self, target: &str) {
        let (repo_root, work_dir) = (&self.config.repo_root, &self.config.work_dir);
        match target_guard::pending_hold(repo_root, work_dir, target) {
            Ok(None) => self.clear_target_hold(),
            Ok(Some(hold)) => {
                self.target_guard_error = None;
                let known = self
                    .target_hold
                    .as_ref()
                    .is_some_and(|previous| same_hold(previous, &hold));
                if !known {
                    let alert = target_guard::hold_alert(target, &hold);
                    tracing::debug!("{alert} (merge lock taken; printed once it is free)");
                    self.target_hold_announced = false;
                }
                self.target_hold = Some(hold);
            }
            Err(error) => self.fail_closed(target, &error),
        }
    }

    /// Keep the target held after `error`: the current hold stays, or an
    /// unevaluable hold takes its place.
    fn fail_closed(&mut self, target: &str, error: &anyhow::Error) {
        self.log_guard_error(error);
        if self.target_hold.is_none() {
            self.target_hold = Some(self.error_hold(target, error));
            self.target_hold_announced = false;
        }
    }

    /// An unevaluable hold for `error`, naming whichever of the accepted
    /// and the live tip can still be read (empty otherwise).
    fn error_hold(&self, target: &str, error: &anyhow::Error) -> Hold {
        let (repo_root, work_dir) = (&self.config.repo_root, &self.config.work_dir);
        let accepted = target_guard::accepted_tip(work_dir, target).ok().flatten();
        let observed = rev_parse(repo_root, &branch_ref(target_key(target))).ok();
        Hold {
            accepted: accepted.unwrap_or_default(),
            observed: observed.unwrap_or_default(),
            reasons: vec![HoldReason::Unevaluable {
                error: format!("{error:#}"),
            }],
            since: Utc::now(),
        }
    }

    fn log_guard_error(&mut self, error: &anyhow::Error) {
        if let Some(line) = self.fresh_guard_error(error) {
            warn_operator(&line);
        }
    }

    /// The line to log for `error`; `None` when it repeats the last error
    /// logged and no check has answered since.
    fn fresh_guard_error(&mut self, error: &anyhow::Error) -> Option<String> {
        let message = format!("target guard: {error:#}");
        if self.target_guard_error.as_deref() == Some(message.as_str()) {
            return None;
        }
        self.target_guard_error = Some(message.clone());
        Some(message)
    }

    /// Whether the last check left the target held.
    pub(in crate::orchestrator::core) fn target_held(&self) -> bool {
        self.target_hold.is_some()
    }

    /// Ancestry of `commit` against the accepted target tip (the live
    /// `target` while the guard has no entry for it).
    pub(in crate::orchestrator::core) fn merged_into_accepted(
        &self,
        commit: &str,
        target: &str,
    ) -> anyhow::Result<bool> {
        let (repo_root, work_dir) = (&self.config.repo_root, &self.config.work_dir);
        target_guard::merged_into_accepted(repo_root, work_dir, target, commit)
    }

    /// [`Self::merged_into_accepted`] for sync's probe of `stage_id`. An
    /// error is logged once per distinct message until a probe answers, so
    /// an unreadable guard record does not log for every stage every tick.
    pub(in crate::orchestrator::core) fn probe_accepted(
        &mut self,
        stage_id: &str,
        commit: &str,
        target: &str,
    ) -> anyhow::Result<bool> {
        let probed = self.merged_into_accepted(commit, target);
        match &probed {
            Ok(_) => self.merge_probe_error = None,
            Err(error) => self.log_probe_error(stage_id, error),
        }
        probed
    }

    fn log_probe_error(&mut self, stage_id: &str, error: &anyhow::Error) {
        let message = format!("{error:#}");
        if self.merge_probe_error.as_deref() == Some(message.as_str()) {
            tracing::debug!(%stage_id, error = %message, "Merge verification errored again");
            return;
        }
        tracing::error!(
            %stage_id,
            error = %message,
            "Cannot verify merges against the accepted target tip; merged flags stay as \
             they are"
        );
        self.merge_probe_error = Some(message);
    }

    /// Why `stage` must not spawn now: `Held` for a held stage, `TargetHeld`
    /// for a knowledge stage while the target is held.
    pub(in crate::orchestrator::core) fn spawn_hold_reason(
        &self,
        stage: &Stage,
    ) -> Option<BlockReason> {
        if stage.held {
            Some(BlockReason::Held)
        } else if stage.stage_type == StageType::Knowledge && self.target_held() {
            Some(BlockReason::TargetHeld)
        } else {
            None
        }
    }

    /// Install the reference-transaction hook and run the first check, then
    /// say what the guard holds when attestation is off.
    pub(in crate::orchestrator::core) fn start_target_guard(&mut self) {
        match hook_install_line(&install_reference_transaction_hook(&self.config.repo_root)) {
            Ok(line) => tracing::info!("{line}"),
            Err(line) => warn_operator(&line),
        }
        self.check_target_guard();
        self.report_attestation_off();
    }

    /// Print one line when attestation is off, saying what is held without it.
    fn report_attestation_off(&self) {
        let (repo_root, work_dir) = (&self.config.repo_root, &self.config.work_dir);
        let AttestationMode::Off { reason } = target_guard::attestation_mode(repo_root, work_dir)
        else {
            return;
        };
        let latched = target_guard::attestation_latched(work_dir, &self.guard_target());
        warn_operator(&attestation_off_line(&reason, &latched));
    }
}

#[cfg(test)]
#[path = "target_hold_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "target_hold_recovery_tests.rs"]
mod recovery_tests;

#[cfg(test)]
#[path = "target_hold_contention_tests.rs"]
mod contention_tests;

#[cfg(test)]
#[path = "target_hold_startup_tests.rs"]
mod startup_tests;
