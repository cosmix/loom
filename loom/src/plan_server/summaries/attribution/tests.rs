//! Brief parsing, per-session claim release, cost lines, seal errors and turn naming.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tempfile::TempDir;

use super::scan::{brief_ids, scan};
use super::*;
use crate::planning::runtime::{create_runtime_dir, runtime_paths_under};
use crate::planning::summary::claims::create_claim_as;

mod costing;
mod sweep;

fn runtime() -> (TempDir, RuntimePaths) {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&root).expect("project root");
    let paths = runtime_paths_under(&tmp.path().join("loom-home"), &root).expect("paths");
    create_runtime_dir(&paths).expect("runtime dir");
    (tmp, paths)
}

fn claim(paths: &RuntimePaths, id: &str, session: Option<&str>) {
    let created = create_claim_as(paths, id, session.map(str::to_owned)).expect("claim");
    created.expect("no earlier claim");
}

fn claimed(paths: &RuntimePaths) -> Vec<String> {
    let claims = list_claims(paths).expect("list claims");
    claims.into_iter().map(|claim| claim.summary).collect()
}

/// An assistant record whose one block is a Bash `tool_use` running `command`.
fn bash(id: &str, command: &str) -> Value {
    json!({
        "type": "assistant", "uuid": format!("u-{id}"), "requestId": format!("req-{id}"),
        "timestamp": "2026-10-29T10:00:01.000Z",
        "message": {
            "id": format!("msg-{id}"), "model": "claude-haiku-4-5", "role": "assistant",
            "content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": command}}],
            "usage": {"input_tokens": 10, "cache_creation_input_tokens": 200,
                      "cache_read_input_tokens": 3000, "output_tokens": 40}
        }
    })
}

/// A user record carrying the result of tool call `id`.
fn result(id: &str, text: &str, is_error: bool) -> Value {
    let block =
        json!({"type": "tool_result", "tool_use_id": id, "content": text, "is_error": is_error});
    json!({"type": "user", "message": {"role": "user", "content": [block]}})
}

fn transcript(dir: &Path, name: &str, records: &[Value]) -> PathBuf {
    let path = dir.join(name);
    let body: String = records.iter().map(|record| format!("{record}\n")).collect();
    std::fs::write(&path, body).expect("transcript");
    path
}

