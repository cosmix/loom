//! An update notice must never leak into `--json` stdout: a dev build never
//! prints one at all, and a release build prints it on stderr only.

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

/// Runs `loom plan verify --json` with a far-future release on record and
/// returns `(stdout, stderr)`. `loom_cmd()`'s shared scratch `LOOM_HOME` opts
/// out of the update check (`check = false`), so this gives the process its
/// own scratch home to opt back in. Its `last_checked` stamp is "now" so
/// `decide()` never schedules a detached refresh fetch — a real network spawn
/// this test must not trigger.
fn verify_json_with_newer_release_on_record() -> (String, String) {
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

    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn test_update_notice_stays_off_json_stdout_and_dev_builds_print_none() {
    let (stdout, stderr) = verify_json_with_newer_release_on_record();

    serde_json::from_str::<serde_json::Value>(&stdout)
        .expect("stdout must be pure JSON even with a newer release on record");
    assert!(!stdout.contains("loom update"), "stdout: {stdout}");

    let version =
        semver::Version::parse(loom::version::VERSION).expect("LOOM_VERSION must be semver");

    // `build.rs` stamps a tagged HEAD with its bare release version and every
    // other commit with a `-dev` prerelease, so this suite meets a release
    // build exactly when a release tag is pushed (the tag push's own
    // pre-push hook, and `release.yml`'s test job); `update_check::decide`
    // exempts only dev builds from the notice. The dev exemption itself is
    // pinned build-independently by the unit test
    // `update_check::tests::dev_build_is_never_notified_and_never_refreshes`.
    if version.pre.is_empty() {
        assert!(
            stderr.contains("(latest 99.0.0)") && stderr.contains("loom update"),
            "a release build must print the update notice on stderr, got: {stderr}"
        );
    } else {
        assert!(
            !stderr.contains("loom update"),
            "a dev build must not print the update notice, got: {stderr}"
        );
    }
}
