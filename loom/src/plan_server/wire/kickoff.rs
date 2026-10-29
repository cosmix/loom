//! Wire types for kickoff proposals and their confirmed target.

use serde::{Deserialize, Serialize};

use crate::plan_session::scope::KickoffSize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeState {
    Settled,
    Wording,
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeSection {
    pub title: String,
    pub state: ShapeState,
}

/// The "taking shape" state a kickoff session emits in a fenced `loom-kickoff` JSON block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickoffShape {
    #[serde(default)]
    pub size: Option<KickoffSize>,
    #[serde(default)]
    pub name: Option<String>,
    /// True while the session asks the user to confirm the size and name.
    #[serde(default)]
    pub proposal: bool,
    #[serde(default)]
    pub problem: String,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub questions: Vec<String>,
    #[serde(default)]
    pub sections: Vec<ShapeSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickoffView {
    pub session: String,
    pub shape: KickoffShape,
    /// Repository-relative document the confirmed kickoff may create; None before the
    /// confirmation.
    pub target: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shape_round_trips_every_field() {
        let shape = KickoffShape {
            size: Some(KickoffSize::Feature),
            name: Some("demo".into()),
            proposal: true,
            problem: "Planning needs a starting point.".into(),
            decisions: vec!["Start with a feature.".into()],
            questions: vec!["Who will use it?".into()],
            sections: vec![ShapeSection {
                title: "Workflow".into(),
                state: ShapeState::Wording,
            }],
        };
        let expected = json!({
            "size": "feature",
            "name": "demo",
            "proposal": true,
            "problem": "Planning needs a starting point.",
            "decisions": ["Start with a feature."],
            "questions": ["Who will use it?"],
            "sections": [{"title": "Workflow", "state": "wording"}]
        });

        assert_eq!(serde_json::to_value(&shape).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<KickoffShape>(expected).unwrap(),
            shape
        );
    }

    #[test]
    fn omitted_shape_keys_parse_to_default() {
        assert_eq!(
            serde_json::from_str::<KickoffShape>("{}").unwrap(),
            KickoffShape::default()
        );
    }

    #[test]
    fn unknown_shape_keys_are_rejected() {
        assert!(serde_json::from_value::<KickoffShape>(json!({"extra": true})).is_err());
        assert!(serde_json::from_value::<KickoffShape>(json!({
            "sections": [{"title": "Workflow", "state": "open", "extra": true}]
        }))
        .is_err());
    }

    #[test]
    fn shape_state_spellings_round_trip() {
        for (state, expected) in [
            (ShapeState::Settled, "settled"),
            (ShapeState::Wording, "wording"),
            (ShapeState::Open, "open"),
        ] {
            assert_eq!(serde_json::to_value(state).unwrap(), json!(expected));
            assert_eq!(
                serde_json::from_value::<ShapeState>(json!(expected)).unwrap(),
                state
            );
        }
        assert!(serde_json::from_str::<ShapeState>(r#""Settled""#).is_err());
    }

    #[test]
    fn kickoff_size_spellings_round_trip() {
        for (size, expected) in [
            (KickoffSize::Feature, "feature"),
            (KickoffSize::Plan, "plan"),
        ] {
            assert_eq!(serde_json::to_value(size).unwrap(), json!(expected));
            assert_eq!(
                serde_json::from_value::<KickoffSize>(json!(expected)).unwrap(),
                size
            );
        }
        assert!(serde_json::from_str::<KickoffSize>(r#""Feature""#).is_err());
    }

    #[test]
    fn unconfirmed_view_round_trips_with_a_null_target() {
        let view = KickoffView {
            session: "kickoff-1".into(),
            shape: KickoffShape::default(),
            target: None,
        };
        let expected = json!({
            "session": "kickoff-1",
            "shape": {"size": null, "name": null, "proposal": false, "problem": "",
                "decisions": [], "questions": [], "sections": []},
            "target": null
        });

        assert_eq!(serde_json::to_value(&view).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<KickoffView>(expected).unwrap(),
            view
        );
    }
}
