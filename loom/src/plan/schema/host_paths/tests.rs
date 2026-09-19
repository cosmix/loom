use super::*;
use crate::plan::schema::tests::make_stage;
use crate::plan::schema::{AcceptanceCriterion, SuccessCriteria, TruthCheck, WiringTest};
use tempfile::TempDir;

fn stage_grants(entries: &[&str]) -> (StageDefinition, Vec<String>) {
    let stage = make_stage("s1", "Stage One");
    let allow_write = entries.iter().map(|s| s.to_string()).collect();
    (stage, allow_write)
}

fn stage_with_acceptance(cmd: &str) -> StageDefinition {
    let mut stage = make_stage("s1", "Stage One");
    stage.acceptance = vec![AcceptanceCriterion::Simple(cmd.to_string())];
    stage
}

fn stage_with_setup(cmd: &str) -> StageDefinition {
    let mut stage = make_stage("s1", "Stage One");
    stage.setup = vec![cmd.to_string()];
    stage
}

// ── G1: ephemeral grants ─────────────────────────────────────────────────

#[test]
fn g1_flags_tmp_root_variants() {
    for entry in ["/tmp/x", "/tmp/x/**", "//tmp/x", "/var/tmp/x"] {
        let (stage, allow) = stage_grants(&[entry]);
        let errors = host_path_errors(&stage, &allow, None);
        assert_eq!(errors.len(), 1, "{entry}: {errors:?}");
        assert!(
            errors[0].contains("does not survive a reboot"),
            "{entry}: {errors:?}"
        );
    }
}

#[test]
fn g1_leaves_relative_and_non_tmp_grants_clean() {
    for entry in ["target/**", "tmp/x"] {
        let (stage, allow) = stage_grants(&[entry]);
        assert!(host_path_errors(&stage, &allow, None).is_empty(), "{entry}");
    }
}

// ── G2: missing grants ───────────────────────────────────────────────────

/// `~/`-relative grant resolved against an injected home rather than the
/// platform's default temp directory - `TempDir::new()` defaults to `/tmp`
/// on this host, which G1 would flag on its own and defeat these G2-only
/// tests; a `~/grant` entry is not itself under an ephemeral root, so G1
/// stays quiet even though the `TempDir` backing the home lives under
/// `/tmp`.
#[test]
fn g2_flags_missing_home_relative_grant() {
    let home = TempDir::new().unwrap();
    let (stage, allow) = stage_grants(&["~/grant"]);

    let errors = host_path_errors(&stage, &allow, Some(home.path()));

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("does not exist on this host"));
}

#[test]
fn g2_clean_once_home_relative_grant_exists() {
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join("grant")).unwrap();
    let (stage, allow) = stage_grants(&["~/grant"]);

    assert!(host_path_errors(&stage, &allow, Some(home.path())).is_empty());
}

#[test]
fn g2_skips_entries_already_flagged_by_g1() {
    let (stage, allow) = stage_grants(&["/tmp/loom-missing-check"]);

    let errors = host_path_errors(&stage, &allow, None);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("does not survive a reboot"));
}

// ── C1: TMPDIR override ──────────────────────────────────────────────────

#[test]
fn c1_flags_tmpdir_assignment_and_export() {
    for cmd in ["TMPDIR=/tmp/x cargo test", "export TMPDIR=~/t"] {
        let stage = stage_with_acceptance(cmd);
        let errors = host_path_errors(&stage, &[], None);
        assert_eq!(errors.len(), 1, "{cmd}: {errors:?}");
        assert!(errors[0].contains("overrides TMPDIR"), "{cmd}: {errors:?}");
    }
}

#[test]
fn c1_leaves_tmpdir_self_reference_and_unrelated_assignment_clean() {
    for cmd in [
        r#"TMPDIR="$TMPDIR" cargo test"#,
        "CARGO_TARGET_DIR=target cargo build",
    ] {
        let stage = stage_with_acceptance(cmd);
        assert!(host_path_errors(&stage, &[], None).is_empty(), "{cmd}");
    }
}

// ── C2: hardcoded temp path ──────────────────────────────────────────────

#[test]
fn c2_flags_hardcoded_temp_paths() {
    for cmd in [
        "cp a /tmp/b",
        "cargo test > /tmp/log 2>&1",
        "test -d /var/tmp/x",
    ] {
        let stage = stage_with_acceptance(cmd);
        let errors = host_path_errors(&stage, &[], None);
        assert_eq!(errors.len(), 1, "{cmd}: {errors:?}");
        assert!(errors[0].contains("hardcodes"), "{cmd}: {errors:?}");
    }
}

