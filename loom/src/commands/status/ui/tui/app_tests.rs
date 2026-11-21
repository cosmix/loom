use super::*;
use std::io::{self, Read, Write};
use std::time::Duration;

fn headless_app() -> TuiApp {
    TuiApp::headless()
}

#[test]
fn completion_retains_scheduler_error_after_following_status_update() {
    let mut app = headless_app();
    app.handle_response(Response::Error {
        message: "Orchestration: Scheduler failure: panic".into(),
    });
    app.handle_response(Response::StatusUpdate {
        data: Box::default(),
    });
    app.handle_response(Response::OrchestrationComplete {
        summary: CompletionSummary {
            total_duration_secs: 0,
            stages: vec![],
            success_count: 0,
            failure_count: 1,
            plan_path: "plan".into(),
            ..Default::default()
        },
    });
    app.handle_response(Response::StatusUpdate {
        data: Box::default(),
    });
    assert_eq!(
        app.last_error.as_deref(),
        Some("Orchestration: Scheduler failure: panic")
    );
    assert_eq!(app.completion_summary.as_ref().unwrap().failure_count, 1);
}

struct ReadError(io::ErrorKind);

impl Read for ReadError {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from(self.0))
    }
}

#[test]
fn read_timeout_kinds_are_recognized() {
    for kind in [io::ErrorKind::WouldBlock, io::ErrorKind::TimedOut] {
        let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
        assert!(reader.poll(&mut ReadError(kind)).unwrap().is_none());
    }
}

#[test]
fn other_io_errors_are_not_read_timeouts() {
    for kind in [
        io::ErrorKind::UnexpectedEof,
        io::ErrorKind::PermissionDenied,
    ] {
        let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
        assert!(reader.poll(&mut ReadError(kind)).is_err());
    }
}

#[test]
fn a_plain_frame_limit_error_is_not_a_read_timeout() {
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
    let error = reader.poll(&mut &u32::MAX.to_be_bytes()[..]).unwrap_err();
    assert!(error.to_string().contains("exceeds the configured limit"));
}

#[test]
fn tui_receives_fragmented_completion_before_daemon_eof() {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(5)))
        .unwrap();
    let mut app = headless_app();
    let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
    let response = Response::OrchestrationComplete {
        summary: CompletionSummary {
            total_duration_secs: 2,
            stages: Vec::new(),
            success_count: 0,
            failure_count: 0,
            plan_path: "completed.yaml".into(),
            ..Default::default()
        },
    };
    let mut frame = Vec::new();
    write_message(&mut frame, &response).unwrap();
    let work_path = Path::new("/unused");

    sender.write_all(&frame[..2]).unwrap();
    assert!(!app
        .receive_response(&mut receiver, work_path, &mut reader)
        .unwrap());
    sender.write_all(&frame[2..6]).unwrap();
    assert!(!app
        .receive_response(&mut receiver, work_path, &mut reader)
        .unwrap());
    sender.write_all(&frame[6..]).unwrap();
    drop(sender);
    assert!(!app
        .receive_response(&mut receiver, work_path, &mut reader)
        .unwrap());
    assert_eq!(
        app.completion_summary.as_ref().unwrap().plan_path,
        "completed.yaml"
    );
    assert!(app
        .receive_response(&mut receiver, work_path, &mut reader)
        .unwrap());
    assert!(app.last_error.is_none());
}

mod report {
    use std::path::PathBuf;

    use chrono::{Duration as Span, TimeZone, Utc};
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::backend::TestBackend;
    use tempfile::TempDir;

    use super::*;
    use crate::commands::status::data::StatusData;
    use crate::models::stage::{Stage, StageStatus};
    use crate::plan_stats::{finish_plan, read_current, PlanStatsRecord};
    use crate::user_config::redirect_user_config;
    use crate::verify::transitions::create_stage;

    const PLAN_TABLE: &str = "[plan]\n\
source_path = \"doc/plans/IN_PROGRESS-PLAN-app-report.md\"\n\
plan_id = \"app-report\"\n\
plan_name = \"App report plan\"\n\
base_branch = \"main\"\n";

