//! Contracts for the `doctrine-and-guards` stage: the integration-verify
//! signal doctrine, the two version-2 bookend lints, the bootstrap warning,
//! the stage model allowlist, and the archive abort.

#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use loom::models::session::Session;
use loom::models::stage::{Stage, StageType};
use loom::models::worktree::Worktree;
use loom::orchestrator::signals::{format_signal_with_metrics, EmbeddedContext};
use loom::plan::schema::{check_knowledge_recommendations, StageDefinition};
use loom::plan::LoomMetadata;
use serde_json::Value;
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn write_file(root: &Path, path: &str, body: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().expect("file has a parent")).expect("create dirs");
    fs::write(&full, body).expect("write file");
}

/// A git repository with `files` committed.
fn repo(files: &[(&str, &str)]) -> TempDir {
    let temp = TempDir::new().expect("create repo dir");
    let root = temp.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "t"]);
    git(root, &["config", "user.email", "t@t"]);
    for (path, body) in files {
        write_file(root, path, body);
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    temp
}

const KNOWLEDGE_CHECK: &str =
    "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt";

/// One version-2 stage. A stage with `contract` carries one cargo-test
/// contract and the matching acceptance command, as add-greeting does in
/// plan_environment_contracts.rs; validate() needs the acceptance, or the
/// v2 lints never run.
fn stage(id: &str, stage_type: &str, dependencies: &[&str], contract: bool) -> String {
    let name = id.replace('-', "_");
    let deps = dependencies
        .iter()
        .map(|dep| format!("\"{dep}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let contracts = if contract {
        format!(
            "      contracts:\n        - id: {id}-works\n          file: \"loom/tests/{name}.rs\"\n          test: \"{name}\"\n          runner: cargo-test\n          scenario: \"the {id} behaviour runs\"\n          rejects: \"an implementation that skips {id}\"\n"
        )
    } else {
        String::new()
    };
    let acceptance = match stage_type {
        "standard" => format!("cargo test --manifest-path loom/Cargo.toml --test {name}"),
        "integration-verify" => "cargo test --manifest-path loom/Cargo.toml --all-targets".into(),
        _ => KNOWLEDGE_CHECK.into(),
    };
    format!(
        "    - id: {id}\n      name: \"Stage {id}\"\n      stage_type: {stage_type}\n      working_dir: \".\"\n      dependencies: [{deps}]\n      summary: \"Do {id}.\"\n      description: \"Do {id}.\"\n{contracts}      acceptance:\n        - \"{acceptance}\"\n\n"
    )
}

/// A version-2 plan document holding `stages`.
fn plan(stages: &[String]) -> String {
    format!(
        "# PLAN: Doctrine Guards\n\n---\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 2\n  stages:\n{}```\n\n<!-- END loom METADATA -->\n",
        stages.concat()
    )
}

/// Run `loom plan verify --json PLAN.md` from `root`, require exit 1, and
/// return `(stage_id, message)` for every entry of its `errors`.
fn verify(root: &Path) -> Vec<(String, String)> {
    let output = helpers::loom_cmd()
        .args(["plan", "verify", "--json", "PLAN.md"])
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn loom plan verify");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let errors: Vec<(String, String)> = json["errors"]
        .as_array()
        .expect("errors array")
        .iter()
        .map(|error| {
            let stage_id = error["stage_id"].as_str().unwrap_or_default().to_string();
            let message = error["message"].as_str().expect("error message string");
            (stage_id, message.to_string())
        })
        .collect();
    assert_eq!(output.status.code(), Some(1), "errors: {errors:?}");
    errors
}

fn has_error(errors: &[(String, String)], stage_id: &str, needle: &str) -> bool {
    errors
        .iter()
        .any(|(id, message)| id == stage_id && message.contains(needle))
}

#[test]
fn iv_signal_never_teaches_knowledge_writes() {
    for knowledge_tree_empty in [false, true] {
        let mut session = Session::new();
        session.assign_to_stage("stage-1".to_string());
        let mut stage = Stage::new("Verify".into(), None);
        stage.id = "stage-1".to_string();
        stage.stage_type = StageType::IntegrationVerify;
        let worktree = Worktree::new(
            "stage-1".into(),
            "/repo/.worktrees/stage-1".into(),
            "loom/stage-1".into(),
        );
        let ctx = EmbeddedContext {
            knowledge_tree_empty,
            ..Default::default()
        };

        let content =
            format_signal_with_metrics(&session, &stage, &worktree, &[], None, None, &ctx).content;

        assert!(
            content.contains("NEVER use 'loom knowledge' in this stage"),
            "knowledge_tree_empty={knowledge_tree_empty}: no prohibition line:\n{content}"
        );
        for forbidden in [
            "loom knowledge update",
            "KNOWLEDGE UPDATES REQUIRED",
            "## Knowledge Management",
        ] {
            assert!(
                !content.contains(forbidden),
                "knowledge_tree_empty={knowledge_tree_empty}: signal contains {forbidden:?}:\n{content}"
            );
        }
    }
}

#[test]
fn distill_lint_rejects_a_stage_it_never_follows() {
    let stages = [
        stage("knowledge-bootstrap", "knowledge", &[], false),
        stage("knowledge-notes", "knowledge", &[], false),
        stage("add-greeting", "standard", &["knowledge-bootstrap"], true),
        stage(
            "integration-verify",
            "integration-verify",
            &["add-greeting"],
            false,
        ),
        stage(
            "knowledge-distill",
            "knowledge-distill",
            &["integration-verify"],
            false,
        ),
    ];
    let repo = repo(&[("PLAN.md", &plan(&stages))]);

    let errors = verify(repo.path());

    let needle = "not an ancestor of knowledge-distill stage";
    assert!(
        has_error(&errors, "knowledge-notes", needle),
        "expected a distill lint on knowledge-notes: {errors:?}"
    );
    for reached in ["add-greeting", "knowledge-bootstrap"] {
        assert!(
            !has_error(&errors, reached, needle),
            "{reached} reaches knowledge-distill transitively: {errors:?}"
        );
    }
}

#[test]
fn iv_lint_rejects_an_unverified_standard_stage() {
    let stages = [
        stage("knowledge-bootstrap", "knowledge", &[], false),
        stage("add-parser", "standard", &["knowledge-bootstrap"], true),
        stage("add-greeting", "standard", &["add-parser"], true),
        stage("add-farewell", "standard", &["knowledge-bootstrap"], true),
        stage(
            "integration-verify",
            "integration-verify",
            &["add-greeting"],
            false,
        ),
        stage(
            "knowledge-distill",
            "knowledge-distill",
            &["integration-verify", "add-farewell"],
            false,
        ),
    ];
    let repo = repo(&[("PLAN.md", &plan(&stages))]);

    let errors = verify(repo.path());

    let needle = "not an ancestor of any integration-verify stage";
    assert!(
        has_error(&errors, "add-farewell", needle),
        "expected an integration-verify lint on add-farewell: {errors:?}"
    );
    for verified in ["add-parser", "add-greeting"] {
        assert!(
            !has_error(&errors, verified, needle),
            "{verified} reaches integration-verify: {errors:?}"
        );
    }
}

#[test]
fn bootstrap_warning_silent_for_a_populated_tree() {
    let temp = TempDir::new().expect("create repo dir");
    write_file(
        temp.path(),
        "doc/loom/knowledge/architecture.md",
        "# Architecture\n\n## Module map\n\n- src/main.rs - entry point\n",
    );
    let stage = StageDefinition {
        id: "feature".into(),
        name: "Feature".into(),
        working_dir: ".".into(),
        ..Default::default()
    };

    let warnings = check_knowledge_recommendations(&[stage], Some(temp.path()));

    assert!(
        warnings.is_empty(),
        "a populated knowledge tree needs no bootstrap warning: {warnings:?}"
    );
}

fn v1_plan_with_model(model: &str) -> String {
    format!(
        "loom:\n  version: 1\n  stages:\n    - id: s1\n      name: \"First Stage\"\n      working_dir: \".\"\n      artifacts: [\"README.md\"]\n      model: \"{model}\"\n"
    )
}

#[test]
fn stage_model_outside_the_allowlist_is_rejected() {
    let rejected = loom::yaml::from_str::<LoomMetadata>(&v1_plan_with_model("claude-opus-5"));
    let err = rejected.expect_err("model claude-opus-5 is not an allowed alias");
    assert!(
        err.to_string().contains("invalid model"),
        "error does not name the invalid model: {err}"
    );

    let accepted = loom::yaml::from_str::<LoomMetadata>(&v1_plan_with_model("sonnet"))
        .expect("model sonnet is an allowed alias");
    assert_eq!(accepted.loom.stages[0].model.as_deref(), Some("sonnet"));
}

#[test]
fn archive_failure_with_journals_is_an_error() {
    let temp = TempDir::new().expect("create root");
    let root = temp.path();
    write_file(root, ".loom/work/memory/stage-a.md", "journal");
    fs::create_dir_all(root.join(".loom/memory")).expect("create memory dir");
    write_file(root, ".loom/memory/archive", "not a directory");

    let result =
        loom::fs::memory::archive_run_state(&root.join(".loom/work"), root, Some("plan-a"));

    let err = result.expect_err("an archive that cannot be created is an error");
    let archive = root.join(".loom/memory/archive").display().to_string();
    assert!(
        format!("{err:#}").contains(&archive),
        "error does not name {archive}: {err:#}"
    );
    assert!(
        root.join(".loom/work/memory/stage-a.md").exists(),
        "the journal must survive a failed archive"
    );
}
