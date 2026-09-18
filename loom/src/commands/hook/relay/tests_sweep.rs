//! The sweep: a ticket whose line never reached the hook is recovered by a
//! later call of the same session, and only when that call may relay its kind.

use super::super::{relay_payload, RelayEnv};
use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::relay::{new_request_id, sha256_hex, AgentRole, InboxEntry, Ticket, MAX_LINES_PER_CALL};
use chrono::Utc;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

const STAGE: &str = "stage-a";

fn make_0700(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

/// The snapshot session `session_id` would see under `root`, with its scratch
/// directory created where loom derives it.
fn env_for(root: &Path, session_id: &str) -> RelayEnv {
    let scratch_root = root.join("scratch");
    let scratch_dir = scratch_root.join(session_id);
    make_0700(&scratch_root);
    make_0700(&scratch_dir);
    RelayEnv {
        session_id: session_id.to_string(),
        stage_id: STAGE.to_string(),
        scratch_dir,
        scratch_root,
        projects_root: root.join("home").join(".claude").join("projects"),
        // SAFETY: `getuid` has no preconditions and cannot fail.
        uid: unsafe { libc::getuid() },
        pid: std::process::id(),
    }
}

/// A Running session of [`STAGE`] whose PID evidence names this test process.
struct Fixture {
    root: TempDir,
    work_dir: PathBuf,
    session: Session,
    env: RelayEnv,
}

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let work_dir = root.path().join("work");
        std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
        let mut session = Session::new();
        session.assign_to_stage(STAGE.to_string());
        session.status = SessionStatus::Running;
        save_session(&session, &work_dir).unwrap();
        write_test_pid_identity(&work_dir, &session, std::process::id()).unwrap();
        let env = env_for(root.path(), &session.id);
        Fixture {
            root,
            work_dir,
            session,
            env,
        }
    }

    fn relay(&self, payload: &[u8], allowed: &[RequestKind]) -> Option<String> {
        relay_payload(payload, &self.env, &self.work_dir, allowed)
    }

    fn inbox(&self) -> PathBuf {
        self.work_dir.join("inbox").join(&self.session.id)
    }

    /// The ids of the entries that reached the inbox.
    fn entry_ids(&self) -> Vec<String> {
        stems(&self.inbox(), ".json")
    }

    /// The ids of the tickets still sitting in the scratch directory.
    fn leftover_ids(&self) -> Vec<String> {
        stems(&self.env.scratch_dir, ".req")
    }
}

fn stems(dir: &Path, suffix: &str) -> Vec<String> {
    let Ok(names) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<String> = names
        .flatten()
        .filter_map(|name| Some(name.file_name().to_str()?.strip_suffix(suffix)?.to_string()))
        .collect();
    found.sort();
    found
}

/// Write a ticket the way the CLI does; return the line that describes it.
fn write_ticket(scratch_dir: &Path, kind: RequestKind) -> RelayLine {
    let ticket = Ticket {
        v: 1,
        id: new_request_id(),
        kind,
        created_at: Utc::now(),
        payload: json!({"content": "note"}),
    };
    let bytes = ticket.encode();
    let line = RelayLine {
        kind,
        id: ticket.id,
        sha256: sha256_hex(&bytes),
        bytes: bytes.len() as u32,
    };
    std::fs::write(ticket_check::ticket_file(scratch_dir, &line.id), &bytes).unwrap();
    line
}

/// Backdate a ticket so the sweep's oldest-first order is unambiguous.
fn backdate(scratch_dir: &Path, line: &RelayLine, seconds: u64) {
    let when = SystemTime::now() - Duration::from_secs(seconds);
    let file = std::fs::File::options()
        .write(true)
        .open(ticket_check::ticket_file(scratch_dir, &line.id))
        .unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(when))
        .unwrap();
}

fn bash_payload(stdout: &str, agent_type: Option<&str>) -> Vec<u8> {
    let mut payload = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "loom memory note x"},
        "tool_response": {"stdout": stdout, "stderr": ""},
        "tool_use_id": "toolu_1",
    });
    if let Some(agent_type) = agent_type {
        payload["agent_type"] = json!(agent_type);
    }
    payload.to_string().into_bytes()
}

/// The reported deadlock: one Bash call writing more tickets than a call's
/// lines are extracted from used to leak every ticket past the cap.
#[test]
fn the_tickets_past_the_line_cap_are_recovered_in_the_same_call() {
    let fx = Fixture::new();
    let written = MAX_LINES_PER_CALL + 1;
    let lines: Vec<RelayLine> = (0..written)
        .map(|_| write_ticket(&fx.env.scratch_dir, RequestKind::Memory))
        .collect();
    let stdout: Vec<String> = lines.iter().map(RelayLine::format).collect();

    let reply = fx
        .relay(
            &bash_payload(&stdout.join("\n"), None),
            &[RequestKind::Memory],
        )
        .unwrap();

    assert_eq!(fx.entry_ids().len(), written, "{reply}");
    assert!(fx.leftover_ids().is_empty(), "{reply}");
    let past_the_cap = &lines[MAX_LINES_PER_CALL].id;
    assert!(fx.entry_ids().contains(past_the_cap), "{reply}");
}

