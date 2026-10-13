//! Contracts for the `plan-environment` stage: the provision executor, the
//! provision schema check, and the plan-time environment lints that
//! `loom plan verify` reports.

#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::{Command, Output};

use loom::orchestrator::provision::run_provision;
use loom::plan::schema::{validate, LoomMetadata, ProvisionEntry};
use serde_json::Value;
use tempfile::TempDir;

fn entry(working_dir: &str, command: &str) -> ProvisionEntry {
    ProvisionEntry {
        working_dir: working_dir.to_string(),
        command: command.to_string(),
    }
}

/// A scratch worktree holding an empty `web/` directory.
fn worktree_with_web() -> TempDir {
    let worktree = TempDir::new().expect("create worktree");
    fs::create_dir(worktree.path().join("web")).expect("create web dir");
    worktree
}

#[test]
fn provision_runs_in_its_working_dir() {
    let worktree = worktree_with_web();

    let result = run_provision(&[entry("web", "pwd > provisioned.txt")], worktree.path());

    assert_eq!(result, Ok(()));
    let web = worktree.path().join("web");
    let written = fs::read_to_string(web.join("provisioned.txt"))
        .expect("the command ran in web/ and wrote provisioned.txt there");
    let ran_in = Path::new(written.trim())
        .canonicalize()
        .expect("pwd printed an existing directory");
    assert_eq!(ran_in, web.canonicalize().expect("canonical web dir"));
}

#[test]
fn failed_provision_names_command_and_dir() {
    let worktree = worktree_with_web();
    let failing = "echo registry refused >&2; exit 7";

    let result = run_provision(
        &[entry("web", failing), entry("web", "touch ran-after")],
        worktree.path(),
    );

    let reason = result.expect_err("a non-zero exit is an Err");
    let prefix = format!("provision `{failing}` in `web` failed: ");
    assert!(reason.starts_with(&prefix), "reason: {reason}");
    assert!(reason.contains("registry refused"), "reason: {reason}");
    assert!(
        !worktree.path().join("web").join("ran-after").exists(),
        "an entry after the failed one ran"
    );
}

#[test]
fn provision_refuses_a_symlinked_escape() {
    let worktree = TempDir::new().expect("create worktree");
    let outside = TempDir::new().expect("create outside dir");
    symlink(outside.path(), worktree.path().join("web")).expect("symlink web outside");

    let result = run_provision(&[entry("web", "touch escaped")], worktree.path());

    assert!(
        result.is_err(),
        "a symlinked escape must be refused: {result:?}"
    );
    assert!(
        !outside.path().join("escaped").exists(),
        "the command ran outside the worktree"
    );
}

/// A version 2 plan with one standard stage carrying one contract, and one
/// provision entry in `working_dir`.
fn v2_metadata_with_provision(working_dir: &str) -> LoomMetadata {
    let yaml = format!(
        r#"loom:
  version: 2
  provision:
    - working_dir: "{working_dir}"
      command: "true"
  stages:
    - id: stage-1
      name: "Stage One"
      stage_type: standard
      working_dir: "."
      dependencies: []
      acceptance:
        - "cargo test"
      contracts:
        - id: parses-v2-plan
          file: "tests/plan_v2.rs"
          test: "parses_v2_plan"
          runner: cargo-test
          scenario: "a plan file declaring version 2 is parsed"
          rejects: "a parser that still accepts only version 1"
"#
    );
    loom::yaml::from_str(&yaml).expect("the plan with provision parses")
}

fn provision_errors(working_dir: &str) -> Vec<String> {
    match validate(&v2_metadata_with_provision(working_dir)) {
        Ok(()) => Vec::new(),
        Err(errors) => errors
            .into_iter()
            .map(|error| error.message)
            .filter(|message| message.contains("provision"))
            .collect(),
    }
}

#[test]
fn provision_working_dir_cannot_escape() {
    assert_eq!(
        provision_errors("web"),
        Vec::<String>::new(),
        "a relative working_dir inside the worktree is valid"
    );
    for escaping in ["../elsewhere", "/abs/web"] {
        assert!(
            !provision_errors(escaping).is_empty(),
            "working_dir {escaping:?} must be a provision error"
        );
    }
}

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

