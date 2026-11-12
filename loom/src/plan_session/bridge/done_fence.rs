//! A main Stop may end an authoring chat once its last assistant text declares it done.

use super::session::Session;
use super::{EndCause, EndMode};
use crate::plan_session::kind::SessionKind;

impl Session {
    pub(super) fn agent_end(&mut self) {
        if self.kind != SessionKind::Authoring || self.flow.end.is_some() {
            return;
        }
        let Some(turn) = self.flow.turn.as_deref() else {
            return;
        };
        match self.mapper.turn_done(turn) {
            Ok(true) if self.flow.queue.is_empty() && self.flow.delivering.is_none() => {
                self.plan_end(EndMode::FinalWrite, EndCause::Agent, None);
            }
            Ok(_) => {}
            Err(reason) => tracing::warn!(session = %self.id, %turn, %reason,
                "ignoring invalid loom-done fence"),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;

    use crate::plan_session::transcript::Mapper;

    const FENCE: &str = "```loom-done\n{\"decisions\":[],\"questions\":[]}\n```";

    fn reply(mapper: &mut Mapper, id: &str, text: &str) {
        let record = json!({"type": "assistant", "uuid": id,
            "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}});
        mapper.ingest_main(&record.to_string());
    }

    #[test]
    fn a_later_plain_item_replaces_an_earlier_done_fence() {
        let mut mapper = Mapper::new("/project".into());
        let (turn, _) = mapper.begin_turn(Utc::now());
        reply(&mut mapper, "done", FENCE);
        assert_eq!(mapper.turn_done(&turn), Ok(true));
        reply(&mut mapper, "later", "One question remains.");
        assert_eq!(mapper.turn_done(&turn), Ok(false));
        mapper.end_turn();
        let (next, _) = mapper.begin_turn(Utc::now());
        assert_eq!(mapper.turn_done(&next), Ok(false));
    }

    #[test]
    fn display_truncation_cannot_turn_an_example_into_an_end() {
        let mut mapper = Mapper::new("/project".into());
        let (turn, _) = mapper.begin_turn(Utc::now());
        let prefix = "x".repeat(64 * 1024 - FENCE.len() - 1);
        reply(
            &mut mapper,
            "example",
            &format!("{prefix}\n{FENCE}\nStill working."),
        );
        assert_eq!(mapper.turn_done(&turn), Ok(false));
        reply(
            &mut mapper,
            "finished",
            &format!("{prefix}more text\n{FENCE}"),
        );
        assert_eq!(mapper.turn_done(&turn), Ok(true));
    }

    #[test]
    fn user_tool_and_subagent_text_do_not_declare_the_main_turn_done() {
        let mut mapper = Mapper::new("/project".into());
        let (turn, _) = mapper.begin_turn(Utc::now());
        for record in [
            json!({"type":"user", "uuid":"user", "message":{"content":FENCE}}),
            json!({"type":"user", "uuid":"result", "message":{"content":[
                {"type":"tool_result", "tool_use_id":"tool", "content":FENCE}]}}),
        ] {
            mapper.ingest_main(&record.to_string());
        }
        let sub = json!({"type":"assistant", "uuid":"sub", "message":{"content":[
            {"type":"text", "text":FENCE}]}});
        mapper.ingest_subagent("a1", None, &sub.to_string());
        assert_eq!(mapper.turn_done(&turn), Ok(false));
    }
}
