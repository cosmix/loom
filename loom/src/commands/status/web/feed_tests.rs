use serde_json::json;

use super::super::feed_split::{CLOCK_STAGE_KEYS, IMMUTABLE_STAGE_KEYS, STATE_TOP_KEYS};
use super::*;
use crate::status::data::{ContractTesting, StageSummary};

pub(super) fn fixture() -> WebSnapshot {
    serde_json::from_str(include_str!(
        "../../../../../web/src/api/fixtures/snapshot.json"
    ))
    .expect("snapshot fixture parses")
}

pub(super) fn stage_mut<'a>(snapshot: &'a mut WebSnapshot, id: &str) -> &'a mut StageSummary {
    snapshot
        .status
        .stages
        .iter_mut()
        .find(|stage| stage.id == id)
        .expect("fixture stage")
}

/// Apply one message the way `web/src/state/merge.ts` does.
pub(super) fn reconstruct(view: &mut Value, frame: &str) {
    let frame: Value = serde_json::from_str(frame).expect("frame is JSON");
    match frame["t"].as_str().expect("frame tag") {
        "full" => *view = frame["snapshot"].clone(),
        "tick" => apply_tick(view, &frame),
        "state" => apply_state(view, &frame),
        "tokens" => {
            let status = view["status"].as_object_mut().expect("status object");
            match &frame["tokens"] {
                Value::Null => status.remove("tokens"),
                tokens => status.insert("tokens".to_owned(), tokens.clone()),
            };
        }
        other => panic!("unknown frame kind {other}"),
    }
}

fn apply_tick(view: &mut Value, frame: &Value) {
    view["generated_at"] = frame["at"].clone();
    view["tick_age_secs"] = frame["tick_age"].clone();
    let listed = frame["stages"].as_object().expect("tick stages");
    for stage in view["status"]["stages"].as_array_mut().expect("stages") {
        let Some(entry) = listed.get(stage["id"].as_str().expect("stage id")) else {
            continue;
        };
        for (key, name) in CLOCK_STAGE_KEYS {
            if let Some(value) = entry.get(name) {
                stage[key] = value.clone();
            }
        }
        if let Some(value) = entry.get("contract") {
            if !stage["contract_testing"].is_null() {
                stage["contract_testing"]["running_secs"] = value.clone();
            }
        }
    }
    for item in view["attention"].as_array_mut().expect("attention") {
        let judge = listed
            .get(item["id"].as_str().expect("attention id"))
            .and_then(|entry| entry.get("judge"));
        if let (Some(judge), false) = (judge, item["judge_heartbeat_secs"].is_null()) {
            item["judge_heartbeat_secs"] = judge.clone();
        }
    }
}

