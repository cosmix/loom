//! The app's pressure driver: a pressure run on the planning server, from the accepted start
//! request to the release of authoring.
//!
//! The run thread goes through the shared pipeline only (`loom::pressure::run_pipeline`); this
//! driver supplies its way of running each step: every Claude step is a pressure session
//! through plan 7's bridge, Codex runs as `codex exec` beside the `/pressure` session. The
//! bridge calls go through [`PressureSessions`], implemented by the bridge itself and by the
//! tests' fake.

mod chat;
mod live;
mod step;

pub(super) use live::{teardown, Run};
#[cfg(test)]
pub(super) use step::TEAMMATE_PANES;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;

use super::install::PressureRuns;
use super::view::{self, RunView, StopCause};
use crate::fs::work_dir::PlanConfig;
use crate::plan_server::authoring::{FreeReason, Refusal, Take};
use crate::plan_server::chat::lease::{AuthoringLease, AuthoringLeases};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::lifecycle;
use crate::plan_server::summaries::rebuilds::{Rebuilds, SuspendGuard};
use crate::plan_server::summaries::scheduler::SuspendReason;
use crate::plan_server::wire::pressure::{
    PressureEnd, PressureRunView, PressureSlots, PressureStart,
};
use crate::plan_session::bridge::{
    Bridge, BridgeError, BridgeEvent, EndCause, EndMode, RecoveryReport, StartRequest, Transcripts,
};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{ChatSnapshot, SessionState};
use crate::planning::guidance::GuidanceModels;
use crate::pressure::paths::{codex_report_path, resolve_plan_path, ResolvedPlan};
use crate::pressure::{run_pipeline, PipelineInput, PressureDriver, PressureResult, StepVerdict};
use crate::process::ProcessIdentity;
use crate::user_config::UserConfig;

/// The bridge calls a pressure run makes; the bridge implements it, the tests fake it.
pub trait PressureSessions: Send + Sync {
    fn start_session(&self, request: StartRequest) -> Result<String, BridgeError>;
    fn subscribe(&self, listener: Box<dyn Fn(&BridgeEvent) + Send + Sync>);
    fn interrupt(&self, id: &str) -> Result<Vec<String>, BridgeError>;
    fn end(&self, id: &str, mode: EndMode, cause: EndCause) -> Result<Vec<String>, BridgeError>;
    fn record_child(
        &self,
        id: &str,
        label: &str,
        child: ProcessIdentity,
    ) -> Result<(), BridgeError>;
    fn sessions(&self) -> Vec<(String, SessionKind, SessionState)>;
    fn chat_snapshot(&self, for_holder: bool) -> ChatSnapshot;
    fn release_session(&self, id: &str) -> Result<(), BridgeError>;
    fn add_token_transcript(&self, id: &str, path: PathBuf) -> Result<(), BridgeError>;
    fn transcripts(&self, id: &str) -> Option<Transcripts>;
    fn on_recovered(&self, f: Box<dyn FnOnce(&RecoveryReport) + Send>);
}

impl PressureSessions for Bridge {
    fn start_session(&self, request: StartRequest) -> Result<String, BridgeError> {
        Bridge::start_session(self, request)
    }
    fn subscribe(&self, listener: Box<dyn Fn(&BridgeEvent) + Send + Sync>) {
        Bridge::subscribe(self, listener);
    }
    fn interrupt(&self, id: &str) -> Result<Vec<String>, BridgeError> {
        Bridge::interrupt(self, id)
    }
    fn end(&self, id: &str, mode: EndMode, cause: EndCause) -> Result<Vec<String>, BridgeError> {
        Bridge::end(self, id, mode, cause)
    }
    fn record_child(
        &self,
        id: &str,
        label: &str,
        child: ProcessIdentity,
    ) -> Result<(), BridgeError> {
        Bridge::record_child(self, id, label, child)
    }
    fn sessions(&self) -> Vec<(String, SessionKind, SessionState)> {
        Bridge::sessions(self)
    }
    fn chat_snapshot(&self, for_holder: bool) -> ChatSnapshot {
        Bridge::chat_snapshot(self, for_holder)
    }
    fn release_session(&self, id: &str) -> Result<(), BridgeError> {
        Bridge::release_session(self, id)
    }
    fn add_token_transcript(&self, id: &str, path: PathBuf) -> Result<(), BridgeError> {
        Bridge::add_token_transcript(self, id, path)
    }
    fn transcripts(&self, id: &str) -> Option<Transcripts> {
        Bridge::transcripts(self, id)
    }
    fn on_recovered(&self, f: Box<dyn FnOnce(&RecoveryReport) + Send>) {
        Bridge::on_recovered(self, f);
    }
}

