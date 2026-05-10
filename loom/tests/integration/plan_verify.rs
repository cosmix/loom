//! Integration tests for `loom plan verify`

use std::fs;
use std::process::Command;
use tempfile::TempDir;

const LOOM: &str = env!("CARGO_BIN_EXE_loom");

// ── Fixtures ──────────────────────────────────────────────────────────────

/// A minimal valid plan (standard stage with acceptance, no artifacts).
fn minimal_valid_plan(name: &str) -> String {
    format!(
        r#"# {name}

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-one
      name: "Stage One"
      stage_type: standard
      working_dir: "."
      acceptance:
        - "true"
```

<!-- END loom METADATA -->
"#
    )
}

/// Plan with version 2 (unsupported).
fn invalid_version_plan() -> &'static str {
    r#"# Bad Version Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  stages:
    - id: stage-one
      name: "Stage One"
      working_dir: "."
      acceptance:
        - "true"
```

<!-- END loom METADATA -->
"#
}

/// Plan where a stage depends on itself.
fn self_dependency_plan() -> &'static str {
    r#"# Self Dep Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-one
      name: "Stage One"
      working_dir: "."
      acceptance:
        - "true"
      dependencies:
        - stage-one
```

<!-- END loom METADATA -->
"#
}

/// Two stages that depend on each other (cycle).
fn dag_cycle_plan() -> &'static str {
    r#"# Cycle Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-a
      name: "Stage A"
      working_dir: "."
      acceptance:
        - "true"
      dependencies:
        - stage-b
    - id: stage-b
      name: "Stage B"
      working_dir: "."
      acceptance:
        - "true"
      dependencies:
        - stage-a
```

<!-- END loom METADATA -->
"#
}

/// Plan that triggers a structural warning (redundant working_dir prefix).
fn structural_warning_plan() -> &'static str {
    r#"# Warning Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-one
      name: "Stage One"
      stage_type: standard
      working_dir: "loom"
      acceptance:
        - "loom/target/debug/loom --help"
```

<!-- END loom METADATA -->
"#
}

/// Plan with malformed YAML inside the metadata block (triggers serde_yaml::Error).
fn malformed_yaml_plan() -> &'static str {
    r#"# Malformed YAML Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages: [unbalanced
```

<!-- END loom METADATA -->
"#
}

/// Plan that triggers a knowledge-recommendation warning (no knowledge-bootstrap stage).
/// Same shape as `minimal_valid_plan` — emits the "Consider adding a 'knowledge-bootstrap'
/// stage" warning under the knowledge category.
fn knowledge_warning_plan() -> &'static str {
    r#"# Knowledge Warning Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-one
      name: "Stage One"
      stage_type: standard
      working_dir: "."
      acceptance:
        - "true"