fn apply_state(view: &mut Value, frame: &Value) {
    view["generated_at"] = frame["at"].clone();
    if let Some(changed) = frame.get("stages").and_then(Value::as_object) {
        for stage in view["status"]["stages"].as_array_mut().expect("stages") {
            let Some(Value::Object(dynamic)) = changed.get(stage["id"].as_str().expect("id"))
            else {
                continue;
            };
            let mut next: Map<String, Value> = stage
                .as_object()
                .expect("stage object")
                .iter()
                .filter(|(key, _)| IMMUTABLE_STAGE_KEYS.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            next.extend(dynamic.clone());
            *stage = Value::Object(next);
        }
    }
    if let Some(changed) = frame.get("status").and_then(Value::as_object) {
        for (key, value) in changed {
            view["status"][key] = value.clone();
        }
    }
    for key in STATE_TOP_KEYS {
        match frame.get(key) {
            Some(Value::Null) if key == "notice" => {
                view.as_object_mut().expect("root").remove("notice");
            }
            Some(value) => view[key] = value.clone(),
            None => {}
        }
    }
}

/// A feed plus the view a client would hold, checked against the snapshot
/// after every accepted snapshot.
pub(super) struct Harness {
    feed: Feed,
    view: Value,
    base: Instant,
    pushes: u64,
}

impl Harness {
    pub(super) fn new() -> Self {
        Self {
            feed: Feed::default(),
            view: Value::Null,
            base: Instant::now(),
            pushes: 0,
        }
    }

    fn push_at(&mut self, snapshot: WebSnapshot, now: Instant) -> Vec<Value> {
        let frames = self.feed.accept(snapshot.clone(), now).expect("accept");
        for frame in &frames {
            reconstruct(&mut self.view, frame);
        }
        assert_eq!(
            self.view,
            serde_json::to_value(&snapshot).unwrap(),
            "the client view diverged from the snapshot"
        );
        frames
            .iter()
            .map(|frame| serde_json::from_str(frame).unwrap())
            .collect()
    }

    /// Accept one second after the previous push, well inside `FULL_INTERVAL`.
    pub(super) fn push(&mut self, snapshot: WebSnapshot) -> Vec<Value> {
        self.pushes += 1;
        let now = self.base + Duration::from_secs(self.pushes);
        self.push_at(snapshot, now)
    }
}

pub(super) fn kinds(frames: &[Value]) -> Vec<&str> {
    frames
        .iter()
        .map(|frame| frame["t"].as_str().unwrap())
        .collect()
}

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn first_accept_sends_only_a_full() {
    let mut harness = Harness::new();
    let frames = harness.push(fixture());
    assert_eq!(kinds(&frames), ["full"]);
    let bare: Value = serde_json::from_str(&harness.feed.snapshot_json().unwrap()).unwrap();
    assert_eq!(
        frames[0]["snapshot"], bare,
        "full carries the /api/status JSON"
    );
}

#[test]
fn clock_only_change_sends_only_a_tick_listing_moving_stages() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut next = fixture();
    next.generated_at += chrono::Duration::seconds(1);
    next.tick_age_secs = Some(5);
    let server = stage_mut(&mut next, "server");
    server.elapsed_secs = Some(906);
    server.execution_secs = Some(641);
    stage_mut(&mut next, "docs").staleness_secs = Some(41);
    let frames = harness.push(next);
    assert_eq!(kinds(&frames), ["tick"]);
    assert_eq!(frames[0]["tick_age"], json!(5));
    let mut listed = keys(&frames[0]["stages"]);
    listed.sort_unstable();
    assert_eq!(listed, ["docs", "server"], "completed stages are absent");
    assert_eq!(
        frames[0]["stages"]["server"],
        json!({"elapsed": 906, "exec": 641, "stale": 3})
    );
    assert!(frames[0]["stages"]["docs"].get("judge").is_none());
}

#[test]
fn an_unchanged_snapshot_still_sends_a_tick() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let frames = harness.push(fixture());
    assert_eq!(kinds(&frames), ["tick"]);
    assert_eq!(frames[0]["stages"], json!({}));
}

#[test]
fn status_change_sends_the_whole_dynamic_part() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut next = fixture();
    next.status.stages[1].held = true;
    next.status.stages[1].elapsed_secs = Some(910);
    let frames = harness.push(next);
    assert_eq!(kinds(&frames), ["state", "tick"]);
    let stage = &frames[0]["stages"]["server"];
    assert_eq!(stage["held"], json!(true));
    assert_eq!(stage["elapsed_secs"], json!(910));
    assert!(
        stage.get("model").is_some(),
        "unchanged dynamic keys are sent too"
    );
    for key in IMMUTABLE_STAGE_KEYS {
        assert!(stage.get(key).is_none(), "{key} is immutable");
    }
    assert_eq!(keys(&frames[0]["stages"]), ["server"]);
    assert!(frames[0].get("status").is_none() && frames[0].get("attention").is_none());
}

#[test]
fn status_keys_and_top_level_keys_are_sent_when_they_change() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut next = fixture();
    next.status.plan_name = Some("another plan".to_owned());
    next.status.progress.completed += 1;
    next.alerts.clear();
    let frames = harness.push(next);
    assert_eq!(kinds(&frames), ["state", "tick"]);
    assert_eq!(keys(&frames[0]["status"]), ["plan_name", "progress"]);
    assert_eq!(frames[0]["alerts"], json!([]));
    assert!(frames[0].get("stages").is_none());
}

#[test]
fn optional_key_disappearing_is_reconstructed() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut next = fixture();
    stage_mut(&mut next, "merge-gate").merge_block = None;
    let frames = harness.push(next);
    assert_eq!(kinds(&frames), ["state", "tick"]);
    assert!(frames[0]["stages"]["merge-gate"]
        .get("merge_block")
        .is_none());
}

