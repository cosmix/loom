//! Naming the summaries a chat turn rebuilt (plan 10): the ids are stored on the turn in the
//! transcript mapper and published as a `turn_updated` delta carrying `Turn.summaries_rebuilding`.

use super::Bridge;

impl Bridge {
    /// Store `ids` as the summaries `turn` (`(session, turn)`) rebuilt and publish them; `None`
    /// names the chat session's latest turn. Only the chat session's turns reach browsers; an
    /// unknown session or turn changes nothing. A chat snapshot and later updates of the same
    /// turn keep the ids.
    pub fn set_rebuilding(&self, turn: Option<(&str, &str)>, ids: Vec<String>) {
        let (session, turn) = match turn {
            Some((session, turn)) => (session.to_owned(), Some(turn)),
            None => match self.chat_session_id() {
                Some(session) => (session, None),
                None => return,
            },
        };
        self.with_session(&session, |session, step| {
            let turns = session.turns();
            let found = match turn {
                Some(turn) => turns.iter().find(|found| found.id == turn),
                None => turns.last(),
            };
            let Some(turn_id) = found.map(|found| found.id.clone()) else {
                return;
            };
            let deltas = session.mapper.set_summaries_rebuilding(&turn_id, ids);
            session.emit(deltas, step);
        });
        self.flush();
    }
}
