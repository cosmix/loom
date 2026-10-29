use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use super::*;
use crate::plan_server::wire::run::{ChangedPath, PlanChoice, RunOffer};

const PLAN: &str = "doc/plans/PLAN-example.md";
const IN_PROGRESS: &str = "doc/plans/IN_PROGRESS-PLAN-example.md";

fn request() -> RunStartRequest {
    RunStartRequest {
        plan: PLAN.into(),
        backend: Backend::Tmux,
        clean: false,
    }
}

fn state() -> RunStateView {
    RunStateView {
        workspace: WorkspaceState::Absent,
        active_plan: None,
        offer: RunOffer::Start,
        plans: vec![PlanChoice {
            path: PLAN.into(),
            slug: "example".into(),
            listed: true,
        }],
        preselected: Some(PLAN.into()),
        branch: "main".into(),
        doc_changes: vec![],
        blocking_changes: vec![],
        backends: vec![Backend::Tmux, Backend::Native],
        default_backend: Backend::Tmux,
        clean_notice: false,
        abandon: None,
        finish_abandon: None,
        dashboard_url: None,
        busy: None,
    }
}

fn fake_loom(script: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let bin = dir.path().join("loom");
    fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(bin, fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn init_then_run_passes_plan_and_tmux_backend_without_clean() {
    let dir = fake_loom("printf '%s\\n' \"$@\" >> args\nprintf '%s\\n' \"$1 output\"");
    let mut lines = Vec::new();

    let code = run_start_sequence(
        dir.path(),
        &dir.path().join("loom"),
        &request(),
        Steps::InitThenRun,
        &mut |step, stream, line| lines.push((step.to_owned(), stream, line.to_owned())),
    )
    .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(dir.path().join("args")).unwrap(),
        "init\ndoc/plans/PLAN-example.md\n--backend\ntmux\nrun\n"
    );
    assert_eq!(
        lines,
        vec![
            (
                "loom init".into(),
                OutputStream::Stdout,
                "init output".into()
            ),
            ("loom run".into(), OutputStream::Stdout, "run output".into()),
        ]
    );
}

#[test]
fn clean_is_passed_only_to_init_when_requested() {
    let dir = fake_loom("printf '%s\\n' \"$@\" >> args");
    let mut request = request();
    request.clean = true;

    let code = run_start_sequence(
        dir.path(),
        &dir.path().join("loom"),
        &request,
        Steps::InitThenRun,
        &mut |_, _, _| {},
    )
    .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(dir.path().join("args")).unwrap(),
        "init\ndoc/plans/PLAN-example.md\n--backend\ntmux\n--clean\nrun\n"
    );
}

#[test]
fn native_backend_is_passed_to_init() {
    let dir = fake_loom("printf '%s\\n' \"$@\" >> args");
    let mut request = request();
    request.backend = Backend::Native;

    let code = run_start_sequence(
        dir.path(),
        &dir.path().join("loom"),
        &request,
        Steps::InitThenRun,
        &mut |_, _, _| {},
    )
    .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(dir.path().join("args")).unwrap(),
        "init\ndoc/plans/PLAN-example.md\n--backend\nnative\nrun\n"
    );
}

#[test]
fn failing_init_stops_before_run_and_keeps_both_streams() {
    let dir = fake_loom(
        "printf '%s\\n' \"$1\" >> calls\nprintf 'before failure\\n'\nprintf 'init failed\\n' >&2\nexit 7",
    );
    let mut lines = Vec::new();

    let code = run_start_sequence(
        dir.path(),
        &dir.path().join("loom"),
        &request(),
        Steps::InitThenRun,
        &mut |step, stream, line| lines.push((step.to_owned(), stream, line.to_owned())),
    )
    .unwrap();

    assert_eq!(code, 7);
    assert_eq!(
        fs::read_to_string(dir.path().join("calls")).unwrap(),
        "init\n"
    );
    assert_eq!(lines.len(), 2);
    assert!(lines.contains(&(
        "loom init".into(),
        OutputStream::Stdout,
        "before failure".into()
    )));
    assert!(lines.contains(&(
        "loom init".into(),
        OutputStream::Stderr,
        "init failed".into()
    )));
}

#[test]
fn run_only_skips_init_and_ignores_backend_and_clean() {
    let dir = fake_loom("printf '%s\\n' \"$@\" >> args\nprintf 'run output\\n'");
    let mut request = request();
    request.clean = true;
    let mut lines = Vec::new();

    let code = run_start_sequence(
        dir.path(),
        &dir.path().join("loom"),
        &request,
        Steps::RunOnly,
        &mut |step, stream, line| lines.push((step.to_owned(), stream, line.to_owned())),
    )
    .unwrap();

    assert_eq!(code, 0);
    assert_eq!(
        fs::read_to_string(dir.path().join("args")).unwrap(),
        "run\n"
    );
    assert_eq!(
        lines,
        vec![("loom run".into(), OutputStream::Stdout, "run output".into())]
    );
}

#[test]
fn silent_run_failure_identifies_run_as_the_failing_step() {
    let dir = fake_loom("if [ \"$1\" = run ]; then exit 9; fi");
    let mut step = "loom init";

    let code = sequence(
        dir.path(),
        &dir.path().join("loom"),
        &request(),
        Steps::InitThenRun,
        &mut step,
        &mut |_, _, _| {},
    )
    .unwrap();

    assert_eq!((step, code), ("loom run", 9));
}

