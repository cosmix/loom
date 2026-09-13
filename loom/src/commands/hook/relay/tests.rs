//! Relay core: session proof, scratch check, kind admission and inbox
//! outcomes, driven through an injected environment snapshot.

use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::Session;
use crate::orchestrator::terminal::native::write_test_pid_identity;
use crate::relay::{new_request_id, sha256_hex, Ticket};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
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

    fn ticket_exists(&self, line: &RelayLine) -> bool {
        ticket_path(&self.env.scratch_dir, line).exists()
    }
}

/// Write a ticket the way the CLI does; return its line and exact bytes.
fn write_ticket(scratch_dir: &Path, kind: RequestKind) -> (RelayLine, Vec<u8>) {
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
    std::fs::write(ticket_path(scratch_dir, &line), &bytes).unwrap();
    (line, bytes)
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

fn json_entries(dir: &Path) -> usize {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .filter(|entry| {
                let name = entry.as_ref().unwrap().file_name();
                name.to_string_lossy().ends_with(".json")
            })
            .count()
    })
}

#[test]
fn a_ticketed_line_is_relayed_into_the_inbox_and_its_ticket_consumed() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&format!("noted\n{}", line.format()), None);
    let reply = fx.relay(&payload, &[RequestKind::Memory]).unwrap();

    assert!(
        reply.contains(&format!("received memory {}", &line.id[..8])),
        "{reply}"
    );
    let stored = std::fs::read(fx.inbox().join(format!("{}.json", line.id))).unwrap();
    let entry = InboxEntry::decode(&stored).unwrap();
    assert_eq!(entry.session_id, fx.session.id);
    assert_eq!(entry.stage_id, STAGE);
    assert_eq!(entry.agent, AgentRole::Main);
    assert_eq!(entry.tool_use_id.as_deref(), Some("toolu_1"));
    assert_eq!(entry.payload, json!({"content": "note"}));
    assert!(!fx.ticket_exists(&line));
}

#[test]
fn the_same_payload_twice_yields_one_entry() {
    let fx = Fixture::new();
    let (line, bytes) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);
    let payload = bash_payload(&line.format(), None);

    assert!(fx.relay(&payload, &[RequestKind::Memory]).is_some());
    assert_eq!(
        fx.relay(&payload, &[RequestKind::Memory]),
        None,
        "a consumed ticket relays nothing"
    );

    // A ticket that outlived its relay (its removal failed) is recognised.
    std::fs::write(ticket_path(&fx.env.scratch_dir, &line), &bytes).unwrap();
    let reply = fx.relay(&payload, &[RequestKind::Memory]).unwrap();
    assert!(reply.contains("already relayed"), "{reply}");
    assert!(!fx.ticket_exists(&line));
    assert_eq!(json_entries(&fx.inbox()), 1);
}

#[test]
fn a_fixture_line_without_a_ticket_produces_nothing() {
    let fx = Fixture::new();
    let line = RelayLine {
        kind: RequestKind::Memory,
        id: new_request_id(),
        sha256: sha256_hex(b"fixture"),
        bytes: 7,
    };

    let payload = bash_payload(&line.format(), None);
    assert_eq!(fx.relay(&payload, &[RequestKind::Memory]), None);
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn a_kind_the_command_does_not_write_is_refused_and_its_ticket_kept() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let reply = fx.relay(&bash_payload(&line.format(), None), &[]).unwrap();
    assert!(
        reply.contains("did not run the loom command that writes memory"),
        "{reply}"
    );
    assert_eq!(json_entries(&fx.inbox()), 0);
    assert!(fx.ticket_exists(&line));
}

#[test]
fn a_control_kind_from_a_subagent_is_refused_even_when_allowed() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);

    let payload = bash_payload(&line.format(), Some("general-purpose"));
    let reply = fx.relay(&payload, &[RequestKind::Block]).unwrap();
    assert!(reply.contains("never relayed from a subagent"), "{reply}");
    assert_eq!(json_entries(&fx.inbox()), 0);
}

#[test]
fn a_subagent_memory_request_is_attributed_to_the_subagent() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&line.format(), Some("general-purpose"));
    fx.relay(&payload, &[RequestKind::Memory]).unwrap();
    let stored = std::fs::read(fx.inbox().join(format!("{}.json", line.id))).unwrap();
    assert_eq!(
        InboxEntry::decode(&stored).unwrap().agent,
        AgentRole::Subagent
    );
}