```

<!-- END loom METADATA -->
"#
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn write_plan(dir: &std::path::Path, filename: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(filename);
    fs::write(&path, content).unwrap();
    path
}

fn run_verify(plan_path: &std::path::Path, extra_args: &[&str]) -> std::process::Output {
    Command::new(LOOM)
        .args(["plan", "verify"])
        .arg(plan_path)
        .args(extra_args)
        .output()
        .expect("failed to run loom plan verify")
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[test]
fn test_valid_plan_exits_zero() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(
        temp.path(),
        "PLAN-test.md",
        &minimal_valid_plan("My Test Plan"),
    );
    let out = run_verify(&plan, &[]);
    assert!(
        out.status.success(),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("0 error(s)"), "stdout: {stdout}");
    assert!(stdout.contains("My Test Plan"), "stdout: {stdout}");
}

#[test]
fn test_invalid_version() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-bad.md", invalid_version_plan());
    let out = run_verify(&plan, &[]);
    assert!(!out.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("Unsupported version") || combined.contains("unsupported version"),
        "combined output: {combined}"
    );
}

#[test]
fn test_self_dependency() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-self.md", self_dependency_plan());
    let out = run_verify(&plan, &[]);
    assert!(!out.status.success());
}

#[test]
fn test_dag_cycle() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-cycle.md", dag_cycle_plan());
    let out = run_verify(&plan, &[]);
    assert!(!out.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.to_lowercase().contains("circular") || combined.to_lowercase().contains("cycle"),
        "combined output: {combined}"
    );
}

#[test]
fn test_structural_warning_not_strict() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-warn.md", structural_warning_plan());
    let out = run_verify(&plan, &[]);
    // Exit 0 in non-strict mode even with warnings
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("redundant working_dir prefix"),
        "stdout: {stdout}"
    );
}

#[test]
fn test_structural_warning_strict() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-warn.md", structural_warning_plan());
    let out = run_verify(&plan, &["--strict"]);
    assert!(!out.status.success(), "expected exit 1 in strict mode");
}

#[test]
fn test_json_valid() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(
        temp.path(),
        "PLAN-json.md",
        &minimal_valid_plan("JSON Plan"),
    );
    let out = run_verify(&plan, &["--json"]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(val["valid"], serde_json::Value::Bool(true));
    assert!(val["plan"]["id"].is_string(), "plan.id must be a string");
    assert!(val["errors"].as_array().unwrap().is_empty());
    assert!(val["levels"].is_array());
}

#[test]
fn test_json_missing_path() {
    let out = Command::new(LOOM)
        .args(["plan", "verify", "--json", "/nonexistent/plan.md"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(val["valid"], serde_json::Value::Bool(false));
    assert!(!val["errors"].as_array().unwrap().is_empty());
    assert!(val["plan"]["id"].is_null(), "plan.id should be null");
    assert!(val["plan"]["name"].is_null(), "plan.name should be null");
}

#[test]
fn test_json_oversized_plan() {
    let temp = TempDir::new().unwrap();
    let oversized = temp.path().join("PLAN-big.md");
    // 1 MiB + 1 byte
    let data = vec![b'x'; 1_048_577];
    fs::write(&oversized, &data).unwrap();

    // Human mode
    let out_human = run_verify(&oversized, &[]);
    assert!(!out_human.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out_human.stdout),
        String::from_utf8_lossy(&out_human.stderr)
    );
    assert!(
        combined.contains("too large") || combined.contains("limit"),
        "human output: {combined}"
    );

    // JSON mode
    let out_json = run_verify(&oversized, &["--json"]);
    assert!(!out_json.status.success());
    let stdout = String::from_utf8_lossy(&out_json.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("JSON must be valid");
    assert_eq!(val["valid"], serde_json::Value::Bool(false));
    let err_msg = val["errors"][0]["message"].as_str().unwrap_or("");
    assert!(
        err_msg.contains("too large") || err_msg.contains("limit"),
        "error message: {err_msg}"
    );
}

#[test]
fn test_human_missing_path() {
    let out = Command::new(LOOM)
        .args(["plan", "verify", "/nonexistent/plan.md"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("Plan file"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_no_color_strips_ansi() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(
        temp.path(),
        "PLAN-color.md",
        &minimal_valid_plan("Color Test"),
    );

    // (a) CLICOLOR_FORCE=1 without --no-color: stdout should contain ANSI codes
    let out_colored = Command::new(LOOM)
        .args(["plan", "verify"])
        .arg(&plan)
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    let stdout_colored = String::from_utf8_lossy(&out_colored.stdout);
    assert!(
        stdout_colored.contains('\x1b'),
        "CLICOLOR_FORCE=1 should produce ANSI codes, got: {stdout_colored:?}"
    );

    // (b) CLICOLOR_FORCE=1 WITH --no-color: stdout must contain no ANSI codes
    let out_plain = Command::new(LOOM)
        .args(["plan", "verify", "--no-color"])
        .arg(&plan)
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();
    let stdout_plain = String::from_utf8_lossy(&out_plain.stdout);
    assert!(
        !stdout_plain.contains('\x1b'),
        "--no-color should strip ANSI codes, got: {stdout_plain:?}"
    );
}

#[test]
fn test_no_side_effects_on_target_repo() {
    use std::collections::BTreeSet;

    // Set up the plan's git repo in its own TempDir
    let plan_repo = TempDir::new().unwrap();
    let plan_root = plan_repo.path();

    // Init git repo
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(plan_root)
        .status()
        .unwrap();

    // Write a valid plan into the repo
    let plan_path = plan_root.join("PLAN-side.md");
    fs::write(&plan_path, minimal_valid_plan("Side Effect Test")).unwrap();

    // Snapshot before
    let before: BTreeSet<String> = snapshot_dir(plan_root);

    // Use a separate cwd to prove we don't pollute the caller's dir
    let caller_cwd = TempDir::new().unwrap();

    Command::new(LOOM)
        .args(["plan", "verify"])
        .arg(&plan_path)
        .current_dir(caller_cwd.path())
        .output()
        .unwrap();

    // Snapshot after
    let after: BTreeSet<String> = snapshot_dir(plan_root);

    assert_eq!(before, after, "plan repo must be unchanged after verify");

    // No .work/ in caller's cwd
    assert!(
        !caller_cwd.path().join(".work").exists(),
        ".work/ must not be created in caller's cwd"
    );
}

/// Recursive directory listing relative to root, sorted.
fn snapshot_dir(root: &std::path::Path) -> std::collections::BTreeSet<String> {
    let mut entries = std::collections::BTreeSet::new();
    collect_entries(root, root, &mut entries);
    entries
}

fn collect_entries(
    base: &std::path::Path,
    dir: &std::path::Path,
    out: &mut std::collections::BTreeSet<String>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let rel = path
            .strip_prefix(base)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        // Skip .git internals to avoid flakiness
        if rel.starts_with(".git") {
            continue;
        }
        out.insert(rel);
        if path.is_dir() {
            collect_entries(base, &path, out);
        }
    }
}

#[test]
fn test_malformed_yaml_in_metadata_block() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-bad-yaml.md", malformed_yaml_plan());

    // Human mode: exit 1 with a YAML parse message on stderr
    let out = run_verify(&plan, &[]);
    assert!(!out.status.success(), "expected exit 1 for malformed YAML");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("YAML parse error") || combined.to_lowercase().contains("yaml"),
        "expected yaml parse error in output, got: {combined}"
    );

    // JSON mode: well-formed JSON envelope with plan.id null and at least one error
    let out_json = run_verify(&plan, &["--json"]);
    assert!(!out_json.status.success());
    let stdout = String::from_utf8_lossy(&out_json.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(val["valid"], serde_json::Value::Bool(false));
    assert!(
        val["plan"]["id"].is_null(),
        "plan.id should be null on yaml-parse failure"
    );
    let errs = val["errors"].as_array().expect("errors must be an array");
    assert!(!errs.is_empty(), "errors must be non-empty");
    let first_msg = errs[0]["message"].as_str().unwrap_or("");
    assert!(
        first_msg.contains("YAML") || first_msg.to_lowercase().contains("yaml"),
        "first error should mention YAML, got: {first_msg}"
    );
}

#[test]
fn test_json_schema_completeness_and_levels() {
    // Locks the wire shape so a future refactor that renames a key or changes
    // a type fails loudly here instead of silently breaking JSON consumers.
    let temp = TempDir::new().unwrap();
    let plan = write_plan(
        temp.path(),
        "PLAN-schema.md",
        &minimal_valid_plan("Schema Plan"),
    );
    let out = run_verify(&plan, &["--json"]);
    assert!(out.status.success(), "expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");

    // Top-level keys
    assert_eq!(val["valid"], serde_json::Value::Bool(true));
    assert!(val["errors"].is_array());
    assert!(val["plan"].is_object());
    assert!(val["warnings"].is_object());
    assert!(val["levels"].is_array());

    // plan.{id,name,source} all present and typed
    assert!(val["plan"]["id"].is_string(), "plan.id must be a string");
    assert!(
        val["plan"]["name"].is_string(),
        "plan.name must be a string"
    );
    assert!(
        val["plan"]["source"].is_string(),
        "plan.source must be a string"
    );
    assert_eq!(val["plan"]["id"], "PLAN-schema");
    assert_eq!(val["plan"]["name"], "Schema Plan");

    // warnings.{structural,knowledge,sandbox} are all arrays (may be empty)
    assert!(val["warnings"]["structural"].is_array());
    assert!(val["warnings"]["knowledge"].is_array());
    assert!(val["warnings"]["sandbox"].is_array());

    // levels is non-empty and the lone stage lands at level 0 with the correct shape
    let levels = val["levels"].as_array().unwrap();
    assert!(!levels.is_empty(), "levels must contain at least one tier");
    let level0 = levels[0].as_array().expect("level 0 must be an array");
    assert_eq!(
        level0.len(),
        1,
        "minimal plan should yield one stage at level 0"
    );
    let stage = &level0[0];
    assert_eq!(stage["id"], "stage-one");
    assert_eq!(stage["name"], "Stage One");
    assert_eq!(stage["stage_type"], "standard");
    assert!(stage["dependencies"].is_array());
    assert!(stage["dependencies"].as_array().unwrap().is_empty());
}

#[test]
fn test_json_strict_with_warnings_exits_one_but_valid_true() {
    // The JSON contract: `valid` reflects schema correctness (no errors), while
    // exit code reflects strictness. A plan with warnings under --strict must
    // exit 1 yet keep valid=true so consumers can tell warnings from errors.
    let temp = TempDir::new().unwrap();
    let plan = write_plan(
        temp.path(),
        "PLAN-strict-json.md",
        structural_warning_plan(),
    );
    let out = run_verify(&plan, &["--json", "--strict"]);
    assert!(
        !out.status.success(),
        "strict mode must exit 1 with warnings"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(
        val["valid"],
        serde_json::Value::Bool(true),
        "valid must remain true: warnings are not schema errors"
    );
    assert!(val["errors"].as_array().unwrap().is_empty());
    assert!(
        !val["warnings"]["structural"].as_array().unwrap().is_empty(),
        "structural warnings must be reported"
    );
}

#[test]
fn test_knowledge_warning_category_populated() {
    // A plan with no knowledge-bootstrap stage triggers the knowledge category.
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-knowledge.md", knowledge_warning_plan());
    let out = run_verify(&plan, &["--json"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    let knowledge = val["warnings"]["knowledge"]
        .as_array()
        .expect("knowledge category must be an array");
    assert!(
        !knowledge.is_empty(),
        "knowledge category must contain at least one warning"
    );
    let joined: String = knowledge
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("knowledge-bootstrap"),
        "knowledge warning must mention 'knowledge-bootstrap', got: {joined}"
    );
}

#[test]
fn test_sandbox_warning_category_populated() {
    // Default `excluded_commands` already includes 'loom'/'git' so the
    // missing-loom warning does not fire on a minimal plan. Setting
    // `allow_unsandboxed_escape: true` is the deterministic trigger for the
    // sandbox category from `check_sandbox_recommendations`.
    let plan_content = r#"# Sandbox Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  sandbox:
    allow_unsandboxed_escape: true
  stages:
    - id: stage-one
      name: "Stage One"
      stage_type: standard
      working_dir: "."
      acceptance:
        - "true"
```

