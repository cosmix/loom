//! Teammates publish progress and usage without becoming nested chat tools.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use super::harness::{Harness, Sess};
use super::registry::{Feed, FeedPaths, Feeds};
use super::team::{TeammateState, TeammateStatus};
use super::tests::{append_record, assistant_tools, pressure, wait_for, wait_until};
use super::BridgeEvent;
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::{Item, TokenCounts};

const PRIVATE_TEXT: &str = "private teammate draft text";
const DOCUMENT: &str = "doc/product/f/section_02.md";

fn agent_path(s: &Sess, agent: &str, meta: Value) -> PathBuf {
    let dir = s.transcript.with_extension("").join("subagents");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("agent-{agent}.meta.json")),
        serde_json::to_vec(&meta).unwrap(),
    )
    .unwrap();
    dir.join(format!("agent-{agent}.jsonl"))
}

fn teammate_record(message: &str, tool: &str, document: &Path) -> Value {
    json!({"type": "assistant", "message": {"id": message, "role": "assistant",
        "content": [
            {"type": "text", "text": PRIVATE_TEXT},
            {"type": "tool_use", "id": "tool-mate", "name": tool,
                "input": {"file_path": document}}
        ], "stop_reason": "tool_use",
        "usage": {"input_tokens": 100, "output_tokens": 7,
            "cache_creation_input_tokens": 3, "cache_read_input_tokens": 5}}})
}

fn tokens() -> TokenCounts {
    TokenCounts {
        input: 100,
        cache_write: 3,
        cache_read: 5,
        output: 7,
    }
}

fn latest_team(h: &Harness, s: &Sess) -> Option<Vec<TeammateStatus>> {
    h.events
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|event| match event {
            BridgeEvent::Team { id, teammates } if *id == s.id => Some(teammates.clone()),
            _ => None,
        })
}

fn assert_hidden(h: &Harness) {
    let snapshot = serde_json::to_string(&h.bridge.chat_snapshot(true)).unwrap();
    let deltas = serde_json::to_string(&*h.deltas.lock().unwrap()).unwrap();
    for rendered in [snapshot, deltas] {
        assert!(!rendered.contains(PRIVATE_TEXT), "{rendered}");
        assert!(!rendered.contains("tool-mate"), "{rendered}");
    }
}

#[test]
fn a_teammate_reports_its_document_and_tokens_without_chat_items() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    let before = h.bridge.chat_snapshot(true).turns.pop().unwrap().items;
    let path = agent_path(
        &s,
        "mate",
        json!({"taskKind": "in_process_teammate", "name": "Drafter"}),
    );
    let document = h.bridge.config.project_root.join(DOCUMENT);

    append_record(
        &path,
        teammate_record("msg-team", "Write", &document),
        &s.id,
    );
    let team = wait_for("teammate progress", || latest_team(&h, &s));

    assert_eq!(
        team,
        [TeammateStatus {
            agent_id: "mate".to_owned(),
            name: "Drafter".to_owned(),
            documents: vec![DOCUMENT.to_owned()],
            current: Some(DOCUMENT.to_owned()),
            state: TeammateState::Working,
            tokens: tokens(),
        }]
    );
    let snapshot = h.bridge.chat_snapshot(true);
    let turn = snapshot.turns.last().unwrap();
    assert!(turn.open);
    assert_eq!(turn.tokens, tokens());
    assert_eq!(turn.items, before);
    assert_hidden(&h);
}

#[test]
fn metadata_published_after_transcript_keeps_teammate_lines_out_of_chat() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    let before = h.bridge.chat_snapshot(true).turns.pop().unwrap().items;
    let path = s
        .transcript
        .with_extension("")
        .join("subagents/agent-mate.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    append_record(
        &path,
        teammate_record("msg-delayed", "Write", Path::new(DOCUMENT)),
        &s.id,
    );
    let mut feed = Feed::new(s.dir.events());
    feed.metadata_grace = Duration::MAX;
    feed.sync(&FeedPaths {
        events: s.dir.events(),
        transcript: Some(s.transcript.clone()),
        tokens: Vec::new(),
    });
    for _ in 0..2 {
        let pending = feed.read();
        assert!(pending.agents.is_empty());
        assert!(pending.teammates.is_empty());
        assert!(pending.tokens.is_empty());
    }
    assert_hidden(&h);

    agent_path(
        &s,
        "mate",
        json!({"taskKind": "in_process_teammate", "name": "Drafter"}),
    );
    let mut feeds = Feeds::from([(s.id.clone(), feed)]);
    h.bridge.tick(&mut feeds);
    let team = wait_for("the delayed teammate", || latest_team(&h, &s));

    assert_eq!(team[0].name, "Drafter");
    assert_eq!(team[0].tokens, tokens());
    assert_eq!(team[0].current.as_deref(), Some(DOCUMENT));
    let turn = h.bridge.chat_snapshot(true).turns.pop().unwrap();
    assert_eq!(turn.tokens, tokens());
    assert_eq!(turn.items, before);
    assert_hidden(&h);
}

