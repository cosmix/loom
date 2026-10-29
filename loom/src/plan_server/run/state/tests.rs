use super::*;

use std::fs;
use std::net::SocketAddr;
use std::process::Command;

use tempfile::TempDir;

use crate::plan_server::context::BindInfo;
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::run::RunJob;
use crate::plan_server::wire::run::{RunOffer as O, WorkspaceState as W};
use crate::planning::watch::OwnWrites;
use crate::web_server::http::RequestHead;

fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "--initial-branch=main"]);
    put(dir.path(), ".gitignore", "/.loom/\n");
    dir
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn workspace(root: &Path, plan: &str) {
    put(root, plan, "# Fixture plan\n");
    put(
        root,
        ".loom/work/config.toml",
        &format!("[plan]\nsource_path = {plan:?}\n"),
    );
}

fn context(root: &Path) -> PlanContext {
    let ctx = PlanContext::new(
        root.to_path_buf(),
        ReadOnly::from_problems(Vec::new()),
        BindInfo::new(SocketAddr::from(([127, 0, 0, 1], 0)), "secret"),
        OwnWrites::default(),
    )
    .unwrap();
    super::super::child::install_jobs(&ctx);
    ctx
}

fn get(ctx: &PlanContext) -> ApiResponse {
    let head = RequestHead {
        method: "GET".into(),
        path: "/api/run/state".into(),
        query: None,
        upgrade_websocket: false,
        origin: None,
        host: None,
        cookie: None,
        content_type: None,
        content_length: None,
        csrf_token: None,
    };
    get_state(&ApiRequest {
        ctx,
        head: &head,
        browser: &BrowserId::mint().unwrap(),
        public_id: &PublicId::mint(),
        query: Default::default(),
        body: Vec::new(),
    })
}

#[test]
fn workspace_state_follows_the_plan_prefix() {
    let dir = repo();
    assert_eq!(
        workspace_state(dir.path()).unwrap(),
        (W::Absent, O::Start, None)
    );
    let cases = [
        ("doc/plans/PLAN-a.md", W::Initialized, O::StartInitialized),
        ("doc/plans/IN_PROGRESS-PLAN-a.md", W::Stopped, O::Resume),
        ("doc/plans/DONE-PLAN-a.md", W::Done, O::StartNext),
    ];
    for (source, state, offer) in cases {
        workspace(dir.path(), source);
        assert_eq!(
            workspace_state(dir.path()).unwrap(),
            (state, offer, Some(PathBuf::from(source))),
        );
    }
}

#[test]
fn cache_only_workspace_is_absent() {
    let dir = repo();
    fs::create_dir_all(dir.path().join(".loom/work/context")).unwrap();

    assert_eq!(
        workspace_state(dir.path()).unwrap(),
        (W::Absent, O::Start, None)
    );
}

#[test]
fn held_daemon_lock_gives_running_with_no_run_offer() {
    let dir = repo();
    workspace(dir.path(), "doc/plans/IN_PROGRESS-PLAN-a.md");
    let work_dir = WorkDir::new(dir.path()).unwrap();
    // The daemon regards a held singleton lock as alive even without a responsive socket.
    let _lock = crate::fs::daemon_state::acquire_lock(work_dir.root()).unwrap();

    assert_eq!(
        workspace_state(dir.path()).unwrap(),
        (
            W::Running,
            O::NoneRunning,
            Some(PathBuf::from("doc/plans/IN_PROGRESS-PLAN-a.md")),
        )
    );
}

#[test]
fn absolute_source_path_is_returned_repo_relative() {
    let dir = repo();
    let path = dir.path().join("doc/plans/PLAN-a.md");
    workspace(dir.path(), path.to_str().unwrap());

    assert_eq!(
        workspace_state(dir.path()).unwrap().2,
        Some(PathBuf::from("doc/plans/PLAN-a.md"))
    );
}

#[test]
fn malformed_config_is_an_error() {
    let dir = repo();
    put(dir.path(), ".loom/work/config.toml", "[plan\n");

    assert!(workspace_state(dir.path()).is_err());
}

fn ordered_plans(root: &Path) {
    for slug in ["z", "a", "c", "b"] {
        put(root, &format!("doc/plans/PLAN-{slug}.md"), "# Plan\n");
    }
    put(root, "doc/plans/IN_PROGRESS-PLAN-running.md", "# Running\n");
    put(root, "doc/plans/DONE-PLAN-done.md", "# Done\n");
    put(root, "doc/plans/archive/PLAN-archived.md", "# Archived\n");
    put(
        root,
        "doc/plans/index.md",
        "# Metaplan\n\n\
        ## 1. `PLAN-z.md`\n\n## 2. `PLAN-a.md`\n\n\
        ## 3. `PLAN-running.md`\n\n## 4. `PLAN-missing.md`\n\n\
        ## 5. `PLAN-done.md`\n\n## 6. `PLAN-z.md`\n",
    );
}

