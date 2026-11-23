//! Control-ticket provenance: the lead's call marker, the call window and the
//! line cap, driven through the same fixtures as the relay core tests.

use std::fs::{File, FileTimes};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde_json::json;

use super::super::tests::{json_entries, write_ticket, Fixture};
use super::{marker_path, require_in_call_window};
use crate::relay::{new_request_id, RelayLine, RequestKind};
use crate::utils::sha256_hex;

fn payload(stdout: &str, tool_use_id: &str, agent_type: Option<&str>) -> Vec<u8> {
    let mut payload = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "loom stage block s why"},
        "tool_response": {"stdout": stdout, "stderr": ""},
        "tool_use_id": tool_use_id,
    });
    if let Some(agent_type) = agent_type {
        payload["agent_type"] = json!(agent_type);
    }
    payload.to_string().into_bytes()
}

fn marker(fx: &Fixture, tool_use_id: &str) -> PathBuf {
    fx.env.scratch_dir.join(format!(".pre-{tool_use_id}"))
}

fn write_marker(fx: &Fixture, tool_use_id: &str) {
    std::fs::write(marker(fx, tool_use_id), b"").unwrap();
}

#[test]
fn forged_lines_do_not_crowd_out_a_ticketed_control_line() {
    let fx = Fixture::new();
    write_marker(&fx, "toolu_a");
    let mut stdout: Vec<String> = (0..16)
        .map(|_| {
            RelayLine {
                kind: RequestKind::Block,
                id: new_request_id(),
                sha256: sha256_hex(b"forged"),
                bytes: 6,
            }
            .format()
        })
        .collect();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);
    stdout.push(line.format());

    let reply = fx
        .relay(
            &payload(&stdout.join("\n"), "toolu_a", None),
            &[RequestKind::Block],
        )
        .unwrap();

    assert!(reply.contains("received block"), "{reply}");
    assert_eq!(json_entries(&fx.inbox()), 1);
}

#[test]
fn a_control_ticket_older_than_the_call_marker_is_refused_and_kept() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);
    let backdated = SystemTime::now() - Duration::from_secs(30);
    File::options()
        .write(true)
        .open(super::super::ticket_check::ticket_path(
            &fx.env.scratch_dir,
            &line,
        ))
        .unwrap()
        .set_times(FileTimes::new().set_modified(backdated))
        .unwrap();
    write_marker(&fx, "toolu_a");

    let reply = fx
        .relay(
            &payload(&line.format(), "toolu_a", None),
            &[RequestKind::Block],
        )
        .unwrap();

    assert!(reply.contains("written before this call"), "{reply}");
    assert_eq!(json_entries(&fx.inbox()), 0);
    assert!(fx.ticket_exists(&line));
}

#[test]
fn a_control_ticket_without_a_marker_is_refused() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);

    let reply = fx
        .relay(
            &payload(&line.format(), "toolu_a", None),
            &[RequestKind::Block],
        )
        .unwrap();

    assert!(reply.contains("no call marker"), "{reply}");
    assert_eq!(json_entries(&fx.inbox()), 0);
    assert!(fx.ticket_exists(&line));
}

#[test]
fn a_marker_from_another_call_does_not_admit_a_ticket() {
    let fx = Fixture::new();
    write_marker(&fx, "toolu_other");
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);

    let reply = fx
        .relay(
            &payload(&line.format(), "toolu_a", None),
            &[RequestKind::Block],
        )
        .unwrap();

    assert!(reply.contains("no call marker"), "{reply}");
    assert_eq!(json_entries(&fx.inbox()), 0);
}

#[test]
fn a_refused_subagent_control_ticket_is_deleted() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);

    let reply = fx
        .relay(
            &payload(&line.format(), "toolu_a", Some("general-purpose")),
            &[RequestKind::Block],
        )
        .unwrap();

    assert!(reply.contains("never relayed from a subagent"), "{reply}");
    assert!(reply.contains("Its ticket was deleted."), "{reply}");
    assert!(!fx.ticket_exists(&line));
}

#[test]
fn a_subagent_line_that_does_not_verify_deletes_nothing() {
    let fx = Fixture::new();
    let (mut line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);
    line.sha256 = sha256_hex(b"another body");

    let reply = fx
        .relay(
            &payload(&line.format(), "toolu_a", Some("general-purpose")),
            &[RequestKind::Block],
        )
        .unwrap();

    assert!(reply.contains("SHA-256"), "{reply}");
    assert!(fx.ticket_exists(&line));
}

#[test]
fn the_relay_removes_its_own_call_marker() {
    let fx = Fixture::new();
    write_marker(&fx, "toolu_a");
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Block);

    fx.relay(
        &payload(&line.format(), "toolu_a", None),
        &[RequestKind::Block],
    )
    .unwrap();

    assert!(!marker(&fx, "toolu_a").exists());
}

#[test]
fn a_memory_ticket_needs_no_marker() {
    let fx = Fixture::new();
    let (line, _) = write_ticket(&fx.env.scratch_dir, RequestKind::Memory);

    let reply = fx
        .relay(
            &payload(&line.format(), "toolu_a", None),
            &[RequestKind::Memory],
        )
        .unwrap();

    assert!(reply.contains("received memory"), "{reply}");
    assert_eq!(json_entries(&fx.inbox()), 1);
}

#[test]
fn marker_paths_exist_only_for_plain_ids() {
    let dir = std::path::Path::new("/scratch");
    assert_eq!(
        marker_path(dir, "toolu_A-1"),
        Some(PathBuf::from("/scratch/.pre-toolu_A-1"))
    );
    assert_eq!(marker_path(dir, ""), None);
    assert_eq!(marker_path(dir, "../evil"), None);
    assert_eq!(marker_path(dir, "a b"), None);
    assert!(marker_path(dir, &"a".repeat(128)).is_some());
    assert_eq!(marker_path(dir, &"a".repeat(129)), None);
}

#[test]
fn a_ticket_dated_after_now_is_refused() {
    let fx = Fixture::new();
    write_marker(&fx, "toolu_a");
    let now = SystemTime::now();

    let refusal = require_in_call_window(
        &fx.env.scratch_dir,
        Some("toolu_a"),
        now + Duration::from_secs(600),
        fx.env.uid,
        now,
    )
    .unwrap_err();

    assert!(refusal.contains("in the future"), "{refusal}");
}

#[test]
fn a_marker_that_is_a_symlink_is_refused() {
    let fx = Fixture::new();
    let target = fx.env.scratch_dir.join("elsewhere");
    std::fs::write(&target, b"").unwrap();
    std::os::unix::fs::symlink(&target, marker(&fx, "toolu_a")).unwrap();
    let now = SystemTime::now();

    let refusal =
        require_in_call_window(&fx.env.scratch_dir, Some("toolu_a"), now, fx.env.uid, now)
            .unwrap_err();

    assert!(refusal.contains("not a plain file"), "{refusal}");
}

#[test]
fn a_call_without_a_usable_id_is_refused() {
    let fx = Fixture::new();
    let now = SystemTime::now();
    for id in [None, Some("../evil")] {
        let refusal =
            require_in_call_window(&fx.env.scratch_dir, id, now, fx.env.uid, now).unwrap_err();
        assert!(refusal.contains("no usable tool_use_id"), "{refusal}");
    }
}
