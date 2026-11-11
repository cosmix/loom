//! The conversation flow: what the driver pastes into the pane (the opening, the queue, the
//! closing message), how a paste is confirmed or times out, and the Escape press. An opening
//! pasted but not submitted is `opening.rs`.

use std::time::Instant;

use super::commands::Step;
use super::session::{Closing, Delivering, Phase, Queued, Session, Slash};
use super::{Bridge, BridgeConfig, EndMode};
use crate::plan_session::attachments::{parse_message, parse_unwrapped, Attachment};
use crate::plan_session::input::{check_delivered, match_key, sanitize};
use crate::plan_session::model::{
    Activity, CommandRef, Delivery, FailureKind, Item, NoticeKind, Origin, SessionState,
};
use crate::plan_session::state::Input;
use crate::plan_session::transcript::{cap_chars, BACKGROUND_TAGS, LINE_CHARS};

/// Deliveries that timed out and may still be confirmed late.
const UNDELIVERED_KEPT: usize = 8;

/// The prompt Claude Code submits for a background task's `<task-notification>` or an
/// agent's `<agent-message>` hand-back (probe P1), or a message that starts the same way.
fn background_prompt(prompt: &str) -> bool {
    let lead = prompt.trim_start();
    BACKGROUND_TAGS.iter().any(|tag| lead.starts_with(tag))
}

/// A paste the driver claimed.
struct Paste {
    socket: String,
    target: String,
    text: String,
}

impl Bridge {
    /// Paste the opening, the queue head or the closing message when the session can take it.
    pub(super) fn deliver_step(&self, id: &str) {
        self.opening_step(id);
        let due = self.with_session(id, |session, _| session.closing_due());
        // The closing text is read outside the lock: the closure belongs to the server.
        let closing = (due == Some(true)).then(|| (self.config.closing_message)());
        let taken = self.with_session(id, |session, step| session.take_paste(closing, step));
        let Some(Some(paste)) = taken else {
            return;
        };
        let pasted = self.host.deliver(&paste.socket, &paste.target, &paste.text);
        let pasted = pasted.map_err(|error| format!("{error:#}"));
        let failed = self.with_session(id, |session, step| session.pasted(pasted, step));
        if let Some(Some(message)) = failed {
            self.fail_live(id, Input::SpawnFailed, FailureKind::Spawn, message);
        }
    }

    /// Press Escape for an interrupt the command accepted.
    pub(super) fn press_step(&self, id: &str) {
        let claim = self.with_session(id, |session, _| {
            let due = session.flow.press_pending && matches!(session.phase, Phase::Live);
            session.flow.press_pending = false;
            due.then(|| (session.socket().to_owned(), session.target().to_owned()))
        });
        if let Some(Some((socket, target))) = claim {
            if let Err(error) = self.host.interrupt(&socket, &target) {
                tracing::warn!(session = %id, %error, "cannot interrupt the session");
            }
        }
    }
}

impl Session {
    /// Ending, idle, opened, with its closing message not yet sent. A session whose opening
    /// never went out had no turn: `end_due` tears it down instead.
    fn closing_due(&self) -> bool {
        matches!(self.phase, Phase::Live)
            && self.flow.opening.is_none()
            && self.flow.delivering.is_none()
            && self.closing() == Some(Closing::NotSent)
            && self.machine.activity() == Activity::Idle
    }

    fn take_paste(&mut self, closing: Option<String>, step: &mut Step) -> Option<Paste> {
        if !matches!(self.phase, Phase::Live) {
            return None;
        }
        if self.flow.delivering.is_none() {
            self.flow.delivering = match closing.filter(|_| self.closing_due()) {
                Some(text) => self.closing_delivery(text, step),
                None => self.next_delivery(step),
            };
        }
        let ending = self.flow.end.is_some();
        let ready = if ending {
            self.machine.activity() == Activity::Idle
        } else {
            self.state() == SessionState::Idle
        };
        let delivering = self.flow.delivering.as_mut()?;
        // While ending only the closing message goes out (px-state-07).
        if delivering.sent_at.is_some() || !ready || (ending && !delivering.closing) {
            return None;
        }
        delivering.sent_at = Some(Instant::now());
        Some(Paste {
            socket: self.record.socket.clone(),
            target: self.record.tmux_session.clone(),
            text: delivering.text.clone(),
        })
    }

    /// The opening first, then the queue head unless the queue waits (px-state-04, -08).
    fn next_delivery(&mut self, step: &mut Step) -> Option<Delivering> {
        if self.flow.end.is_some() || self.state() != SessionState::Idle {
            return None;
        }
        if let Some(opening) = self.flow.opening.take() {
            let mut delivering = self.opening_delivery(opening, step);
            delivering.opening = Some(0);
            return Some(delivering);
        }
        let skip = std::mem::take(&mut self.flow.skip_delivery);
        let waits = self.flow.paused || self.flow.quiet_until.is_some();
        if skip || waits || self.flow.queue.is_empty() {
            return None;
        }
        let taken = self.next_queued(step);
        self.publish_queue(step);
        let (queued, slash) = taken?;
        let delivering = self.delivery(queued.text, queued.attachments, None, step);
        Some(delivering.with_slash(slash))
    }

