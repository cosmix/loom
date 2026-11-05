//! Start tests: what a refused start leaves behind, the provider deciding the offer ids, and
//! an ending chat's hold beside the next start.

use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::super::guidance::{FixedGuidance, GuidanceProvider, GuidanceSlot};
use super::super::tests::{browser, ok_result, start, wait_for, wait_until, Fixture, Tab};
use super::super::ChatState;
use super::refused;
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::bridge::{BridgeError, EndCause, EndMode, LaunchGuidance, StartRequest};
use crate::plan_session::input::InputRefusal;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::Origin;

/// The bridge's start of a chat whose guidance waits to be released.
type Preparing = thread::JoinHandle<Result<String, BridgeError>>;

/// Refuses the first start once released; records whether `holder` held authoring when each
/// later start, reserved by then, ran its guidance.
struct GatedGuidance {
    holder: BrowserId,
    entered: mpsc::Sender<()>,
    release: Mutex<Option<mpsc::Receiver<()>>>,
    held: Mutex<Vec<bool>>,
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
        let gate = self.release.lock().unwrap().take();
        if let Some(release) = gate {
            self.entered.send(()).unwrap();
            // Bounded: a failing test still ends the blocked start, so its scope can join it.
            let _ = release.recv_timeout(Duration::from_secs(5));
            anyhow::bail!("the first start is refused");
        }
        let held = ctx.authoring().is_holder(&self.holder);
        self.held.lock().unwrap().push(held);
        FixedGuidance.authoring(ctx, dir, offer, first_message, feature)
    }

    fn closing_message(&self) -> String {
        FixedGuidance.closing_message()
    }
}

#[test]
fn a_refused_start_leaves_authoring_free() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        feature: None,
        plan: None,
        text: Some("/help".into()),
        attachments: Vec::new(),
    };

    assert_eq!(f.command(&tab, cmd), refused(InputRefusal::SlashCommand));
    assert!(f.ctx.authoring().holder().is_none());
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn a_start_with_blank_text_and_no_offer_is_refused_and_launches_nothing() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    for text in [Some(""), Some("  \n\t "), None] {
        let cmd = ChatCommand::Start {
            client_id: Some("c1".into()),
            offer: None,
            feature: None,
            plan: None,
            text: text.map(Into::into),
            attachments: Vec::new(),
        };

        assert_eq!(
            f.command(&tab, cmd),
            refused("a chat starts with your first message")
        );
    }
    let send = ChatCommand::Send {
        client_id: Some("c1".into()),
        text: " ".into(),
        attachments: Vec::new(),
    };
    // A send that would start a chat is refused too: `check_delivered` rejects a blank message.
    assert_eq!(f.command(&tab, send), refused(InputRefusal::Empty));
    assert!(f.ctx.authoring().holder().is_none());
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn a_start_with_an_id_the_checklist_does_not_offer_is_refused_by_the_provider() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let offer = "refine:doc/product/index.md";
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: Some(offer.into()),
        feature: None,
        plan: None,
        text: None,
        attachments: Vec::new(),
    };

    let result = f.command(&tab, cmd);

    assert_eq!(
        result,
        refused(format!("the step \"{offer}\" is not offered now"))
    );
    assert!(f.ctx.authoring().holder().is_none());
    assert!(f.bridge.chat_session_id().is_none());
}

#[test]
fn a_replaced_provider_decides_which_offer_ids_start_a_chat() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let slot = f.ctx.extensions().get::<GuidanceSlot>().unwrap();
    // FixedGuidance accepts every id, as plan 8's provider accepts its own start ids.
    slot.set(Arc::new(FixedGuidance));
    let offer = "refine:doc/product/index.md";
    let cmd = ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: Some(offer.into()),
        feature: None,
        plan: None,
        text: None,
        attachments: Vec::new(),
    };

    assert_eq!(f.command(&tab, cmd), ok_result());
    wait_for("the chat", || f.bridge.chat_session_id());
    assert!(f.ctx.authoring().is_holder(&tab.conn.browser));
    let session = f.bridge.chat_snapshot(true).session.unwrap();
    assert_eq!(session.offer.as_deref(), Some(offer));
}

