//! The Claude steps of a run: each `/pressure` and `/address` step as a pressure session,
//! waited for through the bridge's events.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::codex::CodexHalf;
use super::super::teammates::Teammates;
use super::super::view::step_usage;
use super::{teardown, RunEnv};
use crate::claude::AGENT_TEAMS_ENV;
use crate::plan_server::lifecycle;
use crate::plan_server::wire::pressure::{PressureStepKind, StepState};
use crate::plan_session::bridge::registry::read_record;
use crate::plan_session::bridge::{
    BridgeEvent, EndOutcome, GuidanceFn, LaunchGuidance, StartRequest,
};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::Origin;
use crate::planning::guidance::{render_session_guidance, PressureCommand, SessionGuidanceKind};
use crate::pressure::{completion_instruction, MarkerHome, PressureResult, StepVerdict};
use crate::process::run_bounded_output;

/// How long one wait for a bridge event lasts before the run looks at its other duties.
pub(super) const TICK: Duration = Duration::from_millis(100);
/// How often Codex's pid is compared with the recorded one (a fallback starts a new process).
const PID_POLL: Duration = Duration::from_millis(500);
/// How often new teammate transcripts are looked for.
const TEAMMATE_POLL: Duration = Duration::from_secs(2);
/// Bound on one `tmux list-panes`.
const TMUX_TIMEOUT: Duration = Duration::from_secs(5);
pub(in crate::plan_server::pressure) const TEAMMATE_PANES: &str =
    "teammates opened panes: teammateMode not honoured by this Claude Code";
const ENDED_EARLY: &str = "the session ended before it finished";
const STOPPED: &str = "the run was stopped";

/// How a wait for a session ended.
pub(super) enum Waited {
    Marker,
    Ended(String),
    Stopped,
    TimedOut,
}

/// One Claude step's command, slots and labels.
struct ClaudeStep<'a> {
    round: u32,
    command: PressureCommand,
    kind: PressureStepKind,
    label: String,
    model: &'a str,
    effort: &'a str,
}

/// A periodic duty of the wait loop.
struct Every {
    interval: Duration,
    next: Instant,
}

impl Every {
    fn new(interval: Duration) -> Self {
        Every {
            interval,
            next: Instant::now() + interval,
        }
    }

    fn due(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        self.next = now + self.interval;
        true
    }
}

impl RunEnv {
    /// Claude `/pressure` as a pressure session with Codex `$pressure` beside it.
    pub(super) fn pressure_step(&self, round: u32, skill: &str) -> PressureResult {
        let step = self.claude_step(round, PressureCommand::Pressure);
        let (id, dir) = match self.start_claude(&step) {
            Ok(started) => started,
            Err(verdict) => return no_codex(verdict),
        };
        let mut codex = match CodexHalf::start(self, round, &id, &dir, skill) {
            Ok(codex) => codex,
            Err(reason) => {
                self.end_claude(&step, &id, StepState::Stopped);
                return no_codex(stop(format!("round {round} Codex"), reason));
            }
        };
        let waited = self.wait_claude(&id, &dir, Some(&mut codex));
        self.end_claude(&step, &id, state_of(&waited));
        match waited {
            Waited::Marker => codex.finish(self),
            other => {
                codex.abandon(self);
                no_codex(self.failed_verdict(&step.label, other))
            }
        }
    }

    /// Claude `/address` as a pressure session.
    pub(super) fn address_step(&self, round: u32) -> StepVerdict {
        let step = self.claude_step(round, PressureCommand::Address);
        let (id, dir) = match self.start_claude(&step) {
            Ok(started) => started,
            Err(verdict) => return verdict,
        };
        let waited = self.wait_claude(&id, &dir, None);
        self.end_claude(&step, &id, state_of(&waited));
        match waited {
            Waited::Marker => {
                self.release_marker_session();
                StepVerdict::Continue
            }
            other => self.failed_verdict(&step.label, other),
        }
    }

