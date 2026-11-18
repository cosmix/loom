//! `find_live_merge_session_for_stage` over merge signals that cannot be read,
//! each attributed through the session record its filename names, and over
//! readable ones whose session record is missing or cannot be read.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::generate_merge_signal;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::Stage;
use crate::orchestrator::signals::find_live_merge_session_for_stage;
use crate::orchestrator::terminal::native::NativeBackend;

/// A merge signal for `session_id` that names no stage, so it cannot be parsed.
fn write_unreadable_signal(work_dir: &Path, session_id: &str) -> PathBuf {
    let signals = work_dir.join("signals");
    fs::create_dir_all(&signals).unwrap();
    let path = signals.join(format!("{session_id}.md"));
    fs::write(
        &path,
        format!("# Merge Signal: {session_id}\n\n## Target\n\n"),
    )
    .unwrap();
    path
}

/// Save a `session_type` record for `stage_id` whose process is `pid`.
fn save_record(work_dir: &Path, session_type: SessionType, stage_id: &str, pid: u32) -> Session {
    let mut session = Session::new();
    session.session_type = session_type;
    session.assign_to_stage(stage_id.to_string());
    session.pid = Some(pid);
    session.status = SessionStatus::Running;
    save_session(&session, work_dir).unwrap();
    let (_, key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
    fs::create_dir_all(work_dir.join("pids")).unwrap();
    fs::write(
        work_dir.join("pids").join(format!("{key}.pid")),
        format!("{pid}\n"),
    )
    .unwrap();
    session
}

/// A PID no process holds: that of a child already reaped.
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

#[test]
fn an_unreadable_signal_attributed_elsewhere_is_left_alone() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let live = std::process::id();
    let other_stage = save_record(work_dir, SessionType::Merge, "other", live);
    let not_a_resolver = save_record(work_dir, SessionType::Stage, "mine", live);
    let signals = [
        write_unreadable_signal(work_dir, &other_stage.id),
        write_unreadable_signal(work_dir, &not_a_resolver.id),
    ];
    let found = find_live_merge_session_for_stage("mine", work_dir).unwrap();
    assert_eq!(found, None);
    assert!(signals.iter().all(|signal| signal.exists()));
}

#[test]
fn an_unreadable_signal_of_a_live_resolver_of_this_stage_reports_it() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let resolver = save_record(work_dir, SessionType::Merge, "mine", std::process::id());
    let signal = write_unreadable_signal(work_dir, &resolver.id);
    let found = find_live_merge_session_for_stage("mine", work_dir).unwrap();
    assert_eq!(found, Some(resolver.id));
    assert!(signal.exists());
}

#[test]
fn an_unreadable_signal_of_a_dead_resolver_of_this_stage_is_retired() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let resolver = save_record(work_dir, SessionType::Merge, "mine", dead_pid());
    let signal = write_unreadable_signal(work_dir, &resolver.id);
    let found = find_live_merge_session_for_stage("mine", work_dir).unwrap();
    assert_eq!(found, None);
    assert!(!signal.exists());
}

#[test]
fn an_unreadable_signal_no_record_attributes_is_an_error() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let signal = write_unreadable_signal(work_dir, "unattributed");
    let error = find_live_merge_session_for_stage("mine", work_dir).unwrap_err();
    let message = format!("{error:#}");
    assert!(
        message.contains("no session record attributes it"),
        "{message}"
    );
    assert!(signal.exists());
}

/// A readable merge signal of stage `mine`, for a resolver given no record.
fn write_readable_signal(work_dir: &Path) -> (Session, PathBuf) {
    let resolver = Session::new_merge("loom/mine".to_string(), "main".to_string());
    let stage = Stage {
        id: "mine".to_string(),
        ..Stage::default()
    };
    let signal =
        generate_merge_signal(&resolver, &stage, "loom/mine", "main", &[], work_dir).unwrap();
    (resolver, signal)
}

#[test]
fn a_readable_signal_whose_record_cannot_be_read_is_an_error() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let (resolver, signal) = write_readable_signal(work_dir);
    let sessions = work_dir.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let record = sessions.join(format!("{}.md", resolver.id));
    fs::write(record, "not a session record").unwrap();
    let error = find_live_merge_session_for_stage("mine", work_dir).unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("cannot be read"), "{message}");
    assert!(message.contains(&resolver.id), "{message}");
    assert!(signal.exists());
}

#[test]
fn a_readable_signal_with_no_record_is_stale_and_retired() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let (_, signal) = write_readable_signal(work_dir);
    let found = find_live_merge_session_for_stage("mine", work_dir).unwrap();
    assert_eq!(found, None);
    assert!(!signal.exists());
}

#[test]
fn a_launching_record_without_a_pid_keeps_its_signal() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let (mut resolver, signal) = write_readable_signal(work_dir);
    resolver.assign_to_stage("mine".to_string());
    save_session(&resolver, work_dir).unwrap();
    assert_eq!(
        find_live_merge_session_for_stage("mine", work_dir).unwrap(),
        Some(resolver.id)
    );
    assert!(signal.exists());
}

#[test]
fn readable_signal_requires_matching_merge_record() {
    let temp = TempDir::new().unwrap();
    for (kind, owner) in [(SessionType::Stage, "mine"), (SessionType::Merge, "other")] {
        let record = save_record(temp.path(), kind, owner, std::process::id());
        let stage = Stage {
            id: "mine".into(),
            ..Stage::default()
        };
        let path =
            generate_merge_signal(&record, &stage, "source", "main", &[], temp.path()).unwrap();
        assert_eq!(
            find_live_merge_session_for_stage("mine", temp.path()).unwrap(),
            None
        );
        assert!(path.exists());
    }
}

#[test]
fn quoted_merge_header_is_not_a_merge_signal() {
    let temp = TempDir::new().unwrap();
    let record = save_record(temp.path(), SessionType::Stage, "mine", std::process::id());
    let dir = temp.path().join("signals");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.md", record.id));
    for quoted in [
        "# Merge Signal: example",
        "```\n# Merge Signal: example\n```",
    ] {
        fs::write(
            &path,
            format!("# Stage Signal\n\n## Target\n- **Stage**: mine\n\n## Assignment\n{quoted}\n"),
        )
        .unwrap();
        assert_eq!(
            find_live_merge_session_for_stage("mine", temp.path()).unwrap(),
            None
        );
        assert!(path.exists(), "a stage session's signal is not retired");
    }
}

#[test]
fn a_reused_pid_does_not_keep_a_dead_resolver_signal() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let (mut resolver, signal) = write_readable_signal(work_dir);
    resolver.assign_to_stage("mine".to_string());
    resolver.status = SessionStatus::Running;
    resolver.pid = Some(std::process::id());
    save_session(&resolver, work_dir).unwrap();
    let (_, key) = NativeBackend::window_title_and_pid_key(&resolver).unwrap();
    fs::create_dir_all(work_dir.join("pids")).unwrap();
    let actual_start = crate::process::process_start_time(std::process::id()).unwrap();
    fs::write(
        work_dir.join("pids").join(format!("{key}.pid")),
        format!("{}\n{}\n", std::process::id(), actual_start + 1),
    )
    .unwrap();
    assert_eq!(
        find_live_merge_session_for_stage("mine", work_dir).unwrap(),
        None
    );
    assert!(!signal.exists());
}
