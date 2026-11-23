use std::fs;

use anyhow::Result;

use crate::models::session::{Session, SessionStatus};
use crate::subagent_lifecycle::WorkerOutcome;

use super::test_support::{at, liquidmon, only_outcome, LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED};

fn session(id: String, stage_id: String, status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.id = id;
    session.stage_id = Some(stage_id);
    session.status = status;
    session
}

#[test]
fn terminal_records_do_not_crowd_out_an_active_session() -> Result<()> {
    let (fixture, _) = liquidmon(&[LIQUIDMON_RUNNING, LIQUIDMON_SUCCEEDED])?;
    let mut sessions: Vec<_> = (0..256)
        .map(|i| {
            let id = format!("old-{i:03}");
            session(id.clone(), id, SessionStatus::Completed)
        })
        .collect();
    sessions.push(fixture.session.clone());

    let report = super::reconcile_at(&fixture.work_dir, &sessions, &fixture.state_root, at(3600))?;

    assert_eq!(only_outcome(&report)?, &WorkerOutcome::Succeeded);
    Ok(())
}

#[test]
fn the_stage_window_rotates_past_the_first_sixty_four() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let work_dir = temp.path().join("work");
    let state_root = temp.path().join("state");
    fs::create_dir(&work_dir)?;
    fs::create_dir(&state_root)?;
    let mut sessions = Vec::new();
    for i in 0..65 {
        let stage_id = format!("s{i:03}");
        let dir = work_dir.join("subagents").join(&stage_id);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("codex.jsonl"), "not json\n")?;
        sessions.push(session(
            format!("sess-{i:03}"),
            stage_id,
            SessionStatus::Running,
        ));
    }

    let now = at(3600);
    let first = super::reconcile_from(&work_dir, &sessions, &state_root, now, 0)?;
    assert_eq!(first.entries.len(), 64);
    assert_eq!(first.next_cursor, 64);

    let second = super::reconcile_from(&work_dir, &sessions, &state_root, now, first.next_cursor)?;
    assert_eq!(second.entries.len(), 64);
    assert_eq!(second.entries[0].stage_id, "s064");
    assert!(matches!(
        second.entries[0].outcome,
        WorkerOutcome::Unknown(_)
    ));
    Ok(())
}

#[test]
fn an_empty_active_set_resets_the_cursor() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let work_dir = temp.path().join("work");
    let state_root = temp.path().join("state");

    let report = super::reconcile_from(&work_dir, &[], &state_root, at(3600), 7)?;

    assert!(report.entries.is_empty());
    assert_eq!(report.next_cursor, 0);
    Ok(())
}
