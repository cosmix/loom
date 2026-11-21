//! Turn lifecycle, cancellation, and deadline transitions.

use serde_json::Value;

use super::{
    wire, Action, Closed, Machine, NativeEvent, Outcome, Pending, RpcError, State,
    STARTUP_DEADLINE_SECS,
};

impl Machine {
    pub(super) fn turn_request(&mut self, thread_id: &str, texts: &[String]) -> Action {
        let config = self.config.clone();
        self.request(Pending::TurnStart, |id| {
            wire::turn_start(id, thread_id, texts, &config.effort, &config.cwd)
        })
    }

    pub(super) fn on_turn_start(&mut self, result: Result<Value, RpcError>) -> Vec<Action> {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                return self.end(
                    Outcome::Failed,
                    format!("turn/start failed: {}", error.message),
                );
            }
        };
        match wire::turn_id_from_result(&value) {
            Some(turn_id) => self.learn_turn(&turn_id),
            None => self.end(Outcome::Failed, "turn/start response missing turn.id"),
        }
    }

    fn learn_turn(&mut self, turn_id: &str) -> Vec<Action> {
        if self.turn_id.is_some() || self.done_turns.iter().any(|done| done == turn_id) {
            return Vec::new();
        }
        self.turn_id = Some(turn_id.to_owned());
        let mut actions = vec![Action::Persist(NativeEvent::TurnStarted {
            turn_id: turn_id.to_owned(),
        })];
        actions.extend(self.messages.delivered_for_turn(turn_id));
        if self.closed.is_some() {
            actions.extend(self.interrupt_action());
        }
        actions
    }

    pub(super) fn on_notification(&mut self, method: &str, params: &Value) -> Vec<Action> {
        if self.state != State::InTurn {
            return Vec::new();
        }
        match wire::parse_notification(method, params) {
            wire::Notification::TurnStarted { thread_id, turn_id }
                if self.thread_id.as_deref() == Some(&thread_id) =>
            {
                self.learn_turn(&turn_id)
            }
            wire::Notification::TurnCompleted {
                thread_id,
                turn_id,
                status,
                error,
            } if self.thread_id.as_deref() == Some(&thread_id) => {
                self.on_turn_completed(&turn_id, &status, error)
            }
            wire::Notification::ItemCompleted {
                thread_id,
                turn_id,
                message: Some(message),
            } if self.thread_id.as_deref() == Some(&thread_id)
                && self.turn_id.as_deref() == Some(&turn_id) =>
            {
                self.report.record(&turn_id, &message);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_turn_completed(
        &mut self,
        turn_id: &str,
        status: &str,
        error: Option<String>,
    ) -> Vec<Action> {
        if !matches!(status, "completed" | "failed" | "interrupted") {
            return Vec::new();
        }
        let mut actions = if self.turn_id.is_none() {
            self.learn_turn(turn_id)
        } else {
            Vec::new()
        };
        if self.turn_id.as_deref() != Some(turn_id) {
            return actions;
        }
        actions.push(Action::Persist(NativeEvent::TurnCompleted {
            turn_id: turn_id.to_owned(),
            status: status.to_owned(),
        }));
        self.done_turns.push(turn_id.to_owned());
        self.turn_id = None;
        if self.closed.is_some() {
            actions.extend(self.end(Outcome::Cancelled, "worker is closing"));
        } else if status == "completed" {
            self.report.mark_completed(turn_id);
            if self.messages.has_queued() {
                actions.extend(self.start_next_turn());
            } else {
                if let Some((text, complete)) = self.report.report() {
                    actions.push(Action::StoreReport { text, complete });
                }
                actions.extend(self.finish(Outcome::Succeeded, "turn completed"));
            }
        } else if status == "failed" {
            actions.extend(self.finish(
                Outcome::Failed,
                error.unwrap_or_else(|| "turn failed".into()),
            ));
        } else {
            actions.extend(self.finish(Outcome::Cancelled, "turn interrupted"));
        }
        actions
    }

    fn start_next_turn(&mut self) -> Vec<Action> {
        let Some(thread_id) = self.thread_id.clone() else {
            return self.end(Outcome::Failed, "cannot start a turn without a thread");
        };
        let (texts, mut actions) = self.messages.dispatch_queued();
        actions.push(self.turn_request(&thread_id, &texts));
        actions
    }

    fn interrupt_action(&mut self) -> Option<Action> {
        if self.interrupt_sent {
            return None;
        }
        let thread_id = self.thread_id.clone()?;
        let turn_id = self.turn_id.clone()?;
        self.interrupt_sent = true;
        Some(self.request(Pending::TurnInterrupt, |id| {
            wire::turn_interrupt(id, &thread_id, &turn_id)
        }))
    }

    pub(super) fn on_cancel(&mut self, reason: String) -> Vec<Action> {
        if self.closed.is_some() {
            return Vec::new();
        }
        let mut actions = vec![Action::Persist(NativeEvent::CancelRequested {
            reason: reason.clone(),
        })];
        self.closed = Some(Closed {
            outcome: Outcome::Cancelled,
            reason,
        });
        match self.state {
            State::New => actions.extend(self.end(Outcome::Cancelled, "worker is closing")),
            State::InTurn => actions.extend(self.interrupt_and_grace()),
            State::AwaitInit | State::AwaitThread | State::Finished => {}
        }
        actions
    }

    pub(super) fn on_deadline(&mut self) -> Vec<Action> {
        if self.closed.is_some() || self.state != State::InTurn {
            return Vec::new();
        }
        self.closed = Some(Closed {
            outcome: Outcome::TimedOut,
            reason: "deadline expired".into(),
        });
        let mut actions = vec![Action::Persist(NativeEvent::DeadlineExpired)];
        actions.extend(self.interrupt_and_grace());
        actions
    }

    /// Before the thread starts there is no turn to interrupt, so expiry takes the
    /// ordinary end path: a prior cancel keeps `Cancelled` and its reason.
    pub(super) fn on_startup_deadline(&mut self) -> Vec<Action> {
        if !matches!(self.state, State::AwaitInit | State::AwaitThread) {
            return Vec::new();
        }
        self.end(
            Outcome::TimedOut,
            format!("startup deadline ({STARTUP_DEADLINE_SECS} s) passed before the Codex thread started"),
        )
    }

    fn interrupt_and_grace(&mut self) -> Vec<Action> {
        let mut actions: Vec<Action> = self.interrupt_action().into_iter().collect();
        actions.push(Action::ArmGrace);
        actions
    }

    pub(super) fn on_grace(&mut self) -> Vec<Action> {
        if self.closed.is_some() {
            self.end(Outcome::Cancelled, "interrupt grace elapsed")
        } else {
            Vec::new()
        }
    }
}
