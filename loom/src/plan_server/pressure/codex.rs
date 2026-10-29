//! The Codex half of a `/pressure` step: `codex exec` beside the Claude session, its process
//! identity recorded in the session record (again after each fallback), its exit classified as
//! the terminal driver classifies it, and the configuration it may have changed.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use super::config_guard::{changed, snapshot, ConfigSnapshot};
use super::driver::RunEnv;
use super::recover::terminate_codex;
use crate::claude::{classify_exit, ExitAction};
use crate::codex::codex_model_candidates;
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_server::wire::pressure::{CodexState, CodexStatus, StepState};
use crate::plan_session::bridge::BridgeError;
use crate::plan_session::kind::SessionDir;
use crate::planning::guidance::StartBlocks;
use crate::pressure::codex_run::{spawn_codex_with_fallback, CodexOutcome, CodexRun};
use crate::pressure::paths::codex_report_path;
use crate::pressure::{PressureResult, StepVerdict};
use crate::process::{process_start_time, ProcessIdentity};

/// Bytes of the Codex log shown on a failure (the terminal driver's tail).
const TAIL_BYTES: u64 = 2000;
/// How often a finishing Codex is looked at.
const POLL: Duration = Duration::from_millis(100);
/// How long a terminated Codex may take to exit before the run stops waiting for it.
const EXIT_GRACE: Duration = Duration::from_secs(10);
/// The start of the reason a configuration change ends a run with (the panel keys on it).
pub(super) const CONFIG_CHANGED: &str = "Codex changed session configuration";

/// A running Codex `$pressure` and what the run knows about it.
pub(super) struct CodexHalf {
    run: Option<CodexRun>,
    round: u32,
    session: String,
    candidates: Vec<String>,
    attempt: usize,
    current: Option<ProcessIdentity>,
    recorded: u32,
    signalled: u32,
    log: PathBuf,
    before: ConfigSnapshot,
}

impl CodexHalf {
    /// Snapshot the session configuration, then start Codex with its fallback models.
    pub(super) fn start(
        env: &RunEnv,
        round: u32,
        session: &str,
        dir: &SessionDir,
        skill: &str,
    ) -> Result<CodexHalf, String> {
        let root = env.ctx.root();
        let before = snapshot(root).map_err(|error| {
            format!("cannot read the session configuration before Codex: {error:#}")
        })?;
        let slots = &env.slots;
        let candidates: Vec<String> = codex_model_candidates(&slots.codex_model)
            .into_iter()
            .map(String::from)
            .collect();
        let log = dir.codex_log();
        let run = spawn_codex_with_fallback(
            &env.codex_path,
            root,
            skill,
            &log,
            candidates.clone(),
            &slots.codex_effort,
        )
        .map_err(|error| format!("Codex did not start: {error:#}"))?;
        env.publish(|view| view.begin_codex(round, &slots.codex_model, &slots.codex_effort));
        let mut half = CodexHalf {
            run: Some(run),
            round,
            session: session.to_owned(),
            candidates,
            attempt: 0,
            current: None,
            recorded: 0,
            signalled: 0,
            log,
            before,
        };
        half.poll(env);
        Ok(half)
    }

    /// Follow the running attempt: a new pid is a fallback attempt, recorded in the session
    /// record (retried while the session is not yet live).
    pub(super) fn poll(&mut self, env: &RunEnv) {
        let pid = self.run.as_ref().map_or(0, CodexRun::current_pid);
        if pid == 0 {
            return;
        }
        if self.current.is_none_or(|current| current.pid != pid) {
            if self.current.is_some() {
                self.attempt += 1;
                if let Some(model) = self.candidates.get(self.attempt) {
                    env.publish(|view| view.codex_model(self.round, model));
                }
            }
            let identity = ProcessIdentity {
                pid,
                start_time: process_start_time(pid),
            };
            self.current = Some(identity);
            env.run.live().set_codex(Some(identity));
        }
        if pid != self.recorded {
            self.record(env);
        }
    }

    fn record(&mut self, env: &RunEnv) {
        let Some(identity) = self.current else {
            return;
        };
        match env
            .runs
            .sessions
            .record_child(&self.session, "codex", identity)
        {
            Ok(()) => self.recorded = identity.pid,
            Err(BridgeError::NotRunning) => {}
            Err(error) => {
                tracing::warn!(session = %self.session, %error, "Codex not recorded");
                self.recorded = identity.pid;
            }
        }
    }

    /// The Claude half ended with its marker: wait for Codex, show how it ended, release the
    /// session (only now: the marker end kept Codex alive and `codex.log` in place,
    /// px-seams-01), then check the configuration Codex could have changed.
    pub(super) fn finish(mut self, env: &RunEnv) -> PressureResult {
        let label = format!("round {} Codex", self.round);
        let joined = self.wait(env);
        env.run.live().set_codex(None);
        let tail = log_tail(&self.log);
        env.release_marker_session();
        let blocked = self.configuration_changed(env);
        let joined = match joined {
            Some(joined) if !env.stopping() => joined,
            _ => {
                self.show_stopped(env, tail);
                return PressureResult {
                    verdict: env.stop_verdict(&label),
                    codex_model_used: None,
                };
            }
        };
        let requested = self.candidates.first().map_or("", String::as_str);
        let (status, failure) = classify(joined, requested, tail, &Report::of(env));
        let model_used = Some(status.model.clone());
        let failed = failure.is_some() || blocked.is_some();
        let state = if failed {
            StepState::Failed
        } else {
            StepState::Done
        };
        env.publish(|view| view.finish_codex(self.round, state, status));
        let verdict = match blocked.or(failure) {
            Some(reason) => StepVerdict::Stop {
                step: label,
                reason,
            },
            None => StepVerdict::Continue,
        };
        PressureResult {
            verdict,
            codex_model_used: model_used,
        }
    }

