//! An update notice must never leak into `--json` stdout.

use std::fs;
use tempfile::TempDir;

use super::helpers::loom_cmd;

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

#[test]
fn test_json_stdout_stays_pure_when_an_update_notice_is_pending() {
    // The invariant: an update notice always goes to stderr
    // (`update_check::notify_and_maybe_refresh`'s `eprintln!`), so `--json`
    // stdout stays pure JSON even when a newer release is on record.
    // `loom_cmd()`'s shared scratch `LOOM_HOME` opts out of the check
    // entirely (`check = false`) so the notice never fires for every other
    // test in this suite; this test deliberately opts back in with its own
    // scratch home naming a far-future version. Its `last_checked` stamp is
    // "now" so `decide()` never schedules a detached refresh fetch either —
    // that would be a real network spawn this test must not trigger.
    let loom_home = TempDir::new().unwrap();
    let state = format!(
        r#"{{"last_checked":"{}","latest_version":"99.0.0"}}"#,
        chrono::Utc::now().to_rfc3339()
    );
    fs::write(loom_home.path().join("update-state.json"), state).unwrap();

    let temp = TempDir::new().unwrap();
    let plan = temp.path().join("PLAN-update-notice.md");
    fs::write(&plan, minimal_valid_plan("Update Notice Plan")).unwrap();

    let out = loom_cmd()
        .env("LOOM_HOME", loom_home.path())
        .args(["plan", "verify", "--json"])
        .arg(&plan)
        .output()
        .expect("failed to run loom plan verify");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    serde_json::from_str::<serde_json::Value>(&stdout)
        .expect("stdout must be pure JSON even with a pending update notice");
    assert!(!stdout.contains("self-update"), "stdout: {stdout}");
    assert!(!stdout.contains("newer version"), "stdout: {stdout}");

    // Prove the notice actually fired, so the assertions above are not
    // vacuously true because the notice never ran at all.
    assert!(
        stderr.contains("self-update"),
        "expected the update notice on stderr, got: {stderr}"
    );
}
