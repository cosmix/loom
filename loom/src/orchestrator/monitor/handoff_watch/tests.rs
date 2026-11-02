use super::*;
use crate::models::session::SessionStatus;
use std::path::PathBuf;
use tempfile::TempDir;

/// A stage as the executor leaves it: executing, naming its agent.
fn executing_stage(session_id: &str) -> Stage {
    let mut stage = Stage::new("Climate".to_string(), None);
    stage.id = "climate-timezone-data".to_string();
    stage.status = StageStatus::Executing;
    stage.assign_session(session_id.to_string());
    stage
}

fn running_session(stage_id: &str) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = SessionStatus::Running;
    session
}

/// A handoff document with the frontmatter shape the generator writes,
/// followed by a body long enough that a whole-file read would be waste.
fn write_handoff(work_dir: &Path, number: u32, stage_id: &str, session_id: &str, origin: &str) {
    let dir = work_dir.join("handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    let origin_line = if origin.is_empty() {
        String::new()
    } else {
        format!("origin: {origin}\n")
    };
    let document = format!(
        "---\nversion: 2\nsession_id: {session_id}\nstage_id: {stage_id}\n\
         context_tokens: 153518\n{origin_line}branch: loom/{stage_id}\n---\n\n\
         # Handoff: {stage_id}\n\n{}",
        "Body text that the watch must never need to read.\n".repeat(400)
    );
    std::fs::write(
        dir.join(format!("{stage_id}-handoff-{number:03}.md")),
        document,
    )
    .unwrap();
}

fn work_dir() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().to_path_buf();
    (temp, path)
}

fn query(
    watch: &mut HandoffWatch,
    session: &Session,
    stages: &[Stage],
    work_dir: &Path,
) -> Option<MonitorEvent> {
    watch.refresh(work_dir);
    watch.needs_handoff_from_document(session, stages)
}

/// The incident itself: the agent's ceiling handoff landed, the stage
/// transition did not, and the daemon had nothing to react to.
#[test]
fn a_ceiling_document_asks_for_the_takedown_the_stage_file_never_recorded() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    write_handoff(
        &work,
        2,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );

    let event = query(&mut HandoffWatch::default(), &session, &stages, &work);

    assert_eq!(
        event,
        Some(MonitorEvent::SessionNeedsHandoff {
            session_id: session.id.clone(),
            stage_id: "climate-timezone-data".to_string(),
        })
    );
}

/// A document from a previous attempt names a session the stage has moved
/// past. Acting on it would take down the successor now doing the work.
#[test]
fn a_document_from_another_session_is_not_this_sessions_request() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    write_handoff(
        &work,
        1,
        "climate-timezone-data",
        "session-from-a-previous-attempt",
        "agent_ceiling",
    );

    assert_eq!(
        query(&mut HandoffWatch::default(), &session, &stages, &work),
        None
    );
}

/// A stage that reached `NeedsHandoff` is the status-triggered path's to
/// recover; this one must not double-emit behind it.
#[test]
fn a_stage_already_needing_handoff_does_not_emit_again() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let mut stage = executing_stage(&session.id);
    stage.status = StageStatus::NeedsHandoff;
    write_handoff(
        &work,
        1,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );

    assert_eq!(
        query(&mut HandoffWatch::default(), &session, &[stage], &work),
        None
    );
}

/// The regression this origin gate exists to prevent: a session at 90%
/// context has an advisory snapshot written for it by the daemon and is
/// expected to keep working. Reading that as a takedown request would kill
/// every session that ever entered the Red band, or compacted.
#[test]
fn routine_documents_are_not_takedown_requests() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    for (number, origin) in [(1, "red_band"), (2, "budget_exceeded"), (3, "")] {
        write_handoff(&work, number, "climate-timezone-data", &session.id, origin);
    }

    assert_eq!(
        query(&mut HandoffWatch::default(), &session, &stages, &work),
        None
    );
}

/// A truncated, malformed or bodyless file is no evidence — and above all
/// must not abort the tick for every other session.
#[test]
fn malformed_documents_do_not_authorize_takedown() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    let dir = work.join("handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("climate-timezone-data-handoff-001.md"), "").unwrap();
    std::fs::write(
        dir.join("climate-timezone-data-handoff-002.md"),
        "---\nversion: 2\nsession_id: [broken\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("climate-timezone-data-handoff-003.md"),
        "# Prose handoff with no frontmatter at all\n",
    )
    .unwrap();

    assert_eq!(
        query(&mut HandoffWatch::default(), &session, &stages, &work),
        None
    );
}

/// A missing handoffs directory is the normal state of a fresh plan.
#[test]
fn a_missing_handoffs_directory_is_not_an_error() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];

    assert_eq!(
        query(&mut HandoffWatch::default(), &session, &stages, &work),
        None
    );
}