#[test]
fn an_ordinary_subagent_keeps_its_nested_tools_and_usage() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    h.record(
        &s,
        assistant_tools("msg-spawn", &[("spawn", "Agent", json!({}))]),
    );
    wait_until("the spawning tool", || {
        h.bridge.chat_snapshot(true).turns.iter().any(|turn| {
            turn.items.iter().any(
                |item| matches!(item, Item::Tool { tool_use_id, .. } if tool_use_id == "spawn"),
            )
        })
    });
    let path = agent_path(
        &s,
        "helper",
        json!({"toolUseId": "spawn", "name": "Helper"}),
    );
    append_record(
        &path,
        teammate_record("msg-helper", "Edit", Path::new(DOCUMENT)),
        &s.id,
    );

    wait_until("the nested tool", || {
        h.bridge.chat_snapshot(true).turns.iter().any(|turn| {
            turn.items.iter().any(|item| {
                matches!(item, Item::Tool {
                subagent: Some(agent), parent: Some(parent), name, ..
            } if agent == "helper" && parent == "spawn" && name == "Edit")
            })
        })
    });
    let turn = h.bridge.chat_snapshot(true).turns.pop().unwrap();
    assert_eq!(
        turn.tokens,
        TokenCounts {
            input: 102,
            output: 16,
            ..tokens()
        }
    );
    assert!(latest_team(&h, &s).is_none());
}

#[test]
fn pressure_registration_of_a_teammate_counts_usage_once() {
    let h = Harness::ready(|_| {});
    let s = h.idle(pressure());
    h.working(&s);
    let path = agent_path(&s, "mate", json!({"taskKind": "in_process_teammate"}));
    h.bridge.add_token_transcript(&s.id, path.clone()).unwrap();
    append_record(
        &path,
        teammate_record("msg-team", "Edit", Path::new(DOCUMENT)),
        &s.id,
    );

    let team = wait_for("the pressure teammate", || latest_team(&h, &s));

    assert_eq!(team[0].name, "mate");
    assert_eq!(team[0].current.as_deref(), Some(DOCUMENT));
    assert_eq!(team[0].tokens, tokens());
    assert_eq!(
        h.bridge.chat_snapshot(true).turns.pop().unwrap().tokens,
        tokens()
    );
    let transcripts = h.bridge.transcripts(&s.id).unwrap();
    assert!(transcripts
        .subagents
        .contains(&("mate".to_owned(), path.clone())));
    assert_hidden(&h);
    replay_token_feeds(&h, &s, path);
    assert_eq!(
        h.bridge.chat_snapshot(true).turns.pop().unwrap().tokens,
        tokens()
    );
}

fn replay_token_feeds(h: &Harness, s: &Sess, path: PathBuf) {
    // Exercise both routes together even if the driver discovered them on different ticks.
    let mut feed = Feed::new(s.dir.events());
    feed.sync(&FeedPaths {
        events: s.dir.events(),
        transcript: Some(s.transcript.clone()),
        tokens: vec![path],
    });
    let batch = feed.read();
    assert_eq!(batch.tokens.len(), 2);
    h.bridge.step(&s.id, |session, _, step| {
        for line in &batch.tokens {
            let deltas = session.mapper.ingest_tokens_only(line);
            session.emit(deltas, step);
        }
    });
}

