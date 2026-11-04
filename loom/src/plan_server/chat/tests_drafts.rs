//! The draft store's own limits: the caps on write and on read, files it must not trust,
//! session ids that would become paths, and the 30-day sweep.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};

use super::drafts::{self, Seed, MAX_FILE_BYTES, MAX_MESSAGES, MAX_MESSAGE_BYTES};
use super::tests::Fixture;
use super::ChatState;

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const OTHER: &str = "22222222-2222-4222-8222-222222222222";

fn dir(f: &Fixture) -> PathBuf {
    let dir = f.ctx.runtime().dir.join("kickoffs");
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn body(session: &str, patch: Value) -> Value {
    let mut draft = json!({
        "session": session,
        "started_at": "2026-11-04T10:47:11Z",
        "updated_at": "2026-11-04T10:50:00Z",
        "messages": ["first idea"],
        "shape": null,
        "name_override": null,
        "live": false
    });
    for (key, value) in patch.as_object().into_iter().flatten() {
        draft[key] = value.clone();
    }
    draft
}

fn put(f: &Fixture, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir(f).join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn put_draft(f: &Fixture, session: &str, patch: Value) -> PathBuf {
    let text = serde_json::to_vec(&body(session, patch)).unwrap();
    put(f, &format!("{session}.json"), &text)
}

fn sessions(f: &Fixture) -> Vec<String> {
    let listed = drafts::list(&f.ctx, &ChatState::default());
    listed.into_iter().map(|view| view.session).collect()
}

#[test]
fn messages_are_capped_on_write() {
    let f = Fixture::new(false);
    let state = ChatState::default();
    let seed = Seed {
        messages: (0..45).map(|n| format!("m{n:02}")).collect(),
        ..Seed::default()
    };
    drafts::create(&f.ctx, &state, SESSION, seed).unwrap();

    let draft = drafts::get(&f.ctx, &state, SESSION).unwrap();
    assert_eq!(draft.messages.len(), MAX_MESSAGES);
    assert_eq!(draft.messages[0], "m00", "the first message is kept");
    assert_eq!(draft.messages[1], "m06", "the oldest after it go first");
    assert_eq!(draft.messages[MAX_MESSAGES - 1], "m44");

    let long = "é".repeat(MAX_MESSAGE_BYTES);
    assert!(drafts::update(&f.ctx, &state, SESSION, |d| d.push_message(&long)));
    let draft = drafts::get(&f.ctx, &state, SESSION).unwrap();
    let last = draft.messages.last().unwrap();
    assert!(last.len() <= MAX_MESSAGE_BYTES, "{} bytes", last.len());
    assert!(last.ends_with(" […]"), "the cut is marked");
    assert!(last.starts_with('é'));
    assert_eq!(draft.messages[0], "m00");
}

#[test]
fn a_blank_message_is_not_recorded() {
    let f = Fixture::new(false);
    let state = ChatState::default();
    drafts::create(&f.ctx, &state, SESSION, Seed::default()).unwrap();

    drafts::update(&f.ctx, &state, SESSION, |d| d.push_message(" \n\t"));

    assert!(drafts::get(&f.ctx, &state, SESSION)
        .unwrap()
        .messages
        .is_empty());
}

/// Files that sit in the directory under a valid name but that the store must skip.
fn untrusted_files(f: &Fixture) -> Vec<PathBuf> {
    let shape = json!({"decisions": vec!["x"; 41]});
    let draft = |session: &str, patch: Value| put_draft(f, session, patch);
    vec![
        put(f, "33333333-3333-4333-8333-333333333333.json", b"not json"),
        put(
            f,
            "44444444-4444-4444-8444-444444444444.json",
            &vec![b' '; 300 * 1024],
        ),
        draft(
            "55555555-5555-4555-8555-555555555555",
            json!({"messages": vec!["m"; 41]}),
        ),
        draft(
            "66666666-6666-4666-8666-666666666666",
            json!({"messages": ["x".repeat(MAX_MESSAGE_BYTES + 1)]}),
        ),
        draft(
            "77777777-7777-4777-8777-777777777777",
            json!({"session": OTHER}),
        ),
        draft(
            "88888888-8888-4888-8888-888888888888",
            json!({"target": "../escape"}),
        ),
        draft(
            "99999999-9999-4999-8999-999999999999",
            json!({"name_override": "Bad Name"}),
        ),
        draft(
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            json!({"shape": shape}),
        ),
        draft(
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            json!({"updated_at": "yesterday"}),
        ),
    ]
}

/// Files whose names the store never reads.
fn ignored_files(f: &Fixture) -> Vec<PathBuf> {
    vec![
        put(f, "tmp-123-0-draft.json", b"{}"),
        put(f, "readme.txt", b"hello"),
        put_draft(f, "CCCCCCCC-CCCC-4CCC-8CCC-CCCCCCCCCCCC", json!({})),
    ]
}

#[test]
fn files_the_store_cannot_trust_are_skipped_and_left_in_place() {
    let f = Fixture::new(false);
    let skipped = untrusted_files(&f);
    let ignored = ignored_files(&f);
    put_draft(&f, SESSION, json!({}));

    assert_eq!(sessions(&f), vec![SESSION.to_owned()]);

    for path in skipped.iter().chain(&ignored) {
        assert!(path.exists(), "{} was removed", path.display());
    }
}

#[test]
fn a_session_id_that_is_not_canonical_never_becomes_a_path() {
    let f = Fixture::new(false);
    let state = ChatState::default();
    let outside = f.ctx.runtime().dir.join("escape.json");
    fs::write(&outside, b"{}").unwrap();

    for session in [
        "../escape",
        "",
        "ABCDEF12-1111-4111-8111-111111111111",
        "1111",
    ] {
        assert!(drafts::create(&f.ctx, &state, session, Seed::default()).is_err());
        assert!(drafts::get(&f.ctx, &state, session).is_none());
        assert!(!drafts::update(&f.ctx, &state, session, |d| d.live = false));
        assert!(!drafts::remove(&f.ctx, &state, session));
    }

    assert!(outside.exists());
    assert!(!f.ctx.runtime().dir.join("kickoffs").exists());
}

#[test]
fn an_update_never_recreates_a_draft_that_is_gone() {
    let f = Fixture::new(false);
    let state = ChatState::default();
    drafts::create(&f.ctx, &state, SESSION, Seed::default()).unwrap();
    assert!(drafts::remove(&f.ctx, &state, SESSION));

    assert!(!drafts::update(&f.ctx, &state, SESSION, |d| d.live = false));

    assert!(!dir(&f).join(format!("{SESSION}.json")).exists());
}

#[test]
fn the_sweep_removes_every_draft_untouched_for_thirty_days() {
    let f = Fixture::new(false);
    let old = "2020-01-01T00:00:00Z";
    let ended = put_draft(&f, SESSION, json!({"updated_at": old, "live": false}));
    let flagged = put_draft(&f, OTHER, json!({"updated_at": old, "live": true}));
    let recent = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let fresh = "33333333-3333-4333-8333-333333333333";
    let kept = put_draft(&f, fresh, json!({"updated_at": recent, "live": false}));
    let fresh_live = "44444444-4444-4444-8444-444444444444";
    let kept_live = put_draft(&f, fresh_live, json!({"updated_at": recent, "live": true}));

    drafts::sweep(&f.ctx);

    assert!(!ended.exists(), "an ended draft past 30 days goes");
    assert!(
        !flagged.exists(),
        "no session runs at start: the flag is stale"
    );
    assert!(kept.exists(), "a recent draft stays");
    assert!(kept_live.exists(), "a recent draft stays whatever its flag");
}

#[test]
fn a_draft_is_never_live_without_its_running_session() {
    let f = Fixture::new(false);
    put_draft(&f, SESSION, json!({"live": true}));

    let listed = drafts::list(&f.ctx, &ChatState::default());

    assert_eq!(listed.len(), 1);
    assert!(!listed[0].live, "nothing runs, whatever the file says");
}

#[test]
fn a_draft_created_for_a_session_that_is_not_running_is_stored_ended() {
    let f = Fixture::new(false);
    let state = ChatState::default();
    assert_eq!(f.bridge.chat_session_id(), None);

    drafts::create(&f.ctx, &state, SESSION, Seed::default()).unwrap();

    let stored = fs::read(dir(&f).join(format!("{SESSION}.json"))).unwrap();
    let stored: Value = serde_json::from_slice(&stored).unwrap();
    assert_eq!(stored["live"], false, "its Ended already came and went");
}

#[test]
fn a_draft_over_the_file_cap_loses_its_oldest_messages_and_still_reads_back() {
    let f = Fixture::new(false);
    let state = ChatState::default();
    // Each quote escapes to two bytes, so 40 full messages encode past the cap.
    let message = "\"".repeat(MAX_MESSAGE_BYTES);
    let seed = Seed {
        messages: (0..MAX_MESSAGES)
            .map(|n| format!("{n:02}{message}"))
            .collect(),
        ..Seed::default()
    };

    drafts::create(&f.ctx, &state, SESSION, seed).unwrap();

    let size = fs::metadata(dir(&f).join(format!("{SESSION}.json")))
        .unwrap()
        .len();
    assert!(size as usize <= MAX_FILE_BYTES, "{size} bytes");
    let draft = drafts::get(&f.ctx, &state, SESSION).expect("the file reads back");
    assert!(draft.messages.len() < MAX_MESSAGES);
    assert!(
        draft.messages[0].starts_with("00"),
        "the first message is kept"
    );
    let last = draft.messages.last().unwrap();
    assert!(last.starts_with("39"), "the newest message is kept");
}
