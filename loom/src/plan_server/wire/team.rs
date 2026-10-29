//! Wire types for draft-all teammate progress.

use serde::{Deserialize, Serialize};

use crate::plan_session::bridge::team::TeammateStatus;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftTeamView {
    pub session: String,
    /// The run's missing documents, repository-relative.
    pub documents: Vec<String>,
    pub teammates: Vec<TeammateStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_session::bridge::team::TeammateState;
    use crate::plan_session::model::TokenCounts;
    use serde_json::json;

    #[test]
    fn draft_team_round_trips_teammate_progress() {
        let view = DraftTeamView {
            session: "team-1".into(),
            documents: vec!["doc/product/demo/section_01.md".into()],
            teammates: vec![TeammateStatus {
                agent_id: "agent-1".into(),
                name: "writer".into(),
                documents: vec!["doc/product/demo/section_01.md".into()],
                current: Some("doc/product/demo/section_01.md".into()),
                state: TeammateState::Working,
                tokens: TokenCounts {
                    input: 10,
                    cache_write: 20,
                    cache_read: 30,
                    output: 40,
                },
            }],
        };
        let expected = json!({
            "session": "team-1",
            "documents": ["doc/product/demo/section_01.md"],
            "teammates": [{
                "agent_id": "agent-1",
                "name": "writer",
                "documents": ["doc/product/demo/section_01.md"],
                "current": "doc/product/demo/section_01.md",
                "state": "working",
                "tokens": {"input": 10, "cache_write": 20, "cache_read": 30, "output": 40}
            }]
        });

        assert_eq!(serde_json::to_value(&view).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<DraftTeamView>(expected).unwrap(),
            view
        );
    }

    #[test]
    fn unknown_draft_team_keys_are_rejected() {
        assert!(serde_json::from_value::<DraftTeamView>(json!({
            "session": "team-1", "documents": [], "teammates": [], "extra": true
        }))
        .is_err());
    }
}