/// Why an otherwise valid start was not accepted.
#[derive(Debug)]
pub(super) enum StartRefused {
    PlanNotWritten,
    Running,
    Authoring(Refusal),
    Unavailable(String),
}

/// Held from the start's acceptance until the run ended. The suspension comes before the open
/// chat's closing write, so that turn's end starts no summary session (px-state-17).
struct Holds {
    suspend: Option<SuspendGuard>,
    lease: Option<AuthoringLease>,
}

impl Holds {
    fn take(ctx: &PlanContext) -> Holds {
        let lease = ctx
            .extensions()
            .get::<AuthoringLeases>()
            .map(|leases| leases.hold());
        let rebuilds = ctx.extensions().get::<Rebuilds>();
        let suspend = rebuilds.map(|rebuilds| rebuilds.suspend(SuspendReason::PressureRun));
        Holds { suspend, lease }
    }
}

/// What one run's thread works with.
pub(super) struct RunEnv {
    pub(super) ctx: Arc<PlanContext>,
    pub(super) runs: Arc<PressureRuns>,
    pub(super) run: Arc<Run>,
    pub(super) events: Receiver<BridgeEvent>,
    pub(super) plan: ResolvedPlan,
    pub(super) slots: PressureSlots,
    pub(super) codex_path: PathBuf,
    pub(super) models: GuidanceModels,
}

impl RunEnv {
    /// Change the view and broadcast it.
    pub(super) fn publish(&self, change: impl FnOnce(&mut RunView)) {
        let view = self.run.update(change);
        self.runs.publish(&view);
    }

    /// The run was asked to stop, or the server is stopping (its chat drain may end a step
    /// before the pressure drain has asked).
    pub(super) fn stopping(&self) -> bool {
        if self.ctx.is_stopping() {
            self.run.request_stop(StopCause::Server);
        }
        self.run.stopped()
    }
}

/// Take authoring, hold the lease and the summary suspension, and start the run's thread.
pub(super) fn accept(
    runs: &Arc<PressureRuns>,
    browser: &BrowserId,
    starter: &PublicId,
    start: PressureStart,
    codex_path: PathBuf,
) -> Result<PressureRunView, StartRefused> {
    let ctx = runs
        .ctx()
        .ok_or_else(|| StartRefused::Unavailable("stopping".into()))?;
    let plan = resolve(&ctx, &start.plan)?;
    if !runs.claim() {
        return Err(StartRefused::Running);
    }
    if let Err(refusal) = ctx.require_authoring(browser, Take::TakeIfFree) {
        runs.unclaim();
        return Err(StartRefused::Authoring(refusal));
    }
    let holds = Holds::take(&ctx);
    let view = RunView::new(&plan.invocation, start.slots.rounds);
    let run = Arc::new(Run::new(starter.clone(), browser.clone(), view));
    let env = RunEnv {
        events: runs.begin(Arc::clone(&run)),
        models: guidance_models(ctx.root()),
        ctx: Arc::clone(&ctx),
        runs: Arc::clone(runs),
        run: Arc::clone(&run),
        plan,
        slots: start.slots,
        codex_path,
    };
    let view = run.view();
    runs.publish(&view);
    let spawned = thread::Builder::new()
        .name("loom-plan-pressure".into())
        .spawn(move || run_thread(env, holds));
    if let Err(error) = spawned {
        ctx.authoring()
            .release_if(starter, FreeReason::Released("pressure run ended"));
        runs.end(&run);
        run.mark_done();
        return Err(StartRefused::Unavailable(format!(
            "cannot start the run: {error}"
        )));
    }
    Ok(view)
}