#[test]
fn a_ticket_no_line_names_is_recovered_with_its_own_wording() {
    let fx = Fixture::new();
    let line = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let reply = fx
        .relay(
            &bash_payload("nothing to see", None),
            &[RequestKind::Memory],
        )
        .unwrap();

    assert_eq!(fx.entry_ids(), vec![line.id.clone()], "{reply}");
    assert!(fx.leftover_ids().is_empty(), "{reply}");
    assert!(
        reply.contains(&format!("received memory {}", &line.id[..8])),
        "{reply}"
    );
    assert!(reply.contains("never reached the relay hook"), "{reply}");
    let stored = std::fs::read(fx.inbox().join(format!("{}.json", line.id))).unwrap();
    let entry = InboxEntry::decode(&stored).unwrap();
    assert_eq!(entry.session_id, fx.session.id);
    assert_eq!(entry.stage_id, STAGE);
    assert_eq!(entry.tool_use_id.as_deref(), Some("toolu_1"));
    assert_eq!(entry.payload, json!({"content": "note"}));
}

/// The sweep has no line to attribute, so it uses the call doing the sweeping.
#[test]
fn a_recovered_ticket_is_attributed_to_the_call_that_swept_it() {
    let fx = Fixture::new();
    let line = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload("nothing to see", Some("general-purpose"));
    fx.relay(&payload, &[RequestKind::Memory]).unwrap();

    let stored = std::fs::read(fx.inbox().join(format!("{}.json", line.id))).unwrap();
    assert_eq!(
        InboxEntry::decode(&stored).unwrap().agent,
        AgentRole::Subagent
    );
}

#[test]
fn a_command_that_writes_no_request_sweeps_nothing() {
    let fx = Fixture::new();
    let line = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    assert_eq!(fx.relay(&bash_payload("ls output", None), &[]), None);
    assert_eq!(fx.leftover_ids(), vec![line.id]);
    assert!(!fx.work_dir.join("inbox").exists());
}

/// A control ticket stays on disk: relaying it from a later call would record
/// one agent's block, handoff or verdict as another's.
#[test]
fn a_control_ticket_is_never_swept() {
    let fx = Fixture::new();
    let line = write_ticket(&fx.env.scratch_dir, RequestKind::Block);

    assert_eq!(
        fx.relay(
            &bash_payload("loom stage block", None),
            &[RequestKind::Block]
        ),
        None
    );
    assert_eq!(fx.leftover_ids(), vec![line.id]);
    assert!(!fx.work_dir.join("inbox").exists());
}

/// Silently: a refusal here would repeat on every later call of the session.
#[test]
fn a_ticket_of_a_kind_the_command_does_not_write_is_passed_over() {
    let fx = Fixture::new();
    let line = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    assert_eq!(
        fx.relay(
            &bash_payload("telemetry only", None),
            &[RequestKind::Telemetry]
        ),
        None
    );
    assert_eq!(fx.leftover_ids(), vec![line.id]);
    assert!(!fx.work_dir.join("inbox").exists());
}

/// A call with nothing to relay never proves the session, so a Bash call that
/// only mentions a loom command cannot be told it was refused.
#[test]
fn an_empty_scratch_directory_provokes_no_refusal() {
    let fx = Fixture::new();
    let forged = env_for(fx.root.path(), "session-forged");
    let payload = bash_payload("loom memory note x", None);

    assert_eq!(
        relay_payload(&payload, &forged, &fx.work_dir, &[RequestKind::Memory]),
        None
    );

    // The same call with a ticket to sweep does prove the session, and names
    // the ticket it refused.
    let line = write_ticket(&forged.scratch_dir, RequestKind::Memory);
    let reply = relay_payload(&payload, &forged, &fx.work_dir, &[RequestKind::Memory]).unwrap();
    assert!(
        reply.contains(&format!("refused ticket {}", &line.id[..8])),
        "{reply}"
    );
    assert!(
        reply.contains("not running inside session session-forged"),
        "{reply}"
    );
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn a_file_that_does_not_read_as_a_ticket_is_reported_and_kept() {
    let fx = Fixture::new();
    let id = new_request_id();
    std::fs::write(ticket_check::ticket_file(&fx.env.scratch_dir, &id), b"junk").unwrap();

    let reply = fx
        .relay(
            &bash_payload("nothing to see", None),
            &[RequestKind::Memory],
        )
        .unwrap();

    assert!(
        reply.contains(&format!("refused ticket {}", &id[..8])),
        "{reply}"
    );
    assert_eq!(fx.leftover_ids(), vec![id]);
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn leftover_tickets_are_recovered_oldest_first() {
    let fx = Fixture::new();
    let lines: Vec<RelayLine> = (0..3)
        .map(|_| write_ticket(&fx.env.scratch_dir, RequestKind::Memory))
        .collect();
    for (age, line) in [30u64, 20, 10].iter().zip(&lines) {
        backdate(&fx.env.scratch_dir, line, *age);
    }

    let reply = fx
        .relay(
            &bash_payload("nothing to see", None),
            &[RequestKind::Memory],
        )
        .unwrap();

    let order: Vec<&str> = reply.lines().collect();
    assert_eq!(order.len(), 3, "{reply}");
    for (position, line) in lines.iter().enumerate() {
        assert!(
            order[position].contains(&line.id[..8]),
            "position {position}: {reply}"
        );
    }
    assert!(fx.leftover_ids().is_empty(), "{reply}");
}
