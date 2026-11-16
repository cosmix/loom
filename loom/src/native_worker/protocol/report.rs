//! Accumulate report text from completed turns.

use super::wire::AgentMessage;

#[derive(Debug)]
struct TurnItems {
    turn_id: String,
    items: Vec<AgentMessage>,
}

#[derive(Debug, Default)]
pub struct ReportAccumulator {
    turns: Vec<TurnItems>,
    completed: Option<String>,
}

impl ReportAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an item, replacing an earlier item with the same id in this turn.
    pub fn record(&mut self, turn_id: &str, message: &AgentMessage) {
        let index = match self.turns.iter().position(|turn| turn.turn_id == turn_id) {
            Some(index) => index,
            None => {
                self.turns.push(TurnItems {
                    turn_id: turn_id.to_owned(),
                    items: Vec::new(),
                });
                self.turns.len() - 1
            }
        };
        let items = &mut self.turns[index].items;
        if let Some(item) = items.iter_mut().find(|item| item.id == message.id) {
            *item = message.clone();
        } else {
            items.push(message.clone());
        }
    }

    /// Marks the latest turn that completed successfully.
    pub fn mark_completed(&mut self, turn_id: &str) {
        self.completed = Some(turn_id.to_owned());
    }

    /// Returns the report text and whether it consists of final answers.
    pub fn report(&self) -> Option<(String, bool)> {
        let turn = self
            .turns
            .iter()
            .find(|turn| Some(&turn.turn_id) == self.completed.as_ref())?;
        let final_texts: Vec<&str> = turn
            .items
            .iter()
            .filter(|item| item.phase.as_deref() == Some("final_answer"))
            .map(|item| item.text.as_str())
            .collect();
        if !final_texts.is_empty() {
            return Some((final_texts.join("\n\n"), true));
        }
        turn.items
            .iter()
            .rev()
            .find(|item| item.phase.is_none())
            .map(|item| (item.text.clone(), false))
    }
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
