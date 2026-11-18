//! Test harness for the runtime contracts: an invocation-record fixture, a scripted
//! codex app-server peer and a stepping clock.

pub(crate) mod clock;
pub(crate) mod peer;

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::native_worker::model::{NativeManifest, NATIVE_DEADLINE_SECS};
use crate::native_worker::store::create_invocation;

pub(crate) use clock::SteppingClock;
pub(crate) use peer::{End, ScriptedPeer};

pub(crate) const STAGE: &str = "native-runtime";
pub(crate) const UNIT: &str = "r1";
pub(crate) const INVOCATION: &str = "inv-00112233445566778899aabbccddeeff";
pub(crate) const THREAD: &str = "thr-runtime";
pub(crate) const TURN: &str = "turn-runtime";
const BRIEF: &[u8] = b"Write the file.";

/// A fresh invocation record, as admission leaves it: manifest, brief and the
/// `Admitted` event.
pub(crate) struct Invocation {
    pub(crate) _tmp: tempfile::TempDir,
    pub(crate) work: PathBuf,
    pub(crate) dir: PathBuf,
}

pub(crate) fn invocation() -> Invocation {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let work = tmp.path().join("work");
    let workspace = tmp.path().join("worktree");
    fs::create_dir_all(&work).expect("work dir");
    fs::create_dir_all(&workspace).expect("worktree dir");
    let manifest = NativeManifest {
        v: 1,
        stage_id: STAGE.to_string(),
        unit_id: UNIT.to_string(),
        invocation_id: INVOCATION.to_string(),
        owner_session_id: "session-runtime".to_string(),
        workspace_root: workspace.clone(),
        brief_path: workspace.join("brief.md"),
        brief_sha256: hex::encode(Sha256::digest(BRIEF)),
        model: "gpt-6.1-terra".to_string(),
        effort: "xhigh".to_string(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc::now(),
    };
    let dir = create_invocation(&work, &manifest, BRIEF).expect("create invocation");
    Invocation {
        _tmp: tmp,
        work,
        dir,
    }
}

/// Number of lines in the invocation's `events.jsonl`.
pub(crate) fn event_lines(dir: &std::path::Path) -> usize {
    fs::read_to_string(dir.join("events.jsonl"))
        .expect("events.jsonl")
        .lines()
        .count()
}