fn owned(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn plain_brief_calls_bind_their_ids() {
    let brief = brief_ids;
    assert_eq!(
        brief("loom plan summary brief plans/a"),
        Some(owned(&["plans/a"]))
    );
    let noted =
        brief("loom plan summary brief plans/a overview/demo/product --note keep the order");
    assert_eq!(noted, Some(owned(&["plans/a", "overview/demo/product"])));
    let pathed = brief("/home/u/.local/bin/loom  plan summary brief plans/a");
    assert_eq!(pathed, Some(owned(&["plans/a"])));
    let compound = [
        "loom plan summary brief plans/a; rm -rf doc",
        "cd doc && loom plan summary brief plans/a",
        "loom plan summary brief plans/a | tee out",
        "loom plan summary brief `cat ids`",
        "loom plan summary brief $(cat ids)",
        "loom plan summary brief plans/a\nloom plan summary brief plans/b",
    ];
    for command in compound {
        assert_eq!(brief(command), None, "{command:?}");
    }
    assert_eq!(
        brief("loom plan summary seal plans/a"),
        None,
        "another verb"
    );
    assert_eq!(
        brief("loom plan summary brief ../etc/passwd"),
        None,
        "an invalid id"
    );
    assert_eq!(brief("loom plan summary brief --note x"), None, "no id");
    assert_eq!(
        brief("notloom plan summary brief plans/a"),
        None,
        "another program"
    );
}

#[test]
fn a_scan_keeps_each_briefed_id_once_and_the_model() {
    let tmp = TempDir::new().expect("temp dir");
    let records = [
        bash("t1", "loom plan summary brief plans/a"),
        bash("t2", "ls doc"),
        bash("t3", "loom plan summary brief plans/a plans/b"),
    ];
    let found = scan(&transcript(tmp.path(), "agent-x.jsonl", &records));
    assert_eq!(found.briefed, ["plans/a", "plans/b"]);
    assert_eq!(found.model.as_deref(), Some("claude-haiku-4-5"));
    let unread = scan(&tmp.path().join("missing.jsonl"));
    assert_eq!(unread, super::scan::Scan::default());
}

#[test]
fn a_subagent_releases_only_its_sessions_claims_on_what_it_briefed() {
    let (tmp, paths) = runtime();
    claim(&paths, "plans/a", Some("s1"));
    claim(&paths, "plans/b", Some("s2"));
    claim(&paths, "plans/c", Some("s1"));
    let briefed = [bash("t1", "loom plan summary brief plans/a plans/b")];
    let path = transcript(tmp.path(), "agent-x.jsonl", &briefed);
    let mut attribution = Attribution::new(paths.clone());
    attribution
        .on_subagent_stopped("s1", "x", &path)
        .expect("stop");
    assert_eq!(
        claimed(&paths),
        ["plans/b", "plans/c"],
        "b is s2's, c was not briefed"
    );
}

#[test]
fn one_summary_is_its_own_line_and_a_set_shares_one() {
    let (tmp, paths) = runtime();
    let mut attribution = Attribution::new(paths.clone());
    attribution.set_sizes(BTreeMap::from([("plans/a".to_owned(), 6_000)]));
    let triggers = [
        ("plans/a", Trigger::Button),
        ("plans/b", Trigger::Background),
    ];
    let mut triggers: BTreeMap<String, Trigger> = triggers
        .into_iter()
        .map(|(id, trigger)| (id.to_owned(), trigger))
        .collect();
    triggers.insert("plans/c".into(), Trigger::Background);
    attribution.summary_started("s1", triggers);
    let one = transcript(
        tmp.path(),
        "agent-x.jsonl",
        &[bash("t1", "loom plan summary brief plans/a")],
    );
    let set = [bash("t2", "loom plan summary brief plans/b plans/c")];
    let set = transcript(tmp.path(), "agent-y.jsonl", &set);
    let x = attribution.on_subagent_stopped("s1", "x", &one).expect("x");
    let y = attribution.on_subagent_stopped("s1", "y", &set).expect("y");
    assert!(x.turn_cost.is_none(), "a summary session has no turn");
    let log = cost::read_log(&paths).expect("log");
    assert_eq!(log.len(), 2);
    let first = (
        &log[0].summaries,
        log[0].trigger,
        log[0].level,
        log[0].source_bytes,
    );
    assert_eq!(
        first,
        (
            &owned(&["plans/a"]),
            Trigger::Button,
            Level::Document,
            Some(6_000)
        )
    );
    // The session is a press's: its background ids ran because of it, so their line is too.
    let second = (&log[1].summaries, log[1].trigger, log[1].source_bytes);
    assert_eq!(
        second,
        (&owned(&["plans/b", "plans/c"]), Trigger::Button, None)
    );
    assert_eq!(log[1].model.as_deref(), Some("claude-haiku-4-5"));
    assert_eq!((x.lines, y.lines), (log[..1].to_vec(), log[1..].to_vec()));
}

#[test]
fn a_summary_sessions_transcript_costs_what_no_subagent_briefed() {
    let (tmp, paths) = runtime();
    claim(&paths, "overview/demo/product", Some("s1"));
    let mut attribution = Attribution::new(paths.clone());
    let ids = ["overview/demo/product", "demo/product/section_01"];
    let triggers = ids.map(|id| (id.to_owned(), Trigger::Background));
    attribution.summary_started("s1", BTreeMap::from(triggers));
    let agent = [bash(
        "t1",
        "loom plan summary brief demo/product/section_01",
    )];
    let agent = transcript(tmp.path(), "agent-x.jsonl", &agent);
    attribution
        .on_subagent_stopped("s1", "x", &agent)
        .expect("x");
    let main = transcript(
        tmp.path(),
        "main.jsonl",
        &[bash("t2", "loom plan summary brief overview/demo/product")],
    );
    attribution.note_main_transcript("s1", Some(main));
    let lines = attribution.on_session_ended("s1").expect("end");
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (&lines[0].summaries, lines[0].level),
        (&owned(&["overview/demo/product"]), Level::Overview)
    );
    assert!(
        claimed(&paths).is_empty(),
        "the session's claims are cleared"
    );
    let again = attribution.on_session_ended("s1").expect("a second end");
    assert!(again.is_empty(), "costed once");
}

