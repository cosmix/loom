//! Contracts for summary rebuilds: the scheduler's rules, the claims the server clears and the
//! `Rebuilds` facade's broadcasts.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use loom::plan_server::summaries::attribution::Attribution;
use loom::plan_server::summaries::rebuilds::Rebuilds;
use loom::plan_server::summaries::scheduler::{
    AnswerRight, Scheduler, SchedulerConfig, SchedulerEvent, SessionSpawner, SuspendReason,
};
use loom::plan_server::wire::message::PlanMessage;
use loom::planning::runtime::{create_runtime_dir, runtime_paths_under, RuntimePaths};
use loom::planning::summary::claims::list_claims;
use loom::planning::summary::cost::Trigger;
use loom::planning::summary::engine::EngineUpdate;
use loom::planning::summary::evaluate::{Evaluation, SummaryEval, SummaryState};
use loom::planning::summary::store::Scope;
use tempfile::TempDir;

/// Records every start and stop; sessions are named `s1`, `s2`, ... in start order.
#[derive(Default)]
struct Stub {
    starts: Mutex<Vec<Vec<String>>>,
    stops: Mutex<Vec<(String, bool)>>,
}

impl SessionSpawner for Stub {
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        let mut starts = self.starts.lock().unwrap();
        starts.push(ids.to_vec());
        Ok(format!("s{}", starts.len()))
    }

    fn stop(&self, session: &str, release_claims: bool) {
        let mut stops = self.stops.lock().unwrap();
        stops.push((session.to_string(), release_claims));
    }
}

impl Stub {
    /// Each start's ids, sorted.
    fn starts(&self) -> Vec<Vec<String>> {
        let starts = self.starts.lock().unwrap();
        starts.iter().map(|ids| sorted(ids.clone())).collect()
    }