/// A chat `other` reserved through the bridge, its guidance closure (the one `launch` builds)
/// held inside the provider: the receiver hears it entered, the sender releases it.
fn reserve_held(f: &Fixture, other: &Tab) -> (mpsc::Receiver<()>, mpsc::Sender<()>, Preparing) {
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel::<()>();
    let (ctx, starter) = (Arc::clone(&f.ctx), other.conn.public_id.clone());
    let guidance = Box::new(move |dir: &SessionDir| {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        let state = ctx.extensions().get::<ChatState>().unwrap();
        let id = dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        state.started(id, starter);
        FixedGuidance.authoring(&ctx, dir, None, None, None)
    });
    let request = StartRequest {
        kind: SessionKind::Authoring,
        guidance,
        opening_origin: Some(Origin::Offer),
        offer: None,
        feature: None,
        plan: None,
        extra_env: Vec::new(),
    };
    let bridge = Arc::clone(&f.bridge);
    let preparing = thread::spawn(move || bridge.start_session(request));
    (entered, release, preparing)
}

/// The finding's race, made deterministic: another tab of the same browser has reserved its
/// chat and runs its guidance (the bridge does not name it the chat yet) while authoring is
/// free, so this tab's start takes authoring and is refused. The refusal must not free the
/// hold the preparing chat keeps until it ends.
#[test]
fn a_start_refused_while_another_tabs_chat_prepares_keeps_authoring() {
    let f = Fixture::new(false);
    let holder = browser();
    let (tab, other) = (f.connect(&holder), f.connect(&holder));
    let (entered, release, preparing) = reserve_held(&f, &other);
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(f.bridge.chat_session_id().is_none());

    assert_eq!(f.command(&tab, start()), refused("a chat is running"));
    assert!(
        f.ctx.authoring().is_holder(&holder),
        "the refusal freed authoring"
    );

    release.send(()).unwrap();
    let id = preparing.join().unwrap().unwrap();
    wait_for("the chat", || f.bridge.chat_session_id());
    f.launched(&id);
    assert!(
        f.ctx.authoring().is_holder(&holder),
        "the chat started free"
    );
    let ended = f.bridge.end(&id, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("the end", || !f.ended(&id).is_empty());
    assert!(
        f.ctx.authoring().holder().is_none(),
        "the chat's end kept authoring"
    );
}

/// Tab A's start takes authoring and its provider refuses; tab B of the same browser
/// starts meanwhile. Without one start at a time, B passes its take while A holds authoring,
/// and A's give-back then frees authoring under B's chat. B must wait for A's give-back and
/// take authoring itself.
#[test]
fn a_second_start_waits_for_a_refused_start_to_give_authoring_back() {
    let f = Fixture::new(false);
    let holder = browser();
    let (tab, other) = (f.connect(&holder), f.connect(&holder));
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    let provider = Arc::new(GatedGuidance {
        holder: holder.clone(),
        entered: entered_tx,
        release: Mutex::new(Some(release_rx)),
        held: Mutex::default(),
    });
    let slot = f.ctx.extensions().get::<GuidanceSlot>().unwrap();
    slot.set(provider.clone());
    let (done_tx, done) = mpsc::channel();

    thread::scope(|scope| {
        let first = scope.spawn(|| f.command(&tab, start()));
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let second = scope.spawn(|| {
            let result = f.command(&other, start());
            done_tx.send(()).unwrap();
            result
        });
        let waited = done.recv_timeout(Duration::from_millis(300)).is_err();
        release.send(()).unwrap();
        assert!(waited, "the second start did not wait for the first");
        assert_eq!(first.join().unwrap()["ok"], false);
        assert_eq!(second.join().unwrap(), ok_result());
    });

    let id = wait_for("the chat", || f.bridge.chat_session_id());
    f.launched(&id);
    assert_eq!(
        *provider.held.lock().unwrap(),
        vec![true],
        "the second chat started with authoring free"
    );
    assert!(f.ctx.authoring().is_holder(&holder));
}

/// Holds the bridge's flush on a thread of its own: its send to `id` claims the flush and the
/// queue delta blocks in a listener until released, so what other threads publish meanwhile
/// waits in the outbox. Returns once the flush is held.
fn hold_flush(f: &Fixture, id: &str) -> (mpsc::Sender<()>, thread::JoinHandle<()>) {
    const HOLDER: &str = "held-flush";
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel::<()>();
    let gate = Arc::new(Mutex::new(Some((entered_tx, release_rx))));
    let listening = Arc::clone(&gate);
    f.bridge.subscribe_chat(Box::new(move |_| {
        if thread::current().name() != Some(HOLDER) {
            return;
        }
        let taken = listening.lock().unwrap().take();
        if let Some((entered, release)) = taken {
            entered.send(()).unwrap();
            // Bounded: a failing test still ends the held flush, so its thread can be joined.
            let _ = release.recv_timeout(Duration::from_secs(5));
        }
    }));
    let (bridge, id) = (Arc::clone(&f.bridge), id.to_owned());
    let holding = thread::Builder::new().name(HOLDER.into()).spawn(move || {
        // A send whose flush the driver had claimed returns at once: send again.
        for _ in 0..100 {
            let sent = bridge.send(&id, "hold".into(), Vec::new());
            assert!(sent.is_ok(), "the holding send was refused");
            if gate.lock().unwrap().is_none() {
                return;
            }
        }
    });
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    (release, holding.unwrap())
}

/// Round 8's race, made deterministic: browser X's chat ends and, before its `Ended` is heard,
/// another tab of X reserves the next chat (the bridge drops the ended chat first, so the
/// reserve passes). The ended chat must not free the hold the next one runs under.
#[test]
fn a_chat_ending_while_its_browsers_next_chat_prepares_keeps_authoring() {
    let f = Fixture::new(false);
    let holder = browser();
    let (tab, other) = (f.connect(&holder), f.connect(&holder));
    let first = f.start(&tab);
    let (unflush, flushing) = hold_flush(&f, &first.id);
    let ended = f.bridge.end(&first.id, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    let listed = |id: &str| f.bridge.sessions().iter().any(|(listed, ..)| listed == id);
    wait_until("the first chat dropped", || !listed(&first.id));
    let (entered, release, preparing) = reserve_held(&f, &other);
    entered.recv_timeout(Duration::from_secs(5)).unwrap();

    unflush.send(()).unwrap();
    flushing.join().unwrap();
    wait_until("the first end", || !f.ended(&first.id).is_empty());
    assert!(
        f.ctx.authoring().is_holder(&holder),
        "the first chat's end freed authoring"
    );

    release.send(()).unwrap();
    let id = preparing.join().unwrap().unwrap();
    f.launched(&id);
    let ended = f.bridge.end(&id, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("the second end", || !f.ended(&id).is_empty());
    assert!(
        f.ctx.authoring().holder().is_none(),
        "the second chat's end kept authoring"
    );
}

/// A chat that ends while a start holds `starting` (here the test) waits for that start, then
/// frees authoring when no chat followed.
#[test]
fn a_chat_ending_during_a_start_frees_authoring_once_the_start_gives_up() {
    let f = Fixture::new(false);
    let holder = browser();
    let tab = f.connect(&holder);
    let first = f.start(&tab);
    let state = f.ctx.extensions().get::<ChatState>().unwrap();
    let starting = state.starting();
    let ended = f.bridge.end(&first.id, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("the end", || !f.ended(&first.id).is_empty());
    assert!(
        f.ctx.authoring().is_holder(&holder),
        "the end freed authoring during a start"
    );

    drop(starting);
    wait_until("authoring freed", || f.ctx.authoring().holder().is_none());
}
