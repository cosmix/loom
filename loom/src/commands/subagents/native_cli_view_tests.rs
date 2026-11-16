use serde_json::Value;

use super::*;

fn seed_terminal_events(h: &Harness) {
    let dir = h.record();
    let mut log = store::EventLog::open(&dir).unwrap();
    for event in [
        NativeEvent::BoundaryCreated {
            sandbox_mode: SandboxMode::WorkspaceWrite,
        },
        NativeEvent::ThreadStarted {
            thread_id: "thread-1".into(),
        },
        NativeEvent::Message {
            message_id: "msg".into(),
            state: MessageState::Delivered,
            turn_id: Some("turn-1".into()),
            detail: None,
        },
        NativeEvent::CancelRequested {
            reason: "stop".into(),
        },
        NativeEvent::ReportStored {
            bytes: 4,
            sha256: sha256_hex(b"text"),
            complete: true,
        },
        NativeEvent::Terminal {
            outcome: Outcome::Cancelled,
            reason: "stop".into(),
        },
    ] {
        log.append(event).unwrap();
    }
}

fn assert_status_text(text: &str) {
    let keys: Vec<_> = text
        .lines()
        .map(|line| line.split(": ").next().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "worker",
            "stage",
            "invocation",
            "phase",
            "outcome",
            "reason",
            "ownership",
            "model",
            "effort",
            "thread",
            "sandbox",
            "cancel requested",
            "last progress",
            "report",
            "messages"
        ]
    );
    for part in [
        "phase: terminal",
        "outcome: cancelled",
        "sandbox: workspace_write",
        "cancel requested: yes",
        "report: 4 bytes, complete",
        "messages: msg=delivered",
    ] {
        assert!(text.contains(part));
    }
}

fn assert_status_json(value: &Value) {
    let mut keys: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "cancel_requested",
            "effort",
            "invocation_id",
            "last_progress_at",
            "messages",
            "model",
            "outcome",
            "outcome_reason",
            "ownership",
            "phase",
            "report",
            "sandbox_mode",
            "stage_id",
            "thread_id",
            "unit_id"
        ]
    );
    assert_eq!(value["invocation_id"], INV);
    assert_eq!(value["phase"], "terminal");
    assert_eq!(value["report"]["complete"], true);
    assert_eq!(value["messages"][0]["turn_id"], "turn-1");
}

#[test]
fn status_text_and_json_fields() {
    let mut h = Harness::new();
    seed_terminal_events(&h);
    assert_eq!(h.run(view(false, false, false)), 0);
    assert_status_text(&String::from_utf8(h.out.clone()).unwrap());
    h.out.clear();
    assert_eq!(h.run(view(false, false, true)), 0);
    assert_status_json(&serde_json::from_slice(&h.out).unwrap());
}
#[test]
fn result_truncates_at_8192_and_says_so() {
    let mut h = Harness::new();
    let dir = h.record();
    store::write_report(&dir, &"x".repeat(REPORT_DEFAULT_BYTES + 50)).unwrap();
    assert_eq!(h.run(view(true, false, false)), 0);
    let text = String::from_utf8(h.out.clone()).unwrap();
    assert_eq!(text.lines().next().unwrap().len(), 8192);
    assert!(text.ends_with("[truncated: showing the first 8192 bytes of the report; run again with --full for all of it]\n"));
    h.out.clear();
    assert_eq!(h.run(view(true, false, true)), 0);
    let value: Value = serde_json::from_slice(&h.out).unwrap();
    assert_eq!(value["truncated"], true);
    assert_eq!(value["report"].as_str().unwrap().len(), 8192);
}
#[test]
fn result_full_prints_everything() {
    let mut h = Harness::new();
    let dir = h.record();
    let text = "é".repeat(9000);
    store::write_report(&dir, &text).unwrap();
    assert_eq!(h.run(view(true, true, false)), 0);
    assert_eq!(h.out, text.as_bytes());
}
#[test]
fn result_without_a_report_exits_1() {
    let mut h = Harness::new();
    h.record();
    assert_eq!(h.run(view(true, false, false)), 1);
    assert!(String::from_utf8_lossy(&h.err).contains("no report is stored for codex:unit"));
    assert!(h.out.is_empty());
}

#[test]
fn views_require_a_stage_and_report_missing_or_corrupt_records() {
    let mut h = Harness::new();
    let args = StatusArgs {
        worker: "codex:unit".into(),
        stage: None,
        json: false,
    };
    assert_eq!(
        native_view::status_in(args, None, &h.root(), &mut h.out, &mut h.err).unwrap(),
        3
    );
    h.refused("--stage");
    h.err.clear();
    assert_eq!(h.run(view(false, false, false)), 1);
    assert!(String::from_utf8_lossy(&h.err)
        .contains("no native worker record for codex:unit in stage stage-a"));
    let dir = h.record();
    fs::write(dir.join("manifest.json"), "{").unwrap();
    h.err.clear();
    assert_eq!(h.run(view(false, false, true)), 1);
    assert!(String::from_utf8_lossy(&h.err).contains("manifest.json"));
    assert!(h.out.is_empty());
}
