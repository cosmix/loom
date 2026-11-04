//! Scope setup runs after guidance and a failed write refuses the whole start.

use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::super::guidance::{GuidanceProvider, GuidanceSlot};
use super::super::tests::{browser, wait_for};
use super::super::{lock, ChatState};
use super::kickoff::offer;
use super::ok_result;
use super::recording::RecordingGuidance;
use super::team::team_fixture;
use crate::plan_server::context::PlanContext;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::SessionDir;
use crate::plan_session::scope::{self, SessionScope};

struct ScopeObstacle(RecordingGuidance);

impl GuidanceProvider for ScopeObstacle {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> anyhow::Result<LaunchGuidance> {
        let guidance = self.0.authoring(ctx, dir, offer, first_message, feature)?;
        std::fs::create_dir(dir.scope())?;
        Ok(guidance)
    }

    fn closing_message(&self) -> String {
        self.0.closing_message()
    }
}

#[test]
fn a_scope_write_failure_refuses_kickoff_and_team_starts() {
    for id in ["kickoff", "draft-all-sections"] {
        let f = team_fixture();
        let tab = f.connect(&browser());
        f.ctx
            .extensions()
            .get::<GuidanceSlot>()
            .unwrap()
            .set(Arc::new(ScopeObstacle(RecordingGuidance::default())));

        let result = f.command(&tab, offer(id, Some("demo")));

        assert_eq!(result["ok"], false);
        assert!(result["error"].as_str().unwrap().contains("scope.json"));
        assert!(f.bridge.chat_session_id().is_none());
        assert!(f.ctx.authoring().holder().is_none());
        let state = f.ctx.extensions().get::<ChatState>().unwrap();
        assert!(lock(&state.team).is_none());
    }
}

struct GatedGuidance {
    entered: mpsc::Sender<SessionDir>,
    release: Mutex<mpsc::Receiver<()>>,
    recording: RecordingGuidance,
}

impl GuidanceProvider for GatedGuidance {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> anyhow::Result<LaunchGuidance> {
        let guidance = self
            .recording
            .authoring(ctx, dir, offer, first_message, feature)?;
        self.entered.send(dir.clone())?;
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(5))?;
        Ok(guidance)
    }

    fn closing_message(&self) -> String {
        self.recording.closing_message()
    }
}

#[test]
fn scope_creation_waits_for_the_provider_and_precedes_launch() {
    let f = team_fixture();
    let tab = f.connect(&browser());
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let provider = Arc::new(GatedGuidance {
        entered: entered_tx,
        release: Mutex::new(release_rx),
        recording: RecordingGuidance::default(),
    });
    f.ctx
        .extensions()
        .get::<GuidanceSlot>()
        .unwrap()
        .set(provider.clone());

    thread::scope(|threads| {
        let starting = threads.spawn(|| f.command(&tab, offer("kickoff-small", None)));
        let dir = entered.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!dir.scope().exists());
        assert!(f.bridge.chat_session_id().is_none());
        release.send(()).unwrap();
        assert_eq!(starting.join().unwrap(), ok_result());
    });

    let id = wait_for("the chat", || f.bridge.chat_session_id());
    let s = f.launched(&id);
    assert!(matches!(
        scope::load(&s.dir).unwrap(),
        SessionScope::Kickoff(_)
    ));
    assert_eq!(
        *provider.recording.calls.lock().unwrap(),
        vec![(Some("kickoff-small".into()), None, None)]
    );
}
