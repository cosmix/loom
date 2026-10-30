//! An opening pasted but not yet submitted: Enter again while it waits for its
//! `UserPromptSubmit`, and the failure of a Summary session whose opening never went in.
//!
//! The paste's own Enter can miss the input box (`tmux_io::paste_text`), leaving the opening
//! typed but unsent. Unhandled, a Summary session would idle until its scheduler gives up.

use std::time::{Duration, Instant};

use super::commands::Step;
use super::session::{Phase, Session};
use super::Bridge;
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{Delivery, FailureKind};
use crate::plan_session::state::Input;

/// How long a pasted opening waits for its `UserPromptSubmit` before Enter is pressed again.
const ENTER_AGAIN_AFTER: Duration = Duration::from_secs(2);
/// How many times Enter is pressed again for one opening, inside the delivery timeout.
const ENTER_AGAIN_TIMES: u8 = 2;

impl Bridge {
    /// A pasted opening with no `UserPromptSubmit` gets Enter again when one is due. A Summary
    /// session whose opening is still unsubmitted at the delivery timeout fails the way a
    /// failed paste of it does; the scheduler then marks its unsealed summaries failed until
    /// they are invalidated again or the rebuild button is pressed.
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

    /// The failure of a live Summary session whose opening is unsubmitted at the timeout.
    fn unsubmitted_opening(
        &mut self,
        timeout: Duration,
        now: Instant,
        step: &mut Step,
    ) -> Option<String> {
        let overdue = self.waited(now).is_some_and(|waited| waited >= timeout);
        if !overdue || !self.opening_fails() || !matches!(self.phase, Phase::Live) {
            return None;
        }
        let late = self.flow.delivering.take()?;
        self.set_delivery(&late.item_id, Delivery::Undelivered, step);
        Some(format!("opening not submitted within {timeout:?}"))
    }

    /// The pane to press Enter in when a pasted opening is due another: every
    /// [`ENTER_AGAIN_AFTER`], at most [`ENTER_AGAIN_TIMES`], inside the timeout. Only Enter, as
    /// the text is already in the box. A chat message never gets one: its box may hold text
    /// the user has half typed.
    fn enter_again(&mut self, timeout: Duration, now: Instant) -> Option<(String, String)> {
        let waited = self.waited(now).filter(|waited| *waited < timeout)?;
        if !matches!(self.phase, Phase::Live) {
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