<!-- END loom METADATA -->
"#;
    let temp = TempDir::new().unwrap();
    let plan = write_plan(temp.path(), "PLAN-sandbox.md", plan_content);
    let out = run_verify(&plan, &["--json"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    let sandbox = val["warnings"]["sandbox"]
        .as_array()
        .expect("sandbox category must be an array");
    assert!(
        !sandbox.is_empty(),
        "allow_unsandboxed_escape=true must produce a sandbox warning"
    );
    let joined: String = sandbox
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        joined.contains("allow_unsandboxed_escape"),
        "sandbox warning must mention allow_unsandboxed_escape, got: {joined}"
    );
}

#[test]
fn test_json_non_utf8_file_emits_envelope() {
    // A file that passes the size check but is not valid UTF-8 must still
    // produce a well-formed JSON envelope under --json (not raw anyhow text
    // on stderr that would corrupt machine-readable consumers).
    let temp = TempDir::new().unwrap();
    let plan = temp.path().join("PLAN-binary.md");
    // Invalid UTF-8: a lone continuation byte.
    fs::write(&plan, [0xC3, 0x28, b'\n']).unwrap();

    let out = run_verify(&plan, &["--json"]);
    assert!(!out.status.success(), "expected exit 1 on read failure");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout)
        .expect("stdout must be valid JSON even when the input isn't UTF-8");
    assert_eq!(val["valid"], serde_json::Value::Bool(false));
    assert!(val["plan"]["id"].is_null());
    let errs = val["errors"].as_array().expect("errors must be array");
    assert!(!errs.is_empty(), "errors must report the read failure");
    let msg = errs[0]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("Failed to read") || msg.to_lowercase().contains("utf-8"),
        "expected read-failure message, got: {msg}"
    );
}

