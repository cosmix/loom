use super::Orchestrator;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::ExecutionGraph;
use serial_test::serial;

/// `Orchestrator::new` eagerly constructs a `NativeBackend`, so it fails on a
/// headless CI runner with no terminal emulator installed. Pinning
/// `LOOM_TERMINAL` maps a name straight to an emulator without probing the
/// host for the binary. Serialized because the detection tests mutate the same
/// process-global variable.
fn pin_terminal_env() -> Option<std::ffi::OsString> {
    let saved = std::env::var_os("LOOM_TERMINAL");
    // SAFETY: the test is serialized and restores the original value below.
    unsafe { std::env::set_var("LOOM_TERMINAL", "xterm") };
    saved
}

fn restore_terminal_env(saved: Option<std::ffi::OsString>) {
    match saved {
        // SAFETY: the serialized test is restoring its saved value.
        Some(value) => unsafe { std::env::set_var("LOOM_TERMINAL", value) },
        // SAFETY: the serialized test is restoring the variable's absence.
        None => unsafe { std::env::remove_var("LOOM_TERMINAL") },
    }
}

#[test]
#[serial]
fn merge_probe_failure_does_not_consume_resolver_attempt_budget() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".work");
    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let mut stage = Stage::new("probe-failure".to_string(), None);
    stage.id = "probe-failure".to_string();
    stage.status = StageStatus::MergeConflict;
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();
    let saved_terminal = pin_terminal_env();
    let constructed = Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap());
    restore_terminal_env(saved_terminal);
    let mut orchestrator = constructed.unwrap();

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(orchestrator.merge_resolver_attempts(&stage.id), 0);
    assert!(!orchestrator
        .merge_resolver_attempts_dir()
        .join(format!("{}.count", stage.id))
        .exists());
}
