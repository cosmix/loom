//! Slash deliveries after their paste (B-D3, B-D4): the record that confirms each class, the
//! chip a prompt-type record sets, and a local delivery's barrier and settle.
//!
//! A record names its command as Claude Code wrote it; it is matched to a delivery by canonical
//! name (or the name as typed), then by args, falling back to name and order.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::commands::Step;
use super::driver::synthetic;
use super::session::{Delivering, LocalSettle, Session};
use crate::plan_session::commands::CommandClass;
use crate::plan_session::model::{CommandRef, Delivery, NoticeKind};
use crate::plan_session::state::{text, Input};
use crate::plan_session::transcript::CommandMark;

/// A local delivery settles this long after its last command or output record (a `/compact`
/// from its boundary on), unless newer main-turn evidence settled it first.
const COMPACT_SETTLE: Duration = Duration::from_secs(2);
/// Confirmed slash deliveries kept waiting for their command record.
const AWAITING_KEPT: usize = 8;

/// What a record does to a confirmed local delivery's settle clock.
#[derive(Clone, Copy)]
pub(super) enum Clock {
    /// Nothing: `PreCompact` confirms a `/compact` long before its boundary.
    Hold,
    /// Start it: the compact boundary or `SessionStart { compact }`.
    Boundary,
    /// Restart it; a `/compact`'s only once its boundary started it.
    Record,
}

impl Session {
    /// A slash command's own record.
    pub(super) fn on_command(&mut self, mark: CommandMark, step: &mut Step) {
        match mark {
            CommandMark::Invoked {
                name,
                args,
                prompt: true,
            } => self.prompt_record(&name, &args, step),
            CommandMark::Invoked { name, args, .. }
            | CommandMark::Output {
                run: Some((name, args)),
            } => self.local_record(&name, Some(&args), Clock::Record, step),
            CommandMark::Output { run: None } => {
                self.confirm_local(Delivering::settling, Clock::Record, step);
            }
            CommandMark::Unknown { name } => self.unknown_record(&name, step),
            CommandMark::Forked { skill } => self.forked(&skill, step),
        }
    }

    /// The prompt-type record of `/name args`: it confirms the pasted Run delivery (whose
    /// `UserPromptSubmit` mostly came first) and gives its item the chip.
    fn prompt_record(&mut self, raw: &str, args: &str, step: &mut Step) {
        let name = self.canonical(raw);
        let pasted = |d: &Delivering| {
            let run = d
                .slash
                .as_ref()
                .is_some_and(|s| s.class == CommandClass::Run);
            run && d.sent_at.is_some() && d.local.is_none() && d.runs(&name, raw)
        };
        let found = if self.flow.delivering.as_ref().is_some_and(pasted) {
            self.flow.delivering.take().map(|d| (d, true))
        } else if let Some(d) = take_command(&mut self.flow.awaiting_record, &name, raw, Some(args))
        {
            Some((d, false))
        } else {
            take_command(&mut self.flow.undelivered, &name, raw, Some(args)).map(|d| (d, true))
        };
        let Some((done, confirm)) = found else {
            return;
        };
        if confirm {
            self.delivered(&done.item_ids, done.closing, step);
        }
        if let Some(slash) = &done.slash {
            let chip = CommandRef {
                name: slash.name.clone(),
                args: slash.args.clone(),
            };
            for id in &done.item_ids {
                self.update_user(id, step, |_, command| *command = Some(chip.clone()));
            }
        }
    }

    /// `PreCompact`: the notice, and a pasted `/compact` confirmed by its manual trigger; its
    /// settle clock waits for its boundary (the compaction itself takes seconds).
    pub(super) fn on_pre_compact(&mut self, payload: &Value, step: &mut Step) {
        self.notice(NoticeKind::Info, "Compacting the context", step);
        if text(payload, "trigger") == Some("manual") {
            self.local_record("compact", None, Clock::Hold, step);
        }
    }

    /// A local command's record of `/name`: it confirms the pasted local delivery and moves its
    /// settle clock, or confirms one that timed out. A local command fires no Stop, so the late
    /// one holds the empty slot again as a settling local, whose settle releases the queue its
    /// timeout paused (`settle_due`).
    pub(super) fn local_record(
        &mut self,
        raw: &str,
        args: Option<&str>,
        clock: Clock,
        step: &mut Step,
    ) {
        let name = self.canonical(raw);
        if self.confirm_local(|d| d.runs(&name, raw), clock, step) {
            return;
        }
        let Some(mut late) = take_command(&mut self.flow.undelivered, &name, raw, args) else {
            return;
        };
        self.delivered(&late.item_ids, late.closing, step);
        if self.flow.delivering.is_none() {
            let compact = late.is_compact();
            let local = late
                .local
                .get_or_insert_with(|| LocalSettle::new(&late.item_ids));
            move_clock(local, compact, clock, Instant::now());
            self.flow.delivering = Some(late);
        }
    }

    /// Confirm the pasted local delivery `ours` accepts (once) and move its settle clock;
    /// false when the slot holds none.
    fn confirm_local(
        &mut self,
        ours: impl Fn(&Delivering) -> bool,
        clock: Clock,
        step: &mut Step,
    ) -> bool {
        let now = Instant::now();
        let slot = self.flow.delivering.as_mut();
        let Some(d) = slot.filter(|d| d.sent_at.is_some() && d.local.is_some() && ours(d)) else {
            return false;
        };
        let compact = d.is_compact();
        let Some(local) = d.local.as_mut() else {
            return false;
        };
        if move_clock(local, compact, clock, now) {
            let item_ids = d.item_ids.clone();
            self.set_delivery(&item_ids, Delivery::Delivered, step);
        }
        true
    }

