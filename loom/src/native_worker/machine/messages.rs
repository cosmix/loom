//! Control-message deduplication, queuing, and delivery tracking.

use std::collections::HashSet;

use super::super::model::{Delivery, MessageState, MAX_QUEUED_MESSAGES};
use super::{wire, Action, ControlMessage, Machine, NativeEvent, Pending, State};

#[derive(Default)]
pub(super) struct MessageBook {
    seen: HashSet<String>,
    queued: Vec<(String, String)>,
    dispatching: Vec<Dispatch>,
}

struct Dispatch {
    message_id: String,
    turn_id: Option<String>,
    awaiting: Awaiting,
}

#[derive(PartialEq, Eq)]
enum Awaiting {
    Steer,
    TurnStart,
}

impl MessageBook {
    pub(super) fn is_new(&mut self, id: &str) -> bool {
        self.seen.insert(id.to_owned())
    }

    pub(super) fn has_queued(&self) -> bool {
        !self.queued.is_empty()
    }

    pub(super) fn enqueue(&mut self, message: &ControlMessage) -> Action {
        if self.queued.len() >= MAX_QUEUED_MESSAGES {
            return message_action(
                &message.message_id,
                MessageState::Rejected,
                None,
                Some("queue full".into()),
            );
        }
        self.queued
            .push((message.message_id.clone(), message.text.clone()));
        message_action(&message.message_id, MessageState::Queued, None, None)
    }

    pub(super) fn begin_steer(&mut self, id: &str, turn_id: &str) -> Action {
        self.dispatching.push(Dispatch {
            message_id: id.to_owned(),
            turn_id: Some(turn_id.to_owned()),
            awaiting: Awaiting::Steer,
        });
        message_action(id, MessageState::Dispatching, None, None)
    }

    pub(super) fn steer_result(&mut self, id: &str, result: Result<(), String>) -> Vec<Action> {
        let Some(index) = self
            .dispatching
            .iter()
            .position(|dispatch| dispatch.message_id == id && dispatch.awaiting == Awaiting::Steer)
        else {
            return Vec::new();
        };
        let dispatch = self.dispatching.remove(index);
        vec![match result {
            Ok(()) => message_action(id, MessageState::Delivered, dispatch.turn_id, None),
            Err(detail) => message_action(id, MessageState::Rejected, None, Some(detail)),
        }]
    }

    pub(super) fn dispatch_queued(&mut self) -> (Vec<String>, Vec<Action>) {
        let mut texts = Vec::new();
        let mut actions = Vec::new();
        for (message_id, text) in self.queued.drain(..) {
            actions.push(message_action(
                &message_id,
                MessageState::Dispatching,
                None,
                None,
            ));
            self.dispatching.push(Dispatch {
                message_id,
                turn_id: None,
                awaiting: Awaiting::TurnStart,
            });
            texts.push(text);
        }
        (texts, actions)
    }

    pub(super) fn delivered_for_turn(&mut self, turn_id: &str) -> Vec<Action> {
        let mut actions = Vec::new();
        self.dispatching.retain(|dispatch| {
            if dispatch.awaiting == Awaiting::Steer {
                return true;
            }
            actions.push(message_action(
                &dispatch.message_id,
                MessageState::Delivered,
                Some(turn_id.to_owned()),
                None,
            ));
            false
        });
        actions
    }

    pub(super) fn settle(&mut self) -> Vec<Action> {
        let mut actions: Vec<Action> = self
            .dispatching
            .drain(..)
            .map(|dispatch| {
                message_action(
                    &dispatch.message_id,
                    MessageState::DeliveryUnknown,
                    None,
                    None,
                )
            })
            .collect();
        actions.extend(
            self.queued.drain(..).map(|(id, _)| {
                message_action(&id, MessageState::CancelledBeforeDelivery, None, None)
            }),
        );
        actions
    }
}

fn message_action(
    id: &str,
    state: MessageState,
    turn_id: Option<String>,
    detail: Option<String>,
) -> Action {
    Action::Persist(NativeEvent::Message {
        message_id: id.to_owned(),
        state,
        turn_id,
        detail,
    })
}

impl Machine {
    pub(super) fn on_control(&mut self, message: ControlMessage) -> Vec<Action> {
        if !self.messages.is_new(&message.message_id) {
            return Vec::new();
        }
        match message.delivery {
            Delivery::Steer => self.steer(message),
            Delivery::NextTurn if self.closed.is_some() => vec![message_action(
                &message.message_id,
                MessageState::Rejected,
                None,
                Some("worker is closing".into()),
            )],
            Delivery::NextTurn => vec![self.messages.enqueue(&message)],
        }
    }

    fn steer(&mut self, message: ControlMessage) -> Vec<Action> {
        let active = if self.state == State::InTurn && self.closed.is_none() {
            self.thread_id.clone().zip(self.turn_id.clone())
        } else {
            None
        };
        let Some((thread_id, turn_id)) = active else {
            return vec![message_action(
                &message.message_id,
                MessageState::Rejected,
                None,
                Some("no active turn".into()),
            )];
        };
        vec![
            self.messages.begin_steer(&message.message_id, &turn_id),
            self.request(
                Pending::TurnSteer {
                    message_id: message.message_id,
                },
                |id| wire::turn_steer(id, &thread_id, &turn_id, &message.text),
            ),
        ]
    }
}
