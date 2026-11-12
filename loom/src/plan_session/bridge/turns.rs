//! The turns a prompt opens and a Stop closes: their pins, `TurnStarted` and `TurnEnded`, and
//! the files each turn changed.

use chrono::{DateTime, Utc};

use super::commands::{Effect, Step};
use super::session::Session;
use super::BridgeEvent;
use crate::plan_session::model::{ChangedFile, ChatDelta};

impl Session {
    /// Open (or adopt) a turn, pin it, announce it.
    pub(super) fn begin_turn(&mut self, at: DateTime<Utc>, step: &mut Step) {
        if self.flow.turn.is_some() {
            self.end_turn(step);
        }
        let (turn, deltas) = self.mapper.begin_turn(at);
        self.emit(deltas, step);
        self.flow.pressed = false;
        self.flow.closing_stopped = false;
        step.effects.push(Effect::BeginPins { turn: turn.clone() });
        let (id, turn_id) = (self.id.clone(), turn.clone());
        step.event(BridgeEvent::TurnStarted { id, turn_id });
        self.flow.turn = Some(turn);
    }

    /// Close the mapper's open turn; a started turn also gets its pins ended and, after them,
    /// `finish_turn`. A turn already ended is not ended again (px-state-03).
    pub(super) fn end_turn(&mut self, step: &mut Step) {
        let deltas = self.mapper.end_turn();
        self.emit(deltas, step);
        if let Some(turn) = self.flow.turn.take() {
            step.effects.push(Effect::EndPins { turn });
        }
        self.flow.pressed = false;
    }

    /// Merge the pins' changes with the tool-reported paths; publish them and `TurnEnded`.
    pub(super) fn finish_turn(
        &mut self,
        turn: &str,
        pinned: Vec<(String, Option<String>)>,
        step: &mut Step,
    ) {
        let by_tool = self.mapper.changed_by_tool(turn);
        let mut changed: Vec<ChangedFile> = pinned
            .into_iter()
            .map(|(path, before_hash)| ChangedFile {
                by_tool: by_tool.contains(&path),
                path,
                before_hash,
            })
            .collect();
        for path in by_tool {
            if !changed.iter().any(|file| file.path == path) {
                let (before_hash, by_tool) = (None, true);
                changed.push(ChangedFile {
                    path,
                    before_hash,
                    by_tool,
                });
            }
        }
        changed.sort_by(|a, b| a.path.cmp(&b.path));
        self.flow.changed.insert(turn.to_owned(), changed.clone());
        if let Some(found) = self.mapper.turns().iter().find(|found| found.id == turn) {
            let delta = ChatDelta::TurnUpdated {
                turn_id: found.id.clone(),
                duration_ms: found.duration_ms,
                open: found.open,
                tokens: found.tokens.clone(),
                changed_files: changed.clone(),
                summaries_rebuilding: found.summaries_rebuilding.clone(),
            };
            self.emit(vec![delta], step);
        }
        let (id, turn_id) = (self.id.clone(), turn.to_owned());
        step.event(BridgeEvent::TurnEnded {
            id,
            turn_id,
            changed,
        });
    }
}
