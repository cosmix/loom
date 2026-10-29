//! The abandon routes' refusals, and the dialog's view of a fixture workspace.

use serde_json::Value;
use serial_test::serial;

use super::super::{routes, view};
use super::{EnvGuard, Repo, PLAN};
use crate::models::stage::{Stage, StageStatus};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::request::{ApiRequest, ApiResponse, QueryParams};
use crate::plan_server::routes::Role;
use crate::plan_server::run::child::{install_jobs, jobs};
use crate::plan_server::wire::run::{RunJob, UnmergedBranch};
use crate::planning::watch::OwnWrites;
use crate::verify::transitions::create_stage;
use crate::web_server::http::parse_head;

/// A context on `repo`, its loom home inside the fixture.
fn context(repo: &Repo) -> (PlanContext, EnvGuard) {
    let home = EnvGuard::set("LOOM_HOME", &repo.temp.path().join("home"));
    let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(repo.root.clone(), read_only, bind, OwnWrites::default()).unwrap();
    install_jobs(&ctx);
    (ctx, home)
}

fn post_abandon(ctx: &PlanContext) -> ApiResponse {
    post(ctx, "/api/run/abandon")
}

fn post(ctx: &PlanContext, path: &str) -> ApiResponse {
    let route = routes().into_iter().find(|r| r.path == path).unwrap();
    let raw = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let head = parse_head(raw.as_bytes()).unwrap().unwrap();
    let browser = BrowserId::parse(&"ab".repeat(16)).unwrap();
    let public_id = PublicId::mint();
    let request = ApiRequest {
        ctx,
        head: &head,
        browser: &browser,
        public_id: &public_id,
        query: QueryParams::parse(None).unwrap(),
        body: br#"{"confirmed_unmerged":[]}"#.to_vec(),
    };
    (route.handler)(&request)
}

fn error_code(response: &ApiResponse) -> (u16, String) {
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    (
        response.status,
        body["error"].as_str().unwrap_or_default().to_string(),
    )
}

#[test]
fn every_abandon_route_is_for_the_holder() {
    let routes = routes();
    let paths: Vec<_> = routes.iter().map(|r| r.path).collect();
    assert_eq!(paths, vec!["/api/run/abandon", "/api/run/finish-abandon"]);
    assert!(routes.iter().all(|r| r.role == Role::Holder));
}

#[test]
#[serial]
fn a_done_workspace_is_not_abandonable() {
    let repo = Repo::new();
    repo.put("doc/plans/DONE-PLAN-a.md", "# A\n");
    repo.workspace("doc/plans/DONE-PLAN-a.md", "main");
    let (ctx, _home) = context(&repo);
    let expected = (409, "not_abandonable".to_string());
    assert_eq!(error_code(&post_abandon(&ctx)), expected);
    assert!(repo.has("doc/plans/DONE-PLAN-a.md"));
}

#[test]
#[serial]
fn an_absent_workspace_is_not_abandonable() {
    let repo = Repo::new();
    repo.put(PLAN, "# A\n");
    let (ctx, _home) = context(&repo);
    let expected = (409, "not_abandonable".to_string());
    assert_eq!(error_code(&post_abandon(&ctx)), expected);
    assert!(repo.has(PLAN));
}

#[test]
#[serial]
fn abandon_is_busy_while_another_job_runs() {
    let repo = Repo::running();
    let (ctx, _home) = context(&repo);
    let _commit = jobs(&ctx).begin(RunJob::Commit).expect("no job runs yet");
    assert_eq!(error_code(&post_abandon(&ctx)), (409, "busy".to_string()));
    assert!(repo.has(PLAN));
    assert!(repo.root.join(".loom/work").exists());
}

#[test]
#[serial]
fn finish_abandon_without_a_marker_refuses_and_keeps_the_name() {
    let repo = Repo::new();
    repo.put(PLAN, "# A\n");
    let (ctx, _home) = context(&repo);
    let response = post(&ctx, "/api/run/finish-abandon");
    assert_eq!(error_code(&response), (409, "refused".to_string()));
    assert!(repo.has(PLAN));
    assert!(
        jobs(&ctx).current().is_none(),
        "the job outlived the request"
    );
}

fn stage(id: &str, status: StageStatus, commit: Option<String>) -> Stage {
    let mut stage = Stage::new(id.to_string(), None);
    stage.id = id.to_string();
    stage.status = status;
    stage.completed_commit = commit;
    stage
}

#[test]
#[serial]
fn view_lists_merged_and_unmerged_stages_and_branches() {
    let repo = Repo::running();
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    repo.branch_ahead("loom/s2", 1);
    let s2_tip = repo.git(&["rev-parse", "loom/s2"]).trim().to_string();
    let work = repo.root.join(".loom/work");
    create_stage(&stage("s1", StageStatus::Completed, Some(head)), &work).unwrap();
    create_stage(&stage("s2", StageStatus::Completed, Some(s2_tip)), &work).unwrap();
    create_stage(&stage("s3", StageStatus::Queued, None), &work).unwrap();
    let (ctx, _home) = context(&repo);

    let view = view(&ctx).expect("a running plan has a view");
    assert_eq!(view.plan, PLAN);
    assert_eq!(view.merged_stages, vec!["s1"]);
    let mut unmerged = view.unmerged_stages.clone();
    unmerged.sort();
    assert_eq!(unmerged, vec!["s2", "s3"]);
    let branch = UnmergedBranch {
        branch: "loom/s2".to_string(),
        ahead: 1,
    };
    assert_eq!(view.unmerged_branches, vec![branch]);
    assert_eq!(view.base_branch, "main");
}
