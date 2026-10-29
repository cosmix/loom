use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Barrier;

use crate::plan_server::authoring::Take;
use crate::plan_server::context::BindInfo;
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::request::QueryParams;
use crate::planning::watch::OwnWrites;
use crate::web_server::http::RequestHead;
use tempfile::TempDir;

#[path = "tests_parser.rs"]
mod parser;

const URL: &str = "http://127.0.0.1:7373/?token=abc123";
const STARTUP: &str =
    "loom dashboard: http://127.0.0.1:7373/?token=abc123  (terminals enabled; Ctrl-C to stop)";

fn context() -> (TempDir, Arc<PlanContext>) {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("doc/product")).unwrap();
    fs::write(temp.path().join("doc/product/index.md"), "# Demo\n").unwrap();
    fs::create_dir_all(temp.path().join(".loom/work")).unwrap();
    fs::write(
        temp.path().join(".loom/work/config.toml"),
        "[terminal]\nbackend = \"tmux\"\n",
    )
    .unwrap();
    let ctx = PlanContext::new(
        temp.path().to_path_buf(),
        ReadOnly::from_problems(vec![]),
        BindInfo::new("127.0.0.1:7272".parse().unwrap(), "plan-token"),
        OwnWrites::default(),
    )
    .unwrap();
    (temp, Arc::new(ctx))
}

fn fake(root: &Path, body: &str) -> std::path::PathBuf {
    let path = root.join("fake-loom");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$$\" >> starts\nprintf '%s\\n' \"$*\" > args\n{body}\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn live_fake(root: &Path) -> std::path::PathBuf {
    fake(root, &format!("printf '%s\\n' '{STARTUP}'\nexec sleep 60"))
}

fn head() -> RequestHead {
    RequestHead {
        method: "POST".into(),
        path: "/api/run/dashboard".into(),
        query: None,
        upgrade_websocket: false,
        origin: None,
        host: None,
        cookie: None,
        content_type: None,
        content_length: None,
        csrf_token: None,
    }
}

fn request<'a>(
    ctx: &'a PlanContext,
    head: &'a RequestHead,
    browser: &'a BrowserId,
    public_id: &'a PublicId,
) -> ApiRequest<'a> {
    ApiRequest {
        ctx,
        head,
        browser,
        public_id,
        query: QueryParams::default(),
        body: vec![],
    }
}

fn call(ctx: &PlanContext, bin: &Path) -> ApiResponse {
    post_dashboard_with_bin(
        &request(ctx, &head(), &BrowserId::mint().unwrap(), &PublicId::mint()),
        bin,
        Duration::from_secs(3),
    )
}

fn json(response: &ApiResponse) -> serde_json::Value {
    serde_json::from_slice(&response.body).unwrap()
}

