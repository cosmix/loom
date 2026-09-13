use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn truncate_failure_output_at_4kb() {
    // 10KB of ASCII — easy byte/char correspondence.
    let mut file = NamedTempFile::new().unwrap();
    let big = "a".repeat(10_000);
    file.write_all(big.as_bytes()).unwrap();
    let truncated = load_and_truncate_failure_output(file.path()).unwrap();
    assert!(truncated.len() <= 4096, "got {} bytes", truncated.len());
    assert!(truncated.is_char_boundary(truncated.len()));
}

#[test]
fn truncate_failure_output_handles_multibyte_chars() {
    // Construct content that would split a multibyte char if naively sliced.
    // '🌀' is 4 bytes UTF-8; many copies push past 4KB exactly between bytes.
    let mut s = String::new();
    while s.len() < 5_000 {
        s.push('🌀');
    }
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(s.as_bytes()).unwrap();
    let truncated = load_and_truncate_failure_output(file.path()).unwrap();
    assert!(truncated.len() <= 4096);
    // Must still be valid UTF-8 ending on a char boundary.
    assert!(truncated.is_char_boundary(truncated.len()));
}

#[test]
fn truncate_failure_output_passthrough_under_limit() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"hello world").unwrap();
    let truncated = load_and_truncate_failure_output(file.path()).unwrap();
    assert_eq!(truncated, "hello world");
}

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

#[test]
fn relay_mode_writes_exactly_one_ticket_with_truncated_failure_output() {
    use crate::models::session::SessionType;
    use crate::relay::emit::test_support::context_for;
    use std::fs;

    let fixture = context_for(SessionType::Stage);
    let mut sink = VecSink::default();

    dispute_criteria_with_mode(
        "stage-a".to_string(),
        0,
        "flaky".to_string(),
        None,
        None,
        RelayMode::Relay(fixture.context.clone()),
        &fixture.cwd,
        &mut sink,
    )
    .unwrap();

    let tickets: Vec<_> = fs::read_dir(&fixture.context.scratch_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(tickets.len(), 1);
    let bytes = fs::read(&tickets[0]).unwrap();
    let ticket = crate::relay::Ticket::decode(&bytes).unwrap();
    assert_eq!(ticket.kind, RequestKind::Dispute);
    let decoded: StageRequest = serde_json::from_value(ticket.payload).unwrap();
    assert_eq!(
        decoded,
        StageRequest::Dispute {
            criterion_index: 0,
            reason: "flaky".to_string(),
            evidence_commit: None,
            failure_output: None,
        }
    );

    let line = crate::relay::RelayLine::parse(
        std::str::from_utf8(&sink.stdout)
            .unwrap()
            .lines()
            .next_back()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(line.sha256, crate::relay::sha256_hex(&bytes));
}

#[test]
fn an_adjudication_session_is_refused_before_any_ticket_is_written() {
    use crate::models::session::SessionType;
    use crate::relay::emit::test_support::context_for;
    use std::fs;

    let fixture = context_for(SessionType::Adjudication);
    let mut sink = VecSink::default();

    let error = dispute_criteria_with_mode(
        "stage-a".to_string(),
        0,
        "flaky".to_string(),
        None,
        None,
        RelayMode::Relay(fixture.context.clone()),
        &fixture.cwd,
        &mut sink,
    )
    .unwrap_err();

    assert!(error.to_string().contains("may not relay"));
    assert_eq!(
        fs::read_dir(&fixture.context.scratch_dir).unwrap().count(),
        0
    );
}
