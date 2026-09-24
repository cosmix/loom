//! Round trips for the dispute kinds beyond a criterion (DESIGN D15), through
//! the same stand-in for the adjudication session as the criterion tests.
//! Declared from `adjudication_e2e.rs`; `use super::*;` reaches its fixtures.

use super::*;
use loom::models::dispute::{DisputeKind, FindingSnapshot};
use loom::verify::review::report::Finding;
use loom::verify::review::store::{self, RecordedFinding, ReviewRound, RulingKind};

fn finding() -> Finding {
    Finding {
        severity: "major".to_string(),
        file: "src/a.rs".to_string(),
        line: 4,
        claim: "leaks the handle".to_string(),
        scenario: Some("early return → handle never closed".to_string()),
        rule: None,
    }
}

/// One harvested review round for `stage_id` holding finding `F-1-1`.
fn record_review(work: &Path, stage_id: &str) {
    let round = ReviewRound {
        version: store::RECORD_VERSION,
        round: 1,
        agent_id: "reviewer".to_string(),
        harvested_at: chrono::Utc::now(),
        fingerprint: "sha256:0".to_string(),
        files: std::collections::BTreeMap::new(),
        malformed: None,
        findings: vec![RecordedFinding {
            id: "F-1-1".to_string(),
            finding: finding(),
        }],
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    store::write_round(work, stage_id, &round).unwrap();
}

/// Dispute 1 of `stage_id` against `F-1-1`, as the daemon files it.
fn write_findings_dispute(work: &Path, stage_id: &str) {
    let disputes_root = work.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join("1")).unwrap();
    let evidence = FindingSnapshot {
        id: "F-1-1".to_string(),
        origin_stage: None,
        round: 1,
        finding: finding(),
    };
    let request = DisputeRequest {
        id: 1,
        stage_id: stage_id.to_string(),
        kind: DisputeKind::Findings {
            finding_ids: vec![evidence.id.clone()],
            evidence: vec![evidence],
        },
        reason: "the early return never runs with a handle open".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: chrono::Utc::now(),
    };
    let yaml = serde_yaml::to_string(&request).unwrap();
    let path = request_file(&disputes_root, stage_id, 1);
    std::fs::write(path, format!("---\n{yaml}---\n\n# Dispute\n")).unwrap();
}

#[test]
fn findings_dispute_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    record_review(work, "s1");
    write_findings_dispute(work, "s1");
    let verdict = serde_json::json!({
        "verdict": "rulings",
        "rulings": [{
            "finding": "F-1-1",
            "ruling": "dismiss",
            "reasoning": "the handle is closed by its guard on every return path",
            "citations": [
                {"file": "src/a.rs", "line": 4, "excerpt": "let _guard", "claim": "guard closes it"}
            ]
        }]
    });

    drive_dispute(&AdjudicatorRegistry::new(), work, "s1", 1, &verdict);

    let rulings = store::load_rulings(work, "s1").unwrap().rulings;
    assert_eq!(rulings.len(), 1);
    assert_eq!(rulings[0].finding, "F-1-1");
    assert_eq!(rulings[0].ruling, RulingKind::Dismiss);
    assert_eq!(rulings[0].dispute, 1);
    assert!(store::open_findings(work, "s1").unwrap().is_empty());
    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert!(applied_marker(&work.join("disputes"), "s1", 1).exists());
}