/// The `v2-valid.md` fixture plan, every stage with a summary, `loom_extra`
/// spliced in under `loom:` and `extra_acceptance` appended to the standard
/// stage's acceptance.
fn plan(loom_extra: &str, extra_acceptance: &str) -> String {
    format!(
        r#"# PLAN: Environment Contracts

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
{loom_extra}  stages:
    - id: knowledge-bootstrap
      name: "Knowledge bootstrap"
      stage_type: knowledge
      working_dir: "."
      dependencies: []
      summary: "Re-verify the knowledge topics."
      description: "Re-verify the knowledge topics the later stages are briefed from."
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"

    - id: add-greeting
      name: "Add a greeting"
      stage_type: standard
      working_dir: "."
      dependencies: ["knowledge-bootstrap"]
      summary: "Greet a user by name."
      description: "Greet a user by name."
      contracts:
        - id: greets-by-name
          file: "loom/tests/greeting.rs"
          test: "greets_by_name"
          runner: cargo-test
          scenario: "a greeting is built for the user named Ada"
          rejects: "a greeting that ignores the name and always says hello world"
      acceptance:
        - "cargo test --manifest-path loom/Cargo.toml --test greeting"
{extra_acceptance}
    - id: integration-verify
      name: "Integration verification"
      stage_type: integration-verify
      working_dir: "."
      dependencies: ["add-greeting"]
      summary: "Verify the merged tree."
      description: "Verify the merged tree."
      acceptance:
        - "cargo test --manifest-path loom/Cargo.toml --all-targets"

    - id: knowledge-distill
      name: "Knowledge distillation"
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      summary: "Curate the stage memories."
      description: "Curate the stage memories into knowledge."
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
```

<!-- END loom METADATA -->
"#
    )
}

fn sandbox_domains(domains: &str) -> String {
    format!("  sandbox:\n    network:\n      allowed_domains: [{domains}]\n")
}

/// Run `loom plan verify --json PLAN.md` from `root` and return the messages
/// of its `errors`, checking the exit status agrees with them.
fn verify_errors(root: &Path) -> Vec<String> {
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
    let messages: Vec<String> = json["errors"]
        .as_array()
        .expect("errors array")
        .iter()
        .map(|error| {
            error["message"]
                .as_str()
                .expect("error message string")
                .to_string()
        })
        .collect();
    if !messages.is_empty() {
        assert_eq!(output.status.code(), Some(1), "errors: {messages:?}");
    }
    messages
}

#[test]
fn js_package_without_provision_is_an_error() {
    let repo = repo(&[
        (
            "web/package.json",
            r#"{"devDependencies":{"vitest":"^3.2.0"}}"#,
        ),
        ("PLAN.md", &plan("", "")),
    ]);

    let errors = verify_errors(repo.path());
    assert!(
        errors
            .iter()
            .any(|m| m.contains("`web`") && m.contains("provision")),
        "expected a JS package lint naming `web`: {errors:?}"
    );

    let provision =
        "  provision:\n    - working_dir: web\n      command: \"bun install --frozen-lockfile\"\n";
    write_file(repo.path(), "PLAN.md", &plan(provision, ""));

    let errors = verify_errors(repo.path());
    assert!(
        !errors.iter().any(|m| m.contains("package `web`")
            || (m.contains("`web`")
                && m.contains("provision")
                && !m.contains("registry.npmjs.org"))),
        "a provision entry for web must clear the JS package lint: {errors:?}"
    );
}

#[test]
fn registry_tool_without_registry_domain_is_an_error() {
    let bunx = "        - \"bunx tsc --noEmit\"\n";
    let repo = repo(&[("PLAN.md", &plan(&sandbox_domains("\"crates.io\""), bunx))]);

    let errors = verify_errors(repo.path());
    assert!(
        errors.iter().any(|m| m.contains("registry.npmjs.org")),
        "expected a registry lint naming registry.npmjs.org: {errors:?}"
    );

    let wildcard = sandbox_domains("\"crates.io\", \"*.npmjs.org\"");
    write_file(repo.path(), "PLAN.md", &plan(&wildcard, bunx));

    let errors = verify_errors(repo.path());
    assert!(
        !errors.iter().any(|m| m.contains("registry.npmjs.org")),
        "*.npmjs.org covers registry.npmjs.org: {errors:?}"
    );
}

#[test]
fn pre_commit_hook_registry_need_is_an_error() {
    let hook =
        "#!/bin/sh\ngit ls-files -z -- '*.md' | xargs -0 bunx markdownlint-cli2 --fix || true\n";
    let repo = repo(&[
        ("hooks/pre-commit", hook),
        ("PLAN.md", &plan(&sandbox_domains("\"crates.io\""), "")),
    ]);
    let hook_path = repo.path().join("hooks").join("pre-commit");
    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).expect("chmod hook");
    git(repo.path(), &["config", "core.hooksPath", "hooks"]);

    let errors = verify_errors(repo.path());
    assert!(
        errors
            .iter()
            .any(|m| m.contains("pre-commit") && m.contains("registry.npmjs.org")),
        "expected a hook lint naming pre-commit and registry.npmjs.org: {errors:?}"
    );
}
