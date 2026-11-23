//! Contracts `forged-lines-crowd-out-a-ticketed-line`,
//! `control-ticket-older-than-the-call-marker` and
//! `refused-subagent-control-ticket-is-kept`: a relayed control request is
//! tied to the lead's own Bash call. The call marker
//! `<scratch_dir>/.pre-<tool_use_id>` is written at PreToolUse; a control
//! ticket is relayed only when it was written at or after that marker, lines
//! without a ticket file never count against the per-call line cap, and a
//! control ticket refused because a subagent printed it is deleted so no
//! later lead call can replay it.

use std::fs::{File, FileTimes};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::json;
use tempfile::TempDir;

use super::ticket_check::ticket_file;
use super::{relay_payload, RelayEnv};
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::relay::{new_request_id, RelayLine, RequestKind, Ticket};
use crate::utils::sha256_hex;

const STAGE: &str = "stage-a";
const TOOL_USE_ID: &str = "toolu_contract";

fn make_0700(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

/// A Running session of [`STAGE`] whose PID evidence names this process, and
/// the relay environment it would see.
struct Fixture {
    _root: TempDir,
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
        let scratch_root = root.path().join("scratch");
        let scratch_dir = scratch_root.join(&session.id);
        make_0700(&scratch_root);
        make_0700(&scratch_dir);
        let env = RelayEnv {
            session_id: session.id.clone(),
            stage_id: STAGE.to_string(),
            scratch_dir,
            scratch_root,
            projects_root: root.path().join("home").join(".claude").join("projects"),
            // SAFETY: `getuid` has no preconditions and cannot fail.
            uid: unsafe { libc::getuid() },
            pid: std::process::id(),
        };
        Fixture {
            _root: root,
            work_dir,
            session,
            env,
        }
    }

    /// The empty call marker `loom-relay-guard.sh` writes at PreToolUse.
    fn write_call_marker(&self) {
        let marker = self.env.scratch_dir.join(format!(".pre-{TOOL_USE_ID}"));
        std::fs::write(marker, b"").unwrap();
    }

    /// Write a Block ticket the way the CLI does; return its id and line.
    fn write_block_ticket(&self) -> (String, String) {
        let ticket = Ticket {
            v: 1,
            id: new_request_id(),
            kind: RequestKind::Block,
            created_at: chrono::Utc::now(),
            payload: json!({"content": "note"}),
        };
        let bytes = ticket.encode();
        std::fs::write(ticket_file(&self.env.scratch_dir, &ticket.id), &bytes).unwrap();
        let line = RelayLine {
            kind: RequestKind::Block,
            id: ticket.id.clone(),
            sha256: sha256_hex(&bytes),
            bytes: bytes.len() as u32,
        };
        (ticket.id, line.format())
    }

    fn relay(&self, stdout: &str, agent_type: Option<&str>) -> Option<String> {
        let mut payload = json!({
            "tool_name": "Bash",
            "tool_input": {"command": "loom stage block stage-a why"},
            "tool_response": {"stdout": stdout, "stderr": ""},
            "tool_use_id": TOOL_USE_ID,
        });
        if let Some(agent_type) = agent_type {
            payload["agent_type"] = json!(agent_type);
        }
        let bytes = payload.to_string().into_bytes();
        relay_payload(&bytes, &self.env, &self.work_dir, &[RequestKind::Block])
    }

    fn inbox_file(&self, id: &str) -> PathBuf {
        self.work_dir
            .join("inbox")
            .join(&self.session.id)
            .join(format!("{id}.json"))
    }
}

#[test]
fn sixteen_forged_lines_do_not_crowd_out_a_ticketed_control_line() {
    let fx = Fixture::new();
    fx.write_call_marker();
    let mut stdout: Vec<String> = (0..16)
        .map(|_| {
            RelayLine {
                kind: RequestKind::Block,
                id: new_request_id(),
                sha256: "0".repeat(64),
                bytes: 64,
            }
            .format()
        })
        .collect();
    let (id, line) = fx.write_block_ticket();
    stdout.push(line);

    let reply = fx.relay(&stdout.join("\n"), None);

    assert!(
        fx.inbox_file(&id).exists(),
        "the ticketed line after sixteen forged ones was not relayed: {reply:?}"
    );
}

#[test]
fn a_control_ticket_older_than_the_call_marker_is_not_relayed() {
    let fx = Fixture::new();
    fx.write_call_marker();
    let (old_id, old_line) = fx.write_block_ticket();
    let backdated = SystemTime::now() - Duration::from_secs(30);
    File::options()
        .write(true)
        .open(ticket_file(&fx.env.scratch_dir, &old_id))
        .unwrap()
        .set_times(FileTimes::new().set_modified(backdated))
        .unwrap();
    let (new_id, new_line) = fx.write_block_ticket();

    let reply = fx.relay(&format!("{old_line}\n{new_line}"), None);

    assert!(
        fx.inbox_file(&new_id).exists(),
        "the ticket written after the call marker was not relayed: {reply:?}"
    );
    assert!(
        !fx.inbox_file(&old_id).exists(),
        "a ticket older than the call marker was relayed: {reply:?}"
    );
    assert!(
        ticket_file(&fx.env.scratch_dir, &old_id).exists(),
        "the refused older ticket was not kept: {reply:?}"
    );
}

#[test]
fn a_refused_subagent_control_ticket_is_deleted() {
    let fx = Fixture::new();
    let (id, line) = fx.write_block_ticket();

    let reply = fx.relay(&line, Some("general-purpose"));

    assert!(
        !fx.inbox_file(&id).exists(),
        "a subagent's control ticket was relayed: {reply:?}"
    );
    assert!(
        !ticket_file(&fx.env.scratch_dir, &id).exists(),
        "the refused subagent control ticket stayed on disk for a later call to replay: {reply:?}"
    );
}