// ── Completion backend (proves shell completion is wired end-to-end) ──────

#[test]
fn test_complete_returns_plan_at_top_level() {
    // The dynamic backend powers the generated zsh script. Verify `loom`
    // appears as a completion target for `plan`.
    let temp = TempDir::new().unwrap();
    let out = Command::new(LOOM)
        .args(["complete", "zsh"])
        .arg(temp.path())
        .args(["loom ", "", "loom"])
        .output()
        .expect("failed to run loom complete");
    assert!(
        out.status.success(),
        "loom complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.contains(&"plan"),
        "expected 'plan' among completions, got: {stdout:?}"
    );
}

#[test]
fn test_complete_returns_verify_under_plan() {
    // Same backend, one level deeper: `loom plan ` must offer `verify`.
    let temp = TempDir::new().unwrap();
    let out = Command::new(LOOM)
        .args(["complete", "zsh"])
        .arg(temp.path())
        .args(["loom plan ", "", "plan"])
        .output()
        .expect("failed to run loom complete");
    assert!(
        out.status.success(),
        "loom complete failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.contains(&"verify"),
        "expected 'verify' as a subcommand, got: {stdout:?}"
    );
}

#[test]
fn test_no_metadata_block() {
    let temp = TempDir::new().unwrap();
    let plan = write_plan(
        temp.path(),
        "PLAN-nometa.md",
        "# No Metadata Plan\n\nJust a markdown file with no loom metadata block.\n",
    );

    // Human mode: exit 1
    let out = run_verify(&plan, &[]);
    assert!(!out.status.success());

    // JSON mode: plan.id is null, errors describe the missing block
    let out_json = run_verify(&plan, &["--json"]);
    assert!(!out_json.status.success());
    let stdout = String::from_utf8_lossy(&out_json.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("JSON must be valid");
    assert!(val["plan"]["id"].is_null(), "plan.id should be null");
    assert!(!val["errors"].as_array().unwrap().is_empty());
}
