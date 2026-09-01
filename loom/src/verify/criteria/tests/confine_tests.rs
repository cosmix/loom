//! Tests for the confined command primitive
//!
//! These tests set process-global environment variables with
//! `std::env::set_var`, so every one of them is `#[serial]`.

use std::time::Duration;

use serial_test::serial;
use tempfile::TempDir;

#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::process::Child;

use crate::models::stage::CommandConfinement;
use crate::verify::criteria::confine::{plan_confinement, resolve_confinement, CommandSpec};
use crate::verify::criteria::executor::run_spec_with_timeout;

/// Value planted in loom's own environment to stand in for an ambient
/// credential (GITHUB_TOKEN, AWS_SECRET_ACCESS_KEY, ...).
const CANARY_VAR: &str = "LOOM_CONFINE_TEST_CANARY";
const CANARY_VALUE: &str = "ambient-secret-canary";

const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Run `spec` and return its stdout, failing the test if it did not succeed.
fn stdout_of(spec: &CommandSpec, confinement: CommandConfinement) -> String {
    let result = run_spec_with_timeout(spec, None, TEST_TIMEOUT, confinement)
        .expect("the command should spawn and complete");
    assert!(
        result.success,
        "command failed: stdout={} stderr={}",
        result.stdout, result.stderr
    );
    result.stdout
}

#[test]
#[serial]
fn confined_shell_command_does_not_see_ambient_secret() {
    std::env::set_var(CANARY_VAR, CANARY_VALUE);

    let environment = stdout_of(&CommandSpec::shell("env"), CommandConfinement::Confined);

    std::env::remove_var(CANARY_VAR);

    assert!(
        !environment.contains(CANARY_VALUE),
        "confined child inherited an ambient secret:\n{environment}"
    );
    assert!(
        !environment.contains(CANARY_VAR),
        "confined child inherited the ambient variable name:\n{environment}"
    );
    // The allowlist still has to leave a usable environment behind.
    assert!(
        environment.contains("PATH="),
        "confined child lost PATH:\n{environment}"
    );
}

#[test]
#[serial]
fn inherited_shell_command_does_see_ambient_secret() {
    std::env::set_var(CANARY_VAR, CANARY_VALUE);

    let environment = stdout_of(&CommandSpec::shell("env"), CommandConfinement::Inherit);

    std::env::remove_var(CANARY_VAR);

    // This is the proof the level is not inert: the same command, same code
    // path, opposite outcome.
    assert!(
        environment.contains(CANARY_VALUE),
        "inherit level did not pass the ambient environment through:\n{environment}"
    );
}

#[test]
#[serial]
fn confined_level_is_the_default_for_plain_criteria() {
    std::env::set_var(CANARY_VAR, CANARY_VALUE);

    let result = crate::verify::criteria::run_single_criterion("env", None)
        .expect("the command should spawn and complete");

    std::env::remove_var(CANARY_VAR);

    assert!(
        !result.stdout.contains(CANARY_VALUE),
        "the default level leaked an ambient secret:\n{}",
        result.stdout
    );
}

#[test]
fn program_spec_passes_metacharacters_as_literal_arguments() {
    // Every one of these is shell soup; none of it may be interpreted.
    let hostile = "; rm -rf / $(whoami) `id` && echo pwned | tee /dev/null";

    let result = run_spec_with_timeout(
        &CommandSpec::program("printf", ["%s", hostile]),
        None,
        TEST_TIMEOUT,
        CommandConfinement::Confined,
    )
    .expect("the program should spawn and complete");

    assert!(result.success, "printf failed: {}", result.stderr);
    assert_eq!(
        result.stdout, hostile,
        "the argument was not delivered literally"
    );
}

