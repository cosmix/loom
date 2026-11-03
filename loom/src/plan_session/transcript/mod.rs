//! The transcript mapper: transcript records to chat model items, incrementally.
//!
//! Turn placement: a record joins the turn that already holds its `message.id` (a tool result,
//! the turn of its tool call); anything else joins the open turn, and with no open turn it
//! opens one implicitly, which the next `begin_turn` adopts. A record of a new message that
//! arrives after `end_turn` therefore opens a turn: the bridge drains the transcript before it
//! applies a Stop.

mod content;
#[cfg(test)]
mod paste_contract_tests;
mod records;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_server;
mod tools;
mod turns;
mod usage;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use self::records::{cap_chars, item_id, Kind};
use self::usage::MessageUsage;
use crate::plan_session::attachments::{parse_unwrapped, unwrap_pasted};
use crate::plan_session::input::sanitize;
use crate::plan_session::model::{ChatDelta, Delivery, Item, NoticeKind, Origin, Turn};

/// What one ingested line produced.
#[derive(Debug, Default)]
pub struct Ingest {
    pub deltas: Vec<ChatDelta>,
    pub marks: Vec<RecordMark>,
}

/// What the bridge feeds the state machine, in record order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordMark {
    /// Any main assistant or tool record.
    Main {
        at: DateTime<Utc>,
    },
    Interrupted {
        at: DateTime<Utc>,
    },
    AgentsKilled {
        at: DateTime<Utc>,
    },
    /// A `<task-notification>` (or origin `task-notification`) user record.
    SelfStartedTurn {
        at: DateTime<Utc>,
    },
    CompactBoundary {
        at: DateTime<Utc>,
    },
    /// The sanitised user text, attachments excluded, for delivery matching.
    HumanUser {
        at: DateTime<Utc>,
        text: String,
    },
    /// A subagent's final assistant record (`stop_reason` `end_turn`).
    SubagentFinal {
        agent_id: String,
        at: DateTime<Utc>,
    },
}

#[derive(Clone, Copy)]
struct Subagent<'a> {
    agent_id: &'a str,
    spawned_by: Option<&'a str>,
}

#[derive(Default)]
struct TurnState {
    usage: MessageUsage,
    by_tool: BTreeSet<String>,
    /// Opened by a record or a server item, not yet adopted by `begin_turn`.
    implicit: bool,
}

/// Maps one session's transcripts into turns and the deltas that publish them.
pub struct Mapper {
    root: PathBuf,
    turns: Vec<Turn>,
    state: Vec<TurnState>,
    open: Option<usize>,
    /// Record uuids seen, main and subagent alike.
    seen: HashSet<String>,
    /// `message.id` to the turn that counts its usage.
    messages: HashMap<String, usize>,
    /// `tool_use_id` to the turn and id of its tool item.
    tools: HashMap<String, (usize, String)>,
    /// Thinking items waiting for the next main record's timestamp.
    thinking: Vec<(usize, String, DateTime<Utc>)>,
    opening: Option<Option<Origin>>,
    closing: Option<String>,
    last_main: Option<DateTime<Utc>>,
    #[cfg(test)]
    last_interrupt: Option<DateTime<Utc>>,
}