    /// A work dir whose plan finished, with its report saved as the current copy.
    fn finished_run(root: &Path) -> (PathBuf, PlanStatsRecord) {
        let work_dir = root.join(".loom").join("work");
        std::fs::create_dir_all(&work_dir).unwrap();
        std::fs::write(work_dir.join("config.toml"), PLAN_TABLE).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 11, 21, 9, 0, 0).unwrap();
        for index in 1..=12 {
            let started = start + Span::minutes(index);
            let stage = Stage {
                id: format!("stage-{index:02}"),
                name: format!("Stage {index:02}"),
                status: StageStatus::Completed,
                started_at: Some(started),
                completed_at: Some(started + Span::minutes(3)),
                execution_secs: Some(180),
                ..Stage::default()
            };
            create_stage(&stage, &work_dir).unwrap();
        }
        finish_plan(&work_dir).unwrap();
        let record = read_current(&work_dir).unwrap();
        (work_dir, record)
    }

    fn socket_with(responses: &[Response]) -> UnixStream {
        let (mut sender, receiver) = UnixStream::pair().unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        for response in responses {
            write_message(&mut sender, response).unwrap();
        }
        receiver
    }

    fn finished(failure_count: usize) -> Response {
        Response::OrchestrationComplete {
            summary: CompletionSummary {
                total_duration_secs: 60,
                success_count: 12 - failure_count,
                failure_count,
                plan_path: "plan.md".into(),
                ..Default::default()
            },
        }
    }

    fn drawn(app: &mut TuiApp, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw_view(frame)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn daemon_eof_without_a_final_message_opens_the_saved_report() {
        let home = TempDir::new().unwrap();
        let _redirect = redirect_user_config(home.path().join("config.toml"));
        let root = TempDir::new().unwrap();
        let (work_dir, _) = finished_run(root.path());
        let mut stream = socket_with(&[]);
        let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
        let mut app = TuiApp::headless();

        assert!(app.advance(&mut stream, &work_dir, &mut reader).unwrap());
        assert!(app.socket_closed && app.plan_report.is_some());
        assert!(app.advance(&mut stream, &work_dir, &mut reader).unwrap());
        assert!(drawn(&mut app, 100, 30).contains("PLAN COMPLETE"));
    }

    #[test]
    fn a_failed_plan_keeps_the_legacy_summary_despite_a_saved_report() {
        let home = TempDir::new().unwrap();
        let _redirect = redirect_user_config(home.path().join("config.toml"));
        let root = TempDir::new().unwrap();
        let (work_dir, _) = finished_run(root.path());
        let mut stream = socket_with(&[finished(1)]);
        let mut reader = ResponseFrameReader::new(Duration::from_secs(1));
        let mut app = TuiApp::headless();

        assert!(app.advance(&mut stream, &work_dir, &mut reader).unwrap());
        // The next read hits the end of a closed stream and the session ends.
        let mut stream = socket_with(&[]);
        assert!(!app.advance(&mut stream, &work_dir, &mut reader).unwrap());
        assert!(app.plan_report.is_none());
        assert!(drawn(&mut app, 100, 30).contains("Orchestration Complete"));
    }

    #[test]
    fn a_status_update_with_the_report_replaces_the_ledger() {
        let home = TempDir::new().unwrap();
        let _redirect = redirect_user_config(home.path().join("config.toml"));
        let root = TempDir::new().unwrap();
        let (_, record) = finished_run(root.path());
        let mut app = TuiApp::headless();
        assert!(!drawn(&mut app, 70, 12).contains("PLAN COMPLETE"));

        app.handle_response(Response::StatusUpdate {
            data: Box::new(StatusData {
                plan_stats: Some(record),
                ..StatusData::default()
            }),
        });
        let first = drawn(&mut app, 70, 12);
        assert!(first.contains("PLAN COMPLETE") && first.contains("rows 1-"));
        assert!(app.graph_state.total_lines > app.graph_state.viewport_height);
    }

    #[test]
    fn the_report_scrolls_with_j_and_k_and_ignores_the_ledger_toggles() {
        let home = TempDir::new().unwrap();
        let _redirect = redirect_user_config(home.path().join("config.toml"));
        let root = TempDir::new().unwrap();
        let (_, record) = finished_run(root.path());
        let mut app = TuiApp::headless();
        app.plan_report = Some(record);
        drawn(&mut app, 70, 12);

        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.graph_state.scroll_y, 2);
        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.graph_state.scroll_y, 0);
        app.handle_key(KeyCode::Char('t'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('?'), KeyModifiers::NONE);
        assert!(!app.show_tokens && !app.legend_open && !app.exiting);
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.exiting);
    }

    #[test]
    fn the_report_opens_at_its_top_whatever_the_ledger_scrolled_to() {
        let home = TempDir::new().unwrap();
        let _redirect = redirect_user_config(home.path().join("config.toml"));
        let root = TempDir::new().unwrap();
        let (work_dir, record) = finished_run(root.path());
        let mut app = TuiApp::headless();
        app.graph_state.scroll_y = 7;
        app.handle_response(Response::StatusUpdate {
            data: Box::new(StatusData {
                plan_stats: Some(record),
                ..StatusData::default()
            }),
        });
        assert_eq!(app.graph_state.scroll_y, 0, "a status update opens it");

        let mut app = TuiApp::headless();
        app.graph_state.scroll_y = 7;
        app.load_report(&work_dir);
        assert!(app.plan_report.is_some());
        assert_eq!(app.graph_state.scroll_y, 0, "the saved report opens it");
        app.graph_state.scroll_y = 3;
        app.load_report(&work_dir);
        assert_eq!(
            app.graph_state.scroll_y, 3,
            "an open report keeps its place"
        );
    }
}
