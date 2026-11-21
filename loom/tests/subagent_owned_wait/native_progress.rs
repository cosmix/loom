use super::support::{assert_exit, Fixture, UNIT_ID};
use chrono::Utc;
use loom::native_worker::ids::NATIVE_DEADLINE_SECS;
use loom::native_worker::model::{NativeEvent, NativeManifest};
use loom::native_worker::store::{create_invocation, EventLog};
use loom::utils::sha256_hex;
use std::process::Output;

const BRIEF: &[u8] = b"brief";

impl Fixture {
    /// Seed an admitted native invocation of `UNIT_ID` for this fixture's stage,
    /// then append `events` after the admission record.
    fn seed_native_worker(&self, events: &[NativeEvent]) {
        let manifest = NativeManifest {
            v: 1,
            stage_id: super::support::STAGE.into(),
            unit_id: UNIT_ID.into(),
            invocation_id: "inv-00000000000000000000000000000001".into(),
            owner_session_id: super::support::LOOM_SESSION.into(),
            workspace_root: self.repo.clone(),
            brief_path: self.repo.join("brief.md"),
            brief_sha256: sha256_hex(BRIEF),
            model: "gpt-6.1-terra".into(),
            effort: "xhigh".into(),
            deadline_secs: NATIVE_DEADLINE_SECS,
            created_at: Utc::now(),
        };
        let dir = create_invocation(&self.work, &manifest, BRIEF).expect("create invocation");
        let mut log = EventLog::open(&dir).expect("open event log");
        for event in events {
            log.append(event.clone()).expect("append event");
        }
    }

    fn watch_native_text(&self) -> Output {
        let worker = format!("codex:{UNIT_ID}");
        self.run(&["subagents", "watch", "--worker", &worker, "--timeout", "3"])
    }
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn progress_lines(lines: &[String]) -> Vec<&String> {
    lines
        .iter()
        .filter(|line| line.contains(&format!("codex:{UNIT_ID} active")))
        .collect()
}

#[test]
fn active_native_worker_reports_provider_progress_age() {
    let fixture = Fixture::new("native-progress-age");
    fixture.seed_native_worker(&[
        NativeEvent::ThreadStarted {
            thread_id: "thread-1".into(),
        },
        NativeEvent::TurnStarted {
            turn_id: "turn-1".into(),
        },
    ]);

    let output = fixture.watch_native_text();

    assert_exit(&output, 2);
    let lines = stdout_lines(&output);
    assert!(lines[0].starts_with("waiting:"), "{lines:?}");
    let progress = progress_lines(&lines);
    assert_eq!(progress.len(), 1, "once per 30s interval: {lines:?}");
    assert!(
        progress[0].contains("active, progress ") && progress[0].ends_with("s ago"),
        "{}",
        progress[0]
    );
    assert!(lines.last().unwrap().starts_with("timed out"), "{lines:?}");
}

#[test]
fn active_native_worker_without_provider_events_has_no_progress_yet() {
    let fixture = Fixture::new("native-progress-none");
    fixture.seed_native_worker(&[NativeEvent::BoundaryCreated]);

    let output = fixture.watch_native_text();

    assert_exit(&output, 2);
    let lines = stdout_lines(&output);
    let progress = progress_lines(&lines);
    assert_eq!(progress.len(), 1, "{lines:?}");
    assert!(progress[0].ends_with("no progress yet"), "{}", progress[0]);
}

#[test]
fn json_stream_carries_no_progress_lines() {
    let fixture = Fixture::new("native-progress-json");
    fixture.seed_native_worker(&[NativeEvent::Progress]);

    let output = fixture.watch(&[&format!("codex:{UNIT_ID}")], 3);

    assert_exit(&output, 2);
    let events = fixture.events(&output);
    assert_eq!(events.len(), 2, "initial and terminal records only");
    assert_eq!(events[0]["outcome"], "waiting");
    assert_eq!(events[1]["outcome"], "timed_out");
}
