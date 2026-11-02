use super::*;
use std::io::{self, Read, Write};
use std::time::Duration;

use ratatui::{layout::Rect, TerminalOptions, Viewport};

fn headless_app() -> TuiApp {
    let terminal = Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 80, 24)),
        },
    )
    .unwrap();
    TuiApp {
        terminal,
        running: Arc::new(AtomicBool::new(true)),
        status: LiveStatus::default(),
        spinner_frame: 0,
        last_error: None,
        graph_state: GraphState::default(),
        activity_log: TuiActivityLog::new(),
        legend_open: false,
        show_tokens: false,
        tick_age_secs: None,
        terminal_guard: TerminalGuard::inactive_for_test(),
        exiting: false,
        completion_summary: None,
        completion_received_at: None,
        cleaned_up: true,
        alerts: Vec::new(),
        alerts_refreshed_at: None,
    }
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
