//! Turn bookkeeping of the mapper: opening, closing, item placement and the deltas they publish.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use super::records::item_id;
use super::{Mapper, TurnState};
use crate::plan_session::model::{ChatDelta, Item, NoticeKind, Turn};

impl Mapper {
    /// The open turn, or a turn opened implicitly at `at` (now when the record has no time).
    pub(super) fn turn_for(
        &mut self,
        at: Option<DateTime<Utc>>,
        deltas: &mut Vec<ChatDelta>,
    ) -> usize {
        match self.open {
            Some(turn) => turn,
            None => self.open_new(at.unwrap_or_else(Utc::now), true, deltas),
        }
    }

    /// The open turn, else the last one; `None` before any turn.
    pub(super) fn current_or_last(&self) -> Option<usize> {
        self.open.or(self.turns.len().checked_sub(1))
    }

    pub(super) fn open_new(
        &mut self,
        at: DateTime<Utc>,
        implicit: bool,
        deltas: &mut Vec<ChatDelta>,
    ) -> usize {
        let turn = Turn {
            id: format!("t{}", self.turns.len() + 1),
            started_at: at.to_rfc3339_opts(SecondsFormat::Millis, true),
            duration_ms: None,
            open: true,
            tokens: Default::default(),
            items: Vec::new(),
            changed_files: Vec::new(),
            summaries_rebuilding: Vec::new(),
        };
        let added = turn.clone();
        deltas.push(ChatDelta::TurnAdded { turn: added });
        self.turns.push(turn);
        self.state.push(TurnState {
            implicit,
            ..TurnState::default()
        });
        let index = self.turns.len() - 1;
        self.open = Some(index);
        index
    }

    pub(super) fn close(&mut self, turn: usize, deltas: &mut Vec<ChatDelta>) {
        self.turns[turn].open = false;
        self.state[turn].implicit = false;
        if self.open == Some(turn) {
            self.open = None;
        }
        deltas.push(self.turn_updated(turn));
    }

    pub(super) fn turn_updated(&self, turn: usize) -> ChatDelta {
        let turn = &self.turns[turn];
        ChatDelta::TurnUpdated {
            turn_id: turn.id.clone(),
            duration_ms: turn.duration_ms,
            open: turn.open,
            tokens: turn.tokens.clone(),
            changed_files: turn.changed_files.clone(),
            summaries_rebuilding: turn.summaries_rebuilding.clone(),
        }
    }

    pub(super) fn turn_duration(&mut self, ms: u64, deltas: &mut Vec<ChatDelta>) {
        if let Some(turn) = self.current_or_last() {
            self.turns[turn].duration_ms = Some(ms);
            deltas.push(self.turn_updated(turn));
        }
    }

    pub(super) fn add_usage(
        &mut self,
        turn: usize,
        message_id: &str,
        usage: &Value,
        deltas: &mut Vec<ChatDelta>,
    ) {
        self.state[turn].usage.record(message_id, usage);
        let total = self.state[turn].usage.total();
        if total != self.turns[turn].tokens {
            self.turns[turn].tokens = total;
            deltas.push(self.turn_updated(turn));
        }
    }

    pub(super) fn notice(
        &mut self,
        uuid: &str,
        at: Option<DateTime<Utc>>,
        kind: NoticeKind,
        text: String,
        deltas: &mut Vec<ChatDelta>,
    ) {
        let turn = self.turn_for(at, deltas);
        let id = format!("n-{uuid}");
        self.push_item(turn, Item::Notice { id, kind, text }, deltas);
    }

    pub(super) fn push_item(&mut self, turn: usize, item: Item, deltas: &mut Vec<ChatDelta>) {
        self.turns[turn].items.push(item.clone());
        let turn_id = self.turns[turn].id.clone();
        deltas.push(ChatDelta::ItemAdded { turn_id, item });
    }

    /// Applies `change` to item `id` of the turn and publishes it when it changed.
    pub(super) fn update_item(
        &mut self,
        turn: usize,
        id: &str,
        deltas: &mut Vec<ChatDelta>,
        change: impl FnOnce(&mut Item),
    ) {
        let entry = &mut self.turns[turn];
        let Some(item) = entry.items.iter_mut().find(|item| item_id(item) == id) else {
            return;
        };
        let before = item.clone();
        change(item);
        if *item != before {
            let item = item.clone();
            let turn_id = entry.id.clone();
            deltas.push(ChatDelta::ItemReplaced { turn_id, item });
        }
    }

    /// The turn and position of the item with this id, latest turn first.
    pub(super) fn find(&self, id: &str) -> Option<(usize, usize)> {
        self.turns
            .iter()
            .enumerate()
            .rev()
            .find_map(|(turn, entry)| {
                let at = entry.items.iter().position(|item| item_id(item) == id)?;
                Some((turn, at))
            })
    }

    /// Fills every thinking item waiting for the next main record's timestamp.
    pub(super) fn fill_thinking(&mut self, at: DateTime<Utc>, deltas: &mut Vec<ChatDelta>) {
        for (turn, id, started) in std::mem::take(&mut self.thinking) {
            let ms = u64::try_from((at - started).num_milliseconds()).unwrap_or(0);
            self.update_item(turn, &id, deltas, |item| {
                if let Item::Thinking { duration_ms, .. } = item {
                    *duration_ms = Some(ms);
                }
            });
        }
    }
}