#[test]
fn the_last_failed_seal_names_the_error() {
    let tmp = TempDir::new().expect("temp dir");
    let failure = "drafts/plans/b.md:3: unknown citation \u{1b}[31ms-1\nsecond line";
    let records = [
        bash("t1", "loom plan summary seal plans/a"),
        result("t1", "plans/a sealed", false),
        bash("t2", "loom plan summary seal plans/b"),
        result("t2", failure, false),
        bash("t3", "ls drafts"),
        result("t3", "drafts/x: no such file", true),
    ];
    let path = transcript(tmp.path(), "main.jsonl", &records);
    let error = last_seal_error(&path).expect("the second seal failed");
    assert!(
        error.starts_with("drafts/plans/b.md:3: unknown citation"),
        "{error}"
    );
    assert!(
        !error.contains('\u{1b}') && !error.contains("second line"),
        "{error:?}"
    );
    let long = format!("drafts/a.md: {}", "x".repeat(500));
    let records = [
        bash("t1", "loom plan summary seal plans/a"),
        result("t1", &long, true),
    ];
    let path = transcript(tmp.path(), "long.jsonl", &records);
    let error = last_seal_error(&path).expect("an error result");
    assert!(error.chars().count() <= 200, "{}", error.len());
    let path = transcript(tmp.path(), "fine.jsonl", &records[..1]);
    assert_eq!(last_seal_error(&path), None);
}

/// One naming: the `(session, turn)` it belongs to, if any, and the rebuilt ids.
type Naming = (Option<(String, String)>, Vec<String>);

/// Records every naming of rebuilt summaries.
#[derive(Default)]
struct Recorded(Mutex<Vec<Naming>>);

impl RebuildingSink for Recorded {
    fn set_rebuilding(&self, turn: Option<(&str, &str)>, ids: Vec<String>) {
        let turn = turn.map(|(session, turn)| (session.to_owned(), turn.to_owned()));
        self.0.lock().unwrap().push((turn, ids));
    }
}

#[test]
fn a_chat_turn_hears_the_summaries_its_subagents_rebuilt() {
    let (tmp, paths) = runtime();
    let sink = Arc::new(Recorded::default());
    let mut attribution = Attribution::new(paths.clone()).with_sink(sink.clone());
    attribution.chat_started("a1");
    attribution.turn_started("a1", "t1");
    let running = [bash("u1", "loom plan summary brief plans/r")];
    let running = transcript(tmp.path(), "agent-r.jsonl", &running);
    attribution.turn_ended("a1", "t1", &[("r".to_owned(), running.clone())]);
    let stopped = [bash("u2", "loom plan summary brief plans/x")];
    let stopped = transcript(tmp.path(), "agent-x.jsonl", &stopped);
    let attributed = attribution
        .on_subagent_stopped("a1", "x", &stopped)
        .expect("x");
    let named = |ids: &[&str]| (Some(("a1".to_owned(), "t1".to_owned())), owned(ids));
    let expected = vec![named(&["plans/r"]), named(&["plans/r", "plans/x"])];
    assert_eq!(*sink.0.lock().unwrap(), expected);
    let cost = attributed.turn_cost.expect("the turn's rows");
    assert_eq!((cost.session.as_str(), cost.turn.as_str()), ("a1", "t1"));
    assert_eq!(cost.rows.len(), 1);
    assert_eq!(cost.rows[0].id, "plans/x");
    assert!(
        cost.rows[0].tokens.is_some(),
        "one summary has its own tokens"
    );
    assert_eq!(attributed.lines[0].trigger, Trigger::Refinement);
    attribution.turn_ended("a1", "t1", &[("r".to_owned(), running)]);
    assert_eq!(sink.0.lock().unwrap().len(), 2, "a subagent named once");
}

#[test]
fn only_terminal_claims_are_released() {
    let (_tmp, paths) = runtime();
    claim(&paths, "plans/a", None);
    claim(&paths, "plans/b", Some("s1"));
    let mut attribution = Attribution::new(paths.clone());
    let released = attribution
        .release_terminal(&owned(&["plans/a", "plans/b"]))
        .expect("release");
    assert_eq!(released, ["plans/a"]);
    assert_eq!(claimed(&paths), ["plans/b"]);
}

#[test]
fn a_vision_summary_is_a_document_level_rebuild() {
    assert_eq!(level_of("demo/product/vision"), Level::Document);
    assert_eq!(level_of("overview/demo/product"), Level::Overview);
}
