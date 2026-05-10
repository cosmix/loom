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