impl Mapper {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            root: project_root,
            turns: Vec::new(),
            state: Vec::new(),
            open: None,
            seen: HashSet::new(),
            messages: HashMap::new(),
            tools: HashMap::new(),
            thinking: Vec::new(),
            opening: None,
            closing: None,
            last_main: None,
            #[cfg(test)]
            last_interrupt: None,
        }
    }

    /// The origin of the next human user record.
    pub fn expect_opening(&mut self, origin: Option<Origin>) {
        self.opening = Some(origin);
    }

    /// A human record with this sanitised text is `Origin::Closing`.
    pub fn expect_closing(&mut self, text: String) {
        self.closing = Some(sanitize(&text));
    }

    /// Opens turn `t<n>` at a UserPromptSubmit, or adopts the turn a record already opened
    /// implicitly (the record arrived first); returns its id. Never opens two turns for one
    /// prompt.
    pub fn begin_turn(&mut self, at: DateTime<Utc>) -> (String, Vec<ChatDelta>) {
        let mut deltas = Vec::new();
        if let Some(turn) = self.open {
            if self.state[turn].implicit {
                self.state[turn].implicit = false;
                return (self.turns[turn].id.clone(), deltas);
            }
            self.close(turn, &mut deltas);
        }
        let turn = self.open_new(at, false, &mut deltas);
        (self.turns[turn].id.clone(), deltas)
    }

    /// Closes the open turn (Stop); nothing when no turn is open.
    pub fn end_turn(&mut self) -> Vec<ChatDelta> {
        let mut deltas = Vec::new();
        if let Some(turn) = self.open {
            self.close(turn, &mut deltas);
        }
        deltas
    }

    /// One complete line of the main transcript.
    pub fn ingest_main(&mut self, line: &str) -> Ingest {
        self.ingest(line, None)
    }

    /// One complete line of subagent `agent_id`'s transcript; `spawned_by` is the spawning
    /// tool call's `tool_use_id`.
    pub fn ingest_subagent(
        &mut self,
        agent_id: &str,
        spawned_by: Option<&str>,
        line: &str,
    ) -> Ingest {
        let agent_id = cap_chars(agent_id, records::ID_CHARS);
        let sub = Subagent {
            agent_id: &agent_id,
            spawned_by,
        };
        self.ingest(line, Some(sub))
    }

    /// Adds a record's usage to the open turn; produces no item.
    pub fn ingest_tokens_only(&mut self, line: &str) -> Vec<ChatDelta> {
        let mut deltas = Vec::new();
        if let (Some(turn), Some((message_id, usage))) = (self.open, records::usage_of(line)) {
            self.add_usage(turn, &message_id, &usage, &mut deltas);
        }
        deltas
    }

    /// Adds a server item (permission card, "sent" user item, notice) to the open turn; an id
    /// already present is replaced instead.
    pub fn add_server_item(&mut self, item: Item) -> Vec<ChatDelta> {
        if self.find(item_id(&item)).is_some() {
            return self.replace_server_item(item);
        }
        let mut deltas = Vec::new();
        let turn = self.turn_for(None, &mut deltas);
        self.push_item(turn, item, &mut deltas);
        deltas
    }

    /// Replaces the item with this id in whichever turn holds it.
    pub fn replace_server_item(&mut self, item: Item) -> Vec<ChatDelta> {
        let Some((turn, at)) = self.find(item_id(&item)) else {
            return Vec::new();
        };
        self.turns[turn].items[at] = item.clone();
        let turn_id = self.turns[turn].id.clone();
        vec![ChatDelta::ItemReplaced { turn_id, item }]
    }

    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    #[cfg(test)]
    pub(crate) fn open_turn(&self) -> Option<&str> {
        self.open.map(|turn| self.turns[turn].id.as_str())
    }

    pub fn last_main_record_at(&self) -> Option<DateTime<Utc>> {
        self.last_main
    }

    /// An interrupt record stamped after `at` was seen.
    #[cfg(test)]
    pub(crate) fn interrupted_since(&self, at: DateTime<Utc>) -> bool {
        self.last_interrupt.is_some_and(|seen| seen > at)
    }

    /// The paths under `doc/` the turn's tool results changed, repo-relative.
    pub fn changed_by_tool(&self, turn_id: &str) -> Vec<String> {
        let turn = self.turns.iter().position(|turn| turn.id == turn_id);
        turn.map(|turn| self.state[turn].by_tool.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn ingest(&mut self, line: &str, sub: Option<Subagent<'_>>) -> Ingest {
        let mut out = Ingest::default();
        let Some(record) = records::parse(line) else {
            return out;
        };
        if !self.seen.insert(record.uuid.clone()) {
            return out;
        }
        let (uuid, at) = (record.uuid.as_str(), record.at);
        match sub {
            None => {
                if let Some(at) = at {
                    self.fill_thinking(at, &mut out.deltas);
                }
                self.apply_main(uuid, at, record.kind, &mut out);
            }
            Some(sub) => self.apply_subagent(uuid, at, record.kind, sub, &mut out),
        }
        out
    }

    fn apply_main(&mut self, uuid: &str, at: Option<DateTime<Utc>>, kind: Kind, out: &mut Ingest) {
        let mark = |out: &mut Ingest, make: fn(DateTime<Utc>) -> RecordMark| {
            out.marks.extend(at.map(make));
        };
        match kind {
            Kind::Assistant(message) => {
                self.main_mark(at, out);
                self.assistant(uuid, at, message, None, &mut out.deltas);
            }
            Kind::ToolResults {
                results,
                tool_use_result,
                interrupted,
            } => {
                self.main_mark(at, out);
                self.tool_results(results, &tool_use_result, &mut out.deltas);
                if interrupted {
                    self.interrupt(uuid, at, out);
                }
            }
            Kind::Interrupt => self.interrupt(uuid, at, out),
            Kind::Human(text) => self.human(uuid, at, &text, out),
            Kind::SelfStarted => mark(out, |at| RecordMark::SelfStartedTurn { at }),
            Kind::TurnDuration(ms) => self.turn_duration(ms, &mut out.deltas),
            Kind::Compacted => {
                let text = "Context compacted".to_owned();
                self.notice(uuid, at, NoticeKind::Compacted, text, &mut out.deltas);
                mark(out, |at| RecordMark::CompactBoundary { at });
            }
            Kind::AgentsKilled => mark(out, |at| RecordMark::AgentsKilled { at }),
            Kind::ApiError(text) => {
                self.notice(uuid, at, NoticeKind::ApiError, text, &mut out.deltas);
            }
            Kind::Skipped => {}
        }
    }

    /// Subagent records show only their tool rows; text, thinking and prompts produce nothing.
    fn apply_subagent(
        &mut self,
        uuid: &str,
        at: Option<DateTime<Utc>>,
        kind: Kind,
        sub: Subagent<'_>,
        out: &mut Ingest,
    ) {
        match kind {
            Kind::Assistant(message) => {
                let end_turn = message.end_turn;
                self.assistant(uuid, at, message, Some(sub), &mut out.deltas);
                if let (true, Some(at)) = (end_turn, at) {
                    let agent_id = sub.agent_id.to_owned();
                    out.marks.push(RecordMark::SubagentFinal { agent_id, at });
                }
            }
            Kind::ToolResults {
                results,
                tool_use_result,
                ..
            } => self.tool_results(results, &tool_use_result, &mut out.deltas),
            _ => {}
        }
    }

    fn main_mark(&mut self, at: Option<DateTime<Utc>>, out: &mut Ingest) {
        if let Some(at) = at {
            self.last_main = self.last_main.max(Some(at));
            out.marks.push(RecordMark::Main { at });
        }
    }

    /// A human prompt: a delivered user item with its attachment chips, replacing the server's
    /// "sent" item of the same sanitised text.
    fn human(&mut self, uuid: &str, at: Option<DateTime<Utc>>, delivered: &str, out: &mut Ingest) {
        let delivered = cap_chars(unwrap_pasted(delivered), records::USER_TEXT_CHARS);
        let (text, attachments) = parse_unwrapped(&delivered);
        let clean = sanitize(&text);
        let origin = self.take_origin(&clean);
        let server_origin = self.take_server_item(&clean, &mut out.deltas);
        let turn = self.turn_for(at, &mut out.deltas);
        let item = Item::User {
            id: format!("u-{uuid}"),
            text,
            attachments,
            delivery: Delivery::Delivered,
            origin: origin.or(server_origin.flatten()),
        };
        self.push_item(turn, item, &mut out.deltas);
        if let Some(at) = at {
            out.marks.push(RecordMark::HumanUser { at, text: clean });
        }
    }

    fn take_origin(&mut self, clean: &str) -> Option<Origin> {
        if self.closing.as_deref() == Some(clean) {
            self.closing = None;
            return Some(Origin::Closing);
        }
        self.opening.take().flatten()
    }

    /// Removes the not yet delivered server user item `s-<n>` with this sanitised text, latest
    /// turn first; returns its origin when one matched.
    fn take_server_item(
        &mut self,
        clean: &str,
        deltas: &mut Vec<ChatDelta>,
    ) -> Option<Option<Origin>> {
        let pending = |item: &Item| {
            matches!(item, Item::User { id, text, delivery, .. }
                if id.starts_with("s-")
                    && *delivery != Delivery::Delivered
                    && sanitize(text) == clean)
        };
        let (turn, at) = self
            .turns
            .iter()
            .enumerate()
            .rev()
            .find_map(|(turn, entry)| {
                let at = entry.items.iter().position(pending)?;
                Some((turn, at))
            })?;
        let removed = self.turns[turn].items.remove(at);
        let turn_id = self.turns[turn].id.clone();
        let id = item_id(&removed).to_owned();
        deltas.push(ChatDelta::ItemRemoved { turn_id, id });
        match removed {
            Item::User { origin, .. } => Some(origin),
            _ => None,
        }
    }
}
