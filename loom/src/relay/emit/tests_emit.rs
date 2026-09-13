//! Coverage for [`super::RelayContext::emit`] and `emit_quiet`.

use super::*;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// An in-memory [`RelaySink`] tests can inspect after a call.
#[derive(Default)]
struct VecSink {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl RelaySink for VecSink {
    fn stdout(&mut self) -> &mut dyn Write {
        &mut self.stdout
    }

    fn stderr(&mut self) -> &mut dyn Write {
        &mut self.stderr
    }
}

fn context(scratch_dir: PathBuf) -> RelayContext {
    RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir,
        stage_id: Some("stage-a".to_string()),
        session_type: Some("stage".to_string()),
        worktree_path: None,
        work_dir: None,
    }
}

fn last_stdout_line(sink: &VecSink) -> &str {
    std::str::from_utf8(&sink.stdout)
        .unwrap()
        .lines()
        .next_back()
        .unwrap()
}

#[test]
fn emit_writes_a_ticket_file_readable_only_by_its_owner() {
    let dir = TempDir::new().unwrap();
    let context = context(dir.path().to_path_buf());
    let mut sink = VecSink::default();

    let line = context
        .emit(
            RequestKind::Memory,
            serde_json::json!({"content": "note"}),
            "memory note",
            false,
            &mut sink,
        )
        .unwrap();

    let ticket_path = dir.path().join(format!("{}.req", line.id));
    let metadata = fs::metadata(&ticket_path).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
}

#[test]
fn the_written_ticket_decodes_back_to_the_same_kind_and_payload() {
    let dir = TempDir::new().unwrap();
    let context = context(dir.path().to_path_buf());
    let mut sink = VecSink::default();
    let payload = serde_json::json!({"content": "note"});

    let line = context
        .emit(
            RequestKind::Memory,
            payload.clone(),
            "memory note",
            false,
            &mut sink,
        )
        .unwrap();

    let bytes = fs::read(dir.path().join(format!("{}.req", line.id))).unwrap();
    let ticket = Ticket::decode(&bytes).unwrap();
    assert_eq!(ticket.id, line.id);
    assert_eq!(ticket.kind, RequestKind::Memory);
    assert_eq!(ticket.payload, payload);
}

#[test]
fn the_relay_lines_sha256_and_bytes_match_the_ticket_file() {
    let dir = TempDir::new().unwrap();
    let context = context(dir.path().to_path_buf());
    let mut sink = VecSink::default();

    let line = context
        .emit(
            RequestKind::Memory,
            serde_json::json!({"content": "note"}),
            "memory note",
            false,
            &mut sink,
        )
        .unwrap();

    let bytes = fs::read(dir.path().join(format!("{}.req", line.id))).unwrap();
    assert_eq!(line.sha256, sha256_hex(&bytes));
    assert_eq!(line.bytes as usize, bytes.len());
}

#[test]
fn the_sinks_last_stdout_line_parses_back_to_the_returned_relay_line() {
    let dir = TempDir::new().unwrap();
    let context = context(dir.path().to_path_buf());
    let mut sink = VecSink::default();

    let line = context
        .emit(
            RequestKind::Memory,
            serde_json::json!({"content": "note"}),
            "memory note",
            false,
            &mut sink,
        )
        .unwrap();

    let parsed = RelayLine::parse(last_stdout_line(&sink)).unwrap();
    assert_eq!(parsed, line);
}

#[test]
fn the_stderr_text_names_the_ticket_id() {
    let dir = TempDir::new().unwrap();
    let context = context(dir.path().to_path_buf());
    let mut sink = VecSink::default();

    let line = context
        .emit(
            RequestKind::Memory,
            serde_json::json!({"content": "note"}),
            "memory note",
            false,
            &mut sink,
        )
        .unwrap();

    let stderr_text = std::str::from_utf8(&sink.stderr).unwrap();
    assert!(stderr_text.contains(&line.id[..8]));
}

#[test]
fn emit_never_touches_a_read_only_stand_in_for_the_state_directory() {
    let protected = TempDir::new().unwrap();
    fs::set_permissions(protected.path(), fs::Permissions::from_mode(0o500)).unwrap();
    assert_eq!(fs::read_dir(protected.path()).unwrap().count(), 0);

    let dir = TempDir::new().unwrap();
    let context = context(dir.path().to_path_buf());
    let mut sink = VecSink::default();
    context
        .emit(
            RequestKind::Memory,
            serde_json::json!({"content": "note"}),
            "memory note",
            false,
            &mut sink,
        )
        .unwrap();

    assert_eq!(fs::read_dir(protected.path()).unwrap().count(), 0);
    let mode = fs::metadata(protected.path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o500);
}

#[test]
fn emit_quiet_writes_the_ticket_and_the_relay_line_but_nothing_to_stderr() {
    let dir = TempDir::new().unwrap();
    let context = context(dir.path().to_path_buf());
    let mut sink = VecSink::default();

    let line = context
        .emit_quiet(
            RequestKind::Telemetry,
            serde_json::json!({
                "kind": "context-pulled",
                "stage_id": null,
                "session_id": null,
                "query_chars": 10,
                "budget_tokens": 100,
                "items": 1,
                "estimated_tokens": 20,
                "unmet_required": 0,
            }),
            &mut sink,
        )
        .unwrap();

    assert!(dir.path().join(format!("{}.req", line.id)).exists());
    let parsed = RelayLine::parse(last_stdout_line(&sink)).unwrap();
    assert_eq!(parsed, line);
    assert!(sink.stderr.is_empty());
}
