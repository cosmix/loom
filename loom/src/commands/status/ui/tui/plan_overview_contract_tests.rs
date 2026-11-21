//! Frozen contract for the finished-plan screen of the live TUI.
//!
//! It drives the app through this seam in `app.rs`, visible to the `tui`
//! module:
//! - `TuiApp::headless() -> TuiApp` (test only): an app whose own terminal is
//!   never drawn to by this contract;
//! - `TuiApp::advance(&mut self, &mut UnixStream, &Path, &mut ResponseFrameReader)
//!   -> Result<bool>`: one event-loop iteration without input polling or
//!   drawing; `false` ends the session;
//! - `TuiApp::draw_view(&mut self, &mut Frame<'_>)`: draws the current view into
//!   any backend and keeps the scroll viewport in step with it;
//! - `TuiApp::handle_key(&mut self, KeyCode, KeyModifiers)`.
//!
//! The saved report comes from `plan_stats::finish_plan` over a fixture work
//! dir; the daemon sends only the completion, never a final status update.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Duration as Span, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use tempfile::TempDir;

use super::app::TuiApp;
use crate::daemon::{write_message, CompletionSummary, Response, ResponseFrameReader};
use crate::models::stage::{Stage, StageStatus};
use crate::plan_stats::{finish_plan, read_current, PlanStatsRecord};
use crate::user_config::redirect_user_config;
use crate::verify::transitions::create_stage;

/// More timeline rows than a 24-row terminal holds.
const STAGES: usize = 30;

const PLAN_TABLE: &str = "[plan]\n\
source_path = \"doc/plans/IN_PROGRESS-PLAN-overview-contract.md\"\n\
plan_id = \"overview-contract\"\n\
plan_name = \"Overview contract plan\"\n\
base_branch = \"main\"\n";

fn finished_work_dir(root: &Path) -> PathBuf {
    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("config.toml"), PLAN_TABLE).unwrap();
    let start = Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).unwrap();
    for index in 1..=STAGES {
        let started = start + Span::minutes(index as i64);
        let stage = Stage {
            id: format!("stage-{index:02}"),
            name: format!("Stage {index:02}"),
            status: StageStatus::Completed,
            started_at: Some(started),
            completed_at: Some(started + Span::minutes(5)),
            execution_secs: Some(300),
            ..Stage::default()
        };
        create_stage(&stage, &work_dir).unwrap();
    }
    work_dir
}

/// A socket that delivers a successful completion and then closes.
fn completion_then_eof() -> UnixStream {
    let (mut sender, receiver) = UnixStream::pair().unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let summary = CompletionSummary {
        total_duration_secs: 2_100,
        success_count: STAGES,
        failure_count: 0,
        plan_path: "doc/plans/IN_PROGRESS-PLAN-overview-contract.md".into(),
        ..Default::default()
    };
    write_message(&mut sender, &Response::OrchestrationComplete { summary }).unwrap();
    drop(sender);
    receiver
}

fn screen(terminal: &mut Terminal<TestBackend>, app: &mut TuiApp) -> String {
    terminal.draw(|frame| app.draw_view(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content()
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The first screen shows the overview, not the legacy summary; returns the
/// last stage id it could not fit.
fn last_hidden_stage(first: &str, record: &PlanStatsRecord) -> String {
    assert!(first.contains("PLAN COMPLETE"), "report lost:\n{first}");
    assert!(
        first.contains(&record.plan_name),
        "plan name missing:\n{first}"
    );
    assert!(
        !first.contains("Orchestration Complete"),
        "legacy summary:\n{first}"
    );
    record
        .stages
        .iter()
        .rfind(|stage| !first.contains(stage.id.as_str()))
        .map(|stage| stage.id.clone())
        .expect("the report overflows a 70x24 terminal")
}

#[test]
fn a_finished_plan_replaces_the_ledger_with_its_overview() {
    let home = TempDir::new().unwrap();
    let _redirect = redirect_user_config(home.path().join("config.toml"));
    let root = TempDir::new().unwrap();
    let work_dir = finished_work_dir(root.path());
    finish_plan(&work_dir).expect("finish saves the report");
    let record = read_current(&work_dir).expect("the saved report");
    assert_eq!(record.stages.len(), STAGES);

    let mut app = TuiApp::headless();
    let mut stream = completion_then_eof();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
    for _ in 0..3 {
        let running = app.advance(&mut stream, &work_dir, &mut reader).unwrap();
        assert!(running, "the app exited at completion or daemon EOF");
    }

    let mut terminal = Terminal::new(TestBackend::new(70, 24)).unwrap();
    let last = last_hidden_stage(&screen(&mut terminal, &mut app), &record);
    app.handle_key(KeyCode::End, KeyModifiers::NONE);
    let end = screen(&mut terminal, &mut app);
    assert!(
        end.contains(&last),
        "{last} unreachable by scrolling:\n{end}"
    );

    // Past the 500 ms delay after which the legacy completion view exits.
    std::thread::sleep(Duration::from_millis(700));
    let running = app.advance(&mut stream, &work_dir, &mut reader).unwrap();
    assert!(running, "the overview exited without q");
    let later = screen(&mut terminal, &mut app);
    assert!(later.contains(&last), "the report vanished:\n{later}");

    app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
    let running = app.advance(&mut stream, &work_dir, &mut reader).unwrap();
    assert!(!running, "q did not quit the overview");
}