fn pid(root: &Path) -> i32 {
    fs::read_to_string(root.join("starts"))
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn assert_dead(pid: i32) {
    assert_eq!(
        nix::sys::signal::kill(Pid::from_raw(pid), None),
        Err(nix::errno::Errno::ESRCH)
    );
}

#[test]
fn response_reveals_token_only_to_holder() {
    let (temp, ctx) = context();
    let bin = live_fake(temp.path());
    let holder = BrowserId::mint().unwrap();
    let public = PublicId::mint();
    ctx.authoring()
        .require(&holder, &public, Take::TakeIfFree, false)
        .unwrap();
    let head = head();
    let held = post_dashboard_with_bin(
        &request(&ctx, &head, &holder, &public),
        &bin,
        Duration::from_secs(3),
    );
    let viewer = call(&ctx, &bin);

    assert_eq!((held.status, json(&held)["url"].as_str()), (200, Some(URL)));
    assert_eq!(
        (viewer.status, json(&viewer)["url"].as_str()),
        (200, Some("http://127.0.0.1:7373/"))
    );
    assert_eq!(current_url(&ctx).as_deref(), Some(URL));
}

#[test]
fn two_calls_reuse_child_and_installation() {
    let (temp, ctx) = context();
    let bin = live_fake(temp.path());
    install_dashboard(&ctx);
    let first_state = ctx.extensions().get::<Dashboard>().unwrap();
    assert_eq!(current_url(&ctx), None);
    install_dashboard(&ctx);

    assert!(Arc::ptr_eq(
        &first_state,
        &ctx.extensions().get::<Dashboard>().unwrap()
    ));
    let first = call(&ctx, &bin);
    let second = call(&ctx, &bin);

    assert_eq!(
        (first.status, json(&first)["url"].clone()),
        (second.status, json(&second)["url"].clone())
    );
    assert_eq!(first.status, 200);
    assert_eq!(
        fs::read_to_string(temp.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn concurrent_first_calls_start_one_child() {
    let (temp, ctx) = context();
    let bin = live_fake(temp.path());
    let barrier = Barrier::new(2);
    let responses = thread::scope(|scope| {
        let first = scope.spawn(|| {
            barrier.wait();
            call(&ctx, &bin)
        });
        let second = scope.spawn(|| {
            barrier.wait();
            call(&ctx, &bin)
        });
        [first.join().unwrap(), second.join().unwrap()]
    });

    assert!(responses.iter().all(|response| response.status == 200));
    assert_eq!(json(&responses[0])["url"], json(&responses[1])["url"]);
    assert_eq!(
        fs::read_to_string(temp.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn shutdown_drain_kills_child_and_prevents_restart() {
    let (temp, ctx) = context();
    let bin = live_fake(temp.path());
    assert_eq!(call(&ctx, &bin).status, 200);
    let child_pid = pid(temp.path());

    ctx.shutdown().begin(|_| {}).join().unwrap();

    assert_dead(child_pid);
    assert_eq!(current_url(&ctx), None);
    assert_eq!(call(&ctx, &bin).status, 502);
}

#[test]
fn shutdown_abort_kills_child() {
    let (temp, ctx) = context();
    let bin = live_fake(temp.path());
    assert_eq!(call(&ctx, &bin).status, 200);
    let child_pid = pid(temp.path());

    ctx.shutdown().abort_all();

    assert_dead(child_pid);
    assert_eq!(current_url(&ctx), None);
}

#[test]
fn timeout_returns_502_and_reaps_child() {
    let (temp, ctx) = context();
    let bin = fake(temp.path(), "exec sleep 60");
    let response = post_dashboard_with_bin(
        &request(
            &ctx,
            &head(),
            &BrowserId::mint().unwrap(),
            &PublicId::mint(),
        ),
        &bin,
        Duration::from_millis(500),
    );

    assert_eq!(
        (response.status, json(&response)["error"].as_str()),
        (502, Some("dashboard_unavailable"))
    );
    assert_dead(pid(temp.path()));
    assert_eq!(current_url(&ctx), None);
}

#[test]
fn startup_stops_after_twenty_lines() {
    let (temp, ctx) = context();
    let bin = fake(temp.path(), &format!(
        "i=0; while [ \"$i\" -lt 20 ]; do echo waiting; i=$((i+1)); done\nprintf '%s\\n' '{STARTUP}'\nexec sleep 60"
    ));

    let response = call(&ctx, &bin);

    assert_eq!(response.status, 502);
    assert_dead(pid(temp.path()));
}

#[test]
fn child_arguments_follow_host_and_terminal_backend() {
    let temp = TempDir::new().unwrap();
    let bin = live_fake(temp.path());
    let child = start_dashboard(
        temp.path(),
        &bin,
        "::1".parse().unwrap(),
        true,
        Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join("args")).unwrap(),
        "status --web --host ::1 --terminals\n"
    );
    drop(child);

    let child = start_dashboard(
        temp.path(),
        &bin,
        "127.0.0.1".parse().unwrap(),
        false,
        Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(temp.path().join("args")).unwrap(),
        "status --web --host 127.0.0.1\n"
    );
    drop(child);
}

#[test]
fn ssh_hint_uses_dashboard_port_and_is_omitted_locally() {
    let (_temp, ctx) = context();
    let head = head();
    let browser = BrowserId::mint().unwrap();
    let public = PublicId::mint();
    let req = request(&ctx, &head, &browser, &public);

    let remote = dashboard_response(&req, URL, true);
    let local = dashboard_response(&req, URL, false);

    assert_eq!(
        json(&remote)["tunnel"],
        crate::plan_server::startup::tunnel_hint(7373, &crate::plan_server::hostname())
    );
    assert!(json(&local).get("tunnel").is_none());
}

#[test]
fn native_backend_omits_terminals() {
    let (temp, ctx) = context();
    fs::write(
        temp.path().join(".loom/work/config.toml"),
        "[terminal]\nbackend = \"native\"\n",
    )
    .unwrap();
    let bin = live_fake(temp.path());

    assert_eq!(call(&ctx, &bin).status, 200);

    assert_eq!(
        fs::read_to_string(temp.path().join("args")).unwrap(),
        "status --web --host 127.0.0.1\n"
    );
}
