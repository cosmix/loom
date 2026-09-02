//! Unit tests for the wrapper script's `LOOM_SESSION_TYPE` export.
//!
//! Split out of `tests.rs` to keep it under the 400-line ceiling (CLAUDE.md
//! Rule 17), matching how `tests_capsule.rs` and `tests_launch.rs` are split
//! out of the same directory.

use super::*;
use tempfile::TempDir;

fn wrapper_script_for(kind: SessionType) -> String {
    let work_dir = TempDir::new().unwrap();
    let path = create_wrapper_script(
        work_dir.path(),
        "loom-test-session",
        "feature",
        "session1",
        "claude 'prompt'",
        None,
        kind,
        100_000,
    )
    .unwrap();
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn wrapper_script_exports_session_type() {
    let script = wrapper_script_for(SessionType::Adjudication);
    assert!(
        script.contains("LOOM_SESSION_TYPE=adjudication"),
        "{script}"
    );
    let script = wrapper_script_for(SessionType::Stage);
    assert!(script.contains("LOOM_SESSION_TYPE=stage"), "{script}");
}
