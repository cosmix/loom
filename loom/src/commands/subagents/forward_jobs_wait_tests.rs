use std::fs;

use serial_test::serial;

use super::*;
use crate::models::forward_receipt::{ForwardBackend, ForwardIdentity, ForwardObservation};

#[test]
fn wait_succeeded_receipt_exits_zero() {
    let fixture = WaitFixture::new("completed", Some("done"));
    let output = wait_until_with_roots(
        &fixture.work,
        Some("stage-a"),
        &fixture.id,
        0,
        std::slice::from_ref(&fixture.state),
    );
    assert_eq!(
        (output.state, wait_exit_code(output.state)),
        (ForwardState::Succeeded, 0)
    );
}

#[test]
fn wait_failed_receipt_exits_one() {
    let fixture = WaitFixture::new("failed", None);
    let output = wait_until_with_roots(
        &fixture.work,
        Some("stage-a"),
        &fixture.id,
        0,
        std::slice::from_ref(&fixture.state),
    );
    assert_eq!(
        (output.state, wait_exit_code(output.state)),
        (ForwardState::Failed, 1)
    );
}

#[test]
fn wait_active_receipt_exits_two_at_deadline() {
    let fixture = WaitFixture::new("running", None);
    let output = wait_until_with_roots(
        &fixture.work,
        Some("stage-a"),
        &fixture.id,
        0,
        std::slice::from_ref(&fixture.state),
    );
    assert_eq!(
        (output.state, wait_exit_code(output.state)),
        (ForwardState::Running, 2)
    );
}

#[test]
fn public_wait_rejects_noncanonical_receipt_id() {
    let error = wait("ABCDEF".into(), 0, false).unwrap_err();

    assert_eq!(
        error.to_string(),
        "receipt must be 64 lowercase hexadecimal characters"
    );
}

#[test]
#[serial]
fn unset_stage_fallback_matches_only_the_exact_receipt() {
    let fixture = WaitFixture::new("completed", Some("done"));
    fixture.write_unrelated_receipt();
    let _stage = EnvVarGuard::unset("LOOM_STAGE_ID");

    let output = wait_until_with_roots(
        &fixture.work,
        None,
        &fixture.id,
        0,
        std::slice::from_ref(&fixture.state),
    );

    assert_eq!(
        wait_stage(&fixture.work, &fixture.id),
        Some("stage-a".into())
    );
    assert_eq!(output.receipt_id, fixture.id);
    assert_eq!(output.backend_id.as_deref(), Some("job-a"));
    assert_eq!(output.state, ForwardState::Succeeded);
}

#[test]
fn wait_output_contains_only_receipt_backend_and_state() {
    let output = WaitOutput {
        receipt_id: "a".repeat(64),
        backend_id: Some("job-a".into()),
        state: ForwardState::Running,
    };

    assert_eq!(
        format_wait_output(&output, false).unwrap(),
        format!("{} job-a running", output.receipt_id)
    );
    assert_eq!(
        format_wait_output(&output, true).unwrap(),
        format!(
            r#"{{"receipt_id":"{}","backend_id":"job-a","state":"running"}}"#,
            output.receipt_id
        )
    );
}

struct WaitFixture {
    _temp: tempfile::TempDir,
    work: std::path::PathBuf,
    state: std::path::PathBuf,
    id: String,
}

impl WaitFixture {
    fn new(status: &str, phase: Option<&str>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let work = root.join("work");
        let state = root.join("state");
        let locator = state.join("workspace/jobs/job-a.json");
        fs::create_dir_all(locator.parent().unwrap()).unwrap();
        fs::create_dir_all(work.join("subagents/stage-a")).unwrap();
        fs::write(
            &locator,
            serde_json::json!({"id":"job-a","status":status,"phase":phase}).to_string(),
        )
        .unwrap();
        let identity =
            ForwardIdentity::new("parent-a", "agent-a", "tool-a", "stage-a", "loom-a").unwrap();
        let observation = ForwardObservation {
            schema: 1,
            receipt_id: identity.receipt_id(),
            parent_session_id: "parent-a".into(),
            agent_id: "agent-a".into(),
            tool_use_id: "tool-a".into(),
            stage_id: "stage-a".into(),
            loom_session_id: "loom-a".into(),
            backend: ForwardBackend::Companion,
            backend_id: "job-a".into(),
            state: ForwardState::Queued,
            observed_at: chrono::Utc::now(),
            exit_code: None,
            codex_thread_id: None,
            locator: Some(locator.display().to_string()),
            model: None,
            effort: None,
        };
        let id = observation.receipt_id.clone();
        fs::write(
            receipts_path(&work, "stage-a").unwrap(),
            format!("{}\n", observation.encode_line().unwrap()),
        )
        .unwrap();
        Self {
            _temp: temp,
            work,
            state,
            id,
        }
    }

    fn write_unrelated_receipt(&self) {
        let identity =
            ForwardIdentity::new("parent-b", "agent-b", "tool-b", "other-stage", "loom-b").unwrap();
        let observation = ForwardObservation {
            schema: 1,
            receipt_id: identity.receipt_id(),
            parent_session_id: "parent-b".into(),
            agent_id: "agent-b".into(),
            tool_use_id: "tool-b".into(),
            stage_id: "other-stage".into(),
            loom_session_id: "loom-b".into(),
            backend: ForwardBackend::Direct,
            backend_id: "thread-b".into(),
            state: ForwardState::Queued,
            observed_at: chrono::Utc::now(),
            exit_code: None,
            codex_thread_id: Some("thread-b".into()),
            locator: None,
            model: None,
            effort: None,
        };
        let path = receipts_path(&self.work, "other-stage").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("{}\n", observation.encode_line().unwrap())).unwrap();
    }
}

struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn unset(key: &'static str) -> Self {
        let original = std::env::var_os(key);
        // SAFETY: this serialized test temporarily owns and restores this variable.
        unsafe { std::env::remove_var(key) };
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            // SAFETY: restore the value saved before this serialized test ran.
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            // SAFETY: restore the original absence before allowing another test to run.
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}