#[test]
fn c2_leaves_grep_family_and_tmpdir_expansion_clean() {
    for cmd in [
        r#"rg -q "/tmp/" src/lib.rs"#,
        r#"H=$(mktemp -d "${TMPDIR:-/tmp}/loom.XXXXXX") && [ -n "$H" ]"#,
        r#"ls "$TMPDIR""#,
        r#"sed -n '/tmp/p' file"#,
        r#"awk '/tmp/ {print}' file"#,
    ] {
        let stage = stage_with_acceptance(cmd);
        assert!(host_path_errors(&stage, &[], None).is_empty(), "{cmd}");
    }
}

// ── C3: write outside the worktree ───────────────────────────────────────

#[test]
fn c3_flags_writes_outside_worktree() {
    for cmd in [
        "mkdir -p ~/.cache/x",
        "touch /var/lib/x",
        "cargo build > /home/u/log",
    ] {
        let stage = stage_with_setup(cmd);
        let errors = host_path_errors(&stage, &[], None);
        assert_eq!(errors.len(), 1, "{cmd}: {errors:?}");
        assert!(
            errors[0].contains("outside the worktree"),
            "{cmd}: {errors:?}"
        );
    }
}

#[test]
fn c3_leaves_worktree_local_and_dev_null_writes_clean() {
    for cmd in [
        "mkdir -p target/x",
        "cargo test > /dev/null 2>&1",
        r#"mkdir -p "$TMPDIR/x""#,
    ] {
        let stage = stage_with_setup(cmd);
        assert!(host_path_errors(&stage, &[], None).is_empty(), "{cmd}");
    }
}

// ── Grouping across indices ──────────────────────────────────────────────

#[test]
fn groups_same_issue_across_indices_in_one_field() {
    let mut stage = make_stage("build", "Build");
    stage.acceptance = vec![
        AcceptanceCriterion::Simple("TMPDIR=/tmp/x cargo build".to_string()),
        AcceptanceCriterion::Simple("TMPDIR=/tmp/x cargo test".to_string()),
        AcceptanceCriterion::Simple("TMPDIR=/tmp/x cargo check".to_string()),
    ];

    let errors = host_path_errors(&stage, &[], None);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("#1, #2, #3"), "{errors:?}");
    assert!(
        errors[0].contains("(first: `TMPDIR=/tmp/x cargo build`)"),
        "{errors:?}"
    );
}

#[test]
fn keeps_separate_messages_for_different_offending_values() {
    let mut stage = make_stage("build", "Build");
    stage.acceptance = vec![
        AcceptanceCriterion::Simple("TMPDIR=/tmp/x cargo build".to_string()),
        AcceptanceCriterion::Simple("TMPDIR=/tmp/y cargo test".to_string()),
    ];

    let errors = host_path_errors(&stage, &[], None);

    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors[0].contains("acceptance #1 (`TMPDIR=/tmp/x cargo build`)"));
    assert!(errors[1].contains("acceptance #2 (`TMPDIR=/tmp/y cargo test`)"));
}

// ── Labels and stage id ───────────────────────────────────────────────────

#[test]
fn labels_and_stage_id_appear_in_each_command_field_message() {
    let bad = "cp a /tmp/b";
    let mut stage = make_stage("my-stage", "My Stage");
    stage.acceptance = vec![AcceptanceCriterion::Simple(bad.to_string())];
    stage.setup = vec![bad.to_string()];
    stage.wiring_tests = vec![WiringTest {
        name: "wt".to_string(),
        command: bad.to_string(),
        success_criteria: SuccessCriteria::default(),
        description: None,
    }];
    stage.before_stage = vec![TruthCheck {
        command: bad.to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: None,
        description: None,
    }];
    stage.after_stage = vec![TruthCheck {
        command: bad.to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: None,
        description: None,
    }];

    let errors = host_path_errors(&stage, &[], None);

    assert_eq!(errors.len(), 5, "{errors:?}");
    for label in [
        "acceptance #1",
        "setup #1",
        "wiring_tests #1",
        "before_stage #1",
        "after_stage #1",
    ] {
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with("Stage 'my-stage':") && e.contains(label)),
            "missing {label} in {errors:?}"
        );
    }
}

// ── Regression: setup + acceptance + grant sharing one /tmp path ────────

#[test]
fn regression_setup_acceptance_and_grant_all_point_at_same_tmp_path() {
    let mut stage = make_stage("build", "Build");
    stage.setup = vec!["mkdir -p /tmp/loom-efficiency-checks".to_string()];
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        "TMPDIR=/tmp/loom-efficiency-checks cargo build --offline".to_string(),
    )];
    let allow_write = vec!["/tmp/loom-efficiency-checks".to_string()];

    let errors = host_path_errors(&stage, &allow_write, None);

    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("allow_write grant")
        && e.contains("/tmp/loom-efficiency-checks")
        && e.contains("does not survive a reboot")));
    assert!(errors
        .iter()
        .any(|e| e.starts_with("Stage 'build': setup #1")
            && e.contains("/tmp/loom-efficiency-checks")));
    assert!(errors
        .iter()
        .any(|e| e.starts_with("Stage 'build': acceptance #1") && e.contains("overrides TMPDIR")));
}
