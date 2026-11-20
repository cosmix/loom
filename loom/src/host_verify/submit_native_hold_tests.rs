//! A native Codex worker holding the stage refuses its host jobs.
use super::*;
use crate::native_worker::model::{NativeManifest, NATIVE_DEADLINE_SECS};
use crate::native_worker::store::create_invocation;

pub(crate) struct ProcessGuard(pub(crate) u32);

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = crate::process::terminate(self.0);
    }
}

pub(crate) fn seed(work: &Path, stage: &str) -> std::path::PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: stage.into(),
        unit_id: "worker".into(),
        invocation_id: "inv-0123456789abcdef0123456789abcdef".into(),
        owner_session_id: "another-session".into(),
        workspace_root: work.to_path_buf(),
        brief_path: work.join("brief.md"),
        brief_sha256: crate::utils::sha256_hex(b"brief"),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc::now(),
    };
    create_invocation(work, &manifest, b"brief").unwrap()
}

#[test]
fn submit_refuses_while_a_native_worker_holds_the_stage() {
    use crate::host_verify::tests_submit::{request, state_root, HEAD_A, STAGE};
    let (_dir, work) = state_root();
    seed(&work, STAGE);
    let result = submit(&work, &request("r1", HEAD_A, false), Utc::now()).unwrap();
    assert!(
        matches!(result, Submission::Refused(text) if text.contains("running native Codex workers"))
    );
    assert!(store::load_current(&work, STAGE).unwrap().is_none());
}

#[test]
#[serial_test::serial]
fn a_passing_run_is_rejected_while_a_native_worker_holds_the_stage() {
    use crate::host_verify::apply::Applied;
    use crate::host_verify::tests_complete_support::{Fixture, STAGE};
    use crate::models::stage::StageType;
    let fx = Fixture::new(StageType::Standard, 1);
    let record = fx.passed();
    seed(&fx.work_dir, STAGE);
    assert!(matches!(fx.apply(&record), Ok(Applied::Recorded)));
    fx.assert_rejected(&record);
}
