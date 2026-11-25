//! Contract `artifact-gaps-carry-their-index`: every artifacts gap starts
//! `[artifact <n>] ` with its pattern's 0-based index, and an invalid stored
//! glob becomes a gap at its index instead of aborting the check.
use super::run_goal_backward_verification;
use crate::plan::schema::{CommandConfinement, StageDefinition};

#[test]
fn artifact_gaps_carry_their_index() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("present.rs"), "fn present() {}\n").unwrap();
    let def = StageDefinition {
        artifacts: vec![
            "present.rs".to_string(),
            "missing.rs".to_string(),
            "bad/[x.rs".to_string(),
        ],
        ..Default::default()
    };

    let result = run_goal_backward_verification(
        &def,
        tmp.path(),
        None,
        CommandConfinement::default(),
        None,
        1,
    )
    .expect("an invalid glob does not abort the check");

    let descriptions: Vec<&str> = result
        .gaps()
        .iter()
        .map(|gap| gap.description.as_str())
        .collect();
    assert_eq!(descriptions.len(), 2, "{descriptions:?}");
    assert!(
        descriptions
            .iter()
            .any(|d| d.starts_with("[artifact 1] ") && d.contains("missing.rs")),
        "{descriptions:?}"
    );
    assert!(
        descriptions
            .iter()
            .any(|d| d.starts_with("[artifact 2] ") && d.contains("bad/[x.rs")),
        "{descriptions:?}"
    );
}