    /// The queue head the session's table lets through now, with the command it names. A slash
    /// item the table refuses now is dropped, never pasted, with a notice naming it, and the
    /// next item is eligible at once (B-D2, "Checked again at delivery").
    fn next_queued(&mut self, step: &mut Step) -> Option<(Queued, Option<Slash>)> {
        while let Some(queued) = self.flow.queue.pop_front() {
            match self.slash_of(&queued.user_text) {
                Some(Err(reason)) => {
                    let line = queued.user_text.lines().next().unwrap_or_default();
                    let text = format!("Not sent ({reason}): {}", cap_chars(line, LINE_CHARS));
                    self.notice(NoticeKind::Info, &text, step);
                }
                slash => return Some((queued, slash.and_then(Result::ok))),
            }
        }
        None
    }

    /// The closing message, sanitised and bounded like every paste. One that cannot be sent is
    /// never pasted: the final write turns into a teardown, and the log and the chat say why.
    fn closing_delivery(&mut self, text: String, step: &mut Step) -> Option<Delivering> {
        let text = sanitize(&text);
        if let Err(refusal) = check_delivered(&text) {
            tracing::warn!(session = %self.id, %refusal, "the closing message cannot be sent");
            let why = format!("The closing message cannot be sent: {refusal}");
            self.notice(NoticeKind::Info, &why, step);
            if let Some(plan) = &mut self.flow.end {
                (plan.mode, plan.closing) = (EndMode::Teardown, Closing::Skip);
            }
            return None;
        }
        self.mapper.expect_closing(text.clone());
        if let Some(plan) = &mut self.flow.end {
            plan.closing = Closing::Sent;
        }
        let mut delivering = self.delivery(text, Vec::new(), Some(Origin::Closing), step);
        delivering.closing = true;
        Some(delivering)
    }

    /// A `sent` user item `s-<n>` for `text` (sanitised, attachment block included).
    pub(super) fn delivery(
        &mut self,
        text: String,
        attachments: Vec<Attachment>,
        origin: Option<Origin>,
        step: &mut Step,
    ) -> Delivering {
        // The server's own `format_message` output, never wrapped: a typed wrapper stays.
        let (user_text, _) = parse_unwrapped(&text);
        let item_id = self.sent_item(user_text, attachments, origin, step);
        self.delivering(vec![item_id], text)
    }

    /// Adds the `sent` user item `s-<n>` showing `text` and returns its id.
    pub(super) fn sent_item(
        &mut self,
        text: String,
        attachments: Vec<Attachment>,
        origin: Option<Origin>,
        step: &mut Step,
    ) -> String {
        let id = self.next_id("s-");
        let item = Item::User {
            id: id.clone(),
            text,
            attachments,
            delivery: Delivery::Sent,
            origin,
            command: None,
        };
        let deltas = self.mapper.add_server_item(item);
        self.emit(deltas, step);
        id
    }

    /// The delivery of the pasted `text`, whose sent items are `item_ids`.
    pub(super) fn delivering(&self, item_ids: Vec<String>, text: String) -> Delivering {
        let match_text = match_key(&parse_unwrapped(&text).0);
        Delivering {
            item_ids,
            text,
            match_text,
            sent_at: None,
            closing: false,
            opening: None,
            slash: None,
            local: None,
        }
    }

    /// A failed paste turns the delivery undelivered and pauses the queue; a failed opening
    /// returns the failure the session ends with, since no turn ever ran to close it.
    fn pasted(&mut self, pasted: Result<(), String>, step: &mut Step) -> Option<String> {
        match pasted {
            Ok(()) => {
                if self
                    .flow
                    .delivering
                    .as_ref()
                    .is_some_and(Delivering::is_compact)
                {
                    self.machine.apply(Input::CompactDelivered);
                }
                None
            }
            Err(error) => {
                tracing::warn!(session = %self.id, %error, "cannot paste into the session");
                let failed = self.flow.delivering.take()?;
                self.set_delivery(&failed.item_ids, Delivery::Undelivered, step);
                self.flow.paused = true;
                let why = format!("the opening message cannot be sent: {error}");
                failed.opening.is_some().then_some(why)
            }
        }
    }

    /// No `UserPromptSubmit` within the timeout: undelivered, and the queue waits. A Summary
    /// session's opening fails the session instead (`Bridge::opening_step`).
    pub(super) fn time_out_delivery(
        &mut self,
        config: &BridgeConfig,
        now: Instant,
        step: &mut Step,
    ) {
        let overdue = self
            .waited(now)
            .is_some_and(|waited| waited >= config.delivery_timeout);
        if !overdue || self.opening_fails() {
            return;
        }
        if let Some(late) = self.flow.delivering.take() {
            self.set_delivery(&late.item_ids, Delivery::Undelivered, step);
            self.flow.undelivered.push(late);
            if self.flow.undelivered.len() > UNDELIVERED_KEPT {
                self.flow.undelivered.remove(0);
            }
            self.flow.paused = true;
        }
    }