/// The cost control: a document is parsed once, ever. A later tick answers
/// from the cache even if the file has since become unreadable.
#[test]
fn a_document_is_parsed_once_and_answered_from_cache() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    write_handoff(
        &work,
        2,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );

    let mut watch = HandoffWatch::default();
    assert!(query(&mut watch, &session, &stages, &work).is_some());

    std::fs::remove_file(
        work.join("handoffs")
            .join("climate-timezone-data-handoff-002.md"),
    )
    .unwrap();

    assert!(
        query(&mut watch, &session, &stages, &work).is_some(),
        "the second tick must not re-read the file it already examined"
    );
}

#[test]
fn an_unreadable_document_is_retried_on_a_later_poll() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    let dir = work.join("handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("climate-timezone-data-handoff-001.md");
    std::os::unix::fs::symlink("missing.md", &path).unwrap();

    let mut watch = HandoffWatch::default();
    assert_eq!(query(&mut watch, &session, &stages, &work), None);
    assert!(matches!(
        watch
            .examined
            .get(path.file_name().unwrap().to_str().unwrap()),
        Some(DocumentState::Retry { .. })
    ));

    std::fs::remove_file(&path).unwrap();
    write_handoff(
        &work,
        1,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );
    assert!(query(&mut watch, &session, &stages, &work).is_some());
}

#[test]
fn an_empty_document_is_retried_when_completed() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    let dir = work.join("handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("climate-timezone-data-handoff-001.md"), "").unwrap();
    let mut watch = HandoffWatch::default();

    assert_eq!(query(&mut watch, &session, &stages, &work), None);
    write_handoff(
        &work,
        1,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );
    assert!(query(&mut watch, &session, &stages, &work).is_some());
}

#[test]
fn a_successful_non_request_is_cached() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    write_handoff(&work, 1, "climate-timezone-data", &session.id, "red_band");
    let mut watch = HandoffWatch::default();
    assert_eq!(query(&mut watch, &session, &stages, &work), None);

    // Replacing a published artifact violates its immutable contract. Keeping
    // the negative answer shows that a successful non-request is not re-parsed.
    write_handoff(
        &work,
        1,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );
    assert_eq!(query(&mut watch, &session, &stages, &work), None);
}

#[test]
fn partial_and_oversized_frontmatter_never_authorize_takedown() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let stages = vec![executing_stage(&session.id)];
    let dir = work.join("handoffs");
    std::fs::create_dir_all(&dir).unwrap();
    let prefix = format!(
        "---\nsession_id: {}\nstage_id: climate-timezone-data\norigin: agent_ceiling\n",
        session.id
    );
    let partial = dir.join("climate-timezone-data-handoff-001.md");
    std::fs::write(&partial, &prefix).unwrap();
    std::fs::write(
        dir.join("climate-timezone-data-handoff-002.md"),
        format!(
            "{prefix}description: {}\n---\n",
            "x".repeat(FRONTMATTER_SCAN_BYTES)
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("climate-timezone-data-handoff-003.md"),
        format!("{prefix}---\n{}", "x".repeat(MAX_HANDOFF_BYTES)),
    )
    .unwrap();
    let mut watch = HandoffWatch::default();

    assert_eq!(query(&mut watch, &session, &stages, &work), None);
    assert_eq!(query(&mut watch, &session, &stages, &work), None);

    std::fs::write(&partial, format!("{prefix}---\n# Complete now\n")).unwrap();
    assert_eq!(query(&mut watch, &session, &stages, &work), None);
    assert!(query(&mut watch, &session, &stages, &work).is_some());
}

#[test]
fn requests_are_indexed_by_exact_stage_and_session() {
    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let other = running_session("other-stage");
    let mut other_stage = executing_stage(&other.id);
    other_stage.id = "other-stage".to_string();
    let stages = vec![executing_stage(&session.id), other_stage];
    write_handoff(
        &work,
        1,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );
    let mut watch = HandoffWatch::default();
    watch.refresh(&work);

    assert!(watch
        .needs_handoff_from_document(&session, &stages)
        .is_some());
    assert_eq!(watch.needs_handoff_from_document(&other, &stages), None);
    assert_eq!(watch.directory_read_count(), 1);
}

#[test]
fn a_multi_session_detection_pass_lists_handoffs_once() {
    use crate::orchestrator::monitor::detection::Detection;
    use crate::orchestrator::monitor::handlers::Handlers;
    use crate::orchestrator::monitor::MonitorConfig;

    let (_temp, work) = work_dir();
    let session = running_session("climate-timezone-data");
    let other = running_session("other-stage");
    let mut other_stage = executing_stage(&other.id);
    other_stage.id = "other-stage".to_string();
    let stages = vec![executing_stage(&session.id), other_stage];
    write_handoff(
        &work,
        1,
        "climate-timezone-data",
        &session.id,
        "agent_ceiling",
    );
    let handlers = Handlers::new(
        MonitorConfig {
            work_dir: work,
            ..Default::default()
        },
        None,
    );
    let mut detection = Detection::new();

    let events = detection.detect_session_changes(&[session, other], &stages, &handlers);

    assert!(events
        .iter()
        .any(|event| matches!(event, MonitorEvent::SessionNeedsHandoff { .. })));
    assert_eq!(detection.handoff_directory_reads(), 1);
}
