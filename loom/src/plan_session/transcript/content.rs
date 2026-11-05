//! Assistant blocks, tool results and interrupts: the records that build and finish tool rows.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::records::{self, cap_chars, Assistant, Block, ToolResult};
use super::tools::repo_relative;
use super::{Ingest, Mapper, RecordMark, Subagent};
use crate::plan_session::model::{ChatDelta, Item, NoticeKind, ToolStatus};

impl Mapper {
    /// One assistant record: a main record shows its text, thinking and tool rows; a subagent
    /// record shows only its tool rows, nested under the spawning tool call.
    pub(super) fn assistant(
        &mut self,
        uuid: &str,
        at: Option<DateTime<Utc>>,
        message: Assistant,
        sub: Option<Subagent<'_>>,
        deltas: &mut Vec<ChatDelta>,
    ) {
        let spawner = sub
            .and_then(|sub| sub.spawned_by)
            .and_then(|id| self.tools.get(id).cloned());
        let known = message
            .message_id
            .as_ref()
            .and_then(|id| self.messages.get(id).copied());
        let turn = match spawner.as_ref().map(|(turn, _)| *turn).or(known) {
            Some(turn) => turn,
            None => self.turn_for(at, deltas),
        };
        let mut nested = 0u32;
        for (index, block) in message.blocks.into_iter().enumerate() {
            let id = format!("a-{uuid}-{index}");
            let Some(item) = self.block_item(turn, id, at, block, sub) else {
                continue;
            };
            nested = nested.saturating_add(u32::from(sub.is_some()));
            self.push_item(turn, item, deltas);
        }
        if let (Some((spawn_turn, spawn_id)), true) = (spawner, nested > 0) {
            self.update_item(spawn_turn, &spawn_id, deltas, |item| {
                if let Item::Tool { nested_tools, .. } = item {
                    *nested_tools = nested_tools.saturating_add(nested);
                }
            });
        }
        if let (Some(message_id), Some(usage)) = (message.message_id, message.usage) {
            self.messages.entry(message_id.clone()).or_insert(turn);
            self.add_usage(turn, &message_id, &usage, deltas);
        }
    }

    fn block_item(
        &mut self,
        turn: usize,
        id: String,
        at: Option<DateTime<Utc>>,
        block: Block,
        sub: Option<Subagent<'_>>,
    ) -> Option<Item> {
        match block {
            Block::Text(text) if sub.is_none() && !text.trim().is_empty() => {
                let markdown = cap_chars(&text, records::ITEM_CHARS);
                Some(Item::Text { id, markdown })
            }
            Block::Thinking if sub.is_none() => {
                if let Some(at) = at {
                    self.thinking.push((turn, id.clone(), at));
                }
                Some(Item::Thinking {
                    id,
                    duration_ms: None,
                })
            }
            Block::ToolUse {
                id: tool_use_id,
                name,
                input,
            } => {
                self.tools.insert(tool_use_id.clone(), (turn, id.clone()));
                let tool = (tool_use_id, name, &input);
                Some(records::tool_item(id, tool, &self.root, sub))
            }
            _ => None,
        }
    }

    /// Completes the tool rows a user record's `tool_result` blocks answer.
    pub(super) fn tool_results(
        &mut self,
        results: Vec<ToolResult>,
        tool_use_result: &Value,
        deltas: &mut Vec<ChatDelta>,
    ) {
        // `toolUseResult` describes the record's one result; with several it is ambiguous.
        let file = (results.len() == 1)
            .then(|| records::file_change(tool_use_result))
            .flatten();
        for outcome in results {
            let Some((turn, id)) = self.tools.get(&outcome.tool_use_id).cloned() else {
                continue;
            };
            // A changed path on the wire is repo-relative under doc/: a session's drafts and
            // temporary files outside it are not reported.
            if let Some(file) = &file {
                let path = repo_relative(&self.root, &file.path);
                if let Some(path) = path.filter(|path| path.starts_with("doc/")) {
                    self.state[turn].by_tool.insert(path);
                }
            }
            self.update_item(turn, &id, deltas, |item| {
                records::complete_tool(item, &outcome, file.as_ref());
            });
        }
    }

    /// An interrupt record: a notice, every pending tool of the turn not finished, the turn
    /// closed (a later `end_turn` for it publishes nothing). When the turn's last item is the
    /// "Stopped" notice the Stop button already added, the record adds no second notice.
    pub(super) fn interrupt(&mut self, uuid: &str, at: Option<DateTime<Utc>>, out: &mut Ingest) {
        if let Some(at) = at {
            #[cfg(test)]
            {
                self.last_interrupt = self.last_interrupt.max(Some(at));
            }
            out.marks.push(RecordMark::Interrupted { at });
        }
        let Some(turn) = self.current_or_last() else {
            return;
        };
        let stopped = matches!(
            self.turns[turn].items.last(),
            Some(Item::Notice {
                kind: NoticeKind::Stopped,
                ..
            })
        );
        if !stopped {
            let notice = Item::Notice {
                id: format!("n-{uuid}"),
                kind: NoticeKind::Interrupted,
                text: "Interrupted".into(),
            };
            self.push_item(turn, notice, &mut out.deltas);
        }
        let turn_id = self.turns[turn].id.clone();
        for item in &mut self.turns[turn].items {
            if let Item::Tool {
                status: status @ ToolStatus::Pending,
                ..
            } = item
            {
                *status = ToolStatus::NotFinished;
                let item = item.clone();
                let turn_id = turn_id.clone();
                out.deltas.push(ChatDelta::ItemReplaced { turn_id, item });
            }
        }
        if self.open == Some(turn) {
            self.close(turn, &mut out.deltas);
        }
    }
}
