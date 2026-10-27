use super::report::{verify_path, verify_text, MetadataPresence, VerifyWarnings};
use super::*;
use crate::plan::schema::base_tree;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_should_fail() {
    assert!(!should_fail(0, 0, false));
    assert!(!should_fail(0, 0, true));
    assert!(should_fail(1, 0, false));
    assert!(!should_fail(0, 1, false));
    assert!(should_fail(0, 1, true));
    assert!(should_fail(1, 1, true));
}

#[test]
fn test_repo_root_walk() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create .git at root level to make this a "git repo"
    fs::create_dir(root.join(".git")).unwrap();

    // Create a plan file 3 levels deep
    let deep_dir = root.join("a/b/c");
    fs::create_dir_all(&deep_dir).unwrap();
    let plan_path = deep_dir.join("plan.md");
    fs::write(&plan_path, "# Test").unwrap();

    let found = find_repo_root(&plan_path);
    assert!(found.is_some());
    assert_eq!(found.unwrap(), root);

    // Tree with no .git anywhere: use a path whose entire ancestor chain
    // is non-existent, so .git cannot exist at any level.
    // exists() returns false for non-existent paths, so this reliably → None.
    let nonexistent_plan = std::path::Path::new("/tmp-loom-verify-nonexistent-12345/a/b/plan.md");
    assert!(find_repo_root(nonexistent_plan).is_none());
}

#[test]
fn test_repo_root_walk_with_dotgit_as_file() {
    // Worktrees use a `.git` *file* (containing `gitdir: <path>`) instead
    // of a directory. `find_repo_root` claims to support this via
    // `Path::exists`, which returns true for both files and dirs.
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    fs::write(root.join(".git"), "gitdir: /some/where/else\n").unwrap();

    let deep_dir = root.join("a/b");
    fs::create_dir_all(&deep_dir).unwrap();
    let plan_path = deep_dir.join("plan.md");
    fs::write(&plan_path, "# Test").unwrap();

    let found = find_repo_root(&plan_path);
    assert_eq!(found.as_deref(), Some(root));
}

#[test]
fn baseline_warnings_count_toward_strict() {
    let warnings = VerifyWarnings {
        baseline: vec!["criterion passes on the untouched tree".to_string()],
        ..VerifyWarnings::default()
    };
    assert_eq!(warnings.total(), 1);
    assert!(should_fail(0, warnings.total(), true));
    assert!(!should_fail(0, warnings.total(), false));
    assert_eq!(warnings.sections()[3].0, "Baseline");
}

#[test]
fn verifies_a_plan_with_no_repository_root() {
    let temp = TempDir::new().unwrap();
    let plan_path = temp.path().join("PLAN-no-repo.md");
    let plan = "# No Repo\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  \
                stages:\n    - id: stage-one\n      name: \"Stage One\"\n      \
                stage_type: standard\n      working_dir: \".\"\n      acceptance:\n        \
                - \"rg -q present notes.txt\"\n```\n\n<!-- END loom METADATA -->\n";
    fs::write(&plan_path, plan).unwrap();

    execute(&plan_path, false, false, false).unwrap();
    let report = base_tree::check_base_tree(&[], None);
    assert!(report.warnings.is_empty());
    assert!(report.note.is_some_and(|note| note.contains("skipped")));
}

// ── verify_text ────────────────────────────────────────────────────────────

/// A plan document wrapping `yaml` (the body of the metadata fence).
fn plan_doc(title: &str, yaml: &str) -> String {
    format!(
        "# {title}\n\n<!-- loom METADATA -->\n\n```yaml\n{yaml}```\n\n\
         <!-- END loom METADATA -->\n"
    )
}

/// `yaml` for a plan of one stage; `extra` is appended to the stage's lines.
fn one_stage(working_dir: &str, acceptance: &str, extra: &str) -> String {
    format!(
        "loom:\n  version: 1\n  stages:\n    - id: stage-one\n      name: \"Stage One\"\n      \
         stage_type: standard\n      working_dir: \"{working_dir}\"\n      acceptance:\n        \
         - \"{acceptance}\"\n{extra}"
    )
}

const MISSING_BRIEF: &str =
    "      description: \"See doc/plans/briefs/nonexistent.md for context.\"\n";