    fn stops(&self) -> Vec<(String, bool)> {
        self.stops.lock().unwrap().clone()
    }
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn plans(count: usize) -> Vec<String> {
    (1..=count).map(|i| format!("plans/p{i:02}")).collect()
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    ids
}

/// A clock origin, a recording spawner and a scheduler with the limit 5 and a 10-minute stall.
fn setup(read_only: Option<&str>) -> (Instant, Stub, Scheduler) {
    let scheduler = Scheduler::new(SchedulerConfig {
        read_only: read_only.map(str::to_string),
        limit: 5,
        no_seal_timeout: Duration::from_secs(10 * 60),
    });
    (Instant::now(), Stub::default(), scheduler)
}

/// The open question's id and ids, when the events ask.
fn asked(events: &[SchedulerEvent]) -> Option<(String, Vec<String>)> {
    events.iter().find_map(|event| match event {
        SchedulerEvent::Asked(question) => Some((question.id.clone(), question.ids.clone())),
        _ => None,
    })
}

fn started(events: &[SchedulerEvent]) -> bool {
    let start = |event: &SchedulerEvent| matches!(event, SchedulerEvent::Started { .. });
    events.iter().any(start)
}

#[test]
fn auto_rebuild_up_to_five() {
    let (now, stub, mut five) = setup(None);
    let events = five.on_update(&plans(5), &plans(5), now, &stub);
    assert_eq!(stub.starts(), vec![plans(5)], "5 ids start one session");
    assert!(started(&events));
    assert!(asked(&events).is_none(), "5 ids are within the limit");

    let (now, stub, mut six) = setup(None);
    let events = six.on_update(&plans(6), &plans(6), now, &stub);
    assert!(stub.starts().is_empty(), "6 ids exceed the limit");
    let (_, question) = asked(&events).expect("6 ids ask");
    assert_eq!(sorted(question), plans(6));
    assert!(six.panel_state().question.is_some());
}

#[test]
fn first_start_asks() {
    let mut all: Vec<String> = (1..=12)
        .map(|i| format!("product/section_{i:02}"))
        .collect();
    all.extend(plans(8));
    all.extend(ids(&["product", "engineering"]));
    let (now, stub, mut s) = setup(None);
    let events = s.on_update(&all, &all, now, &stub);
    assert!(stub.starts().is_empty(), "missing is not free");
    let (_, question) = asked(&events).expect("the first start asks");
    assert_eq!(sorted(question), sorted(all));
}

#[test]
fn decline_counts_only_new() {
    let (now, stub, mut s) = setup(None);
    let events = s.on_update(&plans(8), &plans(8), now, &stub);
    let (question, _) = asked(&events).expect("8 ids ask");
    s.answer(&question, false, AnswerRight::Holder, now, &stub)
        .expect("the holder declines");
    assert!(stub.starts().is_empty());
    assert_eq!(sorted(s.panel_state().declined), plans(8));

    let fresh = ids(&["plans/n1", "plans/n2"]);
    let mut out_of_date = plans(8);
    out_of_date.extend(fresh.clone());
    let events = s.on_update(&fresh, &out_of_date, now, &stub);
    assert!(asked(&events).is_none(), "the declined 8 count no more");
    assert_eq!(stub.starts(), vec![fresh]);
}

#[test]
fn no_start_during_turn() {
    let (now, stub, mut s) = setup(None);
    let three = ids(&["plans/a", "plans/b", "plans/c"]);
    s.turn_started();
    let _ = s.on_update(&three, &three, now, &stub);
    assert!(stub.starts().is_empty(), "no start during a turn");
    let still = ids(&["plans/a", "plans/b"]);
    s.turn_ended(&still, now, &stub);
    assert_eq!(stub.starts(), vec![still], "start at turn end");

    let (now, stub, mut s) = setup(None);
    let token = s.suspend(SuspendReason::PressureRun);
    let two = ids(&["plans/d", "plans/e"]);
    let _ = s.on_update(&two, &two, now, &stub);
    assert!(stub.starts().is_empty(), "no start during a pressure run");
    s.resume(token, &two, now, &stub);
    assert_eq!(stub.starts(), vec![two], "resume starts the queue");
}

#[test]
fn one_summary_session_at_a_time() {
    let (now, stub, mut s) = setup(None);
    let ab = ids(&["plans/a", "plans/b"]);
    let c = ids(&["plans/c"]);
    let _ = s.on_update(&ab, &ab, now, &stub);
    assert_eq!(stub.starts(), vec![ab.clone()]);
    let abc = ids(&["plans/a", "plans/b", "plans/c"]);
    let _ = s.on_update(&c, &abc, now, &stub);
    assert_eq!(stub.starts().len(), 1, "no second session");
    assert_eq!(s.panel_state().queue, c, "c waits as the next batch");
    s.seal_seen("plans/a", now);
    s.seal_seen("plans/b", now);
    s.session_ended("s1", now, &stub);
    assert_eq!(stub.starts(), vec![ab, c], "c starts once s1 ends");
}

#[test]
fn stalled_session_stopped() {
    let (t0, stub, mut s) = setup(None);
    let at = |minutes: u64| t0 + Duration::from_secs(minutes * 60);
    let ab = ids(&["plans/a", "plans/b"]);
    let _ = s.on_update(&ab, &ab, t0, &stub);
    assert_eq!(stub.starts().len(), 1);
    s.seal_seen("plans/a", at(4));
    let events = s.tick(at(13), &stub);
    assert!(stub.stops().is_empty(), "9 minutes since a seal");
    let stop = |event: &SchedulerEvent| matches!(event, SchedulerEvent::Stopped { .. });
    assert!(!events.iter().any(stop));

    let events = s.tick(at(15), &stub);
    assert_eq!(stub.stops(), vec![("s1".to_string(), true)]);
    assert!(events.iter().any(stop), "a stall stops s1");
    let failed: Vec<Vec<String>> = events
        .iter()
        .filter_map(|event| match event {
            SchedulerEvent::Failed { ids, .. } => Some(ids.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(failed, vec![ids(&["plans/b"])], "the unsealed id");
}

#[test]
fn session_stopped_when_claims_done() {
    let (now, stub, mut s) = setup(None);
    let ab = ids(&["plans/a", "plans/b"]);
    let _ = s.on_update(&ab, &ab, now, &stub);
    s.claims_changed("s1", &[], now, &stub);
    assert!(stub.stops().is_empty(), "no claim taken yet");
    s.claims_changed("s1", &ab, now, &stub);
    s.claims_changed("s1", &ids(&["plans/b"]), now, &stub);
    assert!(stub.stops().is_empty(), "b is still claimed");
    s.claims_changed("s1", &[], now, &stub);
    s.claims_changed("s1", &[], now, &stub);
    let stops = stub.stops();
    assert_eq!(stops.len(), 1, "stop exactly once");
    assert_eq!(stops[0].0, "s1");
}

/// A runtime directory under a TempDir, with an empty claims directory.
fn runtime() -> (TempDir, RuntimePaths) {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&root).expect("project root");
    let paths = runtime_paths_under(&tmp.path().join("loom-home"), &root).expect("runtime paths");
    create_runtime_dir(&paths).expect("runtime dir");
    std::fs::create_dir_all(&paths.claims).expect("claims dir");
    let mode = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(&paths.claims, mode).expect("claims dir mode");
    (tmp, paths)
}

/// Writes the claim file of `id`, as `loom plan summary brief` does.
fn claim(paths: &RuntimePaths, id: &str, session: Option<&str>) {
    let body = serde_json::json!({
        "summary": id, "session": session, "pid": 1, "created_at": "2026-10-29T10:00:00Z"
    });
    let file = paths.claims.join(format!("{}.json", id.replace('/', "__")));
    std::fs::write(file, body.to_string()).expect("claim file");
}

fn claimed(paths: &RuntimePaths) -> Vec<String> {
    let claims = list_claims(paths).expect("list claims");
    claims.into_iter().map(|claim| claim.summary).collect()
}

/// A subagent transcript line: an assistant Bash `tool_use` running `COMMAND`.
const BASH_LINE: &str = r#"{"type":"assistant","uuid":"x1","parentUuid":null,"isSidechain":true,"agentId":"x","timestamp":"2026-10-29T10:00:01.000Z","sessionId":"105e2c4a-7f1b-4c3d-9a8e-2b6d0f1e3a57","requestId":"req_x1","message":{"id":"msg_x1","model":"claude-sonnet-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_x1","name":"Bash","input":{"command":"COMMAND","description":"Brief the summary"}}],"stop_reason":"tool_use","usage":{"input_tokens":10,"cache_creation_input_tokens":200,"cache_read_input_tokens":3000,"output_tokens":40}}}"#;

#[test]
fn claims_cleared_on_session_end() {
    let (tmp, paths) = runtime();
    claim(&paths, "plans/a", Some("s1"));
    claim(&paths, "plans/b", Some("s1"));
    claim(&paths, "plans/c", Some("s2"));
    let transcript = tmp.path().join("agent-x.jsonl");
    let line = BASH_LINE.replace("COMMAND", "loom plan summary brief plans/c");
    std::fs::write(&transcript, format!("{line}\n")).expect("transcript");

    let mut attribution = Attribution::new(paths.clone());
    let ended = attribution.on_session_ended("s1");
    ended.expect("session end clears its claims");
    assert_eq!(claimed(&paths), ids(&["plans/c"]), "only s1's go");
    let stopped = attribution.on_subagent_stopped("s2", "x", &transcript);
    stopped.expect("subagent stop releases what it briefed");
    assert!(claimed(&paths).is_empty(), "agent x briefed c");
}

#[test]
fn answer_only_by_holder_or_free() {
    let (now, stub, mut s) = setup(None);
    let events = s.on_update(&plans(6), &plans(6), now, &stub);
    let (question, _) = asked(&events).expect("6 ids ask");
    let refused = s.answer(&question, true, AnswerRight::NotAllowed, now, &stub);
    assert!(refused.is_err(), "no right, no answer");
    let open = s.panel_state().question.expect("the question stays open");
    assert_eq!(open.id, question);
    assert!(stub.starts().is_empty());

    s.answer(&question, true, AnswerRight::FreeAuthoring, now, &stub)
        .expect("anyone answers while authoring is free");
    assert!(s.panel_state().question.is_none());
    assert_eq!(stub.starts(), vec![plans(6)]);
}

#[test]
fn read_only_starts_nothing() {
    let (now, stub, mut s) = setup(Some("tmux is not installed"));
    let mut events = s.on_update(&plans(2), &plans(2), now, &stub);
    events.extend(s.on_update(&plans(9), &plans(9), now, &stub));
    events.extend(s.tick(now + Duration::from_secs(5), &stub));
    assert!(stub.starts().is_empty(), "read-only spawns nothing");
    assert!(!started(&events));
    assert!(asked(&events).is_none(), "read-only asks nothing");
    let view = s.panel_state();
    assert_eq!(view.read_only.as_deref(), Some("tmux is not installed"));
    assert!(view.question.is_none());
}

#[test]
fn startup_sweeps_dead_claims() {
    let (_tmp, paths) = runtime();
    claim(&paths, "plans/dead", Some("dead1"));
    claim(&paths, "plans/live", Some("live1"));
    claim(&paths, "plans/left", Some("left1"));
    claim(&paths, "plans/term", None);
    let mut attribution = Attribution::new(paths.clone());
    let swept = attribution.sweep_after_recovery(&["live1"], &["left1"]);
    let terminal = swept.expect("sweep");
    assert_eq!(terminal, ids(&["plans/term"]), "offered for release");
    let kept = ids(&["plans/left", "plans/live", "plans/term"]);
    assert_eq!(claimed(&paths), kept, "only dead1's claim goes");
}

#[test]
fn turn_suspension_released_on_idle() {
    let (now, stub, mut s) = setup(None);
    let two = ids(&["plans/a", "plans/b"]);
    s.turn_started();
    let _ = s.on_update(&two, &two, now, &stub);
    assert!(stub.starts().is_empty());
    s.authoring_state_idle("a1", &two, now, &stub);
    assert_eq!(stub.starts(), vec![two], "idle ends the turn");
}

/// An update in which every id is a missing product section listed by the vision.
fn missing_sections(ids: &[String]) -> EngineUpdate {
    let eval = |id: &String| SummaryEval {
        id: id.clone(),
        source: Some(PathBuf::from(format!("doc/{id}.md"))),
        state: SummaryState::Missing,
        file: None,
        sentences: Vec::new(),
        claim: None,
        error: None,
    };
    let evaluation = Evaluation {
        summaries: ids.iter().map(|id| (id.clone(), eval(id))).collect(),
        scope: Scope {
            product: ids.to_vec(),
            ..Scope::default()
        },
        orphans: Vec::new(),
    };
    EngineUpdate {
        evaluation,
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated: ids.to_vec(),
    }
}

/// The step's last `concept_changed` names the sections' pages and the product overview.
fn assert_concept_changed(sent: &Mutex<Vec<PlanMessage>>, sections: &[String], step: &str) {
    let messages = std::mem::take(&mut *sent.lock().unwrap());
    let changed = messages
        .iter()
        .rev()
        .find_map(|message| match message {
            PlanMessage::ConceptChanged(changed) => Some(changed.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{step} broadcasts no concept_changed"));
    let mut pages = sections.to_vec();
    pages.push("product".to_string());
    assert_eq!(sorted(changed.pages), sorted(pages), "{step}");
    let out_of_date = sorted(changed.out_of_date);
    assert_eq!(out_of_date, sorted(sections.to_vec()), "{step}");
    assert!(changed.sources.is_empty(), "{step}: no source");
}

#[test]
fn rebuild_state_change_notifies_concept() {
    let stub = Arc::new(Stub::default());
    let sent: Arc<Mutex<Vec<PlanMessage>>> = Arc::default();
    let sink = sent.clone();
    let broadcast = Arc::new(move |m: &PlanMessage| sink.lock().unwrap().push(m.clone()));
    let rebuilds = Rebuilds::new_for_test(stub.clone(), None, broadcast);
    let sections: Vec<String> = (1..=6).map(|i| format!("product/section_{i:02}")).collect();

    rebuilds.on_update(&missing_sections(&sections));
    assert_concept_changed(&sent, &sections, "opening the question");
    let question = rebuilds.panel().question.expect("6 ids ask").id;
    let answered = rebuilds.answer(&question, true, AnswerRight::Holder);
    answered.expect("the holder answers");
    assert_eq!(stub.starts(), vec![sorted(sections.clone())]);
    assert_concept_changed(&sent, &sections, "starting the rebuild");
}

#[test]
fn rebuild_route_starts_session() {
    let stub = Arc::new(Stub::default());
    let rebuilds = Rebuilds::new_for_test(stub.clone(), None, Arc::new(|_: &PlanMessage| {}));
    rebuilds.enqueue(plans(9), Trigger::Button);
    assert_eq!(stub.starts(), vec![plans(9)], "above the limit");
    assert!(rebuilds.panel().question.is_none(), "a button never asks");
}
