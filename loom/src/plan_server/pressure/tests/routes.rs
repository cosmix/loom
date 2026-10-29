//! The routes: each disabled reason of the options and their order, the start refusals, and
//! the Stop and confirm refusals.

use std::path::PathBuf;

use serde_json::Value;
use serial_test::serial;

use super::fake::Script;
use super::{start_body, Fixture, PLAN};
use crate::plan_server::authoring::Take;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::wire::pressure::disabled_reason;
use crate::planning::guidance::StartBlocks;

fn options(f: &Fixture, browser: &BrowserId) -> Value {
    let (status, body) = f.call("/api/pressure/options", browser, None);
    assert_eq!(status, 200, "{body}");
    body
}

fn reason(f: &Fixture, browser: &BrowserId) -> Option<String> {
    let body = options(f, browser);
    assert_eq!(
        body["available"],
        body["disabled_reason"].is_null(),
        "{body}"
    );
    body["disabled_reason"].as_str().map(str::to_owned)
}

/// Another browser takes authoring.
fn other_holder(f: &Fixture) -> BrowserId {
    let other = BrowserId::mint().unwrap();
    f.ctx.require_authoring(&other, Take::TakeIfFree).unwrap();
    other
}

/// A run that holds its first step until stopped.
fn running(f: &Fixture) {
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
    f.start(1);
}

fn stop(f: &Fixture) {
    let (status, body) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!(status, 202, "{body}");
    f.finished();
}

#[test]
#[serial]
fn options_are_available_with_the_written_plans_and_defaults() {
    let f = Fixture::new();
    let body = options(&f, &f.holder);
    assert_eq!(body["available"], true, "{body}");
    assert_eq!(body["plans"], serde_json::json!([PLAN]));
    assert_eq!(body["defaults"]["rounds"], 2);
}

#[test]
#[serial]
fn options_say_codex_is_missing_without_codex_on_path() {
    let mut f = Fixture::new();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let has_codex = |dir: &PathBuf| dir.join("codex").exists();
    let kept: Vec<PathBuf> = std::env::split_paths(&path)
        .filter(|dir| !has_codex(dir))
        .collect();
    f.env.set("PATH", std::env::join_paths(kept).unwrap());
    f.env.set("HOME", f.root.join("no-home"));
    let missing = reason(&f, &f.holder);
    drop(f.env);
    assert_eq!(missing.as_deref(), Some(disabled_reason::CODEX_MISSING));
}

#[test]
#[serial]
fn options_say_the_skill_is_missing_with_an_empty_codex_home() {
    let mut f = Fixture::new();
    f.env.set("CODEX_HOME", f.root.join("empty-codex-home"));
    assert_eq!(
        reason(&f, &f.holder).as_deref(),
        Some(disabled_reason::CODEX_SKILL_MISSING)
    );
}

#[test]
#[serial]
fn options_say_read_only() {
    let f = Fixture::read_only();
    assert_eq!(
        reason(&f, &f.holder).as_deref(),
        Some(disabled_reason::READ_ONLY)
    );
}

#[test]
#[serial]
fn options_say_another_browser_holds_authoring() {
    let f = Fixture::new();
    let other = other_holder(&f);
    assert_eq!(
        reason(&f, &f.holder).as_deref(),
        Some(disabled_reason::NOT_HOLDER)
    );
    assert_eq!(
        reason(&f, &other),
        None,
        "the holder is not offered the run"
    );
}

#[test]
#[serial]
fn options_say_a_run_is_running() {
    let f = Fixture::new();
    running(&f);
    let body = options(&f, &f.holder);
    assert_eq!(body["disabled_reason"], disabled_reason::RUNNING);
    assert_eq!(body["running"], true);
    stop(&f);
}

#[test]
#[serial]
fn options_give_the_first_reason_in_order() {
    let mut f = Fixture::new();
    running(&f);
    let watcher = BrowserId::mint().unwrap();
    assert_eq!(
        reason(&f, &watcher).as_deref(),
        Some(disabled_reason::NOT_HOLDER)
    );
    f.env.set("CODEX_HOME", f.root.join("empty-codex-home"));
    assert_eq!(
        reason(&f, &watcher).as_deref(),
        Some(disabled_reason::CODEX_SKILL_MISSING)
    );
    stop(&f);
}

#[test]
#[serial]
fn options_report_a_configuration_block_with_its_reason() {
    let f = Fixture::new();
    let why = "Codex changed session configuration: .mcp.json";
    StartBlocks::block(f.ctx.runtime(), why, &[".mcp.json".into()]).unwrap();
    assert_eq!(reason(&f, &f.holder).as_deref(), Some(why));
}

fn refused(f: &Fixture, body: Value) -> (u16, String) {
    let (status, answer) = f.call("/api/pressure/start", &f.holder, Some(body));
    (
        status,
        answer["error"].as_str().unwrap_or_default().to_owned(),
    )
}

#[test]
#[serial]
fn start_refuses_a_second_run() {
    let f = Fixture::new();
    running(&f);
    assert_eq!(
        refused(&f, start_body(PLAN, 1)),
        (409, "pressure_running".into())
    );
    stop(&f);
}

#[test]
#[serial]
fn start_refuses_a_model_outside_its_list() {
    let f = Fixture::new();
    let mut body = start_body(PLAN, 1);
    body["codex_model"] = "gpt-0".into();
    assert_eq!(refused(&f, body), (400, "invalid_model".into()));
    let mut body = start_body(PLAN, 1);
    body["address_effort"] = "extreme".into();
    assert_eq!(refused(&f, body), (400, "invalid_model".into()));
}

#[test]
#[serial]
fn start_refuses_zero_or_more_than_ten_rounds() {
    let f = Fixture::new();
    for rounds in [0, 11] {
        assert_eq!(
            refused(&f, start_body(PLAN, rounds)),
            (400, "invalid_rounds".into())
        );
    }
}

#[test]
#[serial]
fn start_refuses_an_in_progress_plan() {
    let f = Fixture::new();
    let body = start_body("doc/plans/IN_PROGRESS-PLAN-b.md", 1);
    assert_eq!(refused(&f, body), (409, "plan_not_written".into()));
    assert!(!f.runs().running());
    assert!(f.fake.calls().is_empty());
}

#[test]
#[serial]
fn stop_without_a_run_and_confirm_without_a_block_are_refused() {
    let f = Fixture::new();
    let (status, body) = f.call("/api/pressure/stop", &f.holder, None);
    assert_eq!((status, body["error"].as_str()), (409, Some("no_run")));
    let (status, body) = f.call("/api/pressure/confirm-config", &f.holder, None);
    assert_eq!((status, body["error"].as_str()), (409, Some("no_block")));
}
