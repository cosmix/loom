//! Presence and authoring state sent to planning clients.

use serde::{Deserialize, Serialize};

use crate::plan_server::ids::PublicId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresenceList {
    pub entries: Vec<PresenceEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresenceEntry {
    pub public_id: PublicId,
    pub name: String,
    pub holder: bool,
    pub away: bool,
    pub reading: Option<String>,
    pub tabs: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoringView {
    pub holder: Option<HolderView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HolderView {
    pub public_id: PublicId,
    pub name: String,
    pub away_since: Option<String>,
    pub frees_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn public_id() -> PublicId {
        serde_json::from_value(json!("0123456789abcdef")).unwrap()
    }

    #[test]
    fn presence_fixture_matches_serde_output() {
        let fixture = include_str!("../../../../web/src/plan/api/fixtures/presence.json");
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let view = PresenceList {
            entries: vec![PresenceEntry {
                public_id: public_id(),
                name: "Ada".into(),
                holder: true,
                away: false,
                reading: Some("doc/plans/PLAN-example.md".into()),
                tabs: 2,
            }],
        };
        assert_eq!(serde_json::to_value(&view).unwrap(), expected);
        assert_eq!(serde_json::from_str::<PresenceList>(fixture).unwrap(), view);
    }

    #[test]
    fn authoring_fixture_matches_serde_output() {
        let fixture = include_str!("../../../../web/src/plan/api/fixtures/authoring.json");
        let expected: Value = serde_json::from_str(fixture).unwrap();
        let view = AuthoringView {
            holder: Some(HolderView {
                public_id: public_id(),
                name: "Ada".into(),
                away_since: Some("2026-10-27T10:00:00Z".into()),
                frees_at: Some("2026-10-27T10:10:00Z".into()),
            }),
        };
        assert_eq!(serde_json::to_value(&view).unwrap(), expected);
        assert_eq!(
            serde_json::from_str::<AuthoringView>(fixture).unwrap(),
            view
        );
    }

    #[test]
    fn nullable_fields_and_unknown_fields() {
        let empty: AuthoringView = serde_json::from_value(json!({"holder": null})).unwrap();
        assert_eq!(
            serde_json::to_value(empty).unwrap(),
            json!({"holder": null})
        );
        let entry = json!({"public_id": "0123456789abcdef", "name": "Ada", "holder": false,
            "away": false, "reading": null, "tabs": 1, "extra": true});
        assert!(serde_json::from_value::<PresenceEntry>(entry).is_err());
        assert!(
            serde_json::from_value::<HolderView>(json!({"public_id": "0123456789abcdef",
            "name": "Ada", "away_since": null, "frees_at": null, "extra": true}))
            .is_err()
        );
    }
}
