//! The command catalogue owner over fake `claude` scripts: one side process per chat session,
//! its tables on the wire, in the session's gate and on the route, ended with the session.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::signal::kill;
use nix::unistd::Pid;
use serde_json::{json, Value};

use super::super::tests::{browser, wait_until, Tab};
use super::fixture;
use crate::plan_server::commands::tests::call_route;
use crate::plan_server::commands::{Builder, Catalogue};
use crate::plan_session::bridge::{EndCause, EndMode};
use crate::plan_session::commands::{CatalogueState, SideSpec};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
);

/// A builder that runs `body` as a `/bin/sh` script in `base`, settings at `<base>/side.json`.
fn builder(base: &Path, body: &str) -> Builder {
    let script = base.join("claude.sh");
    fs::write(&script, format!("#!/bin/sh\n{body}")).unwrap();
    let base = base.to_path_buf();
    Arc::new(move |_spec| {
        Ok(SideSpec {
            program: "/bin/sh".into(),
            args: vec![script.clone().into_os_string()],
            env: vec![("PATH".into(), "/usr/bin:/bin".into())],
            cwd: base.clone(),
            settings_path: base.join("side.json"),
            settings: json!({"disableAllHooks": true}),
        })
    })
}

/// A fake that logs each start, answers `initialize` with fixture line 1 under the id it was
/// sent, prints line 2 (a `commands_changed`) and stays alive until killed.
fn answering_body(base: &Path) -> String {
    let fixture = fs::read_to_string(FIXTURE).unwrap();
    let lines: Vec<&str> = fixture.lines().collect();
    fs::write(base.join("answer.jsonl"), format!("{}\n", lines[0])).unwrap();
    fs::write(base.join("changed.jsonl"), format!("{}\n", lines[1])).unwrap();
    let dir = base.display();
    format!(
        "echo started >> '{dir}/starts.log'\nIFS= read -r line\n\
         id=$(printf '%s' \"$line\" | sed 's/.*\"request_id\":\"\\([^\"]*\\)\".*/\\1/')\n\
         sed \"s/probe-init-1/$id/\" '{dir}/answer.jsonl'\ncat '{dir}/changed.jsonl'\n\
         exec sleep 60\n"
    )
}

fn alive(pid: u32) -> bool {
    let pid = Pid::from_raw(i32::try_from(pid).unwrap());
    kill(pid, None) != Err(Errno::ESRCH)
}

/// The `commands` frames a tab was sent.
fn frames(tab: &Tab) -> Vec<Value> {
    let sent = tab.frames().into_iter();
    sent.filter(|frame| frame["type"] == "commands").collect()
}

fn names(frame: &Value) -> Vec<&str> {
    let entries = frame["entries"].as_array().unwrap();
    entries.iter().filter_map(|e| e["name"].as_str()).collect()
}

fn state_of(f: &super::Fixture, id: &str) -> CatalogueState {
    let cell = f.bridge.command_cell(id).unwrap();
    let table = cell.read().unwrap();
    table.state().clone()
}

/// The frames the first tab was sent: generations 1 then 2, the session's gate at generation 2.
fn assert_frames_and_gate(f: &super::Fixture, tab: &Tab, id: &str) -> Vec<Value> {
    wait_until("generation 2", || {
        frames(tab).iter().any(|frame| frame["generation"] == 2)
    });
    let sent = frames(tab);
    let generations: Vec<&Value> = sent.iter().map(|frame| &frame["generation"]).collect();
    assert_eq!(generations, [&json!(1), &json!(2)]);
    assert!(sent
        .iter()
        .all(|f| f["session"] == id && f["state"] == "ready"));
    assert!(
        names(&sent[1]).contains(&"probe-added"),
        "{:?}",
        names(&sent[1])
    );
    assert!(names(&sent[0]).contains(&"clear"));
    assert_eq!(state_of(f, id), CatalogueState::Ready { generation: 2 });
    sent
}