    fn claude_step(&self, round: u32, command: PressureCommand) -> ClaudeStep<'_> {
        let (kind, slash, model, effort) = match command {
            PressureCommand::Pressure => (
                PressureStepKind::ClaudePressure,
                "/pressure",
                self.slots.claude_model.as_str(),
                self.slots.claude_effort.as_str(),
            ),
            PressureCommand::Address => (
                PressureStepKind::ClaudeAddress,
                "/address",
                self.slots.address_model.as_str(),
                self.slots.address_effort.as_str(),
            ),
        };
        ClaudeStep {
            round,
            command,
            kind,
            label: format!("round {round} {slash}"),
            model,
            effort,
        }
    }

    fn start_claude(&self, step: &ClaudeStep<'_>) -> Result<(String, SessionDir), StepVerdict> {
        if self.stopping() {
            return Err(self.stop_verdict(&step.label));
        }
        let request = StartRequest {
            kind: SessionKind::Pressure {
                plan: PathBuf::from(&self.plan.invocation),
            },
            guidance: self.guidance(step),
            opening_origin: Some(Origin::Offer),
            offer: None,
            extra_env: vec![(AGENT_TEAMS_ENV.to_owned(), "1".to_owned())],
        };
        let started = self.runs.sessions.start_session(request);
        let id = started.map_err(|error| {
            stop(
                step.label.clone(),
                format!("the session did not start: {error}"),
            )
        })?;
        self.run.live().started(&id);
        let (round, kind) = (step.round, step.kind.clone());
        self.publish(|view| view.begin_claude(round, kind, &id, step.model, step.effort));
        let dir = SessionDir::new(self.ctx.runtime().sessions.join(&id));
        Ok((id, dir))
    }

    /// Plan 8's pressure guidance; the brief is the completion instruction with this session's
    /// absolute marker path, and the opening goes on the launch line.
    fn guidance(&self, step: &ClaudeStep<'_>) -> GuidanceFn {
        let (command, plan_rel) = (step.command, self.plan.invocation.clone());
        let (model, effort) = (step.model.to_owned(), step.effort.to_owned());
        let (ctx, models) = (Arc::clone(&self.ctx), self.models.clone());
        Box::new(move |dir: &SessionDir| {
            let brief = completion_instruction(&dir.marker(), MarkerHome::PlanSession);
            let kind = SessionGuidanceKind::Pressure {
                command,
                plan_rel,
                brief,
                model,
                effort,
            };
            let project = ctx.project().clone();
            let guidance = render_session_guidance(&kind, &project, ctx.runtime(), dir, &models)?;
            Ok(LaunchGuidance {
                opening_on_argv: true,
                ..LaunchGuidance::from(guidance)
            })
        })
    }

    /// Wait for the session's end, polling Codex's pid, the teammate transcripts and the
    /// session's tmux panes meanwhile.
    fn wait_claude(&self, id: &str, dir: &SessionDir, mut codex: Option<&mut CodexHalf>) -> Waited {
        let mut pid = Every::new(PID_POLL);
        let mut found = Every::new(TEAMMATE_POLL);
        let mut panes = Every::new(self.runs.tools.pane_check);
        let mut teammates = Teammates::default();
        loop {
            if let Some(waited) = self.next_end(id) {
                return waited;
            }
            if self.stopping() {
                return Waited::Stopped;
            }
            let now = Instant::now();
            if let Some(codex) = codex.as_deref_mut().filter(|_| pid.due(now)) {
                codex.poll(self);
            }
            if found.due(now) {
                teammates.register_new(&*self.runs.sessions, id);
            }
            if panes.due(now) && self.extra_panes(dir) {
                return Waited::Ended(TEAMMATE_PANES.to_owned());
            }
        }
    }

    fn next_end(&self, id: &str) -> Option<Waited> {
        match self.events.recv_timeout(TICK) {
            Ok(BridgeEvent::Ended {
                id: ended, outcome, ..
            }) if ended == id => Some(self.step_ended(outcome)),
            Ok(_) | Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => Some(Waited::Ended(ENDED_EARLY.to_owned())),
        }
    }

    fn step_ended(&self, outcome: EndOutcome) -> Waited {
        if outcome == EndOutcome::Marker {
            self.run.live().marker_ended();
            return Waited::Marker;
        }
        self.run.live().ended();
        match outcome {
            _ if self.stopping() => Waited::Stopped,
            EndOutcome::Failed(why) => Waited::Ended(why),
            _ => Waited::Ended(ENDED_EARLY.to_owned()),
        }
    }

    /// The session's tmux server lists more than one pane: teammates left the session's own
    /// process. Evidence that cannot be read decides nothing.
    fn extra_panes(&self, dir: &SessionDir) -> bool {
        let Ok(record) = read_record(dir) else {
            return false;
        };
        let mut command = Command::new(&self.runs.tools.tmux);
        command.args(["-L", record.socket.as_str(), "list-panes", "-a"]);
        command.stdin(Stdio::null());
        match run_bounded_output(&mut command, TMUX_TIMEOUT, "tmux list-panes") {
            Ok(output) if output.status.success() => {
                let listing = String::from_utf8_lossy(&output.stdout);
                listing
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .count()
                    > 1
            }
            _ => false,
        }
    }

    /// Show the step's end with its tokens and edits; a step that did not finish is torn
    /// down, Codex with it.
    fn end_claude(&self, step: &ClaudeStep<'_>, id: &str, state: StepState) {
        if state != StepState::Done {
            for line in teardown(&*self.runs.sessions, &self.run) {
                lifecycle::line(&line);
            }
        }
        let usage = step_usage(&self.runs.sessions.chat_snapshot(false), id);
        let (round, kind) = (step.round, &step.kind);
        self.publish(|view| view.finish_claude(round, kind, state, usage));
    }

    /// Release the marker-ended step's session (its children and directory), once.
    pub(in crate::plan_server::pressure) fn release_marker_session(&self) {
        let claimed = self.run.live().claim_release();
        if let Some(id) = claimed {
            if let Err(error) = self.runs.sessions.release_session(&id) {
                tracing::warn!(session = %id, %error, "cannot release a pressure session");
            }
        }
    }

    pub(in crate::plan_server::pressure) fn stop_verdict(&self, label: &str) -> StepVerdict {
        stop(label.to_owned(), STOPPED.to_owned())
    }

    fn failed_verdict(&self, label: &str, waited: Waited) -> StepVerdict {
        match waited {
            Waited::Ended(reason) => stop(label.to_owned(), reason),
            Waited::Marker | Waited::Stopped | Waited::TimedOut => self.stop_verdict(label),
        }
    }
}

fn state_of(waited: &Waited) -> StepState {
    match waited {
        Waited::Marker => StepState::Done,
        Waited::Stopped => StepState::Stopped,
        Waited::Ended(_) | Waited::TimedOut => StepState::Failed,
    }
}

fn stop(step: String, reason: String) -> StepVerdict {
    StepVerdict::Stop { step, reason }
}

fn no_codex(verdict: StepVerdict) -> PressureResult {
    PressureResult {
        verdict,
        codex_model_used: None,
    }
}
