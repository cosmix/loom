//! Shared checklist test helpers and the project plan-row ordering regression.

use super::{offer_view, Offer, OfferView, Project, ProjectChecklistView, Value};
use std::path::Path;

pub(super) fn put(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

pub(super) fn pitch_of(text: &str) -> Option<String> {
    let temp = tempfile::tempdir().unwrap();
    put(temp.path(), "doc/product/demo/index.md", text);
    let project = Project::load(temp.path()).unwrap();
    ProjectChecklistView::from(&project)
        .checklists
        .remove(0)
        .pitch
}

/// Fills `"blocked_reason": null` into an expected offer object that omits it,
/// since the wire always serializes the field and only the metaplan gate sets it.
pub(super) fn open_offer(mut expected: Value) -> Value {
    if let Some(object) = expected.as_object_mut() {
        object.entry("blocked_reason").or_insert(Value::Null);
    }
    expected
}

pub(super) fn assert_offer(offer: Offer, expected: Value) {
    let expected = open_offer(expected);
    let view = offer_view(&offer);
    assert_eq!(serde_json::to_value(&view).unwrap(), expected);
    assert_eq!(serde_json::from_value::<OfferView>(expected).unwrap(), view);
}

#[test]
fn feature_plan_rows_follow_metaplan_order_then_unlisted_plans() {
    let temp = tempfile::tempdir().unwrap();
    for (path, text) in [
        (
            "doc/plans/demo/index.md",
            "# Metaplan\n\n## 1. Zeta (`PLAN-zeta.md`)\n\n## 2. Beta (`PLAN-beta.md`)\n",
        ),
        ("doc/plans/demo/PLAN-alpha.md", "# Unlisted\n"),
        ("doc/plans/demo/PLAN-beta.md", "# Second\n"),
        ("doc/plans/demo/PLAN-zeta.md", "# First\n"),
    ] {
        put(temp.path(), path, text);
    }
    let project = Project::load(temp.path()).unwrap();
    let view = ProjectChecklistView::from(&project).checklists.remove(0);

    assert_eq!(view.feature, "demo");
    let plans: Vec<_> = view
        .plan_rows
        .iter()
        .map(|row| (row.slug.as_str(), row.listed))
        .collect();
    assert_eq!(plans, [("zeta", true), ("beta", true), ("alpha", false)]);
}

#[test]
fn a_standalone_plan_carries_its_latest_pressure_date_or_null() {
    let temp = tempfile::tempdir().unwrap();
    let tested =
        "# Tested\n\n## Pressure tests\n\n- 2026-11-05: Reviewed; 1 round\n- 2026-10-01: Earlier\n";
    put(temp.path(), "doc/plans/PLAN-tested.md", tested);
    put(temp.path(), "doc/plans/PLAN-untested.md", "# Untested\n");
    let snapshot = ProjectChecklistView::from(&Project::load(temp.path()).unwrap());
    let dates: Vec<_> = snapshot
        .standalone
        .iter()
        .map(|plan| (plan.slug.as_str(), plan.pressure_date.as_deref()))
        .collect();
    assert_eq!(dates, [("tested", Some("2026-11-05")), ("untested", None)]);
}
