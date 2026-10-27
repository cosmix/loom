//! Tests for the status report.

use serde_json::Value;

use super::*;
use crate::planning::summary::claims::create_claim_as;
use crate::planning::summary::evaluate::fixtures::*;

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
    assert_eq!(ids, [ID1, ID2, "product", "plans/gone"]);
    let keys = |value: &Value| {
        value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&rows[0]),
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
        keys(&rows[0]["sentences"][0]),
        ["id", "line", "state", "text"]
    );
    assert_eq!(rows[0]["state"], "current");
    assert_eq!(rows[0]["source"], S1);
    assert_eq!(rows[0]["path"], "doc/loom/summaries/product/section_01.md");
    assert_eq!(rows[0]["sentences"][0]["text"], "Alpha holds.");
    assert!(rows[0]["sentences"][0]["line"].as_u64().unwrap() > 0);
    assert_eq!(rows[1]["state"], "rebuilding");
    assert_eq!(keys(&rows[1]["claim"]), ["created_at", "pid", "session"]);
    assert_eq!(rows[1]["claim"]["session"], "session-2");
    assert_eq!(rows[3]["state"], "missing");
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
    assert!(
        rendered
            .lines()
            .any(|line| line
                == "rebuilding product/section_01  1/2 sentences  [claimed by session-1]")
    );
    assert!(rendered
        .lines()
        .any(|line| line == "missing product/section_02  0/0 sentences"));
}