#[test]
fn written_plans_follow_metaplan_headings_then_unlisted_filenames() {
    let dir = repo();
    ordered_plans(dir.path());

    let plans = written_plans(dir.path());

    let choices: Vec<_> = plans
        .iter()
        .map(|plan| (plan.path.as_str(), plan.slug.as_str(), plan.listed))
        .collect();
    assert_eq!(
        choices,
        vec![
            ("doc/plans/PLAN-z.md", "z", true),
            ("doc/plans/PLAN-a.md", "a", true),
            ("doc/plans/PLAN-b.md", "b", false),
            ("doc/plans/PLAN-c.md", "c", false),
        ]
    );
}

#[test]
fn first_written_plan_is_preselected() {
    let dir = repo();
    ordered_plans(dir.path());
    let ctx = context(dir.path());

    let view = state_view(&ctx).unwrap();

    assert_eq!(view.preselected.as_deref(), Some("doc/plans/PLAN-z.md"));
    assert_eq!(view.branch, "main");
    assert_eq!(view.default_backend, Backend::Tmux);
    assert!(!view.clean_notice);
}

#[test]
fn initialized_plan_overrides_the_first_written_plan() {
    let dir = repo();
    ordered_plans(dir.path());
    workspace(dir.path(), "doc/plans/PLAN-c.md");
    let ctx = context(dir.path());

    let view = state_view(&ctx).unwrap();

    assert_eq!(view.preselected.as_deref(), Some("doc/plans/PLAN-c.md"));
    assert_eq!(view.active_plan, view.preselected);
    assert!(!view.clean_notice);
}

#[test]
fn done_workspace_shows_clean_notice_and_selects_the_next_written_plan() {
    let dir = repo();
    ordered_plans(dir.path());
    workspace(dir.path(), "doc/plans/DONE-PLAN-old.md");
    let ctx = context(dir.path());

    let view = state_view(&ctx).unwrap();

    assert!(view.clean_notice);
    assert_eq!(view.preselected.as_deref(), Some("doc/plans/PLAN-z.md"));
    assert_eq!(view.offer, RunOffer::StartNext);
    assert_eq!(view.abandon, None);
    assert_eq!(view.finish_abandon, None);
}

#[test]
fn no_written_plans_means_no_preselection() {
    let dir = repo();
    let ctx = context(dir.path());

    assert_eq!(state_view(&ctx).unwrap().preselected, None);
}

fn desktop() -> EnvView {
    EnvView {
        ssh_connection: false,
        ssh_tty: false,
        display: true,
        wayland: false,
        linux: true,
    }
}

#[test]
fn headless_backends_omit_native() {
    let env = EnvView {
        display: false,
        ..desktop()
    };

    assert_eq!(backends(&env), vec![Backend::Tmux]);
}

#[test]
fn ssh_backends_omit_native_even_with_a_display() {
    for env in [
        EnvView {
            ssh_connection: true,
            ..desktop()
        },
        EnvView {
            ssh_tty: true,
            ..desktop()
        },
    ] {
        assert_eq!(backends(&env), vec![Backend::Tmux]);
    }
}

#[test]
fn desktop_backends_offer_tmux_then_native() {
    assert_eq!(backends(&desktop()), vec![Backend::Tmux, Backend::Native]);
    let wayland = EnvView {
        display: false,
        wayland: true,
        ..desktop()
    };
    assert_eq!(backends(&wayland), vec![Backend::Tmux, Backend::Native]);
}

#[test]
fn state_view_exposes_the_current_job() {
    let dir = repo();
    let ctx = context(dir.path());
    let jobs = super::super::child::jobs(&ctx);
    let _guard = jobs.begin(RunJob::Start).unwrap();

    assert_eq!(state_view(&ctx).unwrap().busy, Some(RunJob::Start));
}

#[test]
fn state_view_partitions_changes_without_truncating_doc_paths() {
    let dir = repo();
    put(dir.path(), "tracked.rs", "staged\n");
    git(dir.path(), &["add", "--", "tracked.rs"]);
    put(dir.path(), "untracked.rs", "untracked\n");
    for index in 0..300 {
        put(
            dir.path(),
            &format!("doc/changed-{index:03}.md"),
            "# Changed\n",
        );
    }
    put(
        dir.path(),
        "doc/loom/summaries/.section_03.md.tmp-41822",
        "temporary\n",
    );
    let ctx = context(dir.path());

    let view = state_view(&ctx).unwrap();

    assert_eq!(view.doc_changes.len(), 300);
    assert_eq!(view.blocking_changes.len(), 1);
    assert_eq!(view.blocking_changes[0].path, "tracked.rs");
    assert_eq!(view.backends, backends(&EnvView::current()));
}

#[test]
fn get_state_returns_the_view_to_a_viewer() {
    let dir = repo();
    let ctx = context(dir.path());

    let response = get(&ctx);

    assert_eq!(response.status, 200);
    let view: RunStateView = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(view.workspace, WorkspaceState::Absent);
    assert_eq!(view.dashboard_url, None);
    assert!(!String::from_utf8(response.body).unwrap().contains("token="));
}

#[test]
fn get_state_hides_internal_error_details() {
    let dir = repo();
    let ctx = context(dir.path());
    put(
        dir.path(),
        ".loom/work/config.toml",
        "[private-invalid-config\n",
    );

    let response = get(&ctx);

    assert_eq!(response.status, 500);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.body).unwrap(),
        serde_json::json!({"error": "internal", "message": "the run state could not be read"})
    );
}