/// The route answers with the latest list, and a tab opened later is replayed it once.
fn assert_route_and_replay(f: &super::Fixture, id: &str, latest: &Value) {
    let route = call_route(&f.ctx, &format!("?session={id}"));
    let body: Value = serde_json::from_slice(&route.body).unwrap();
    assert_eq!(
        (&body["state"], &body["generation"]),
        (&json!("ready"), &json!(2))
    );
    assert_eq!(names(&body), names(latest));

    let late = f.connect(&browser());
    let replayed: Vec<&Value> = late
        .opened
        .iter()
        .filter(|m| m["type"] == "commands")
        .collect();
    assert_eq!(replayed.len(), 1, "{:?}", late.opened);
    assert_eq!(replayed[0]["generation"], 2);
}

#[test]
fn a_chat_has_one_side_process_whose_lists_reach_the_gate_the_route_and_new_tabs() {
    let f = fixture(false);
    let base = tempfile::tempdir().unwrap();
    let catalogue = f.ctx.extensions().get::<Catalogue>().unwrap();
    catalogue.set_builder(builder(base.path(), &answering_body(base.path())));
    let tab = f.connect(&browser());
    let spec = f.start(&tab);

    let sent = assert_frames_and_gate(&f, &tab, &spec.id);
    assert_route_and_replay(&f, &spec.id, &sent[1]);

    let starts = fs::read_to_string(base.path().join("starts.log")).unwrap();
    assert_eq!(starts.lines().count(), 1, "one start per chat session");
    let pids = catalogue.pids();
    assert_eq!(pids.len(), 1);
    assert!(alive(pids[0]));

    let ended = f.bridge.end(&spec.id, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("the side process to stop", || {
        catalogue.pids().is_empty() && !alive(pids[0])
    });
    let route = call_route(&f.ctx, &format!("?session={}", spec.id));
    let body: Value = serde_json::from_slice(&route.body).unwrap();
    assert_eq!(body["state"], "loading");
}

#[test]
fn a_side_process_that_never_answers_makes_the_list_unavailable_with_the_native_rows() {
    let f = fixture(false);
    let base = tempfile::tempdir().unwrap();
    let catalogue = f.ctx.extensions().get::<Catalogue>().unwrap();
    catalogue.set_builder(builder(base.path(), "exec sleep 60\n"));
    catalogue.set_deadline(Duration::from_millis(300));
    let tab = f.connect(&browser());
    let spec = f.start(&tab);

    wait_until("the unavailable frame", || !frames(&tab).is_empty());
    let sent = frames(&tab);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["state"], "unavailable");
    assert_eq!(sent[0]["generation"], Value::Null);
    assert!(sent[0]["reason"]
        .as_str()
        .is_some_and(|reason| !reason.is_empty()));
    assert!(sent[0]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["source"] == "built-in"));
    let unavailable = matches!(state_of(&f, &spec.id), CatalogueState::Unavailable { .. });
    assert!(unavailable, "the gate's table");
    let route = call_route(&f.ctx, &format!("?session={}", spec.id));
    let body: Value = serde_json::from_slice(&route.body).unwrap();
    assert_eq!(body["state"], "unavailable");
}

#[test]
fn a_session_whose_settings_cannot_be_read_gets_an_unavailable_list() {
    // The fake host writes no settings file, so the default builder cannot copy it.
    let f = fixture(false);
    let tab = f.connect(&browser());
    let spec = f.start(&tab);

    wait_until("the unavailable frame", || !frames(&tab).is_empty());
    assert_eq!(frames(&tab)[0]["state"], "unavailable");
    let unavailable = matches!(state_of(&f, &spec.id), CatalogueState::Unavailable { .. });
    assert!(unavailable);
    let catalogue = f.ctx.extensions().get::<Catalogue>().unwrap();
    assert!(catalogue.pids().is_empty());
}