/// The plan's path on disk and as the commands are given it (repo-relative).
fn resolve(ctx: &PlanContext, plan: &str) -> Result<ResolvedPlan, StartRefused> {
    let root = ctx.root().canonicalize();
    let root = root.map_err(|error| StartRefused::Unavailable(error.to_string()))?;
    resolve_plan_path(plan, &root).map_err(|_| StartRefused::PlanNotWritten)
}

fn guidance_models(root: &Path) -> GuidanceModels {
    GuidanceModels::load(root).unwrap_or_else(|error| {
        tracing::warn!("cannot load planning guidance models; using defaults: {error}");
        GuidanceModels::resolve(&PlanConfig::default(), &UserConfig::default())
    })
}

/// The run, then its end: the view, the holds, authoring, the running flag.
fn run_thread(env: RunEnv, holds: Holds) {
    let end = match catch_unwind(AssertUnwindSafe(|| run_body(&env))) {
        Ok(end) => end,
        Err(_) => view::failed(None, "the pressure run failed unexpectedly".to_owned()),
    };
    finish(&env, end, holds);
}

fn run_body(env: &RunEnv) -> PressureEnd {
    if let Err(end) = env.close_chat() {
        return end;
    }
    execute(env)
}

/// Run the shared pipeline with this driver; no second step list.
fn execute(env: &RunEnv) -> PressureEnd {
    let report = codex_report_path(&env.plan.fs_path);
    let input = PipelineInput {
        rounds: env.slots.rounds,
        invocation: &env.plan.invocation,
        report: &report,
        plan_path: &env.plan.fs_path,
        claude_model: &env.slots.claude_model,
        today: chrono::Local::now().date_naive(),
    };
    let mut driver = AppDriver(env);
    let outcome = run_pipeline(&mut driver, &input);
    if outcome.is_err() {
        // A failed record or cleanup: the report must still not outlive the run.
        if let Err(error) = crate::claude::remove_if_exists(&report) {
            tracing::warn!(%error, "cannot delete the Codex report");
        }
    }
    view::ended(&outcome, env.run.cause().as_ref())
}

/// Whatever ended the run: nothing of it keeps running, the end is shown, the suspension and
/// the lease go (plan 10 then queues the plan's summary), and the starter's authoring is
/// released unless another browser took it meanwhile.
fn finish(env: &RunEnv, end: PressureEnd, holds: Holds) {
    for line in teardown(&*env.runs.sessions, &env.run) {
        lifecycle::line(&line);
    }
    env.publish(|view| view.end(end));
    let Holds { suspend, lease } = holds;
    drop(suspend);
    drop(lease);
    let reason = FreeReason::Released("pressure run ended");
    env.ctx.authoring().release_if(&env.run.starter, reason);
    env.runs.end(&env.run);
    env.run.mark_done();
}

/// The planning server's [`PressureDriver`]: Claude steps are pressure sessions.
pub struct AppDriver<'a>(&'a RunEnv);

impl PressureDriver for AppDriver<'_> {
    fn delete_report(&mut self, report: &Path) -> anyhow::Result<()> {
        crate::claude::remove_if_exists(report)
    }

    fn run_pressure(
        &mut self,
        round: u32,
        _claude: &str,
        codex: &str,
    ) -> anyhow::Result<PressureResult> {
        Ok(self.0.pressure_step(round, codex))
    }

    fn run_address(&mut self, round: u32, _slash: &str) -> anyhow::Result<StepVerdict> {
        Ok(self.0.address_step(round))
    }
}
