use super::clusters::Cluster;
use super::receipt::{
    refresh_plan, ClusterStatus, Receipt, ReceiptCluster, MAX_RECEIPT_BYTES, RECEIPT_FILENAME,
    RECEIPT_VERSION,
};

fn test_cluster(id: &str, digest: &str) -> Cluster {
    Cluster {
        id: id.to_string(),
        files: Vec::new(),
        symbols: 0,
        digest: digest.to_string(),
        hot_files: Vec::new(),
    }
}

fn test_receipt(clusters: Vec<ReceiptCluster>) -> Receipt {
    Receipt {
        version: RECEIPT_VERSION,
        source_revision: "abc123".to_string(),
        model: "test-model".to_string(),
        effort: "medium".to_string(),
        completed_at: "2026-01-01T00:00:00+00:00".to_string(),
        clusters,
    }
}

#[test]
fn save_and_load_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let receipt = test_receipt(vec![ReceiptCluster {
        id: ".".to_string(),
        digest: "sha256:aaa".to_string(),
        files: 3,
    }]);

    receipt.save(temp.path()).unwrap();
    let loaded = Receipt::load(temp.path()).unwrap();

    assert_eq!(loaded, Some(receipt));
}

#[test]
fn load_missing_file_gives_none() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(Receipt::load(temp.path()).unwrap(), None);
}

#[test]
fn load_symlinked_receipt_gives_err() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("real.json");
    std::fs::write(&target, "{}").unwrap();
    let link = temp.path().join(RECEIPT_FILENAME);
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert!(Receipt::load(temp.path()).is_err());
}

#[test]
fn load_corrupt_json_gives_err_naming_path() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(RECEIPT_FILENAME);
    std::fs::write(&path, "not json").unwrap();

    let error = Receipt::load(temp.path()).unwrap_err();
    assert!(error.to_string().contains(&path.display().to_string()));
}

#[test]
fn load_unknown_field_gives_err_naming_path() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(RECEIPT_FILENAME);
    std::fs::write(
        &path,
        r#"{"version":1,"source_revision":"a","model":"m","effort":"e","completed_at":"t","clusters":[],"extra":true}"#,
    )
    .unwrap();

    let error = Receipt::load(temp.path()).unwrap_err();
    assert!(error.to_string().contains(&path.display().to_string()));
}

#[test]
fn load_oversized_receipt_gives_err() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(RECEIPT_FILENAME);
    // Valid JSON padded past the cap: only the size check can reject it.
    let mut json = serde_json::to_string(&test_receipt(vec![])).unwrap();
    json.push_str(&" ".repeat(MAX_RECEIPT_BYTES));
    std::fs::write(&path, json).unwrap();

    let error = format!("{:#}", Receipt::load(temp.path()).unwrap_err());
    assert!(error.contains("byte verification limit"), "{error}");
}

#[test]
fn save_refuses_a_symlinked_tmp_file() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_file = outside.path().join("real.json");
    std::fs::write(&outside_file, "outside content").unwrap();

    let tmp_path = temp.path().join(format!("{RECEIPT_FILENAME}.tmp"));
    std::os::unix::fs::symlink(&outside_file, &tmp_path).unwrap();

    let receipt = test_receipt(vec![ReceiptCluster {
        id: ".".to_string(),
        digest: "sha256:aaa".to_string(),
        files: 1,
    }]);
    assert!(receipt.save(temp.path()).is_err());
    assert_eq!(
        std::fs::read_to_string(&outside_file).unwrap(),
        "outside content"
    );
}

#[test]
fn load_wrong_version_gives_err_naming_path() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(RECEIPT_FILENAME);
    std::fs::write(
        &path,
        r#"{"version":2,"source_revision":"a","model":"m","effort":"e","completed_at":"t","clusters":[]}"#,
    )
    .unwrap();

    let error = Receipt::load(temp.path()).unwrap_err();
    assert!(error.to_string().contains(&path.display().to_string()));
}

#[test]
fn from_clusters_builds_matching_receipt() {
    let clusters = vec![Cluster {
        id: ".".to_string(),
        files: vec!["a.rs".to_string()],
        symbols: 3,
        digest: "sha256:aaa".to_string(),
        hot_files: Vec::new(),
    }];

    let receipt = Receipt::from_clusters(&clusters, "revabc", "model-x", "high");

    assert_eq!(receipt.version, RECEIPT_VERSION);
    assert_eq!(receipt.source_revision, "revabc");
    assert_eq!(receipt.model, "model-x");
    assert_eq!(receipt.effort, "high");
    assert_eq!(
        receipt.clusters,
        vec![ReceiptCluster {
            id: ".".to_string(),
            digest: "sha256:aaa".to_string(),
            files: 1,
        }]
    );
    assert!(!receipt.completed_at.is_empty());
}

#[test]
fn refresh_plan_with_no_receipt_marks_everything_new() {
    let clusters = vec![
        test_cluster(".", "sha256:a"),
        test_cluster("src", "sha256:b"),
    ];

    let plan = refresh_plan(&clusters, None);

    assert_eq!(
        plan.statuses,
        vec![
            (".".to_string(), ClusterStatus::New),
            ("src".to_string(), ClusterStatus::New),
        ]
    );
    assert!(plan.removed.is_empty());
}

#[test]
fn refresh_plan_classifies_new_changed_unchanged_and_removed() {
    let receipt = test_receipt(vec![
        ReceiptCluster {
            id: ".".to_string(),
            digest: "sha256:a".to_string(),
            files: 1,
        },
        ReceiptCluster {
            id: "src".to_string(),
            digest: "sha256:old".to_string(),
            files: 2,
        },
        ReceiptCluster {
            id: "gone".to_string(),
            digest: "sha256:x".to_string(),
            files: 1,
        },
    ]);
    let current = vec![
        test_cluster(".", "sha256:a"),       // digest matches -> unchanged
        test_cluster("src", "sha256:new"),   // digest differs -> changed
        test_cluster("new-dir", "sha256:z"), // no receipt entry -> new
    ];

    let plan = refresh_plan(&current, Some(&receipt));

    assert_eq!(
        plan.statuses,
        vec![
            (".".to_string(), ClusterStatus::Unchanged),
            ("src".to_string(), ClusterStatus::Changed),
            ("new-dir".to_string(), ClusterStatus::New),
        ]
    );
    assert_eq!(plan.removed, vec!["gone".to_string()]);
}