fn feed_fixture(meta: Option<&[u8]>) -> (tempfile::TempDir, Feed, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let transcript = tmp.path().join("session.jsonl");
    fs::write(&transcript, b"").unwrap();
    let dir = tmp.path().join("session/subagents");
    fs::create_dir_all(&dir).unwrap();
    if let Some(meta) = meta {
        fs::write(dir.join("agent-mate.meta.json"), meta).unwrap();
    }
    let path = dir.join("agent-mate.jsonl");
    append_record(
        &path,
        teammate_record("msg-team", "Write", Path::new(DOCUMENT)),
        "session",
    );
    let mut feed = Feed::new(tmp.path().join("events.jsonl"));
    feed.sync(&FeedPaths {
        events: tmp.path().join("events.jsonl"),
        transcript: Some(transcript),
        tokens: Vec::new(),
    });
    (tmp, feed, path)
}

#[test]
fn teammate_classification_is_cached_and_excluded_from_ordered_entries() {
    let meta = br#"{"taskKind":"in_process_teammate","name":"Drafter"}"#;
    let (_tmp, mut feed, path) = feed_fixture(Some(meta));
    let first = feed.read();
    assert!(first.agents.is_empty());
    assert!(first.in_time_order().is_empty());
    assert_eq!(first.tokens.len(), 1);
    assert_eq!(first.teammates[0].1, "Drafter");
    fs::remove_file(path.with_extension("meta.json")).unwrap();
    append_record(
        &path,
        teammate_record("msg-next", "Edit", Path::new(DOCUMENT)),
        "session",
    );

    let next = feed.read();

    assert!(next.agents.is_empty());
    assert_eq!(next.teammates[0].1, "Drafter");
    assert_eq!(next.tokens.len(), 1);
    assert!(feed.read().teammates.is_empty());
}

#[test]
fn absent_invalid_and_oversized_metadata_fall_back_to_ordinary_subagents() {
    let oversized = serde_json::to_vec(&json!({
        "taskKind": "in_process_teammate", "padding": " ".repeat(64 * 1024)
    }))
    .unwrap();
    for meta in [None, Some(b"{".as_slice()), Some(oversized.as_slice())] {
        let (_tmp, mut feed, path) = feed_fixture(meta);
        if meta.is_none() || meta == Some(b"{".as_slice()) {
            feed.metadata_grace = Duration::MAX;
            let pending = feed.read();
            assert!(pending.agents.is_empty());
            assert!(pending.in_time_order().is_empty());
            assert!(pending.teammates.is_empty());
            assert!(pending.tokens.is_empty());
            feed.metadata_grace = Duration::ZERO;
        }

        let batch = feed.read();

        assert_eq!(batch.agents.len(), 1);
        assert_eq!(batch.in_time_order().len(), 1);
        assert!(batch.teammates.is_empty());
        assert!(batch.tokens.is_empty());
        fs::write(
            path.with_extension("meta.json"),
            br#"{"taskKind":"in_process_teammate"}"#,
        )
        .unwrap();
        append_record(
            &path,
            teammate_record("msg-next", "Edit", Path::new(DOCUMENT)),
            "session",
        );
        let next = feed.read();
        assert_eq!(next.agents.len(), 1);
        assert!(next.teammates.is_empty());
        assert!(next.tokens.is_empty());
    }
}

#[test]
fn symlinked_teammate_metadata_is_never_followed() {
    let (tmp, mut feed, path) = feed_fixture(None);
    let target = tmp.path().join("meta.json");
    fs::write(&target, br#"{"taskKind":"in_process_teammate"}"#).unwrap();
    std::os::unix::fs::symlink(target, path.with_extension("meta.json")).unwrap();

    let batch = feed.read();

    assert_eq!(batch.agents.len(), 1);
    assert!(batch.teammates.is_empty());
}

#[test]
fn unchanged_teammate_status_emits_no_new_team_event() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    let path = agent_path(&s, "mate", json!({"taskKind": "in_process_teammate"}));
    append_record(
        &path,
        teammate_record("msg-team", "Write", Path::new(DOCUMENT)),
        &s.id,
    );

    wait_for("the teammate", || latest_team(&h, &s));
    h.bridge.tick(&mut Feeds::new());

    assert_eq!(
        h.count_events(|event| matches!(event, BridgeEvent::Team { .. })),
        1
    );
    assert_eq!(
        h.bridge.chat_snapshot(true).turns.pop().unwrap().tokens,
        tokens()
    );
}
