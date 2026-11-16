//! Contract tests for the native worker stage gate (common.md section 14): a stage with a
//! running worker, or one whose processes could not be proven stopped, refuses; a stage with no
//! worker, or whose workers all finished and released ownership, passes.

use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};

use super::native_gate::ensure_no_native_workers;
use crate::native_worker::model::{NativeEvent, NativeManifest, Outcome};
use crate::native_worker::store::{create_invocation, EventLog};

const GATED: &str = "gated-stage";
const RELEASED: &str = "released-stage";
const GATED_UNIT: &str = "gate-unit-1";
const RELEASED_UNIT: &str = "done-unit-2";
const GATED_INV: &str = "inv-0123456789abcdef0123456789abcdef";
const RELEASED_INV: &str = "inv-fedcba9876543210fedcba9876543210";

fn manifest(work: &Path, stage: &str, unit: &str, invocation: &str) -> NativeManifest {
    NativeManifest {
        v: 1,
        stage_id: stage.to_string(),
        unit_id: unit.to_string(),
        invocation_id: invocation.to_string(),
        owner_session_id: "session-1".to_string(),
        workspace_root: work.to_path_buf(),
        brief_path: work.join("brief.md"),
        brief_sha256: hex::encode(Sha256::digest(b"brief")),
        model: "gpt-6.1-terra".to_string(),
        effort: "xhigh".to_string(),
        deadline_secs: 1500,
        created_at: Utc::now(),
    }
}

fn seed(work: &Path, stage: &str, unit: &str, invocation: &str) -> PathBuf {
    create_invocation(work, &manifest(work, stage, unit, invocation), b"brief")
        .expect("create invocation")
}

fn append(dir: &Path, events: Vec<NativeEvent>) {
    let mut log = EventLog::open(dir).expect("open event log");
    for event in events {
        log.append(event).expect("append event");
    }
}

fn succeeded() -> NativeEvent {
    NativeEvent::Terminal {
        outcome: Outcome::Succeeded,
        reason: "turn completed".to_string(),
    }
}

#[test]
fn native_gate_blocks_active_and_unverified() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let work = tmp.path();

    ensure_no_native_workers(work, GATED).expect("a stage that never had a worker passes");

    let gated = seed(work, GATED, GATED_UNIT, GATED_INV);
    let active = ensure_no_native_workers(work, GATED).expect_err("an admitted worker refuses");
    let active = format!("{active:#}");
    assert!(
        active.contains(GATED_UNIT),
        "refusal names the unit: {active}"
    );

    let unverified = NativeEvent::OwnershipUnverified {
        detail: "boundary still alive".to_string(),
    };
    append(&gated, vec![succeeded(), unverified]);
    let held = ensure_no_native_workers(work, GATED).expect_err("unverified ownership refuses");
    let held = format!("{held:#}");
    assert!(
        held.contains(&format!("loom subagents release --stage {GATED}")),
        "refusal names the release command: {held}"
    );

    let done = seed(work, RELEASED, RELEASED_UNIT, RELEASED_INV);
    let released = NativeEvent::OwnershipReleased {
        by: "supervisor".to_string(),
    };
    append(&done, vec![succeeded(), released]);
    ensure_no_native_workers(work, RELEASED).expect("a finished, released worker passes");
}
