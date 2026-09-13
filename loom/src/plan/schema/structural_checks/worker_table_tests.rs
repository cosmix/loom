use super::super::types::StageDefinition;
use super::check_file_ownership;

fn stage(id: &str, files: &[&str], description: Option<&str>) -> StageDefinition {
    let mut stage = crate::plan::schema::tests::make_stage(id, id);
    stage.files = files.iter().map(|path| (*path).to_string()).collect();
    stage.description = description.map(str::to_string);
    stage
}

#[test]
fn worker_table_indented_without_divider_warns_for_shared_path() {
    let stages = vec![stage(
        "context-admission",
        &[
            "loom-hooks/skill-trigger.sh",
            "loom/src/orchestrator/signals/format/skills.rs",
            "loom/src/plan/schema/structural_checks.rs",
            "loom/tests/integration/hooks_read_guard*",
            "loom-hooks/tests/post-tool-use*",
        ],
        Some(concat!(
            "    | worker | files owned |\n",
            "    | Terra skill-routing-ownership | loom-hooks/skill-trigger.sh; ",
            "loom/src/orchestrator/signals/format/skills.rs; ",
            "loom/src/plan/schema/structural_checks.rs |\n",
            "    | Terra read-receipts | loom/tests/integration/hooks_read_guard*; ",
            "loom-hooks/tests/post-tool-use*; ./loom/src/plan/schema/structural_checks.rs |",
        )),
    )];

    let warnings = check_file_ownership(&stages);

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("context-admission"));
    assert!(warnings[0].contains("Terra skill-routing-ownership"));
    assert!(warnings[0].contains("Terra read-receipts"));
    assert!(warnings[0].contains("loom/src/plan/schema/structural_checks.rs"));
}

#[test]
fn worker_table_path_outside_declared_files_warns() {
    let stages = vec![stage(
        "context-admission",
        &["loom/src/plan/schema/structural_checks.rs"],
        Some(
            "| Worker | Files owned |\n\
             | --- | --- |\n\
             | Terra | `loom/src/plan/schema/validation.rs` |",
        ),
    )];

    let warnings = check_file_ownership(&stages);

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("context-admission"));
    assert!(warnings[0].contains("Terra"));
    assert!(warnings[0].contains("loom/src/plan/schema/validation.rs"));
}

#[test]
fn worker_table_distinct_paths_do_not_warn() {
    let stages = vec![stage(
        "context-admission",
        &["loom/src/plan/schema/**"],
        Some(
            "| Worker | Files owned |\n\
             | --- | --- |\n\
             | Terra | `loom/src/plan/schema/structural_checks.rs`; `loom/src/plan/schema/structural_checks/worker_table.rs` |\n\
             | Sol | loom/src/plan/schema/validation.rs, loom/src/plan/schema/types.rs |",
        ),
    )];

    assert!(check_file_ownership(&stages).is_empty());
}

#[test]
fn worker_table_repeated_worker_label_is_not_a_conflict() {
    let stages = vec![stage(
        "context-admission",
        &["loom/src/plan/schema/structural_checks.rs"],
        Some(
            "| Worker | Files owned |\n\
             | Terra | loom/src/plan/schema/structural_checks.rs |\n\
             | Terra | ./loom/src/plan/schema/structural_checks.rs |",
        ),
    )];

    assert!(check_file_ownership(&stages).is_empty());
}

#[test]
fn worker_table_wildcard_claim_inside_and_outside_declared_files() {
    let stages = vec![stage(
        "context-admission",
        &[
            "loom/tests/integration/**",
            "loom/src/quota/**",
        ],
        Some(
            "| Worker | Files owned |\n\
             | Terra | loom/tests/integration/hooks_read_guard*; loom/src/quota/**; loom/src/context/* |",
        ),
    )];

    let warnings = check_file_ownership(&stages);

    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("loom/src/context/*"));
}

#[test]
fn malformed_or_absent_worker_table_is_silent() {
    let malformed = stage(
        "malformed",
        &[],
        Some("| Worker | Files owned |\n| --- |\n| Terra | loom/src/main.rs |"),
    );
    let absent = stage(
        "absent",
        &[],
        Some("Terra owns loom/src/main.rs; this is not a worker table."),
    );

    assert!(check_file_ownership(&[malformed, absent]).is_empty());
}
