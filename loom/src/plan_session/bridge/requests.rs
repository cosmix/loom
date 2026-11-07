//! Permission requests: the card each request shows, its answer file, the binding to the tool
//! call it names, its expiry, and the cards closed when the session stops waiting on them.
//!
//! The state machine's pending set drives the state; the bridge keeps one `Request` per card
//! and closes every open card the machine no longer holds (Stop or StopFailure, interrupt
//! record, the bound tool's result) as denied `session_ended`, with a deny answer file
//! (px-state-05, -R03). A `WebFetch` request a standing rule answers gets a closed card at once
//! (`fetches.rs`).

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
use crate::plan_session::spool::{SpoolEvent, TRUNCATED};
use crate::plan_session::state::Input;
use crate::plan_session::web_fetch::{fetch_view, FETCH_TOOL};

/// The note and message of a card the session no longer waits on.
pub(super) const GONE: &str = "the session no longer waits for this answer";
const EXPIRED: &str = "no answer arrived in time";
/// A card shows at most this much of the tool input, as the mapper's tool rows do.
const INPUT_CHARS: usize = 64 * 1024;
/// The line a card's input ends with when the card cannot show the whole input.
const CUT_LINE: &str = "\n…[cut: the request is larger than the chat shows]";
/// Why a request the card cannot show whole is denied on arrival (section 12: the text the
/// holder approves is the text that runs).
const TOO_LARGE: &str =
    "the request is too large to show in the chat in full; ask for it in smaller steps";

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
        if event.auto_denied() {
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
            fetch: (tool == Some(FETCH_TOOL)).then(|| fetch_view(&input)),
            input,
            expires_at: later(event.received_at, config.permission_deny_after),
            tool_use: None,
            open: true,
        };
        if let Some(rule) = self.arrival_rule(&request_id, &request, config) {
            self.answer_by_rule_on_arrival(request_id, request, rule, step);
            return;
        }
        let card = card(&request_id, &request, PermissionStatus::Pending, None, None);
        let deltas = self.mapper.add_server_item(card);
        self.emit(deltas, step);
        self.flow.requests.insert(request_id.clone(), request);
        self.answer_on_arrival(&request_id, step);
    }

    /// A recovered session shows the answer its request had at the restart; a request the
    /// card cannot show whole is denied before anyone can allow it, since Allow would approve
    /// the uncut call; an ending session that denies new requests denies this one at once
    /// (px-state-06, -12).
    fn answer_on_arrival(&mut self, request_id: &str, step: &mut Step) {
        let recovered = self.flow.recovered.as_ref();
        let cut = self
            .flow
            .requests
            .get(request_id)
            .is_some_and(|request| shown_input(&request.input).1);
        if let Some(answer) = recovered
            .and_then(|answers| answers.get(request_id))
            .cloned()
        {
            self.close_card(request_id, &answer, step);
            let request_id = request_id.to_owned();
            self.machine.apply(Input::PermissionAnswered { request_id });
        } else if cut {
            self.deny(request_id, DenyReason::HookError, TOO_LARGE, step);
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
        self.reply(request_id, answer, expired, step);
    }

    /// Leave the answer file for after the lock and tell the machine; the card is closed.
    pub(super) fn reply(
        &mut self,
        request_id: &str,
        answer: Answer,
        expired: bool,
        step: &mut Step,
    ) {
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
        let (status, reason) = shown_status(answer);
        let note = answer.message.clone();
        let card = card(request_id, request, status, reason, note);
        let deltas = self.mapper.replace_server_item(card);
        self.emit(deltas, step);
    }
}

/// The status and deny reason a card shows for `answer`.
pub(super) fn shown_status(answer: &Answer) -> (PermissionStatus, Option<DenyReason>) {
    match answer.behavior {
        Behavior::Allow => (PermissionStatus::Allowed, None),
        Behavior::Deny => (PermissionStatus::Denied, answer.reason),
    }
}

pub(super) fn deny_answer(reason: DenyReason, message: &str) -> Answer {
    Answer {
        behavior: Behavior::Deny,
        message: Some(message.to_owned()),
        reason: Some(reason),
    }
}

pub(super) fn card(
    request_id: &str,
    request: &Request,
    status: PermissionStatus,
    deny_reason: Option<DenyReason>,
    note: Option<String>,
) -> Item {
    Item::Permission {
        id: format!("p-{request_id}"),
        request_id: request_id.to_owned(),
        tool: request.tool.clone(),
        input: shown_input(&request.input).0,
        status,
        deny_reason,
        note,
        expires_at: stamp(request.expires_at),
        fetch: request.fetch.clone(),
    }
}

/// The input text a card shows, and whether it is cut: the pretty input is longer than
/// `INPUT_CHARS` characters, or the spool already cut one of its strings. A cut input shows its
/// first `INPUT_CHARS` characters and `CUT_LINE`.
pub(super) fn shown_input(input: &Value) -> (String, bool) {
    let pretty = serde_json::to_string_pretty(input).unwrap_or_default();
    if pretty.chars().nth(INPUT_CHARS).is_none() && !spool_cut(input) {
        return (pretty, false);
    }
    let mut shown: String = pretty.chars().take(INPUT_CHARS).collect();
    shown.push_str(CUT_LINE);
    (shown, true)
}

/// Whether any string in `value` ends with the spool's cut marker. An honest string that
/// happens to end with it only makes its request denied, the safe direction.
fn spool_cut(value: &Value) -> bool {
    match value {
        Value::String(text) => text.ends_with(TRUNCATED),
        Value::Array(values) => values.iter().any(spool_cut),
        Value::Object(fields) => fields.values().any(spool_cut),
        _ => false,
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
