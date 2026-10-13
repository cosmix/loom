//! A contract's optional `cwd`: a relative directory, free of `..`, that
//! holds the contract file (`working_dir` joined with `file`).

use super::create_valid_metadata_v2;
use crate::plan::schema::validation::validate;

/// The validation messages for stage 1 of the valid v2 plan with its one
/// contract given `working_dir`, `file` and `cwd`.
fn messages_for(working_dir: &str, file: &str, cwd: Option<&str>) -> Vec<String> {
    let mut metadata = create_valid_metadata_v2();
    let stage = &mut metadata.loom.stages[0];
    stage.working_dir = working_dir.to_string();
    stage.contracts[0].file = file.to_string();
    stage.contracts[0].cwd = cwd.map(str::to_string);
    validate(&metadata)
        .err()
        .unwrap_or_default()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_cwd_holding_the_contract_file_passes() {
    for (working_dir, file, cwd) in [
        (".", "apps/web/src/a.contract.test.ts", Some("apps/web")),
        (
            ".",
            "./apps/web/src/a.contract.test.ts",
            Some("./apps/web/"),
        ),
        ("apps", "web/src/a.contract.test.ts", Some("apps/web")),
        ("apps/web", "src/a.contract.test.ts", Some(".")),
        ("apps/web", "src/a.contract.test.ts", None),
    ] {
        let messages = messages_for(working_dir, file, cwd);
        assert!(
            messages.is_empty(),
            "{working_dir} {file} {cwd:?}: {messages:?}"
        );
    }
}

#[test]
fn a_cwd_outside_the_contract_files_directories_is_rejected() {
    let messages = messages_for("apps", "web/src/a.contract.test.ts", Some("apps/api"));

    assert_eq!(
        messages,
        [
            "Stage 'stage-1': Contract #1 cwd 'apps/api' does not contain the contract file \
          'web/src/a.contract.test.ts' (relative to working_dir 'apps')"
        ]
    );
}

#[test]
fn a_cwd_that_is_not_a_plain_relative_path_is_rejected() {
    let file = "apps/web/src/a.contract.test.ts";
    for (cwd, problem) in [
        (
            "../apps/web",
            "cwd '../apps/web' cannot contain a `..` component",
        ),
        ("/apps/web", "cwd '/apps/web' must be a relative path"),
        (
            " ",
            "cwd must not be empty (use \".\" for the worktree root)",
        ),
    ] {
        let messages = messages_for(".", file, Some(cwd));
        assert_eq!(
            messages,
            [format!("Stage 'stage-1': Contract #1 {problem}")],
            "{cwd}"
        );
    }
}
