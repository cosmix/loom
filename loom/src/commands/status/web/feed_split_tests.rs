use std::collections::BTreeSet;

use serde_json::json;

use super::*;
use crate::status::data::{ContractTesting, StageSummary};

fn fixture() -> WebSnapshot {
    serde_json::from_str(include_str!(
        "../../../../../web/src/api/fixtures/snapshot.json"
    ))
    .expect("snapshot fixture parses")
}

fn stage_mut<'a>(snapshot: &'a mut WebSnapshot, id: &str) -> &'a mut StageSummary {
    snapshot
        .status
        .stages
        .iter_mut()
        .find(|stage| stage.id == id)
        .expect("fixture stage")
}

fn split(snapshot: &WebSnapshot) -> Split {
    Split::of(snapshot).expect("split snapshot")
}

fn key_set(map: &Map<String, Value>) -> BTreeSet<String> {
    map.keys().cloned().collect()
}

fn names(groups: &[&[&str]]) -> BTreeSet<String> {
    groups
        .iter()
        .flat_map(|group| group.iter())
        .map(|name| (*name).to_owned())
        .collect()
}

#[test]
fn every_snapshot_key_is_classified() {
    let mut snapshot = fixture();
    snapshot.notice = Some("notice".to_owned());
    assert!(snapshot.status.tokens.is_some(), "fixture carries tokens");
    let Value::Object(root) = serde_json::to_value(&snapshot).unwrap() else {
        panic!("snapshot is not an object");
    };
    let expected = names(&[
        &STATE_TOP_KEYS,
        &CLOCK_TOP_KEYS,
        &FULL_ONLY_KEYS,
        &["status"],
    ]);
    assert_eq!(
        key_set(&root),
        expected,
        "a snapshot key is missing from feed_split.rs"
    );
    let Value::Object(status) = &root["status"] else {
        panic!("status is not an object");
    };
    assert_eq!(
        key_set(status),
        names(&[&STATE_STATUS_KEYS, &["stages", "tokens"]]),
        "a status key is missing from feed_split.rs"
    );
    assert_eq!(
        key_set(&split(&snapshot).full_only),
        names(&[&FULL_ONLY_KEYS])
    );
}

#[test]
fn immutable_and_clock_keys_exist_on_stage_summary() {
    let stage = StageSummary {
        contract_testing: Some(ContractTesting {
            done: 0,
            total: 1,
            queued: false,
            running_secs: Some(3),
        }),
        ..StageSummary::default()
    };
    let Value::Object(object) = serde_json::to_value(stage).unwrap() else {
        panic!("stage is not an object");
    };
    for key in IMMUTABLE_STAGE_KEYS {
        assert!(object.contains_key(key), "no stage key {key}");
    }
    for (key, _) in CLOCK_STAGE_KEYS {
        assert!(object.contains_key(key), "no stage key {key}");
    }
    assert!(object["contract_testing"].get("running_secs").is_some());
}

#[test]
fn masking_hides_numbers_and_keeps_null() {
    let mut value = json!({"a": 5, "b": null, "c": "text", "d": 7});
    mask_clock(&mut value, &["a", "b", "c", "missing"]);
    assert_eq!(value, json!({"a": "#", "b": null, "c": "text", "d": 7}));
}

#[test]
fn a_clock_moving_between_numbers_is_not_a_state_change() {
    let prev = fixture();
    let mut next = fixture();
    let server = stage_mut(&mut next, "server");
    server.elapsed_secs = server.elapsed_secs.map(|secs| secs + 1);
    server.staleness_secs = Some(4);
    let (prev, next) = (split(&prev), split(&next));
    let (stages, status, top) = state_changes(&prev, &next);
    assert!(stages.is_empty() && status.is_empty() && top.is_empty());
    assert!(!needs_full(&prev, &next));
    let ticks = tick_stages(&prev, &next).unwrap();
    assert_eq!(
        Value::Object(ticks),
        json!({"server": {"elapsed": 906, "exec": 640, "stale": 4}})
    );
}

#[test]
fn a_clock_going_from_null_to_a_number_is_a_state_change() {
    let prev = fixture();
    let mut next = fixture();
    stage_mut(&mut next, "design").judge_heartbeat_secs = Some(5);
    let (stages, _, _) = state_changes(&split(&prev), &split(&next));
    assert_eq!(key_set(&stages), names(&[&["design"]]));
    assert_eq!(stages["design"]["judge_heartbeat_secs"], json!(5));
    assert!(stages["design"].get("name").is_none(), "no immutable keys");
}

#[test]
fn the_attention_judge_heartbeat_is_masked_between_numbers() {
    let mut prev = fixture();
    prev.attention[0].judge_heartbeat_secs = Some(10);
    let mut next = prev.clone();
    next.attention[0].judge_heartbeat_secs = Some(11);
    let (_, _, top) = state_changes(&split(&prev), &split(&next));
    assert!(top.is_empty());
    next.attention[0].judge_heartbeat_secs = None;
    let (_, _, top) = state_changes(&split(&prev), &split(&next));
    assert_eq!(key_set(&top), names(&[&["attention"]]));
}

#[test]
fn contract_running_secs_is_masked_but_progress_is_not() {
    let testing = |done, running_secs| {
        Some(ContractTesting {
            done,
            total: 3,
            queued: false,
            running_secs,
        })
    };
    let mut prev = fixture();
    stage_mut(&mut prev, "design").contract_testing = testing(1, Some(5));
    let mut next = prev.clone();
    stage_mut(&mut next, "design").contract_testing = testing(1, Some(6));
    let (stages, _, _) = state_changes(&split(&prev), &split(&next));
    assert!(stages.is_empty());
    let ticks = tick_stages(&split(&prev), &split(&next)).unwrap();
    assert_eq!(ticks["design"], json!({"elapsed": 0, "contract": 6}));
    stage_mut(&mut next, "design").contract_testing = testing(2, Some(6));
    let (stages, _, _) = state_changes(&split(&prev), &split(&next));
    assert_eq!(key_set(&stages), names(&[&["design"]]));
}

#[test]
fn a_cleared_notice_is_sent_as_null() {
    let mut prev = fixture();
    prev.notice = Some("degraded".to_owned());
    let next = fixture();
    let (_, _, top) = state_changes(&split(&prev), &split(&next));
    assert_eq!(Value::Object(top), json!({"notice": null}));
}

#[test]
fn stage_set_immutable_and_full_only_changes_need_a_full() {
    let base = split(&fixture());
    let mut removed = fixture();
    removed.status.stages.pop();
    assert!(needs_full(&base, &split(&removed)));
    let mut renamed = fixture();
    stage_mut(&mut renamed, "server").name = "renamed".to_owned();
    assert!(needs_full(&base, &split(&renamed)));
    let mut terminals = fixture();
    terminals.terminals = !terminals.terminals;
    assert!(needs_full(&base, &split(&terminals)));
    assert!(!needs_full(&base, &split(&fixture())));
}

#[test]
fn a_busy_line_appearing_or_clearing_needs_a_full() {
    let idle = fixture();
    let mut busy = fixture();
    busy.busy = Some(crate::commands::status::web::model::WebBusy {
        op: "brief-generation".to_owned(),
        what: "generating the brief for server".to_owned(),
        stage_id: "server".to_owned(),
        started_at: idle.generated_at,
        ceiling_secs: 300,
        global_ceiling_secs: 1800,
    });
    assert!(needs_full(&split(&idle), &split(&busy)));
    assert!(needs_full(&split(&busy), &split(&idle)));
    assert!(!needs_full(&split(&busy), &split(&busy)));
}
