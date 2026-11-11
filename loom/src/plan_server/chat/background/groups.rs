//! The planner token pass's input: the chat session's transcripts.

use crate::plan_session::bridge::registry::read_agent_meta;
use crate::plan_session::bridge::Bridge;
use crate::tokens::PlannerGroup;

/// The chat session's group, or none while there is no chat or no main transcript yet.
/// A teammate's transcript is skipped (a teammate is not a background agent); an agent's type
/// comes from its meta file. Takes the bridge's lock only through the two calls below.
pub(super) fn chat_groups(bridge: &Bridge) -> Vec<PlannerGroup> {
    let Some(id) = bridge.chat_session_id() else {
        return Vec::new();
    };
    let Some(transcripts) = bridge.transcripts(&id) else {
        return Vec::new();
    };
    let Some(main) = transcripts.main else {
        return Vec::new();
    };
    let subagents = transcripts
        .subagents
        .into_iter()
        .filter_map(|(agent, path)| {
            let meta = path
                .parent()
                .and_then(|dir| read_agent_meta(dir, &agent))
                .unwrap_or_default();
            meta.teammate
                .is_none()
                .then_some((agent, path, meta.agent_type))
        })
        .collect();
    vec![PlannerGroup {
        id,
        main,
        subagents,
    }]
}