#[test]
fn program_spec_reports_the_program_and_arguments_on_failure() {
    let spec = CommandSpec::program("loom-no-such-program", ["--flag"]);
    assert_eq!(spec.to_string(), "loom-no-such-program --flag");

    let error = crate::verify::criteria::spawn_confined(&spec, None, CommandConfinement::Confined)
        .expect_err("a missing program cannot spawn");
    assert!(
        error.to_string().contains("loom-no-such-program --flag"),
        "unhelpful spawn error: {error}"
    );
}

#[test]
fn working_dir_is_honored_for_both_spec_kinds() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    std::fs::write(dir.join("marker.txt"), "here").unwrap();

    let shell = run_spec_with_timeout(
        &CommandSpec::shell("cat marker.txt"),
        Some(&dir),
        TEST_TIMEOUT,
        CommandConfinement::Confined,
    )
    .unwrap();
    assert!(shell.success, "shell spec failed: {}", shell.stderr);
    assert_eq!(shell.stdout, "here");

    let program = run_spec_with_timeout(
        &CommandSpec::program("cat", ["marker.txt"]),
        Some(&dir),
        TEST_TIMEOUT,
        CommandConfinement::Confined,
    )
    .unwrap();
    assert!(program.success, "program spec failed: {}", program.stderr);
    assert_eq!(program.stdout, "here");
}

#[cfg(unix)]
#[test]
fn spawned_child_leads_its_own_process_group() {
    // The timeout path kills `-pgid`, which only reaches grandchildren of a
    // compound command when the child is a process group leader.
    let mut child = crate::verify::criteria::spawn_confined(
        &CommandSpec::shell("ps -o pgid= -p $$"),
        None,
        CommandConfinement::Confined,
    )
    .expect("the shell should spawn");

    let reported_pgid = read_stdout(&mut child);
    child.wait().expect("the child should exit");

    let child_pid = i32::try_from(child.id()).unwrap();
    assert_eq!(
        reported_pgid.trim(),
        child_pid.to_string(),
        "child pgid should equal its own pid"
    );
    assert_ne!(
        reported_pgid.trim(),
        nix::unistd::getpgrp().to_string(),
        "child must not share loom's process group"
    );
}

#[cfg(unix)]
fn read_stdout(child: &mut Child) -> String {
    let mut stdout = child.stdout.take().expect("stdout should be piped");
    let mut buffer = String::new();
    stdout.read_to_string(&mut buffer).unwrap();
    buffer
}

#[test]
fn resolve_confinement_prefers_the_stage_override() {
    assert_eq!(
        resolve_confinement(Some(CommandConfinement::Inherit), None),
        CommandConfinement::Inherit
    );
    assert_eq!(
        resolve_confinement(
            Some(CommandConfinement::Confined),
            Some(CommandConfinement::Inherit)
        ),
        CommandConfinement::Confined
    );
    assert_eq!(
        resolve_confinement(None, Some(CommandConfinement::Inherit)),
        CommandConfinement::Inherit
    );
    // Nothing configured anywhere resolves to the fail-safe level.
    assert_eq!(
        resolve_confinement(None, None),
        CommandConfinement::Confined
    );
}

/// Create an empty `.loom/work` directory, as `loom init` would.
fn work_dir(temp: &TempDir) -> std::path::PathBuf {
    let work_dir = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    work_dir
}

#[test]
fn plan_confinement_without_a_persisted_snapshot_is_none() {
    let temp = TempDir::new().unwrap();
    assert_eq!(plan_confinement(&work_dir(&temp)), None);
}

#[test]
fn plan_confinement_reads_the_persisted_snapshot() {
    let temp = TempDir::new().unwrap();
    let work_dir = work_dir(&temp);

    let sandbox = crate::plan::schema::SandboxConfig {
        command_confinement: CommandConfinement::Inherit,
        ..Default::default()
    };
    crate::fs::work_dir::write_plan_sandbox(&work_dir, &sandbox).unwrap();

    assert_eq!(
        plan_confinement(&work_dir),
        Some(CommandConfinement::Inherit)
    );
}
