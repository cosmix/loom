//! Permission requests: the card each request shows, its answer file, the binding to the tool
//! call it names, its expiry, and the cards closed when the session stops waiting on them.
//!
//! The state machine's pending set drives the state; the bridge keeps one `Request` per card
//! and closes every open card the machine no longer holds (Stop, interrupt record, the bound
//! tool's result) as denied `session_ended`, with a deny answer file (px-state-05, -R03).

use std::collections::HashSet;

use chrono::Utc;
use serde_json::Value;

use super::commands::{Effect, Step};
use super::session::{later, stamp, Request, Session};
use super::BridgeConfig;
use crate::plan_session::model::{
    DenyReason, Item, PermissionStatus, SessionState, ToolStatus, Turn,
};
use crate::plan_session::permission::{Answer, Behavior};
use crate::plan_session::spool::SpoolEvent;
use crate::plan_session::state::Input;

/// The note and message of a card the session no longer waits on.
pub(super) const GONE: &str = "the session no longer waits for this answer";
const EXPIRED: &str = "no answer arrived in time";
/// A card shows at most this much of the tool input, as the mapper's tool rows do.
const INPUT_CHARS: usize = 64 * 1024;

impl Session {
    /// After every step: resolve requests by their tool's result, close the cards the machine
    /// no longer waits on, publish the state.
    pub(super) fn settle(&mut self, step: &mut Step) {
        if self.flow.requests.values().any(|request| request.open) {
            self.bind_requests();
            let resolved: Vec<String> = self
                .flow
                .requests
                .iter()
                .filter(|(_, request)| request.open)
                .filter(|(_, request)| {
                    let tool_use = request.tool_use.as_deref();
                    tool_use.is_some_and(|tool_use| !self.tool_pending(tool_use))
                })
                .map(|(id, _)| id.clone())
                .collect();
            for request_id in resolved {
                self.machine.apply(Input::RequestResolved { request_id });
            }
            if !matches!(self.state(), SessionState::Ended | SessionState::Failed) {
                let pending = self.machine.pending_requests();
                let gone: Vec<String> = self
                    .flow
                    .requests
                    .iter()
                    .filter(|(id, request)| request.open && !pending.contains(*id))
                    .map(|(id, _)| id.clone())
                    .collect();
                for request_id in gone {
                    self.deny(&request_id, DenyReason::SessionEnded, GONE, step);
                }
            }
        }
        self.sync(step);
    }

    /// Bind each unbound request to the earliest pending tool call with its tool name and an
    /// equal input that no other request holds.
    fn bind_requests(&mut self) {
        let mut bound: HashSet<String> = self
            .flow
            .requests
            .values()
            .filter_map(|request| request.tool_use.clone())
            .collect();
        let unbound: Vec<String> = self
            .flow
            .requests
            .iter()
            .filter(|(_, request)| request.open && request.tool_use.is_none())
            .map(|(id, _)| id.clone())
            .collect();
        for request_id in unbound {
            let Some(request) = self.flow.requests.get(&request_id) else {
                continue;
            };
            let turns = self.mapper.turns();
            let Some(tool_use) = pending_tool(turns, &request.tool, &request.input, &bound) else {
                continue;
            };
            bound.insert(tool_use.clone());
            if let Some(request) = self.flow.requests.get_mut(&request_id) {
                request.tool_use = Some(tool_use);
            }
        }
    }

    fn tool_pending(&self, tool_use: &str) -> bool {
        let mut items = self.mapper.turns().iter().flat_map(|turn| &turn.items);
        items.any(|item| {
            matches!(item, Item::Tool { tool_use_id, status: ToolStatus::Pending, .. }
                if tool_use_id == tool_use)
        })
    }

    /// A `PermissionRequest` the machine took: a pending card, then any answer due on arrival.
    pub(super) fn on_request(
        &mut self,
        event: &SpoolEvent,
        config: &BridgeConfig,
        step: &mut Step,
    ) {
        if event
            .payload
            .get("auto_denied")
            .is_some_and(|tag| !tag.is_null())
        {
            return;
        }
        let named = event
            .request_id
            .as_deref()
            .or_else(|| event.payload.get("request_id").and_then(Value::as_str));
        let Some(request_id) = named.filter(|id| canonical(id)).map(str::to_owned) else {
            tracing::warn!(session = %self.id, "a permission request without a usable id");
            return;
        };
        if self.flow.requests.contains_key(&request_id) {
            return;
        }
        let tool = event.payload.get("tool_name").and_then(Value::as_str);
        let input = event
            .payload
            .get("tool_input")
            .cloned()
            .unwrap_or(Value::Null);
        let request = Request {
            tool: tool.unwrap_or("unknown").to_owned(),
            input,
            expires_at: later(event.received_at, config.permission_deny_after),
            tool_use: None,
            open: true,
        };
        let card = card(&request_id, &request, PermissionStatus::Pending, None, None);
        let deltas = self.mapper.add_server_item(card);
        self.emit(deltas, step);
        self.flow.requests.insert(request_id.clone(), request);
        self.answer_on_arrival(&request_id, step);
    }

