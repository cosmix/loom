use super::*;
use crate::fs::knowledge::catalog::EvidenceUnavailableReason;
use std::path::PathBuf;

fn issue(reason: EvidenceUnavailableReason) -> CatalogIssue {
    CatalogIssue::EvidenceUnavailable {
        file: PathBuf::from("patterns.md"),
        source_path: "src/patterns.rs".to_string(),
        reason,
    }
}

#[test]
fn strict_evidence_counts_review_only_evidence_but_strict_does_not() {
    let issues = vec![
        CatalogIssue::EvidenceChanged {
            file: PathBuf::from("patterns.md"),
            source_path: "src/patterns.rs".to_string(),
            verified: "0123456789abcdef".to_string(),
        },
        issue(EvidenceUnavailableReason::MissingRevision),
    ];

    let structural = strict_issue_count(&issues);
    assert_eq!(strict_failure_count(true, false, structural, &issues), 0);
    assert_eq!(strict_failure_count(false, true, structural, &issues), 2);
}

#[test]
fn json_payload_preserves_fields_and_reports_unassessed_knowledge() {
    let temp = tempfile::tempdir().expect("create knowledge root");
    let root = temp.path().join("knowledge");
    std::fs::create_dir(&root).expect("create knowledge directory");
    std::fs::write(root.join("topic.md"), "# Topic\n").expect("write unassessed topic");
    let catalog = Catalog {
        revision: String::new(),
        chunks: Vec::new(),
        issues: Vec::new(),
    };

    let payload = json_payload(&root, &catalog);

    assert!(payload.get("issues").is_some());
    assert!(payload.get("review").is_some());
    assert!(payload.get("count").is_some());
    assert_eq!(payload["evidence"]["status"], "unassessed");
    assert_eq!(payload["evidence"]["unassessed"], 1);
    assert_eq!(payload["evidence"]["current"], 0);
}
