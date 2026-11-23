//! Contract of stage runtime-and-dashboard-server: daemon recovery reclaims a
//! torn last journal line, and refuses to repair a journal corrupt elsewhere.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::ids::mint_invocation_id;
use super::model::{NativeEvent, NativeManifest, Outcome};
use super::runner::recover_after_restart;
use super::store::{create_invocation, read_events};

const TORN: &[u8] = br#"{"seq":2"#;
const TOMBSTONE: &[u8] = br#"{"pid":0,"start":""}"#;

fn record(work: &Path) -> PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: "torn-contract".to_string(),
        unit_id: "unit-a".to_string(),
        invocation_id: mint_invocation_id().expect("id"),
        owner_session_id: "session-lead".to_string(),
        workspace_root: work.to_path_buf(),
        brief_path: work.join("brief.md"),
        brief_sha256: crate::utils::sha256_hex(b"brief"),
        model: "gpt-6.1-terra".to_string(),
        effort: "xhigh".to_string(),
        deadline_secs: 1500,
        created_at: Utc::now(),
    };
    create_invocation(work, &manifest, b"brief").expect("record")
}

fn invocation_id(dir: &Path) -> String {
    let name = dir.file_name().expect("invocation dir name");
    name.to_string_lossy().into_owned()
}

/// The `events.jsonl.torn-<digits>` sidecars in `dir`.
fn sidecars(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .expect("list invocation dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            name.strip_prefix("events.jsonl.torn-")
                .is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                })
        })
        .collect()
}

fn assert_torn_tail_settled(work: &Path, dir: &Path) {
    let mut events = OpenOptions::new()
        .append(true)
        .open(dir.join("events.jsonl"))
        .expect("open events.jsonl");
    events.write_all(TORN).expect("append torn tail");
    drop(events);
    fs::write(dir.join("supervisor.json"), TOMBSTONE).expect("tombstone");
    let settled = recover_after_restart(work);
    assert_eq!(
        settled,
        vec![invocation_id(dir)],
        "the torn record is settled"
    );
    let events = read_events(dir).expect("the repaired journal parses");
    let tail: Vec<&NativeEvent> = events[events.len() - 2..]
        .iter()
        .map(|r| &r.event)
        .collect();
    assert!(
        matches!(
            tail[..],
            [
                NativeEvent::Terminal { outcome: Outcome::Unknown, .. },
                NativeEvent::OwnershipReleased { by },
            ] if by == "daemon"
        ),
        "tail: {tail:?}"
    );
    let sidecars = sidecars(dir);
    assert_eq!(sidecars.len(), 1, "one sidecar holds the dropped tail");
    assert_eq!(fs::read(&sidecars[0]).expect("read sidecar"), TORN);
}

fn assert_corrupt_middle_refused(work: &Path, dir: &Path) {
    let path = dir.join("events.jsonl");
    let original = fs::read(&path).expect("read events.jsonl");
    let mut body = b"broken\n".to_vec();
    body.extend_from_slice(&original);
    body.extend_from_slice(br#"{"seq":3"#);
    fs::write(&path, &body).expect("rewrite events.jsonl");
    fs::write(dir.join("supervisor.json"), TOMBSTONE).expect("tombstone");
    let settled = recover_after_restart(work);
    assert!(
        !settled.contains(&invocation_id(dir)),
        "a journal corrupt before its tail is never settled: {settled:?}"
    );
    assert_eq!(fs::read(&path).expect("reread events.jsonl"), body);
    assert!(sidecars(dir).is_empty(), "no repair sidecar is written");
}

#[test]
fn a_torn_tail_is_repaired_and_the_record_settled_but_a_corrupt_middle_is_refused() {
    let torn = tempfile::TempDir::new().expect("temp dir");
    let dir = record(torn.path());
    assert_torn_tail_settled(torn.path(), &dir);
    let corrupt = tempfile::TempDir::new().expect("temp dir");
    let dir = record(corrupt.path());
    assert_corrupt_middle_refused(corrupt.path(), &dir);
}