#[test]
fn notice_set_then_cleared_sends_null() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut degraded = fixture();
    degraded.notice = Some("daemon lane degraded".to_owned());
    let frames = harness.push(degraded);
    assert_eq!(frames[0]["notice"], json!("daemon lane degraded"));
    let frames = harness.push(fixture());
    assert_eq!(kinds(&frames), ["state", "tick"]);
    assert_eq!(frames[0]["notice"], Value::Null);
    assert!(frames[0].as_object().unwrap().contains_key("notice"));
}

#[test]
fn judge_null_to_number_is_a_state_not_a_tick() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut next = fixture();
    stage_mut(&mut next, "design").judge_heartbeat_secs = Some(3);
    let frames = harness.push(next);
    assert_eq!(kinds(&frames), ["state", "tick"]);
    assert_eq!(
        frames[0]["stages"]["design"]["judge_heartbeat_secs"],
        json!(3)
    );
    let mut later = fixture();
    stage_mut(&mut later, "design").judge_heartbeat_secs = Some(4);
    let frames = harness.push(later);
    assert_eq!(kinds(&frames), ["tick"]);
    assert_eq!(frames[0]["stages"]["design"]["judge"], json!(4));
}

#[test]
fn contract_progress_is_a_state_and_its_clock_a_tick() {
    let testing = |done, running_secs| {
        Some(ContractTesting {
            done,
            total: 3,
            queued: false,
            running_secs,
        })
    };
    let mut harness = Harness::new();
    let mut first = fixture();
    stage_mut(&mut first, "design").contract_testing = testing(1, Some(5));
    harness.push(first);
    let mut second = fixture();
    stage_mut(&mut second, "design").contract_testing = testing(1, Some(6));
    assert_eq!(kinds(&harness.push(second)), ["tick"]);
    let mut third = fixture();
    stage_mut(&mut third, "design").contract_testing = testing(2, Some(7));
    assert_eq!(kinds(&harness.push(third)), ["state", "tick"]);
}

#[test]
fn tokens_change_sends_tokens_frame_and_none_sends_null() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut changed = fixture();
    let observed_at = changed.status.tokens.as_ref().unwrap().observed_at + 30;
    changed.status.tokens.as_mut().unwrap().observed_at = observed_at;
    let frames = harness.push(changed);
    assert_eq!(kinds(&frames), ["tokens", "tick"]);
    assert_eq!(frames[0]["tokens"]["observed_at"], json!(observed_at));
    let mut gone = fixture();
    gone.status.tokens = None;
    let frames = harness.push(gone);
    assert_eq!(kinds(&frames), ["tokens", "tick"]);
    assert_eq!(frames[0]["tokens"], Value::Null);
}

#[test]
fn stage_added_or_immutable_changed_sends_full() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut removed = fixture();
    removed.status.stages.pop();
    let frames = harness.push(removed.clone());
    assert_eq!(kinds(&frames), ["full"]);
    assert_eq!(
        frames[0]["snapshot"]["status"]["stages"]
            .as_array()
            .unwrap()
            .len(),
        removed.status.stages.len()
    );
    let mut renamed = removed;
    renamed.status.stages[0].name = "renamed".to_owned();
    assert_eq!(kinds(&harness.push(renamed.clone())), ["full"]);
    renamed.terminals = !renamed.terminals;
    assert_eq!(kinds(&harness.push(renamed)), ["full"]);
}

#[test]
fn full_is_resent_after_30s() {
    let mut harness = Harness::new();
    let base = harness.base;
    assert_eq!(kinds(&harness.push_at(fixture(), base)), ["full"]);
    let almost = base + FULL_INTERVAL - Duration::from_secs(1);
    assert_eq!(kinds(&harness.push_at(fixture(), almost)), ["tick"]);
    let due = base + FULL_INTERVAL;
    assert_eq!(kinds(&harness.push_at(fixture(), due)), ["full"]);
    let next = due + Duration::from_secs(2);
    assert_eq!(kinds(&harness.push_at(fixture(), next)), ["tick"]);
}

#[test]
fn the_primer_is_the_latest_full_even_after_deltas() {
    let mut harness = Harness::new();
    harness.push(fixture());
    let mut next = fixture();
    next.status.plan_name = Some("changed".to_owned());
    harness.push(next.clone());
    let primer = harness.feed.full_frame().expect("primer");
    let mut view = Value::Null;
    reconstruct(&mut view, &primer);
    assert_eq!(view, serde_json::to_value(&next).unwrap());
    assert!(Feed::default().full_frame().is_none());
}