/// The fixture plans of `tests/integration/plan_verify.rs`, plus one with no metadata block.
fn fixture_plans() -> Vec<String> {
    let stage = |id: &str, dependency: &str| {
        format!(
            "    - id: {id}\n      name: \"Stage {id}\"\n      working_dir: \".\"\n      \
             acceptance:\n        - \"true\"\n      dependencies:\n        - {dependency}\n"
        )
    };
    let cycle = format!(
        "loom:\n  version: 1\n  stages:\n{}{}",
        stage("stage-a", "stage-b"),
        stage("stage-b", "stage-a")
    );
    let bad_version = one_stage(".", "true", "").replace("version: 1", "version: 3");
    let self_dependency = "      dependencies:\n        - stage-one\n";
    vec![
        plan_doc("Minimal", &one_stage(".", "true", "")),
        plan_doc("Bad Version", &bad_version),
        plan_doc("Self Dep", &one_stage(".", "true", self_dependency)),
        plan_doc("Cycle", &cycle),
        plan_doc(
            "Warning",
            &one_stage("loom", "loom/target/debug/loom --help", ""),
        ),
        plan_doc("Missing Brief", &one_stage(".", "true", MISSING_BRIEF)),
        plan_doc("Malformed", "loom:\n  version: 1\n  stages: [unbalanced\n"),
        "# No metadata\n\nJust prose.\n".to_string(),
    ]
}

#[test]
fn verify_text_matches_verify_path_for_every_fixture() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join(".git")).unwrap();
    let path = temp.path().join("PLAN-fixture.md");
    for text in fixture_plans() {
        fs::write(&path, &text).unwrap();
        let from_path = verify_path(&path).unwrap();
        let root = find_repo_root(&std::path::absolute(&path).unwrap());
        let from_text = verify_text(root.as_deref(), &path, &text);
        assert_eq!(
            serde_json::to_value(&from_text).unwrap(),
            serde_json::to_value(&from_path).unwrap(),
            "{text}"
        );
        assert_eq!(from_text.metadata, from_path.metadata);
        assert_eq!(from_text.yaml_location, from_path.yaml_location);
        assert_eq!(from_text.stopped, from_path.stopped);
    }
}

#[test]
fn missing_metadata_block_is_reported_as_missing() {
    let report = verify_text(None, Path::new("PLAN-x.md"), "# Prose\n\nNo plan here.\n");
    assert_eq!(report.metadata, MetadataPresence::Missing);
    assert!(!report.valid);
    assert_eq!(report.errors[0].message, "No loom METADATA block found");
    assert!(report.plan.id.is_none() && report.yaml_location.is_none());
    assert!(report.stopped, "the human output prints only the reason");
}

#[test]
fn yaml_error_location_is_the_line_in_the_file() {
    let yaml = "loom:\n  version: 1\n  stages:\n    - id: a\n      name: a: b\n";
    let text = plan_doc("Bad line", yaml);
    let report = verify_text(None, Path::new("PLAN-bad.md"), &text);
    assert_eq!(report.metadata, MetadataPresence::Present);
    assert!(report.errors[0].message.starts_with("YAML parse error"));
    assert!(report.stopped, "the human output prints only the reason");
    let location = report
        .yaml_location
        .expect("a YAML syntax error has a location");
    // Lines 1-5 are the title, a blank, the marker, a blank and the fence; YAML line 5 is line 10.
    assert_eq!(text.lines().nth(9), Some("      name: a: b"));
    assert_eq!(location.line, 10, "{location:?}");
    assert!(location.column > 1, "{location:?}");
}

#[test]
fn proposed_text_input_errors_do_not_come_from_the_disk() {
    let temp = TempDir::new().unwrap();
    fs::create_dir(temp.path().join(".git")).unwrap();
    let path = temp.path().join("PLAN-edit.md");
    fs::write(&path, plan_doc("Edit", &one_stage(".", "true", ""))).unwrap();
    let proposed = plan_doc("Edit", &one_stage(".", "true", MISSING_BRIEF));
    let mentions_brief = |report: &VerifyReport| {
        report
            .errors
            .iter()
            .any(|e| e.message.contains("doc/plans/briefs/nonexistent.md"))
    };

    let on_disk = verify_path(&path).unwrap();
    assert!(!mentions_brief(&on_disk), "{:?}", on_disk.errors);
    let report = verify_text(Some(temp.path()), &path, &proposed);
    assert!(mentions_brief(&report), "{:?}", report.errors);
    assert!(!report.valid);
}