#[test]
fn signal_exit_returns_minus_one_and_does_not_run_next_step() {
    let dir = fake_loom("printf '%s\\n' \"$1\" >> calls\nkill -TERM \"$$\"");

    let code = run_start_sequence(
        dir.path(),
        &dir.path().join("loom"),
        &request(),
        Steps::InitThenRun,
        &mut |_, _, _| {},
    )
    .unwrap();

    assert_eq!(code, -1);
    assert_eq!(
        fs::read_to_string(dir.path().join("calls")).unwrap(),
        "init\n"
    );
}

#[test]
fn tracked_changes_outside_doc_refuse_start() {
    let mut state = state();
    state.blocking_changes.push(ChangedPath {
        path: "src/lib.rs".into(),
        status: " M".into(),
    });

    assert_eq!(admit(&state, &request()), Err("blocked"));
}

#[test]
fn plan_missing_from_written_choices_refuses_start() {
    let mut state = state();
    state.plans.clear();

    assert_eq!(admit(&state, &request()), Err("not_written"));
}

#[test]
fn running_workspace_refuses_even_with_clean() {
    let mut state = state();
    state.workspace = WorkspaceState::Running;
    let mut request = request();
    request.clean = true;

    assert_eq!(admit(&state, &request), Err("running"));
}

#[test]
fn done_workspace_requires_clean() {
    let mut state = state();
    state.workspace = WorkspaceState::Done;

    assert_eq!(admit(&state, &request()), Err("needs_clean"));
}

#[test]
fn initialized_workspace_for_another_plan_requires_clean() {
    let mut state = state();
    state.workspace = WorkspaceState::Initialized;
    state.active_plan = Some("doc/plans/PLAN-other.md".into());

    assert_eq!(admit(&state, &request()), Err("needs_clean"));
}

#[test]
fn clean_allows_done_and_different_initialized_workspaces() {
    let mut request = request();
    request.clean = true;
    for workspace in [WorkspaceState::Done, WorkspaceState::Initialized] {
        let mut state = state();
        state.workspace = workspace;
        state.active_plan = Some("doc/plans/PLAN-other.md".into());

        assert_eq!(admit(&state, &request), Ok(Steps::InitThenRun));
    }
}

#[test]
fn initialized_workspace_for_selected_plan_runs_loom_run_alone() {
    let mut state = state();
    state.workspace = WorkspaceState::Initialized;
    state.active_plan = Some(PLAN.into());

    assert_eq!(admit(&state, &request()), Ok(Steps::RunOnly));
}

#[test]
fn clean_reinitializes_the_selected_initialized_plan() {
    let mut state = state();
    state.workspace = WorkspaceState::Initialized;
    state.active_plan = Some(PLAN.into());
    let mut request = request();
    request.clean = true;

    assert_eq!(admit(&state, &request), Ok(Steps::InitThenRun));
}

#[test]
fn absent_workspace_allows_start_without_clean() {
    assert_eq!(admit(&state(), &request()), Ok(Steps::InitThenRun));
}

fn stopped_on(plan: &str) -> RunStateView {
    let mut state = state();
    state.workspace = WorkspaceState::Stopped;
    state.offer = RunOffer::Resume;
    state.active_plan = Some(plan.into());
    state
}

#[test]
fn resume_runs_loom_run_alone_for_the_unwritten_in_progress_plan() {
    let mut request = request();
    request.plan = IN_PROGRESS.into();

    assert_eq!(
        admit(&stopped_on(IN_PROGRESS), &request),
        Ok(Steps::RunOnly)
    );
}

#[test]
fn stopped_workspace_refuses_another_written_plan_even_with_clean() {
    for clean in [false, true] {
        let mut request = request();
        request.clean = clean;

        assert_eq!(admit(&stopped_on(IN_PROGRESS), &request), Err("stopped"));
    }
}

#[test]
fn stopped_workspace_refuses_clean_for_its_own_plan() {
    let mut request = request();
    request.plan = IN_PROGRESS.into();
    request.clean = true;

    assert_eq!(admit(&stopped_on(IN_PROGRESS), &request), Err("stopped"));
}

#[test]
fn resume_still_refuses_tracked_changes_outside_doc() {
    let mut state = stopped_on(IN_PROGRESS);
    state.blocking_changes.push(ChangedPath {
        path: "src/lib.rs".into(),
        status: " M".into(),
    });
    let mut request = request();
    request.plan = IN_PROGRESS.into();

    assert_eq!(admit(&state, &request), Err("blocked"));
}

#[test]
fn running_workspace_refuses_its_own_in_progress_plan_as_running() {
    let mut state = state();
    state.workspace = WorkspaceState::Running;
    state.active_plan = Some(IN_PROGRESS.into());
    let mut request = request();
    request.plan = IN_PROGRESS.into();

    assert_eq!(admit(&state, &request), Err("running"));
}

#[test]
fn doc_changes_do_not_add_a_start_check() {
    let mut state = state();
    state.doc_changes.push(ChangedPath {
        path: "doc/spec.md".into(),
        status: "??".into(),
    });

    assert_eq!(admit(&state, &request()), Ok(Steps::InitThenRun));
}
