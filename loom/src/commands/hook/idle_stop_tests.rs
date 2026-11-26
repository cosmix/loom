use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::monitor::{write_heartbeat, Heartbeat};
use crate::verify::transitions::create_stage;

fn blocking() -> StopFacts {
    StopFacts {
        owns_executing_stage: true,
        stage_session: true,
        children_quiet: Some(true),
        host_job_live: false,
        worked_since_last_block: true,
        blocks_so_far: 0,
        background_tasks: 0,
    }
}

#[test]
fn a_session_that_does_not_own_the_executing_stage_can_stop() {
    let facts = StopFacts {
        owns_executing_stage: false,
        ..blocking()
    };
    assert_eq!(decide("test-stage", &facts), None);
}

#[test]
fn a_session_that_does_not_run_stage_work_can_stop() {
    let facts = StopFacts {
        stage_session: false,
        ..blocking()
    };
    assert_eq!(decide("test-stage", &facts), None);
}

#[test]
fn active_or_unknown_children_let_the_session_stop() {
    for children_quiet in [Some(false), None] {
        let facts = StopFacts {
            children_quiet,
            ..blocking()
        };
        assert_eq!(decide("test-stage", &facts), None);
    }
}

#[test]
fn a_live_host_job_lets_the_session_stop() {
    let facts = StopFacts {
        host_job_live: true,
        ..blocking()
    };
    assert_eq!(decide("test-stage", &facts), None);
}

#[test]
fn a_session_that_has_not_used_a_tool_since_the_last_block_can_stop() {
    let facts = StopFacts {
        worked_since_last_block: false,
        ..blocking()
    };
    assert_eq!(decide("test-stage", &facts), None);
}

#[test]
fn a_session_at_the_block_limit_can_stop() {
    let facts = StopFacts {
        blocks_so_far: MAX_IDLE_STOP_BLOCKS,
        ..blocking()
    };
    assert_eq!(decide("test-stage", &facts), None);
}

#[test]
fn the_reason_names_the_stage_and_three_recorded_exits() {
    let reason = decide("test-stage", &blocking()).unwrap();
    assert!(reason.contains("Stage test-stage"));
    assert!(reason.contains("loom stage complete test-stage"));
    assert!(reason.contains("loom stage block test-stage"));
    assert!(reason.contains("file the dispute"));
}

fn timestamp() -> DateTime<Utc> {
    "2026-11-24T09:12:28.456789Z".parse().unwrap()
}

#[test]
fn the_marker_round_trips_and_a_new_heartbeat_allows_another_block() {
    let dir = TempDir::new().unwrap();
    let scratch = safe_open_dirfd(dir.path()).unwrap();
    let timestamp = timestamp();
    assert_eq!(marker_facts(&scratch, timestamp), (0, true));

    write_marker(&scratch, 2, timestamp).unwrap();
    assert_eq!(
        read_marker(&scratch).unwrap(),
        Some(Marker {
            count: 2,
            timestamp
        })
    );
    let next = timestamp + chrono::Duration::milliseconds(1);
    assert_eq!(marker_facts(&scratch, next), (2, true));
}

#[test]
fn a_symlinked_marker_counts_as_spent_even_if_its_target_is_missing() {
    let dir = TempDir::new().unwrap();
    let scratch = safe_open_dirfd(dir.path()).unwrap();
    symlink("absent", dir.path().join(MARKER_NAME)).unwrap();
    assert_eq!(
        marker_facts(&scratch, timestamp()),
        (MAX_IDLE_STOP_BLOCKS, false)
    );
}

#[test]
fn a_marker_with_the_current_heartbeat_timestamp_prevents_another_block() {
    let dir = TempDir::new().unwrap();
    let scratch = safe_open_dirfd(dir.path()).unwrap();
    let timestamp = timestamp();
    write_marker(&scratch, 1, timestamp).unwrap();
    assert_eq!(marker_facts(&scratch, timestamp), (1, false));
}