#[test]
fn a_forged_session_id_writes_nothing() {
    let fx = Fixture::new();
    let forged = env_for(fx.root.path(), "session-forged");
    let (line, _) = write_ticket(&forged.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&line.format(), None);
    let reply = relay_payload(&payload, &forged, &fx.work_dir, &[RequestKind::Memory]).unwrap();
    assert!(
        reply.contains("not running inside session session-forged"),
        "{reply}"
    );
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn a_session_that_stopped_running_writes_nothing() {
    let mut fx = Fixture::new();
    fx.session.status = SessionStatus::Completed;
    save_session(&fx.session, &fx.work_dir).unwrap();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&line.format(), None);
    let reply = fx.relay(&payload, &[RequestKind::Memory]).unwrap();
    assert!(reply.contains("is not a Running session"), "{reply}");
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn a_session_of_another_stage_writes_nothing() {
    let mut fx = Fixture::new();
    fx.env.stage_id = "stage-b".to_string();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&line.format(), None);
    let reply = fx.relay(&payload, &[RequestKind::Memory]).unwrap();
    assert!(
        reply.contains("is not a Running session of stage stage-b"),
        "{reply}"
    );
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn a_scratch_dir_other_than_the_derived_one_is_refused() {
    let mut fx = Fixture::new();
    let elsewhere = fx.root.path().join("elsewhere").join(&fx.session.id);
    make_0700(&elsewhere);
    fx.env.scratch_dir = elsewhere;
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&line.format(), None);
    let reply = fx.relay(&payload, &[RequestKind::Memory]).unwrap();
    assert!(
        reply.contains("is not this session's scratch directory"),
        "{reply}"
    );
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn a_scratch_dir_other_users_can_read_is_refused() {
    let fx = Fixture::new();
    std::fs::set_permissions(&fx.env.scratch_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&line.format(), None);
    let reply = fx.relay(&payload, &[RequestKind::Memory]).unwrap();
    assert!(reply.contains("expected 700"), "{reply}");
    assert!(!fx.work_dir.join("inbox").exists());
}

#[test]
fn a_full_inbox_is_refused_and_the_ticket_kept() {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.inbox()).unwrap();
    for n in 0..MAX_PENDING_ENTRIES {
        std::fs::write(fx.inbox().join(format!("filler-{n}.json")), b"{}").unwrap();
    }
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let payload = bash_payload(&line.format(), None);
    let reply = fx.relay(&payload, &[RequestKind::Memory]).unwrap();
    assert!(reply.contains("pending requests"), "{reply}");
    assert!(fx.ticket_exists(&line));
}

#[test]
fn a_ticketed_line_inside_a_genuine_persisted_file_is_relayed() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);
    let results = fx
        .env
        .projects_root
        .join("proj")
        .join("sess")
        .join("tool-results");
    std::fs::create_dir_all(&results).unwrap();
    let persisted = results.join("out.txt");
    std::fs::write(&persisted, format!("a lot of output\n{}\n", line.format())).unwrap();
    let payload = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "loom memory note x"},
        "tool_response": {"stdout": "a lot of", "persistedOutputPath": persisted},
    });

    let reply = fx
        .relay(payload.to_string().as_bytes(), &[RequestKind::Memory])
        .unwrap();
    assert!(reply.contains("received memory"), "{reply}");
}

#[test]
fn a_non_bash_payload_is_ignored() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);
    let payload = json!({"tool_name": "Read", "tool_response": {"stdout": line.format()}});

    let reply = fx.relay(payload.to_string().as_bytes(), &[RequestKind::Memory]);
    assert_eq!(reply, None);
    assert!(fx.ticket_exists(&line));
}

#[test]
fn an_unusable_environment_is_reported_only_for_relay_lines() {
    let line = RelayLine {
        kind: RequestKind::Memory,
        id: new_request_id(),
        sha256: sha256_hex(b"x"),
        bytes: 1,
    };
    let reason = "LOOM_STAGE_ID is not set";

    let reply = unavailable(&bash_payload(&line.format(), None), reason).unwrap();
    assert!(reply.contains(reason), "{reply}");
    assert_eq!(
        unavailable(&bash_payload("plain output", None), reason),
        None
    );
}

#[test]
fn allowed_kinds_keep_known_names_only() {
    assert_eq!(
        parse_allowed_kinds("memory,merge-resolved,bogus,"),
        vec![RequestKind::Memory, RequestKind::MergeResolved]
    );
    assert!(parse_allowed_kinds("").is_empty());
}
