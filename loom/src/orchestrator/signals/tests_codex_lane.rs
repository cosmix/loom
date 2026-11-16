//! Codex lane selection driven through signal generation.
//!
//! The formatter tests call `format_codex_implementers_section` with a
//! hand-built lane. These tests pin the wiring one level up: the builder must
//! resolve the lane from the state directory it is given (the `.loom/work`
//! dir holding `config.toml`), not from some other path.

use std::{fs, path::Path};
use tempfile::TempDir;

use crate::codex::{codex_lane, CodexLane};
use crate::models::stage::{Implementer, Implementers};

use super::generate::{build_embedded_context_with_stage, generate_signal_with_metrics};
use super::tests::{create_test_session, create_test_stage, create_test_worktree};

const FORWARDER_SPAWN: &str = "subagent_type: \"loom-codex-forwarder\"";

fn state_dir(temp: &TempDir) -> std::path::PathBuf {
    let work_dir = temp.path().join(".loom").join("work");
    fs::create_dir_all(&work_dir).unwrap();
    work_dir
}

fn codex_signal(work_dir: &Path) -> String {
    let mut stage = create_test_stage();
    stage.implementers = Implementers::new(vec![Implementer::Codex]);
    let (signal_path, _) = generate_signal_with_metrics(
        &create_test_session(),
        &stage,
        &create_test_worktree(),
        &[],
        None,
        None,
        work_dir,
    )
    .unwrap();
    fs::read_to_string(signal_path).unwrap()
}

#[test]
fn native_backend_in_work_dir_config_never_selects_legacy_lane() {
    let temp = TempDir::new().unwrap();
    let work_dir = state_dir(&temp);
    fs::write(
        work_dir.join("config.toml"),
        "[codex]\nbackend = \"native\"\n",
    )
    .unwrap();

    let embedded = build_embedded_context_with_stage(&work_dir, None, None);
    assert_ne!(
        embedded.codex_lane,
        CodexLane::Legacy,
        "a native backend configured in the work dir must not yield the legacy lane"
    );
    assert_eq!(embedded.codex_lane, codex_lane(&work_dir));

    let signal = codex_signal(&work_dir);
    assert!(signal.contains("## Codex Implementers"));
    assert!(
        !signal.contains(FORWARDER_SPAWN),
        "the native lane must not instruct a forwarder spawn"
    );
}

#[test]
fn unconfigured_work_dir_lane_matches_codex_lane() {
    let temp = TempDir::new().unwrap();
    let work_dir = state_dir(&temp);

    let embedded = build_embedded_context_with_stage(&work_dir, None, None);
    assert_eq!(embedded.codex_lane, codex_lane(&work_dir));
}