    /// `Unknown command: /name`: its pasted delivery (the name left the list after the check at
    /// delivery) is confirmed as a local one.
    fn unknown_record(&mut self, raw: &str, step: &mut Step) {
        let name = self.canonical(raw);
        let slot = self.flow.delivering.as_mut();
        if let Some(d) = slot.filter(|d| d.sent_at.is_some() && d.runs(&name, raw)) {
            d.local.get_or_insert_with(|| LocalSettle::new(&d.item_ids));
        }
        self.local_record(raw, None, Clock::Record, step);
    }

    /// A fork skill's launch: its delivery, which its `UserPromptSubmit` confirmed and set busy,
    /// holds the slot again as a local one, and settles into an idle.
    fn forked(&mut self, raw: &str, step: &mut Step) {
        let name = self.canonical(raw);
        let Some(mut done) = take_command(&mut self.flow.awaiting_record, &name, raw, None) else {
            // Its launch record came before its prompt (which then sets busy), or after its
            // timeout.
            return self.unknown_record(raw, step);
        };
        if self.flow.delivering.is_some() {
            let why = "a fork skill launched while another delivery holds the slot";
            tracing::warn!(session = %self.id, skill = %name, "{why}");
            return;
        }
        let now = Instant::now();
        let mut local = LocalSettle::new(&done.item_ids);
        (local.confirmed_at, local.last_record, local.busy) = (Some(now), Some(now), true);
        done.local = Some(local);
        self.flow.delivering = Some(done);
    }

    /// The `UserPromptSubmit` of the settling local delivery itself (a fork skill whose launch
    /// record came first): it set busy, so the settle will be an idle.
    pub(super) fn own_local_prompt(&mut self, text: Option<&str>) -> bool {
        let slot = self.flow.delivering.as_mut();
        let own = slot.filter(|d| d.settling() && Some(d.match_text.as_str()) == text);
        match own.and_then(|d| d.local.as_mut()) {
            Some(local) => {
                local.busy = true;
                true
            }
            None => false,
        }
    }

    /// A slash delivery its `UserPromptSubmit` confirmed waits for its command record.
    pub(super) fn await_record(&mut self, done: Delivering) {
        if done.slash.is_some() {
            self.flow.awaiting_record.push(done);
            if self.flow.awaiting_record.len() > AWAITING_KEPT {
                self.flow.awaiting_record.remove(0);
            }
        }
    }

    /// Newer main-turn evidence (a prompt, a self-started turn, an assistant record, a Stop)
    /// settles a confirmed local delivery at once, without an idle: the settle timer never
    /// closes a turn it did not start.
    pub(super) fn settle_now(&mut self) {
        if self
            .flow
            .delivering
            .as_ref()
            .is_some_and(Delivering::settling)
        {
            self.flow.delivering = None;
        }
    }

    /// The settle timer: a local delivery whose last record is [`COMPACT_SETTLE`] old frees the
    /// slot. One whose prompt set busy goes Idle; otherwise its command's own turn closes.
    pub(super) fn settle_due(&mut self, now: Instant, step: &mut Step) {
        let local = self.flow.delivering.as_ref().and_then(|d| d.local.as_ref());
        let seen = local.and_then(|local| local.last_record);
        let due = seen.is_some_and(|seen| now.duration_since(seen) >= COMPACT_SETTLE);
        if !due {
            return;
        }
        let Some(local) = self.flow.delivering.take().and_then(|d| d.local) else {
            return;
        };
        let delivery = &local.delivery_id;
        tracing::debug!(session = %self.id, %delivery, "a local command settled");
        // The queue paused at a timeout waits for no idle: a local command fires none. Only a
        // late local (`local_record`) or a fork holds the slot while it is paused.
        self.flow.paused = false;
        if local.busy {
            let idle = synthetic("Notification", json!({"notification_type": "idle_prompt"}));
            self.machine.apply(Input::Spool(&idle));
            self.on_idle(false, step);
        } else if self.flow.turn.is_none() {
            self.end_turn(step);
        }
    }
}

/// Confirm `local` (once) and move its settle clock as `clock` says; true at its first
/// confirmation.
fn move_clock(local: &mut LocalSettle, compact: bool, clock: Clock, now: Instant) -> bool {
    let first = local.confirmed_at.is_none();
    local.confirmed_at.get_or_insert(now);
    let started = local.last_record.is_some();
    match clock {
        Clock::Hold => {}
        Clock::Record if compact && !started => {}
        Clock::Boundary | Clock::Record => local.last_record = Some(now),
    }
    first
}

/// Take the oldest delivery of `/name` from `list`: the first with these args when `args` is
/// given, else the first of the name.
fn take_command(
    list: &mut Vec<Delivering>,
    name: &str,
    raw: &str,
    args: Option<&str>,
) -> Option<Delivering> {
    let same = |d: &Delivering| {
        let given = args.zip(d.slash.as_ref());
        given.is_some_and(|(args, slash)| same_args(&slash.args, args))
    };
    let of_name = |d: &Delivering| d.runs(name, raw);
    let at = list.iter().position(|d| of_name(d) && same(d));
    let at = at.or_else(|| list.iter().position(of_name))?;
    Some(list.remove(at))
}

/// Args are compared trimmed, whitespace runs alike.
fn same_args(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}
