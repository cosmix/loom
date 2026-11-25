//! Start refusal messages and the job guard around admission.

use tempfile::TempDir;

use super::tests::state;
use super::*;
use crate::plan_server::context::BindInfo;
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::run::ChangedPath;
use crate::planning::runtime::runtime_paths_under;
use crate::planning::watch::OwnWrites;
use crate::test_git::TestRepo;
use crate::web_server::http::RequestHead;

fn change(path: &str, status: &str) -> ChangedPath {
    ChangedPath {
        path: path.into(),
        status: status.into(),
    }
}

#[test]
fn tracked_changes_get_the_commit_or_stash_advice() {
    let root = TempDir::new().unwrap();
    let mut state = state();
    state.blocking_changes.push(change("src/lib.rs", " M"));

    assert_eq!(
        blocked_message(root.path(), &state),
        "commit or stash tracked changes outside doc/ before starting"
    );
}

#[test]
fn the_run_state_entry_is_refused_with_its_own_status() {
    let root = TempDir::new().unwrap();
    let status =
        "stage files remain; no plan names this run, so run `loom clean --all` in a terminal";
    let mut state = state();
    state.blocking_changes.push(change(".loom/work", status));

    assert_eq!(blocked_message(root.path(), &state), status);
}

#[test]
fn a_tracked_change_beside_the_run_state_entry_does_not_hide_its_remedy() {
    let root = TempDir::new().unwrap();
    let mut state = state();
    state.blocking_changes.push(change("src/lib.rs", " M"));
    state
        .blocking_changes
        .push(change(".loom/work", "worktrees remain"));

    assert_eq!(blocked_message(root.path(), &state), "worktrees remain");
}

fn dirty_repo() -> (TestRepo, Arc<PlanContext>) {
    let repo = TestRepo::builder()
        .seed("src.txt", "original\n", "initial")
        .create();
    std::fs::write(repo.path().join("src.txt"), "changed\n").unwrap();
    let runtime = runtime_paths_under(&repo.path().join("loom-home"), repo.path()).unwrap();
    let ctx = Arc::new(
        PlanContext::new(
            repo.path().to_path_buf(),
            ReadOnly::from_problems(Vec::new()),
            BindInfo::new("127.0.0.1:0".parse().unwrap(), "token"),
            OwnWrites::default(),
        )
        .unwrap()
        .with_runtime(runtime),
    );
    child::install_jobs(&ctx);
    install(&ctx);
    (repo, ctx)
}

fn post_body(ctx: &PlanContext, body: Vec<u8>) -> ApiResponse {
    let head = RequestHead {
        method: "POST".into(),
        path: "/api/run/start".into(),
        query: None,
        upgrade_websocket: false,
        origin: None,
        host: None,
        cookie: None,
        content_type: None,
        content_length: None,
        csrf_token: None,
    };
    post_start(&ApiRequest {
        ctx,
        head: &head,
        browser: &BrowserId::mint().unwrap(),
        public_id: &PublicId::mint(),
        query: Default::default(),
        body,
    })
}

fn start_body(extra: Option<(&str, &str)>) -> Vec<u8> {
    let mut body = serde_json::json!({ "plan": "doc/plans/PLAN-demo.md", "clean": false });
    if let Some((key, value)) = extra {
        body[key] = value.into();
    }
    serde_json::to_vec(&body).unwrap()
}

fn assert_start_refused(ctx: &PlanContext, code: &str) {
    let response = post_body(ctx, start_body(None));
    assert_eq!(response.status, 409);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["error"], code);
}

#[test]
fn a_start_while_a_job_runs_is_busy_before_any_state_read() {
    let (_repo, ctx) = dirty_repo();
    let jobs = child::jobs(&ctx);
    let _guard = jobs.begin(RunJob::Commit).unwrap();

    assert_start_refused(&ctx, "busy");
    assert_eq!(jobs.current(), Some(RunJob::Commit));
}

#[test]
fn a_refused_start_leaves_no_job_active() {
    let (_repo, ctx) = dirty_repo();

    assert_start_refused(&ctx, "blocked");
    assert_eq!(child::jobs(&ctx).current(), None);
}

#[test]
fn a_start_naming_a_backend_is_a_client_error_before_any_job_begins() {
    let (_repo, ctx) = dirty_repo();

    let response = post_body(&ctx, start_body(Some(("backend", "tmux"))));

    assert_eq!(response.status, 400);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["error"], "bad_request");
    assert_eq!(child::jobs(&ctx).current(), None);
    // The jobs guard is free: a valid start gets past the guard to the state check.
    assert_start_refused(&ctx, "blocked");
}
