use std::fs;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;

use super::*;
use crate::plan_session::guard::GuardPolicy;

const FAST: Duration = Duration::from_millis(50);

/// A canonical session directory with `answers/` and, unless `kind` is `None`, `guard.json`.
struct Session {
    _tmp: TempDir,
    dir: PathBuf,
}

impl Session {
    fn new(kind: Option<SessionKind>) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let dir = base.join("sessions/s1");
        fs::create_dir_all(dir.join(ANSWERS)).unwrap();
        if let Some(kind) = kind {
            let policy = GuardPolicy {
                schema: 1,
                kind,
                project_root: base.join("proj"),
                runtime_dir: base.clone(),
                session_dir: dir.clone(),
                home: base.join("home"),
            };
            fs::write(dir.join("guard.json"), serde_json::to_vec(&policy).unwrap()).unwrap();
        }
        Self { _tmp: tmp, dir }
    }

    fn answers(&self) -> PathBuf {
        self.dir.join(ANSWERS)
    }

    fn spool_lines(&self) -> Vec<Value> {
        let text = fs::read_to_string(self.dir.join("events.jsonl")).unwrap_or_default();
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// Run the hook on a thread with `wait`; the receiver yields its stdout.
    fn spawn(&self, payload: &Value, wait: Duration) -> Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        let dir = self.dir.clone();
        let stdin = serde_json::to_vec(payload).unwrap();
        std::thread::spawn(move || {
            let mut out = Vec::new();
            run_hook_with(&dir, stdin.as_slice(), &mut out, wait, FAST);
            let _ = tx.send(out);
        });
        rx
    }

    /// The request id of the first spool line, waiting up to 5 s for it.
    fn request_id(&self) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(line) = self.spool_lines().first() {
                return line["request_id"].as_str().unwrap().to_string();
            }
            assert!(Instant::now() < deadline, "no spool line within 5 s");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn payload() -> Value {
    json!({
        "session_id": "105e2c4a-7f1b-4c3d-9a8e-2b6d0f1e3a57",
        "transcript_path": "/home/u/.claude/projects/p/105e.jsonl",
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf build"},
    })
}

fn decision(out: &[u8]) -> Value {
    let text = std::str::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    let answer: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        answer["hookSpecificOutput"]["hookEventName"], EVENT,
        "{answer}"
    );
    answer["hookSpecificOutput"]["decision"].clone()
}

fn deny(message: &str) -> Value {
    json!({"behavior": "deny", "message": message})
}

fn answer(behavior: Behavior, message: Option<&str>, reason: Option<DenyReason>) -> Answer {
    Answer {
        behavior,
        message: message.map(str::to_string),
        reason,
    }
}

const LONG: Duration = Duration::from_secs(30);
const ANSWERED: Duration = Duration::from_secs(5);

#[test]
fn an_allow_answer_allows() {
    let session = Session::new(Some(SessionKind::Authoring));
    let done = session.spawn(&payload(), LONG);
    let id = session.request_id();
    assert!(uuid::Uuid::try_parse(&id).is_ok(), "{id}");
    let line = &session.spool_lines()[0];
    assert_eq!(line["event"], EVENT);
    assert_eq!(line["payload"]["tool_name"], "Bash");
    assert!(done.try_recv().is_err(), "the hook waits for the answer");

    write_answer(&session.dir, &id, &answer(Behavior::Allow, None, None)).unwrap();
    let out = done.recv_timeout(ANSWERED).unwrap();
    assert_eq!(decision(&out), json!({"behavior": "allow"}));
}

#[test]
fn a_holder_deny_carries_the_note() {
    let session = Session::new(Some(SessionKind::Authoring));
    let done = session.spawn(&payload(), LONG);
    let id = session.request_id();
    let denied = answer(
        Behavior::Deny,
        Some("use cargo clean"),
        Some(DenyReason::Holder),
    );
    write_answer(&session.dir, &id, &denied).unwrap();
    assert_eq!(
        decision(&done.recv_timeout(ANSWERED).unwrap()),
        deny("use cargo clean")
    );
}

#[test]
fn a_deny_without_a_note_names_its_reason() {
    let session = Session::new(Some(SessionKind::Pressure {
        plan: "doc/plans/PLAN-x.md".into(),
    }));
    let done = session.spawn(&payload(), LONG);
    let id = session.request_id();
    let expired = answer(Behavior::Deny, None, Some(DenyReason::Timeout));
    write_answer(&session.dir, &id, &expired).unwrap();
    let message = deny_text(Some(DenyReason::Timeout));
    assert_eq!(
        decision(&done.recv_timeout(ANSWERED).unwrap()),
        deny(message)
    );
}

#[test]
fn removing_answers_mid_wait_denies_within_a_second() {
    let session = Session::new(Some(SessionKind::Authoring));
    let done = session.spawn(&payload(), LONG);
    session.request_id();
    fs::remove_dir_all(session.answers()).unwrap();
    let out = done
        .recv_timeout(Duration::from_secs(1))
        .expect("a deny within 1 s");
    let message = "loom plan: the session ended before an answer arrived";
    assert_eq!(decision(&out), deny(message));
}

#[test]
fn the_wait_expiring_denies() {
    let session = Session::new(Some(SessionKind::Authoring));
    let done = session.spawn(&payload(), Duration::from_millis(200));
    let out = done.recv_timeout(ANSWERED).unwrap();
    assert_eq!(decision(&out), deny("loom plan: no answer arrived in time"));
    assert_eq!(session.spool_lines().len(), 1);
}

