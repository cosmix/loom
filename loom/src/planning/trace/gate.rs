//! Whether the metaplan is ready for plan authoring.

use crate::planning::project::Feature;

use super::{DocRole, SettleState};

/// Plans may be authored once the metaplan is settled or no longer needs settling.
pub fn metaplan_settled(feature: &Feature) -> bool {
    feature
        .trace
        .documents
        .iter()
        .find(|doc| doc.role == DocRole::Metaplan)
        .is_some_and(|doc| !doc.settleable || doc.settle == SettleState::Settled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::project::Project;

    fn feature() -> Feature {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("doc/plans/f");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.md"), "# Metaplan\n").unwrap();
        Project::load(temp.path()).unwrap().features.remove(0)
    }

    #[test]
    fn only_a_current_settlement_opens_the_gate() {
        let mut feature = feature();
        for state in [
            SettleState::Draft,
            SettleState::Revised,
            SettleState::StubChanged,
            SettleState::UpstreamChanged,
            SettleState::Settled,
        ] {
            let doc = &mut feature.trace.documents[0];
            assert_eq!(doc.role, DocRole::Metaplan);
            doc.settleable = true;
            doc.settle = state;
            assert_eq!(metaplan_settled(&feature), state == SettleState::Settled);
            feature.trace.documents[0].settleable = false;
            assert!(metaplan_settled(&feature));
        }
    }

    #[test]
    fn missing_metaplan_stays_closed_even_with_other_settled_documents() {
        let mut feature = feature();
        feature.trace.documents[0].role = DocRole::Plan;
        feature.trace.documents[0].settle = SettleState::Settled;
        feature.trace.documents[0].settleable = false;
        assert!(!metaplan_settled(&feature));
        feature.trace.documents.clear();
        assert!(!metaplan_settled(&feature));
    }
}
