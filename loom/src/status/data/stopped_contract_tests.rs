//! Contract: the real status collector and the web wire name a stage's saved
//! capture by the kind of the session that left it.
//!
//! Two ended tmux sessions of one blocked stage each left a capture. The newest
//! one ran fullscreen (`Session.fullscreen_seen`), so the stage summary and the
//! blocked attention entry carry its id with `terminal_history_kind` set to
//! `"last_screen"`. Once the newest session's capture is gone, the older capture
//! does not stand in: both fields are omitted, on the stage and on the entry.

use chrono::{Duration, Utc};
use serde_json::Value;

use super::collect_status_data;
use crate::commands::status::web::model::{collect_snapshot, SnapshotSource};
use crate::fs::session_files::save_session;
use crate::fs::work_dir::WorkDir;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::terminal::tmux::{history_relpath, write_history};

const STAGE_ID: &str = "server";

fn blocked_stage_dir() -> (tempfile::TempDir, WorkDir) {
    let temp = tempfile::tempdir().unwrap();
    let work = WorkDir::new(temp.path()).unwrap();
    work.initialize().unwrap();
    let stage = Stage {
        id: STAGE_ID.into(),
        name: STAGE_ID.into(),
        status: StageStatus::Blocked,
        close_reason: Some("needs a decision".into()),
        ..Stage::default()
    };
    crate::verify::transitions::save_stage(&stage, work.root()).unwrap();
    (temp, work)
}

/// An ended tmux session of the stage, persisted, with a capture of its own.
fn ended_session(work: &WorkDir, age_secs: i64, fullscreen: bool, capture: &[u8]) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(STAGE_ID.to_string());
    session.status = SessionStatus::Completed;
    session.created_at = Utc::now() - Duration::seconds(age_secs);
    session.fullscreen_seen = fullscreen;
    save_session(&session, work.root()).unwrap();
    write_history(work.root(), &session.id, capture).unwrap();
    session
}

/// The stage summary and the attention entry for the stage, as the web wire
/// serializes them from a fresh collection.
fn wire(work: &WorkDir) -> (Value, Value) {
    let status = collect_status_data(work).unwrap();
    let snapshot = collect_snapshot(work.root(), status, SnapshotSource::Files, true);
    let json = serde_json::to_value(&snapshot).unwrap();
    let pick = |list: &Value| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == STAGE_ID)
            .cloned()
            .unwrap_or_else(|| panic!("no {STAGE_ID} entry in {list}"))
    };
    (pick(&json["status"]["stages"]), pick(&json["attention"]))
}

fn absent(value: &Value, field: &str) -> bool {
    matches!(value.get(field), None | Some(Value::Null))
}

#[test]
fn a_fullscreen_session_is_named_as_its_last_screen() {
    let (_temp, work) = blocked_stage_dir();
    let older = ended_session(&work, 600, false, b"OLDER-SCROLLBACK");
    let newer = ended_session(&work, 60, true, b"NEWER-ALT-SCREEN");

    let (stage, attention) = wire(&work);
    assert_eq!(
        stage["terminal_history_session"],
        newer.id.as_str(),
        "{stage}"
    );
    assert_eq!(stage["terminal_history_kind"], "last_screen", "{stage}");
    assert_eq!(attention["terminal_history"], true, "{attention}");
    assert_eq!(
        attention["terminal_history_kind"], "last_screen",
        "{attention}"
    );

    std::fs::remove_file(work.root().join(history_relpath(&newer.id))).unwrap();
    assert!(work.root().join(history_relpath(&older.id)).exists());

    let (stage, attention) = wire(&work);
    assert!(absent(&stage, "terminal_history_session"), "{stage}");
    assert!(absent(&stage, "terminal_history_kind"), "{stage}");
    assert_eq!(attention["terminal_history"], false, "{attention}");
    assert!(absent(&attention, "terminal_history_kind"), "{attention}");
}
