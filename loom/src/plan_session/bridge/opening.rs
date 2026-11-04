//! An opening pasted but not yet submitted: Enter again while it waits for its
//! `UserPromptSubmit`, and the failure of a Summary session whose opening never went in.
//!
//! The paste's own Enter can miss the input box (`tmux_io::paste_text`), leaving the opening
//! typed but unsent. Unhandled, a Summary session would idle until its scheduler gives up.

use std::time::{Duration, Instant};

use super::commands::Step;
use super::session::{Delivering, Opening, Phase, Session};
use super::Bridge;
use crate::plan_session::attachments::parse_unwrapped;
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{Activity, Delivery, FailureKind};
use crate::plan_session::state::Input;

/// How the failure of a Summary session's unsubmitted opening starts; the rebuild scheduler
/// tells it apart by this, since no turn ran and its summaries deserve another session.
pub const OPENING_NOT_SUBMITTED: &str = "opening not submitted";
/// How long a pasted opening waits for its `UserPromptSubmit` before Enter is pressed again.
const ENTER_AGAIN_AFTER: Duration = Duration::from_secs(2);
/// How many times Enter is pressed again for one opening, inside the delivery timeout.
const ENTER_AGAIN_TIMES: u8 = 2;

impl Bridge {
    /// A pasted opening with no `UserPromptSubmit` gets Enter again when one is due. A Summary
    /// session whose opening is still unsubmitted at the delivery timeout fails the way a
    /// failed paste of it does; the scheduler then requeues its unsealed summaries once, and
    /// marks them failed when the next session's opening goes the same way.
    pub(super) fn opening_step(&self, id: &str) {
        let timeout = self.config.delivery_timeout;
        let claimed = self.with_session(id, |session, step| {
            let now = Instant::now();
            match session.unsubmitted_opening(timeout, now, step) {
                Some(message) => Err(message),
                None => Ok(session.enter_again(timeout, now)),
            }
        });
        match claimed {
            Some(Err(message)) => {
                self.fail_live(id, Input::SpawnFailed, FailureKind::Spawn, message);
            }
            Some(Ok(Some((socket, target)))) => {
                if let Err(error) = self.host.press_enter(&socket, &target) {
                    tracing::warn!(session = %id, %error, "cannot press Enter for the opening");
                }
            }
            _ => {}
        }
    }
}

impl Session {
    /// The sent items of the opening: one, or, when it carries typed text after the guidance,
    /// the guidance as an origin item and the typed text (with its attachments) as a plain user
    /// item. The paste is the whole opening either way.
    pub(super) fn opening_delivery(&mut self, opening: Opening, step: &mut Step) -> Delivering {
        let Opening {
            text,
            origin,
            typed,
        } = opening;
        let guidance = typed.as_deref().and_then(|typed| {
            let head = text.strip_suffix(typed)?.strip_suffix("\n\n")?;
            Some(head.to_owned())
        });
        let (Some(typed), Some(guidance)) = (typed, guidance) else {
            let chips = parse_unwrapped(&text).1;
            return self.delivery(text, chips, origin, step);
        };
        let guidance_id = self.sent_item(parse_unwrapped(&guidance).0, Vec::new(), origin, step);
        let (typed_text, chips) = parse_unwrapped(&typed);
        let typed_id = self.sent_item(typed_text, chips, None, step);
        self.delivering(vec![guidance_id, typed_id], text)
    }

    /// How long the pasted delivery has waited for its `UserPromptSubmit`.
    pub(super) fn waited(&self, now: Instant) -> Option<Duration> {
        let sent = self.flow.delivering.as_ref().and_then(|d| d.sent_at)?;
        Some(now.duration_since(sent))
    }

    /// The delivery is a Summary session's opening, outside an end: nobody can resend it, so
    /// one never submitted fails the session instead of waiting undelivered.
    pub(super) fn opening_fails(&self) -> bool {
        let opening = self
            .flow
            .delivering
            .as_ref()
            .is_some_and(|d| d.opening.is_some());
        opening && matches!(self.kind, SessionKind::Summary) && self.flow.end.is_none()
    }

    /// The failure of a live Summary session whose opening is unsubmitted at the timeout. Only
    /// an idle session is unsubmitted: one working or on a permission prompt took the opening
    /// and lost its `UserPromptSubmit`, and has a turn to finish.
    fn unsubmitted_opening(
        &mut self,
        timeout: Duration,
        now: Instant,
        step: &mut Step,
    ) -> Option<String> {
        let overdue = self.waited(now).is_some_and(|waited| waited >= timeout);
        if !overdue || !self.opening_fails() || !self.idle_and_live() {
            return None;
        }
        let late = self.flow.delivering.take()?;
        self.set_delivery(&late.item_ids, Delivery::Undelivered, step);
        Some(format!("{OPENING_NOT_SUBMITTED} within {timeout:?}"))
    }

    fn idle_and_live(&self) -> bool {
        matches!(self.phase, Phase::Live) && self.machine.activity() == Activity::Idle
    }

    /// The pane to press Enter in when a pasted opening is due another: every
    /// [`ENTER_AGAIN_AFTER`], at most [`ENTER_AGAIN_TIMES`], inside the timeout. Only Enter, as
    /// the text is already in the box. A chat message never gets one: its box may hold text
    /// the user has half typed. A session not idle gets none either: Enter would answer its
    /// permission prompt.
    fn enter_again(&mut self, timeout: Duration, now: Instant) -> Option<(String, String)> {
        let waited = self.waited(now).filter(|waited| *waited < timeout)?;
        if !self.idle_and_live() {
            return None;
        }
        let presses = self.flow.delivering.as_mut()?.opening.as_mut()?;
        let due = ENTER_AGAIN_AFTER * (u32::from(*presses) + 1);
        if *presses >= ENTER_AGAIN_TIMES || waited < due {
            return None;
        }
        *presses += 1;
        Some((self.socket().to_owned(), self.target().to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use serde_json::json;

    use super::super::fake_host::*;
    use super::*;
    use crate::plan_session::kind::SessionKind::Summary;
    use crate::plan_session::model::SessionState;

    #[test]
    fn a_session_not_idle_gets_no_enter_and_does_not_fail() {
        let timeout = Duration::from_millis(2500);
        let h = Harness::ready(|c| c.delivery_timeout = timeout);
        let s = h.start(Summary);
        h.spool(&s, "SessionStart", json!({"source": "startup"}));
        wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
        h.permission(&s, "Bash", json!({"command": "true"}));
        h.wait_state(&s, SessionState::Permission);
        thread::sleep(timeout + Duration::from_millis(500));
        let presses = h.host.count(&Call::PressEnter(s.socket.clone()));
        assert_eq!(presses, 0, "Enter would answer the permission prompt");
        assert!(h.ended(&s.id).is_none(), "the session has a turn to finish");
    }
}