#[test]
fn an_unparsable_or_symlinked_answer_denies() {
    let session = Session::new(Some(SessionKind::Authoring));
    let done = session.spawn(&payload(), LONG);
    let id = session.request_id();
    fs::write(session.answers().join(format!("{id}.json")), b"{not json").unwrap();
    let message = decision(&done.recv_timeout(ANSWERED).unwrap())["message"].clone();
    assert!(
        message.as_str().unwrap().contains("unparsable"),
        "{message}"
    );

    let linked = Session::new(Some(SessionKind::Authoring));
    let target = linked.dir.join("allow.json");
    fs::write(
        &target,
        br#"{"behavior":"allow","message":null,"reason":null}"#,
    )
    .unwrap();
    let done = linked.spawn(&payload(), LONG);
    let id = linked.request_id();
    std::os::unix::fs::symlink(&target, linked.answers().join(format!("{id}.json"))).unwrap();
    let verdict = decision(&done.recv_timeout(ANSWERED).unwrap());
    assert_eq!(
        verdict["behavior"], "deny",
        "a symlinked answer is never followed"
    );
}

#[test]
fn a_summary_session_is_denied_at_once_and_logged() {
    let session = Session::new(Some(SessionKind::Summary));
    let done = session.spawn(&payload(), LONG);
    let out = done
        .recv_timeout(Duration::from_secs(2))
        .expect("answered at once");
    assert_eq!(decision(&out), deny(SUMMARY_DENY));
    let lines = session.spool_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["payload"]["auto_denied"], "summary_session");
}

#[test]
fn a_missing_policy_or_session_directory_denies_at_once() {
    let session = Session::new(None);
    let out = session
        .spawn(&payload(), LONG)
        .recv_timeout(ANSWERED)
        .unwrap();
    let message = decision(&out)["message"].as_str().unwrap().to_string();
    assert!(
        message.starts_with("loom plan: cannot load the guard policy"),
        "{message}"
    );
    assert_eq!(
        session.spool_lines()[0]["payload"]["auto_denied"],
        "hook_error"
    );

    let gone = Session::new(Some(SessionKind::Authoring));
    fs::remove_dir_all(&gone.dir).unwrap();
    let out = gone.spawn(&payload(), LONG).recv_timeout(ANSWERED).unwrap();
    assert_eq!(decision(&out)["behavior"], "deny");
}

#[test]
fn an_oversized_payload_denies_and_another_event_is_silent() {
    let session = Session::new(Some(SessionKind::Authoring));
    let huge = vec![b' '; 1024 * 1024 + 1];
    let mut out = Vec::new();
    run_hook_with(&session.dir, huge.as_slice(), &mut out, LONG, FAST);
    assert_eq!(decision(&out)["behavior"], "deny");

    let mut other = payload();
    other["hook_event_name"] = json!("PreToolUse");
    let mut out = Vec::new();
    let stdin = serde_json::to_vec(&other).unwrap();
    run_hook_with(&session.dir, stdin.as_slice(), &mut out, LONG, FAST);
    assert!(out.is_empty(), "{}", String::from_utf8_lossy(&out));
    assert!(session.spool_lines().is_empty());
}

#[test]
fn write_answer_refuses_ids_that_are_not_canonical_uuids() {
    let session = Session::new(None);
    let denied = answer(Behavior::Deny, Some("no"), Some(DenyReason::Holder));
    let id = uuid::Uuid::new_v4();
    let refused = [
        "../x".to_string(),
        "a/b".to_string(),
        "not-a-uuid".to_string(),
        String::new(),
        id.hyphenated().to_string().to_uppercase(),
        id.simple().to_string(),
        id.braced().to_string(),
        format!("../{id}"),
    ];
    for bad in &refused {
        assert!(write_answer(&session.dir, bad, &denied).is_err(), "{bad:?}");
    }
    assert_eq!(fs::read_dir(session.answers()).unwrap().count(), 0);

    write_answer(&session.dir, &id.to_string(), &denied).unwrap();
    let names: Vec<String> = fs::read_dir(session.answers())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [format!("{id}.json")]);
}

#[test]
fn write_answer_replaces_a_stale_temp_and_fits_a_long_note() {
    let session = Session::new(None);
    let id = uuid::Uuid::new_v4().to_string();
    fs::write(session.answers().join(format!(".{id}.tmp")), b"partial").unwrap();
    let note = "é\"\n".repeat(3_000);
    let long = answer(Behavior::Deny, Some(&note), Some(DenyReason::Holder));
    write_answer(&session.dir, &id, &long).unwrap();

    let bytes = fs::read(session.answers().join(format!("{id}.json"))).unwrap();
    assert!(bytes.len() <= MAX_ANSWER_BYTES, "{}", bytes.len());
    let written: Answer = serde_json::from_slice(&bytes).unwrap();
    let message = written.message.unwrap();
    let kept = message.strip_suffix('…').expect("the cut is marked");
    assert!(note.starts_with(kept), "{kept:?}");
    // 6 serialised bytes per 3 characters: most of the 4 KiB bound is the note.
    assert!(kept.chars().count() > 1_900, "{}", kept.chars().count());
    assert!(!session.answers().join(format!(".{id}.tmp")).exists());
}

#[test]
fn write_answer_fails_when_answers_is_a_symlink() {
    let session = Session::new(None);
    let elsewhere = session.dir.join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::remove_dir(session.answers()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, session.answers()).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let allow = answer(Behavior::Allow, None, None);
    assert!(write_answer(&session.dir, &id, &allow).is_err());
    assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
}
