//! The run's panel view (`PressureRunView`), built from the driver's state and broadcast on
//! every change, and how the pipeline's outcome reads as the run's end.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::plan_server::wire::pressure::{
    CodexState, CodexStatus, PressureEnd, PressureRunView, PressureStepKind, PressureStepView,
    RunEndKind, StepState, TokenTotals,
};
use crate::plan_session::model::ChatSnapshot;
use crate::pressure::{PipelineOutcome, RECORD_STEP_LABEL};

/// Shown as who stopped a run that ended at the holder's grace expiry.
pub(super) const HOLDER_AWAY: &str = "the holder was away for 10 minutes";
/// The reason of a run the server's stop interrupted.
pub(super) const SERVER_STOPPED: &str = "loom plan stopped";

/// Why a run was asked to stop; the first cause wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StopCause {
    /// The holder pressed Stop; their display name.
    Holder(String),
    /// The holder's tabs were gone for the grace period.
    GraceExpired,
    /// `loom plan` is stopping.
    Server,
}

/// What a finished Claude step used and changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct StepUsage {
    pub(super) tokens: Option<TokenTotals>,
    pub(super) edits: Vec<String>,
}

/// The run's view as the panel shows it.
pub(super) struct RunView(PressureRunView);

impl RunView {
    /// A run that has started no step yet (the open chat may still be closing).
    pub(super) fn new(plan: &str, rounds: u32) -> Self {
        RunView(PressureRunView {
            plan: plan.to_owned(),
            rounds,
            round: 1,
            step: PressureStepKind::ClaudePressure,
            started_ms: now_ms(),
            tokens_total: 0,
            steps: Vec::new(),
            ended: None,
        })
    }

    pub(super) fn view(&self) -> PressureRunView {
        self.0.clone()
    }

    /// A Claude step of `round` started in session `session`.
    pub(super) fn begin_claude(
        &mut self,
        round: u32,
        kind: PressureStepKind,
        session: &str,
        model: &str,
        effort: &str,
    ) {
        self.0.round = round;
        self.0.step = kind.clone();
        let step = running_step(round, kind, Some(session.to_owned()), model, effort);
        self.0.steps.push(step);
    }

    /// Codex started beside round `round`'s `/pressure` step; the step shown stays Claude's.
    pub(super) fn begin_codex(&mut self, round: u32, model: &str, effort: &str) {
        let mut step = running_step(round, PressureStepKind::Codex, None, model, effort);
        step.codex = Some(CodexStatus {
            model: model.to_owned(),
            state: CodexState::Running,
            report_path: None,
            log_tail: None,
        });
        self.0.steps.push(step);
    }

    /// The Claude half of the round finished and Codex still runs.
    pub(super) fn codex_alone(&mut self) {
        self.0.step = PressureStepKind::Codex;
    }

    /// The model the running Codex attempt uses (after a fallback).
    pub(super) fn codex_model(&mut self, round: u32, model: &str) {
        let codex = self.find(round, &PressureStepKind::Codex);
        if let Some(status) = codex.and_then(|step| step.codex.as_mut()) {
            model.clone_into(&mut status.model);
        }
    }

    /// A Claude step ended in `state` with what it used and changed.
    pub(super) fn finish_claude(
        &mut self,
        round: u32,
        kind: &PressureStepKind,
        state: StepState,
        usage: StepUsage,
    ) {
        if let Some(step) = self.find(round, kind) {
            step.state = state;
            step.ended_ms = Some(now_ms());
            step.tokens = usage.tokens;
            step.edits = usage.edits;
        }
        let totals = self.0.steps.iter().filter_map(|step| step.tokens.as_ref());
        self.0.tokens_total = totals.fold(0u64, |total, tokens| total.saturating_add(sum(tokens)));
    }

    /// Codex finished with `status`.
    pub(super) fn finish_codex(&mut self, round: u32, state: StepState, status: CodexStatus) {
        if let Some(step) = self.find(round, &PressureStepKind::Codex) {
            step.state = state;
            step.ended_ms = Some(now_ms());
            step.codex = Some(status);
        }
    }

    /// The run ended: every step still running is shown stopped.
    pub(super) fn end(&mut self, end: PressureEnd) {
        let now = now_ms();
        for step in &mut self.0.steps {
            if step.state == StepState::Running {
                step.state = StepState::Stopped;
                step.ended_ms = Some(now);
                if let Some(codex) = step
                    .codex
                    .as_mut()
                    .filter(|c| c.state == CodexState::Running)
                {
                    codex.state = CodexState::NoReview;
                }
            }
        }
        self.0.ended = Some(end);
    }

