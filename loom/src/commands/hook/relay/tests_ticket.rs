//! Ticket verification: a ticket counts only when it is exactly the file its
//! relay line describes.

use super::*;
use crate::relay::{new_request_id, RequestKind};
use chrono::Utc;
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;

fn uid() -> u32 {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// A ticket of `kind`, encoded, and the line describing exactly those bytes.
fn encoded(kind: RequestKind) -> (Vec<u8>, RelayLine) {
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
    (bytes, line)
}

fn ticket_at(dir: &Path, line: &RelayLine) -> PathBuf {
    dir.join(ticket_file_name(&line.id))
}

fn refusal(dir: &Path, line: &RelayLine, uid: u32) -> String {
    format!("{:#}", read_verified(dir, line, uid).unwrap_err())
}

fn unlisted_refusal(dir: &Path, id: &str, uid: u32) -> String {
    format!("{:#}", read_unlisted(dir, id, uid).unwrap_err())
}

#[test]
fn a_ticket_matching_its_line_is_returned() {
    let dir = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    std::fs::write(ticket_at(dir.path(), &line), &bytes).unwrap();

    let ticket = read_verified(dir.path(), &line, uid()).unwrap();
    assert_eq!(ticket.id, line.id);
    assert_eq!(ticket.payload, json!({"content": "note"}));
}

#[test]
fn a_tampered_ticket_of_the_same_size_is_refused() {
    let dir = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    let tampered = String::from_utf8(bytes)
        .unwrap()
        .replace("\"note\"", "\"nota\"");
    std::fs::write(ticket_at(dir.path(), &line), tampered).unwrap();

    assert!(refusal(dir.path(), &line, uid()).contains("SHA-256"));
}

#[test]
fn a_ticket_whose_size_differs_from_its_line_is_refused() {
    let dir = TempDir::new().unwrap();
    let (mut bytes, line) = encoded(RequestKind::Memory);
    bytes.push(b'\n');
    std::fs::write(ticket_at(dir.path(), &line), &bytes).unwrap();

    assert!(refusal(dir.path(), &line, uid()).contains("the line says"));
}

#[test]
fn a_ticket_owned_by_another_uid_is_refused() {
    let dir = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    std::fs::write(ticket_at(dir.path(), &line), &bytes).unwrap();

    assert!(refusal(dir.path(), &line, uid().wrapping_add(1)).contains("owned by uid"));
}

#[test]
fn a_symlinked_ticket_is_refused() {
    let dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    let real = elsewhere.path().join("real.req");
    std::fs::write(&real, &bytes).unwrap();
    std::os::unix::fs::symlink(&real, ticket_at(dir.path(), &line)).unwrap();

    assert!(refusal(dir.path(), &line, uid()).contains("not a plain single-link file"));
}

#[test]
fn a_hard_linked_ticket_is_refused() {
    let dir = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    let other = dir.path().join("other-name");
    std::fs::write(&other, &bytes).unwrap();
    std::fs::hard_link(&other, ticket_at(dir.path(), &line)).unwrap();

    assert!(refusal(dir.path(), &line, uid()).contains("hard link"));
}

#[test]
fn a_ticket_naming_another_kind_than_its_line_is_refused() {
    let dir = TempDir::new().unwrap();
    let (bytes, mut line) = encoded(RequestKind::Block);
    line.kind = RequestKind::Memory;
    std::fs::write(ticket_at(dir.path(), &line), &bytes).unwrap();

    assert!(refusal(dir.path(), &line, uid()).contains("the ticket names block"));
}

#[test]
fn a_line_claiming_more_than_the_ticket_cap_is_refused_before_any_open() {
    let dir = TempDir::new().unwrap();
    let (_, mut line) = encoded(RequestKind::Memory);
    line.bytes = u32::try_from(MAX_TICKET_BYTES + 1).unwrap();

    assert!(refusal(dir.path(), &line, uid()).contains("ticket cap"));
}

#[test]
fn a_ticket_no_line_describes_is_returned() {
    let dir = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    std::fs::write(ticket_at(dir.path(), &line), &bytes).unwrap();

    let ticket = read_unlisted(dir.path(), &line.id, uid()).unwrap();
    assert_eq!(ticket.kind, RequestKind::Memory);
    assert_eq!(ticket.payload, json!({"content": "note"}));
}

#[test]
fn an_unlisted_symlinked_ticket_is_refused_and_left_in_place() {
    let dir = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    let real = elsewhere.path().join("real.req");
    std::fs::write(&real, &bytes).unwrap();
    let planted = ticket_at(dir.path(), &line);
    std::os::unix::fs::symlink(&real, &planted).unwrap();

    let refused = unlisted_refusal(dir.path(), &line.id, uid());
    assert!(
        refused.contains("not a plain single-link file"),
        "{refused}"
    );
    assert!(planted.symlink_metadata().is_ok());
}

#[test]
fn an_unlisted_ticket_owned_by_another_uid_is_refused_and_left_in_place() {
    let dir = TempDir::new().unwrap();
    let (bytes, line) = encoded(RequestKind::Memory);
    std::fs::write(ticket_at(dir.path(), &line), &bytes).unwrap();

    let refused = unlisted_refusal(dir.path(), &line.id, uid().wrapping_add(1));
    assert!(refused.contains("owned by uid"), "{refused}");
    assert!(ticket_at(dir.path(), &line).exists());
}

#[test]
fn an_unlisted_ticket_above_the_ticket_cap_is_refused_and_left_in_place() {
    let dir = TempDir::new().unwrap();
    let id = new_request_id();
    let path = dir.path().join(format!("{id}.req"));
    std::fs::write(&path, vec![b'x'; MAX_TICKET_BYTES + 1]).unwrap();

    let refused = unlisted_refusal(dir.path(), &id, uid());
    assert!(refused.contains("ticket cap"), "{refused}");
    assert!(path.exists());
}

#[test]
fn an_unlisted_ticket_naming_another_id_than_its_file_is_refused() {
    let dir = TempDir::new().unwrap();
    let (bytes, _) = encoded(RequestKind::Memory);
    let id = new_request_id();
    let path = dir.path().join(format!("{id}.req"));
    std::fs::write(&path, &bytes).unwrap();

    let refused = unlisted_refusal(dir.path(), &id, uid());
    assert!(refused.contains("its file is named"), "{refused}");
    assert!(path.exists());
}

#[test]
fn consuming_a_missing_ticket_is_silent() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("gone.req");

    assert_eq!(consume(&path), "");
}

#[test]
fn consuming_a_ticket_that_cannot_be_removed_is_reported() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("not-a-file.req");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("child"), b"x").unwrap();

    assert!(consume(&path).contains("could not be removed"));
}