    /// A `UserPromptSubmit`: the pasted message arrived, or a late one (matched by its text).
    /// A background task's prompt confirms only a message with its text (one the user wrote
    /// starting with the same tag): Claude Code can start that turn between the bridge's idle
    /// and the paste, and the pasted message then waits behind it unsubmitted. A local command
    /// fires no prompt, so none confirms it; any prompt but its own settles one that runs.
    pub(super) fn confirm_prompt(&mut self, prompt: Option<&str>, step: &mut Step) {
        let text = prompt.map(|prompt| match_key(&parse_message(prompt).0));
        if self.own_local_prompt(text.as_deref()) {
            return;
        }
        self.settle_now();
        let background = prompt.is_some_and(background_prompt);
        let ours = |d: &Delivering| {
            let typed = !background || text.as_deref() == Some(&d.match_text);
            d.sent_at.is_some() && d.local.is_none() && typed
        };
        if self.flow.delivering.as_ref().is_some_and(ours) {
            if let Some(done) = self.flow.delivering.take() {
                self.delivered(&done.item_ids, done.closing, step);
                self.await_record(done);
            }
        } else if let Some(text) = text {
            self.late_confirm(&text, true, step);
        }
    }

    /// The transcript's user record of a delivery (the mapper already replaced its item). A
    /// slash delivery is left to its prompt and command records: a record body is no typed line.
    pub(super) fn confirm_text(&mut self, text: &str, step: &mut Step) {
        let ours =
            |d: &Delivering| d.sent_at.is_some() && d.slash.is_none() && d.match_text == text;
        if self.flow.delivering.as_ref().is_some_and(ours) {
            if let Some(done) = self.flow.delivering.take() {
                self.delivered(&done.item_ids, done.closing, step);
            }
        } else {
            self.late_confirm(text, false, step);
        }
    }

    /// A timed-out delivery confirmed after all by its text (a slash one only by its `prompt`,
    /// a local one never); a late closing message still closes.
    fn late_confirm(&mut self, text: &str, prompt: bool, step: &mut Step) {
        let waiting = |d: &Delivering| {
            let by_text = prompt || d.slash.is_none();
            d.match_text == text && d.local.is_none() && by_text
        };
        let found = self.flow.undelivered.iter().rposition(waiting);
        if let Some(at) = found {
            let late = self.flow.undelivered.remove(at);
            self.delivered(&late.item_ids, late.closing, step);
            self.await_record(late);
        }
    }

    pub(super) fn delivered(&mut self, item_ids: &[String], closing: bool, step: &mut Step) {
        self.set_delivery(item_ids, Delivery::Delivered, step);
        if let Some(plan) = self.flow.end.as_mut().filter(|_| closing) {
            plan.closing = Closing::Confirmed;
        }
    }

    pub(super) fn set_delivery(
        &mut self,
        item_ids: &[String],
        delivery: Delivery,
        step: &mut Step,
    ) {
        for item_id in item_ids {
            self.update_user(item_id, step, |shown, _| *shown = delivery);
        }
    }

    /// Change the delivery or the command of user item `item_id` and publish it.
    pub(super) fn update_user(
        &mut self,
        item_id: &str,
        step: &mut Step,
        change: impl FnOnce(&mut Delivery, &mut Option<CommandRef>),
    ) {
        let found = self
            .mapper
            .turns()
            .iter()
            .flat_map(|turn| &turn.items)
            .find(|item| matches!(item, Item::User { id, .. } if id == item_id))
            .cloned();
        if let Some(mut item) = found {
            if let Item::User {
                delivery, command, ..
            } = &mut item
            {
                change(delivery, command);
            }
            let deltas = self.mapper.replace_server_item(item);
            self.emit(deltas, step);
        }
    }

    /// The queue and a message not yet pasted go back to the caller (px-state-07).
    pub(super) fn return_unsent(&mut self, step: &mut Step) -> Vec<String> {
        let mut texts = self.take_queue(step);
        let unsent = self.flow.delivering.as_ref();
        if unsent.is_some_and(|d| d.sent_at.is_none() && !d.closing) {
            if let Some(unsent) = self.flow.delivering.take() {
                self.set_delivery(&unsent.item_ids, Delivery::Undelivered, step);
                texts.insert(0, parse_unwrapped(&unsent.text).0);
            }
        }
        texts
    }

    fn take_queue(&mut self, step: &mut Step) -> Vec<String> {
        if self.flow.queue.is_empty() {
            return Vec::new();
        }
        let texts: Vec<String> = self.flow.queue.drain(..).map(|q| q.user_text).collect();
        self.publish_queue(step);
        texts
    }
}