    fn find(&mut self, round: u32, kind: &PressureStepKind) -> Option<&mut PressureStepView> {
        let mut steps = self.0.steps.iter_mut().rev();
        steps.find(|step| step.round == round && step.kind == *kind)
    }
}

fn running_step(
    round: u32,
    kind: PressureStepKind,
    session_id: Option<String>,
    model: &str,
    effort: &str,
) -> PressureStepView {
    PressureStepView {
        round,
        kind,
        session_id,
        model: model.to_owned(),
        effort: effort.to_owned(),
        state: StepState::Running,
        started_ms: now_ms(),
        ended_ms: None,
        tokens: None,
        codex: None,
        edits: Vec::new(),
    }
}

fn sum(tokens: &TokenTotals) -> u64 {
    [
        tokens.input,
        tokens.cache_write,
        tokens.cache_read,
        tokens.output,
    ]
    .iter()
    .fold(0u64, |total, n| total.saturating_add(*n))
}

/// The tokens of session `session`'s turns and the union of the paths they changed, from a
/// chat snapshot taken when it ended (an ended chat is the snapshot's `previous`).
pub(super) fn step_usage(snapshot: &ChatSnapshot, session: &str) -> StepUsage {
    let turns = match (&snapshot.session, &snapshot.previous) {
        (Some(view), _) if view.id == session => &snapshot.turns,
        (_, Some(ended)) if ended.session.id == session => &ended.turns,
        _ => return StepUsage::default(),
    };
    let mut tokens = TokenTotals {
        input: 0,
        cache_write: 0,
        cache_read: 0,
        output: 0,
    };
    let mut edits = BTreeSet::new();
    for turn in turns {
        tokens.input = tokens.input.saturating_add(turn.tokens.input);
        tokens.cache_write = tokens.cache_write.saturating_add(turn.tokens.cache_write);
        tokens.cache_read = tokens.cache_read.saturating_add(turn.tokens.cache_read);
        tokens.output = tokens.output.saturating_add(turn.tokens.output);
        edits.extend(turn.changed_files.iter().map(|file| file.path.clone()));
    }
    StepUsage {
        tokens: Some(tokens),
        edits: edits.into_iter().collect(),
    }
}

/// How the run ended: a stop request decides the end of a run that did not record.
pub(super) fn ended(
    outcome: &anyhow::Result<PipelineOutcome>,
    cause: Option<&StopCause>,
) -> PressureEnd {
    match (outcome, cause) {
        (Ok(PipelineOutcome::Recorded { line }), _) => PressureEnd {
            line: Some(line.clone()),
            ..end(RunEndKind::Recorded, None, None, None)
        },
        (Ok(PipelineOutcome::RecordSkipped { line, .. }), _) => failed(
            Some(RECORD_STEP_LABEL),
            format!("plan moved during the run; not recorded: {line}"),
        ),
        (_, Some(cause)) => stopped(cause),
        (Ok(PipelineOutcome::Stopped { step, reason }), None) => {
            failed(Some(step.as_str()), reason.clone())
        }
        (Err(error), None) => failed(None, format!("{error:#}")),
    }
}

/// The end of a run stopped for `cause`.
pub(super) fn stopped(cause: &StopCause) -> PressureEnd {
    match cause {
        StopCause::Holder(name) => end(RunEndKind::Stopped, Some(name.as_str()), None, None),
        StopCause::GraceExpired => end(RunEndKind::Stopped, Some(HOLDER_AWAY), None, None),
        StopCause::Server => end(RunEndKind::Interrupted, None, None, Some(SERVER_STOPPED)),
    }
}

/// A run that failed before or outside the pipeline's steps.
pub(super) fn failed(step: Option<&str>, reason: String) -> PressureEnd {
    end(RunEndKind::Failed, None, step, Some(reason.as_str()))
}

fn end(
    kind: RunEndKind,
    by: Option<&str>,
    step: Option<&str>,
    reason: Option<&str>,
) -> PressureEnd {
    PressureEnd {
        kind,
        by: by.map(str::to_owned),
        step: step.map(str::to_owned),
        reason: reason.map(str::to_owned),
        line: None,
    }
}

/// Milliseconds since the Unix epoch; a clock before 1970 reads 0.
pub(super) fn now_ms() -> u64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}