    /// A recovered session shows the answer its request had at the restart; an ending session
    /// that denies new requests denies this one at once (px-state-06, -12).
    fn answer_on_arrival(&mut self, request_id: &str, step: &mut Step) {
        let recovered = self.flow.recovered.as_ref();
        if let Some(answer) = recovered
            .and_then(|answers| answers.get(request_id))
            .cloned()
        {
            self.close_card(request_id, &answer, step);
            let request_id = request_id.to_owned();
            self.machine.apply(Input::PermissionAnswered { request_id });
        } else if let Some((reason, message)) = self.flow.end.as_ref().and_then(|e| e.deny_new) {
            self.deny(request_id, reason, message, step);
        }
    }

    /// Close the card with `answer`, leave the answer file for after the lock, tell the
    /// machine (an expiry is reported as one).
    pub(super) fn answer(
        &mut self,
        request_id: &str,
        answer: Answer,
        expired: bool,
        step: &mut Step,
    ) {
        self.close_card(request_id, &answer, step);
        let dir = self.dir.path().to_path_buf();
        let request = request_id.to_owned();
        step.effects.push(Effect::Answer {
            dir,
            request_id: request.clone(),
            answer,
        });
        let input = match expired {
            true => Input::PermissionExpired {
                request_id: request,
            },
            false => Input::PermissionAnswered {
                request_id: request,
            },
        };
        self.machine.apply(input);
    }

    pub(super) fn deny(
        &mut self,
        request_id: &str,
        reason: DenyReason,
        message: &str,
        step: &mut Step,
    ) {
        let answer = deny_answer(reason, message);
        self.answer(request_id, answer, false, step);
    }

    /// Deny every open request.
    pub(super) fn deny_open(&mut self, reason: DenyReason, message: &str, step: &mut Step) {
        let open = self
            .flow
            .requests
            .iter()
            .filter(|(_, request)| request.open);
        let open: Vec<String> = open.map(|(id, _)| id.clone()).collect();
        for request_id in open {
            self.deny(&request_id, reason, message, step);
        }
    }

    /// The bridge's own wait ends `permission_deny_after` after the request, whatever the
    /// machine's set holds.
    pub(super) fn expire_requests(&mut self, step: &mut Step) {
        let now = Utc::now();
        let due = self.flow.requests.iter();
        let due = due.filter(|(_, request)| request.open && request.expires_at <= now);
        let due: Vec<String> = due.map(|(id, _)| id.clone()).collect();
        for request_id in due {
            let answer = deny_answer(DenyReason::Timeout, EXPIRED);
            self.answer(&request_id, answer, true, step);
        }
    }

    fn close_card(&mut self, request_id: &str, answer: &Answer, step: &mut Step) {
        let Some(request) = self.flow.requests.get_mut(request_id) else {
            return;
        };
        request.open = false;
        let (status, reason) = match answer.behavior {
            Behavior::Allow => (PermissionStatus::Allowed, None),
            Behavior::Deny => (PermissionStatus::Denied, answer.reason),
        };
        let note = answer.message.clone();
        let card = card(request_id, request, status, reason, note);
        let deltas = self.mapper.replace_server_item(card);
        self.emit(deltas, step);
    }
}

fn deny_answer(reason: DenyReason, message: &str) -> Answer {
    Answer {
        behavior: Behavior::Deny,
        message: Some(message.to_owned()),
        reason: Some(reason),
    }
}

fn card(
    request_id: &str,
    request: &Request,
    status: PermissionStatus,
    deny_reason: Option<DenyReason>,
    note: Option<String>,
) -> Item {
    let pretty = serde_json::to_string_pretty(&request.input).unwrap_or_default();
    Item::Permission {
        id: format!("p-{request_id}"),
        request_id: request_id.to_owned(),
        tool: request.tool.clone(),
        input: pretty.chars().take(INPUT_CHARS).collect(),
        status,
        deny_reason,
        note,
        expires_at: stamp(request.expires_at),
    }
}

/// The earliest pending tool call named `name` with an input equal to `input`, not bound.
fn pending_tool(
    turns: &[Turn],
    name: &str,
    input: &Value,
    bound: &HashSet<String>,
) -> Option<String> {
    let mut items = turns.iter().flat_map(|turn| &turn.items);
    items.find_map(|item| match item {
        Item::Tool {
            tool_use_id,
            name: tool,
            status: ToolStatus::Pending,
            input: shown,
            ..
        } if tool == name && !bound.contains(tool_use_id) => {
            let same = serde_json::from_str::<Value>(shown).is_ok_and(|shown| shown == *input);
            same.then(|| tool_use_id.clone())
        }
        _ => None,
    })
}

/// A lowercase hyphenated uuid: the id becomes a file name.
pub(super) fn canonical(id: &str) -> bool {
    uuid::Uuid::try_parse(id).is_ok_and(|uuid| uuid.hyphenated().to_string() == id)
}