    /// The Claude half did not finish: Codex is terminated and the run waits briefly for it.
    /// What it changed in the configuration until then still blocks later launches.
    pub(super) fn abandon(mut self, env: &RunEnv) {
        self.terminate();
        let deadline = Instant::now() + EXIT_GRACE;
        while self.running() && Instant::now() < deadline {
            thread::sleep(POLL);
            self.poll(env);
            self.terminate();
        }
        if !self.running() {
            if let Some(Err(error)) = self.run.take().map(CodexRun::join) {
                tracing::debug!(%error, "an abandoned Codex run failed");
            }
        }
        env.run.live().set_codex(None);
        let tail = log_tail(&self.log);
        self.configuration_changed(env);
        self.show_stopped(env, tail);
    }

    fn running(&self) -> bool {
        self.run.as_ref().is_some_and(|run| !run.is_finished())
    }

    /// Wait for the last attempt; a stop terminates it and waits at most [`EXIT_GRACE`].
    fn wait(&mut self, env: &RunEnv) -> Option<anyhow::Result<CodexOutcome>> {
        if self.running() {
            env.publish(|view| view.codex_alone());
        }
        let mut stopped_at: Option<Instant> = None;
        while self.running() {
            if env.stopping() {
                let since = *stopped_at.get_or_insert_with(Instant::now);
                if since.elapsed() >= EXIT_GRACE {
                    return None;
                }
                self.terminate();
            }
            thread::sleep(POLL);
            self.poll(env);
        }
        self.run.take().map(CodexRun::join)
    }

    /// Signal the current attempt once, through its verified identity only.
    fn terminate(&mut self) {
        let Some(identity) = self.current.filter(|current| current.pid != self.signalled) else {
            return;
        };
        self.signalled = identity.pid;
        if let Some(message) = terminate_codex(identity) {
            line(&message);
        }
    }

    fn show_stopped(&self, env: &RunEnv, tail: Option<String>) {
        let model = self
            .candidates
            .get(self.attempt)
            .cloned()
            .unwrap_or_default();
        let status = CodexStatus {
            model,
            state: CodexState::NoReview,
            report_path: None,
            log_tail: tail,
        };
        env.publish(|view| view.finish_codex(self.round, StepState::Stopped, status));
    }

    /// A changed configuration blocks every later launch until the holder confirms it.
    fn configuration_changed(&self, env: &RunEnv) -> Option<String> {
        let (reason, changed) = match snapshot(env.ctx.root()) {
            Ok(after) => {
                let changed = changed(&self.before, &after);
                if changed.is_empty() {
                    return None;
                }
                (format!("{CONFIG_CHANGED}: {}", changed.join(", ")), changed)
            }
            Err(error) => {
                let why = format!("{CONFIG_CHANGED}: unreadable after Codex ran ({error:#})");
                (why, Vec::new())
            }
        };
        if let Err(error) = StartBlocks::block(env.ctx.runtime(), &reason, &changed) {
            line(&format!(
                "pressure run: the configuration block was not saved: {}",
                safe(&format!("{error:#}"))
            ));
        }
        Some(reason)
    }
}

/// The run's Codex report: where Codex writes it and the repo-relative path the panel links.
struct Report {
    path: PathBuf,
    shown: String,
}

impl Report {
    fn of(env: &RunEnv) -> Report {
        Report {
            path: codex_report_path(&env.plan.fs_path),
            shown: codex_report_path(Path::new(&env.plan.invocation))
                .display()
                .to_string(),
        }
    }
}

/// Codex's status and, for an exit the terminal driver treats as fatal (`should_stop`), the
/// stop reason in the terminal driver's words.
fn classify(
    joined: anyhow::Result<CodexOutcome>,
    requested: &str,
    tail: Option<String>,
    report: &Report,
) -> (CodexStatus, Option<String>) {
    let outcome = match joined {
        Ok(outcome) => outcome,
        Err(error) => {
            let status = status(requested, CodexState::Failed, None, tail);
            return (status, Some(format!("Codex failed: {error:#}")));
        }
    };
    let model = outcome.model_used.as_str();
    match classify_exit(outcome.status) {
        ExitAction::Continue if report.path.is_file() => {
            let path = Some(report.shown.clone());
            (status(model, CodexState::ReviewWritten, path, None), None)
        }
        ExitAction::Continue => (status(model, CodexState::NoReview, None, None), None),
        ExitAction::Warn => (status(model, CodexState::NoReview, None, tail), None),
        ExitAction::Abort => {
            let reason = match outcome.status.code() {
                Some(code) => format!("codex exited with code {code} — stopping pressure run."),
                None => "codex was terminated by a signal — stopping pressure run.".to_owned(),
            };
            (status(model, CodexState::Failed, None, tail), Some(reason))
        }
    }
}

fn status(
    model: &str,
    state: CodexState,
    report_path: Option<String>,
    log_tail: Option<String>,
) -> CodexStatus {
    CodexStatus {
        model: model.to_owned(),
        state,
        report_path,
        log_tail,
    }
}

/// The last [`TAIL_BYTES`] of the log, lossily decoded; `None` when it cannot be read.
fn log_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL_BYTES)))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}
