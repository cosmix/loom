use super::*;
use crate::verify::integrity::EventKind;

fn request(kind: DisputeKind) -> DisputeRequest {
    DisputeRequest {
        id: 1,
        stage_id: "stage-a".to_string(),
        kind,
        reason: "criterion impossible".to_string(),
        evidence_commit: Some("abc123".to_string()),
        failure_output: Some("error: ...".to_string()),
        fix_attempts_at_dispute: 1,
        created_at: Utc::now(),
    }
}

fn open(id: &str, origin_stage: Option<&str>) -> OpenFinding {
    OpenFinding {
        id: id.to_string(),
        origin_stage: origin_stage.map(str::to_string),
        finding: Finding {
            severity: "high".to_string(),
            file: "src/a.rs".to_string(),
            line: 7,
            claim: "unchecked input".to_string(),
            scenario: Some("an empty list".to_string()),
            rule: None,
        },
    }
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn dispute_request_round_trip_yaml() {
    let req = request(DisputeKind::Criterion { criterion_index: 2 });
    let y = serde_yaml::to_string(&req).unwrap();
    let back: DisputeRequest = serde_yaml::from_str(&y).unwrap();
    assert_eq!(req, back);
}

#[test]
fn the_kind_sits_beside_the_request_fields_in_the_frontmatter() {
    let req = request(DisputeKind::Criterion { criterion_index: 2 });
    let yaml: serde_yaml::Value = serde_yaml::to_value(&req).unwrap();
    assert_eq!(yaml["kind"], serde_yaml::Value::from("criterion"));
    assert_eq!(yaml["criterion_index"], serde_yaml::Value::from(2));
}

#[test]
fn a_findings_request_round_trips_with_its_evidence() {
    let evidence = select_findings(&[open("s0/F-2-1", Some("s0"))], &ids(&["s0/F-2-1"])).unwrap();
    let req = request(DisputeKind::Findings {
        finding_ids: ids(&["s0/F-2-1"]),
        evidence,
    });
    let y = serde_yaml::to_string(&req).unwrap();
    assert!(y.contains("kind: findings"), "{y}");
    let back: DisputeRequest = serde_yaml::from_str(&y).unwrap();
    assert_eq!(req, back);
}

#[test]
fn an_integrity_request_round_trips_with_its_evidence() {
    let event = IntegrityEvent {
        id: "TI-assert-rust".to_string(),
        kind: EventKind::AssertTotal,
        language: Some("rust".to_string()),
        path: None,
        base: Some(9),
        current: Some(7),
        current_sha256: None,
        detail: Vec::new(),
    };
    let req = request(DisputeKind::Integrity {
        event_ids: ids(&["TI-assert-rust"]),
        evidence: select_events(&[event], &ids(&["TI-assert-rust"])).unwrap(),
    });
    let y = serde_yaml::to_string(&req).unwrap();
    let back: DisputeRequest = serde_yaml::from_str(&y).unwrap();
    assert_eq!(req, back);
}

#[test]
fn selecting_findings_keeps_the_named_order_and_reads_the_round() {
    let open = [open("F-1-1", None), open("F-3-2", None)];
    let picked = select_findings(&open, &ids(&["F-3-2", "F-1-1"])).unwrap();
    let rounds: Vec<(&str, u32)> = picked.iter().map(|s| (s.id.as_str(), s.round)).collect();
    assert_eq!(rounds, vec![("F-3-2", 3), ("F-1-1", 1)]);
}

#[test]
fn selecting_refuses_an_unknown_a_repeated_or_no_id() {
    let open = [open("F-1-1", None)];
    let unknown = select_findings(&open, &ids(&["F-9-9"])).unwrap_err();
    assert!(unknown
        .to_string()
        .contains("'F-9-9' names no open review finding"));
    let twice = select_findings(&open, &ids(&["F-1-1", "F-1-1"])).unwrap_err();
    assert!(twice.to_string().contains("named twice"));
    assert!(select_events(&[], &[]).is_err());
}

#[test]
fn verdict_accept_serializes_with_citations() {
    let v = DisputeVerdictRecord {
        id: 1,
        stage_id: "stage-a".to_string(),
        verdict: DisputeVerdict::Accept {
            plan_patch: PlanPatch {
                inner: serde_json::json!({"foo": "bar"}),
            },
            citations: vec![Citation {
                file: "src/foo.rs".to_string(),
                line: Some(42),
                excerpt: "fn foo()".to_string(),
                claim: "function exists".to_string(),
            }],
            reasoning: "evidence supports".to_string(),
        },
        adjudicator_attempt_count: 1,
        created_at: Utc::now(),
        model: "claude-sonnet".to_string(),
        session_id: Some("session-abc".to_string()),
    };
    let y = serde_yaml::to_string(&v).unwrap();
    let back: DisputeVerdictRecord = serde_yaml::from_str(&y).unwrap();
    assert_eq!(v, back);
}

#[test]
fn verdict_reject_no_plan_patch_field() {
    let v = DisputeVerdict::Reject {
        citations: vec![],
        reasoning: "no evidence".to_string(),
    };
    let s = serde_yaml::to_string(&v).unwrap();
    assert!(s.contains("reject"), "verdict tag missing: {s}");
    assert!(
        !s.contains("plan_patch"),
        "Reject must not serialize plan_patch: {s}"
    );
}

#[test]
fn verdict_needs_more_evidence_carries_questions() {
    let v = DisputeVerdict::NeedsMoreEvidence {
        questions: vec!["clarify A".to_string(), "clarify B".to_string()],
    };
    let s = serde_yaml::to_string(&v).unwrap();
    assert!(s.contains("needs-more-evidence"), "wrong tag: {s}");
    assert!(s.contains("clarify A"));
}

#[test]
fn a_rulings_verdict_reads_the_wire_shape() {
    let wire = serde_json::json!({
        "verdict": "rulings",
        "rulings": [{"finding": "F-1-1", "ruling": "dismiss", "reasoning": "handled upstream"}],
    });
    let verdict: DisputeVerdict = serde_json::from_value(wire).unwrap();
    let DisputeVerdict::Rulings { rulings } = verdict else {
        panic!("expected rulings, got {verdict:?}");
    };
    assert_eq!(rulings[0].ruling, RulingKind::Dismiss);
    assert_eq!(rulings[0].target_stage, None);
    assert!(rulings[0].citations.is_empty());
}
