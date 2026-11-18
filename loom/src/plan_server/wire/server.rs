//! Wire types for server status, sharing and command refusals.

use serde::{Deserialize, Serialize};

use crate::plan_server::ids::PublicId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerInfo {
    pub project: String,
    pub project_key: String,
    pub port: u16,
    pub remote: bool,
    pub read_only: Vec<MissingPrereq>,
    pub stopping: bool,
    pub you: You,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissingPrereq {
    pub what: PrereqKind,
    pub message: String,
    pub fix: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrereqKind {
    Tmux,
    Claude,
    ClaudeLogin,
    Sandbox,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct You {
    pub public_id: PublicId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareLink {
    pub url: String,
    pub bind: BindKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindKind {
    Loopback,
    Remote,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refused {
    pub command: String,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};

    fn assert_fixture<T: DeserializeOwned + Serialize>(fixture: &str) {
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let model: T = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(serde_json::to_value(model).unwrap(), expected);
    }

    #[test]
    fn server_info_fixture_matches_serde_output() {
        assert_fixture::<ServerInfo>(include_str!(
            "../../../../web/src/plan/api/fixtures/server-info.json"
        ));
    }

    #[test]
    fn share_fixture_matches_serde_output() {
        assert_fixture::<ShareLink>(include_str!(
            "../../../../web/src/plan/api/fixtures/share.json"
        ));
    }

    #[test]
    fn refused_fixture_matches_serde_output() {
        assert_fixture::<Refused>(include_str!(
            "../../../../web/src/plan/api/fixtures/refused.json"
        ));
    }

    #[test]
    fn enum_values_use_snake_case() {
        for (kind, value) in [
            (PrereqKind::Tmux, "tmux"),
            (PrereqKind::Claude, "claude"),
            (PrereqKind::ClaudeLogin, "claude_login"),
            (PrereqKind::Sandbox, "sandbox"),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), json!(value));
        }
        assert_eq!(
            serde_json::to_value(BindKind::Loopback).unwrap(),
            json!("loopback")
        );
        assert_eq!(
            serde_json::to_value(BindKind::Remote).unwrap(),
            json!("remote")
        );
    }

    #[test]
    fn structs_reject_unknown_fields() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../../../web/src/plan/api/fixtures/server-info.json"
        ))
        .unwrap();
        value["you"]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<ServerInfo>(value).is_err());
        assert!(serde_json::from_value::<Refused>(json!({
            "command": "save", "reason": "denied", "unexpected": true
        }))
        .is_err());
        assert!(serde_json::from_value::<ShareLink>(json!({
            "url": "http://localhost", "bind": "loopback", "unexpected": true
        }))
        .is_err());
    }
}