#[test]
fn oversized_and_malformed_markers_count_as_spent() {
    let dir = TempDir::new().unwrap();
    let scratch = safe_open_dirfd(dir.path()).unwrap();
    let marker = dir.path().join(MARKER_NAME);
    for bytes in [
        vec![b'0'; MAX_MARKER_BYTES + 1],
        b"1\nnot-a-timestamp\n".to_vec(),
    ] {
        fs::write(&marker, bytes).unwrap();
        assert_eq!(
            marker_facts(&scratch, timestamp()),
            (MAX_IDLE_STOP_BLOCKS, false)
        );
    }
}

const STAGE_ID: &str = "test-stage";
const SESSION_ID: &str = "test-session";

struct StopFixture {
    work: TempDir,
    scratch: PathBuf,
}

impl StopFixture {
    fn new(session_type: SessionType) -> Self {
        let work = TempDir::new().unwrap();
        let scratch = work.path().join("scratch");
        fs::create_dir(&scratch).unwrap();
        create_stage(
            &Stage {
                id: STAGE_ID.to_owned(),
                status: StageStatus::Executing,
                session: Some(SESSION_ID.to_owned()),
                ..Stage::new(STAGE_ID.to_owned(), None)
            },
            work.path(),
        )
        .unwrap();
        save_session(
            &Session {
                id: SESSION_ID.to_owned(),
                stage_id: Some(STAGE_ID.to_owned()),
                status: SessionStatus::Running,
                session_type,
                ..Session::new()
            },
            work.path(),
        )
        .unwrap();
        let mut heartbeat = Heartbeat::new(STAGE_ID.to_owned(), SESSION_ID.to_owned());
        heartbeat.timestamp = timestamp();
        heartbeat.progress_at = Some(heartbeat.timestamp);
        write_heartbeat(work.path(), &heartbeat).unwrap();
        Self { work, scratch }
    }

    fn block(&self) -> Option<String> {
        self.block_with_tasks(0)
    }

    fn block_with_tasks(&self, background_tasks: u32) -> Option<String> {
        block_line(
            self.work.path(),
            STAGE_ID,
            SESSION_ID,
            &self.scratch,
            background_tasks,
        )
    }
}

#[test]
fn an_unreadable_host_job_record_lets_the_session_stop() {
    let fixture = StopFixture::new(SessionType::Stage);
    let work = fixture.work.path();
    let jobs = crate::host_verify::store::stage_dir(work, STAGE_ID).unwrap();
    fs::create_dir_all(&jobs).unwrap();
    let current = jobs.join("current.json");
    fs::write(&current, b"invalid job record").unwrap();

    assert!(load_current(work, STAGE_ID).is_err());
    assert_eq!(fixture.block(), None);

    fs::remove_file(current).unwrap();
    assert!(fixture.block().is_some());
}

#[test]
fn only_stage_work_and_knowledge_session_records_can_block() {
    for (session_type, should_block) in [
        (SessionType::Stage, true),
        (SessionType::Knowledge, true),
        (SessionType::Contract, false),
        (SessionType::Adjudication, false),
        (SessionType::Merge, false),
        (SessionType::BaseConflict, false),
    ] {
        let fixture = StopFixture::new(session_type);
        assert_eq!(fixture.block().is_some(), should_block, "{session_type}");
    }
}

#[test]
fn a_same_session_subagent_heartbeat_can_block_once_per_timestamp() {
    let fixture = StopFixture::new(SessionType::Stage);
    let work = fixture.work.path();

    let mut heartbeat = read_heartbeat(&heartbeat_path(work, STAGE_ID)).unwrap();
    heartbeat.session_id = "other-session".to_owned();
    write_heartbeat(work, &heartbeat).unwrap();
    assert_eq!(fixture.block(), None);

    heartbeat.session_id = SESSION_ID.to_owned();
    heartbeat.subagent = true;
    write_heartbeat(work, &heartbeat).unwrap();
    assert!(fixture.block().is_some());
    assert_eq!(fixture.block(), None);

    heartbeat.timestamp += chrono::Duration::milliseconds(1);
    write_heartbeat(work, &heartbeat).unwrap();
    assert!(fixture.block().is_some());
}

