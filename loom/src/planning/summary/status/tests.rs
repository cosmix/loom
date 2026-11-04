//! Tests for the status report.

use serde_json::Value;

use super::*;
use crate::planning::summary::claims::create_claim_as;
use crate::planning::summary::evaluate::fixtures::*;
use crate::planning::summary::store::is_overview;

fn json_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>()
}

#[test]
fn status_json_reports_sentences_claims_and_order() {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    seal_document(&fx.root, ID2, &[("Beta holds.", "b")]);
    create_claim_as(&fx.paths, ID2, Some("session-2".into()))
        .unwrap()
        .unwrap();
    put(&fx.root, "doc/loom/summaries/plans/gone.md", "orphan");
    let report = status_report(&fx.root, &fx.paths).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema"], 1);
    let rows = json["summaries"].as_array().unwrap();
    let ids: Vec<_> = rows.iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [VISION_ID, ID1, ID2, PRODUCT, "plans/gone"]);
    assert_eq!(
        json_keys(&rows[1]),
        [
            "claim",
            "error",
            "id",
            "path",
            "sentences",
            "source",
            "state"
        ]
    );
    assert_eq!(
        json_keys(&rows[1]["sentences"][0]),
        ["id", "line", "state", "text"]
    );
    assert_eq!(rows[1]["state"], "current");
    assert_eq!(rows[1]["source"], S1);
    assert_eq!(
        rows[1]["path"],
        "doc/loom/summaries/demo/product/section_01.md"
    );
    assert_eq!(rows[1]["sentences"][0]["text"], "Alpha holds.");
    assert!(rows[1]["sentences"][0]["line"].as_u64().unwrap() > 0);
    assert_eq!(rows[2]["state"], "rebuilding");
    assert_eq!(
        json_keys(&rows[2]["claim"]),
        ["created_at", "pid", "session"]
    );
    assert_eq!(rows[2]["claim"]["session"], "session-2");
    assert_eq!(rows[4]["state"], "missing");
}

#[test]
fn human_status_includes_stale_counts_and_claimant() {
    let fx = two_sections();
    seal_document(
        &fx.root,
        ID1,
        &[("Alpha holds.", "a"), ("Beta holds.", "b")],
    );
    put(&fx.root, S1, &section(1, "Changed.", "Beta."));
    create_claim_as(&fx.paths, ID1, Some("session-1".into()))
        .unwrap()
        .unwrap();
    let rendered = render_status(&status_report(&fx.root, &fx.paths).unwrap());
    assert!(rendered
        .lines()
        .any(|line| line
            == "rebuilding demo/product/section_01  1/2 sentences  [claimed by session-1]"));
    assert!(rendered
        .lines()
        .any(|line| line == "missing demo/product/section_02  0/0 sentences"));
}

#[test]
fn human_status_neutralises_a_hostile_session_name() {
    let fx = two_sections();
    seal_document(&fx.root, ID1, &[("Alpha holds.", "a")]);
    create_claim_as(&fx.paths, ID1, Some("evil\u{202E}name".into()))
        .unwrap()
        .unwrap();
    let rendered = render_status(&status_report(&fx.root, &fx.paths).unwrap());
    assert!(!rendered.contains('\u{202E}'), "{rendered:?}");
    assert!(rendered.contains("[claimed by evil name]"), "{rendered:?}");
}

#[test]
fn status_has_a_row_for_each_features_overviews() {
    let fx = two_features();
    let other = "overview/other/product";
    create_claim_as(&fx.paths, other, Some("other-session".into()))
        .unwrap()
        .unwrap();

    let report = status_report(&fx.root, &fx.paths).unwrap();
    let json = serde_json::to_value(&report).unwrap();

    let rows: Vec<_> = json["summaries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| is_overview(row["id"].as_str().unwrap()))
        .collect();
    let ids: Vec<_> = rows.iter().map(|row| row["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        [PRODUCT, ENGINEERING, other, "overview/other/engineering"]
    );
    for row in &rows {
        let id = row["id"].as_str().unwrap();
        assert_eq!(row["path"], format!("doc/loom/summaries/{id}.md"));
        assert_eq!(row["source"], Value::Null);
    }
    assert_eq!(rows[2]["claim"]["session"], "other-session");
    assert!(rows[0]["claim"].is_null());
    let rendered = render_status(&report);
    for id in ids {
        assert!(rendered.contains(&format!("missing {id}  0/0 sentences")));
    }
}
