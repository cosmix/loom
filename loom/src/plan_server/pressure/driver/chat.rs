//! Before the first step: the open authoring chat ends with its closing write, and its queued
//! messages go back to the starter's tabs (section 11, "The dialog").

use std::sync::mpsc::RecvTimeoutError;
use std::time::Instant;

use super::super::view::{self, StopCause};
use super::step::{Waited, TICK};
use super::RunEnv;
use crate::plan_server::wire::chat::ChatFrame;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::pressure::PressureEnd;
use crate::plan_session::bridge::{BridgeEvent, EndCause, EndMode};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::SessionState;

const CLOSE_STEP: &str = "close the open chat";
const CHAT_DID_NOT_END: &str = "the open chat did not end within its closing write's bound";

impl RunEnv {
    /// End every open authoring chat (the holder is present: its prompts stay answerable) and
    /// wait for its end; plan 7 keeps authoring with the starter while the run's lease lives.
    pub(super) fn close_chat(&self) -> Result<(), PressureEnd> {
        for id in self.open_chats() {
            match self
                .runs
                .sessions
                .end(&id, EndMode::FinalWrite, EndCause::Holder)
            {
                Ok(texts) => self.return_queue(texts),
                Err(error) => tracing::debug!(session = %id, %error, "the chat is already ending"),
            }
            match self.wait_ended(&id) {
                Waited::Stopped => {
                    let cause = self.run.cause().unwrap_or(StopCause::Server);
                    return Err(view::stopped(&cause));
                }
                Waited::TimedOut => {
                    let reason = CHAT_DID_NOT_END.to_owned();
                    return Err(view::failed(Some(CLOSE_STEP), reason));
                }
                Waited::Marker | Waited::Ended(_) => {}
            }
        }
        Ok(())
    }

    fn open_chats(&self) -> Vec<String> {
        let sessions = self.runs.sessions.sessions().into_iter();
        let open = sessions.filter(|(_, kind, state)| {
            *kind == SessionKind::Authoring
                && !matches!(state, SessionState::Ended | SessionState::Failed)
        });
        open.map(|(id, ..)| id).collect()
    }

    fn return_queue(&self, texts: Vec<String>) {
        if texts.is_empty() {
            return;
        }
        let frame = PlanMessage::Chat(Box::new(ChatFrame::QueueReturned { texts }));
        self.ctx.hub().send_to_browser(&self.run.browser, &frame);
    }

    fn wait_ended(&self, id: &str) -> Waited {
        let deadline = Instant::now() + self.runs.tools.chat_close;
        loop {
            match self.events.recv_timeout(TICK) {
                Ok(BridgeEvent::Ended { id: ended, .. }) if ended == id => {
                    return Waited::Ended(String::new());
                }
                Err(RecvTimeoutError::Disconnected) => return Waited::Ended(String::new()),
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            }
            if self.stopping() {
                return Waited::Stopped;
            }
            if Instant::now() >= deadline {
                return Waited::TimedOut;
            }
        }
    }
}