#[test]
fn unknown_native_children_map_to_none_and_let_the_session_stop() {
    let fixture = StopFixture::new(SessionType::Stage);
    let work = fixture.work.path();
    let native_stage = crate::native_worker::store::stage_dir(work, STAGE_ID);
    fs::create_dir_all(native_stage.parent().unwrap()).unwrap();
    fs::write(&native_stage, b"not a worker directory").unwrap();

    assert_eq!(children_quiet(work, STAGE_ID, SESSION_ID), None);
    assert_eq!(fixture.block(), None);

    fs::remove_file(native_stage).unwrap();
    assert!(fixture.block().is_some());
}

#[test]
fn each_block_records_the_incremented_count_until_the_limit() {
    let fixture = StopFixture::new(SessionType::Stage);
    let work = fixture.work.path();
    let mut heartbeat = read_heartbeat(&heartbeat_path(work, STAGE_ID)).unwrap();
    let scratch = safe_open_dirfd(&fixture.scratch).unwrap();

    for expected in 1..=MAX_IDLE_STOP_BLOCKS {
        assert!(fixture.block().is_some(), "block {expected}");
        assert_eq!(read_marker(&scratch).unwrap().unwrap().count, expected);
        heartbeat.timestamp += chrono::Duration::milliseconds(1);
        write_heartbeat(work, &heartbeat).unwrap();
    }
    assert_eq!(fixture.block(), None);
    assert_eq!(
        read_marker(&scratch).unwrap().unwrap().count,
        MAX_IDLE_STOP_BLOCKS
    );
}

#[test]
fn background_tasks_get_their_own_reason_and_name_the_stage() {
    let reason = decide(
        "test-stage",
        &StopFacts {
            background_tasks: 2,
            ..blocking()
        },
    )
    .unwrap();
    assert!(reason.contains("test-stage"));
    assert!(reason.contains("2 background task"));
    assert!(reason.contains("not counted"));
    assert!(reason.contains("loom stage complete test-stage"));
    assert!(reason.contains("loom stage block test-stage"));
    assert_ne!(reason, decide("test-stage", &blocking()).unwrap());
}

#[test]
fn background_tasks_do_not_lift_the_limit_or_the_heartbeat_rule() {
    let spent = StopFacts {
        background_tasks: 2,
        blocks_so_far: MAX_IDLE_STOP_BLOCKS,
        ..blocking()
    };
    assert_eq!(decide("test-stage", &spent), None);

    let no_tool_since = StopFacts {
        background_tasks: 2,
        worked_since_last_block: false,
        ..blocking()
    };
    assert_eq!(decide("test-stage", &no_tool_since), None);
}

#[test]
fn background_task_count_reads_the_stop_payload() {
    assert_eq!(background_task_count(&json!({})), Some(0));
    assert_eq!(
        background_task_count(&json!({"stop_hook_active": false})),
        Some(0)
    );
    assert_eq!(
        background_task_count(&json!({"background_tasks": null})),
        Some(0)
    );
    assert_eq!(
        background_task_count(&json!({"background_tasks": []})),
        Some(0)
    );
    assert_eq!(
        background_task_count(&json!({"background_tasks": [{"id": "a"}, {"id": "b"}]})),
        Some(2)
    );
    assert_eq!(
        background_task_count(&json!({"background_tasks": "two"})),
        None
    );
    assert_eq!(
        background_task_count(&json!({"background_tasks": {}})),
        None
    );
}

#[test]
fn background_tasks_block_without_spending_the_limit() {
    let fixture = StopFixture::new(SessionType::Stage);
    let work = fixture.work.path();
    let mut heartbeat = read_heartbeat(&heartbeat_path(work, STAGE_ID)).unwrap();
    let scratch = safe_open_dirfd(&fixture.scratch).unwrap();

    assert!(fixture.block_with_tasks(2).is_some());
    assert_eq!(read_marker(&scratch).unwrap().unwrap().count, 0);
    assert_eq!(fixture.block_with_tasks(2), None);

    heartbeat.timestamp += chrono::Duration::milliseconds(1);
    write_heartbeat(work, &heartbeat).unwrap();
    assert!(fixture.block().is_some());
    assert_eq!(read_marker(&scratch).unwrap().unwrap().count, 1);
}
