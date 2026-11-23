//! Contract of stage runtime-and-dashboard-server: a journal filled to its cap
//! still has room for the records that settle the invocation.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::ids::mint_invocation_id;
use super::model::{NativeEvent, NativeManifest, Outcome, MAX_RECORD_BYTES};
use super::store::{create_invocation, read_events, EventLog, StoreError};

const CHUNKS: [usize; 4] = [1024 * 1024, 64 * 1024, 4 * 1024, 200];

fn record(work: &Path) -> PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: "journal-contract".to_string(),
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

/// Appends refused-request events of each chunk size until the log reports
/// `TooLarge` for that size, leaving the journal as full as ordinary events
/// can make it.
fn fill(log: &mut EventLog) {
    for size in CHUNKS {
        loop {
            let event = NativeEvent::ServerRequestRefused {
                method: "m".repeat(size),
            };
            match log.append(event) {
                Ok(_) => {}
                Err(StoreError::TooLarge(_)) => break,
                Err(error) => panic!("filling the journal with {size}-byte events: {error}"),
            }
        }
    }
}

#[test]
fn a_full_journal_still_accepts_the_terminal_and_ownership_records() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let dir = record(tmp.path());
    let mut log = EventLog::open(&dir).expect("event log");
    fill(&mut log);
    let settlement = [
        NativeEvent::Terminal {
            outcome: Outcome::Failed,
            reason: "r".repeat(1000),
        },
        NativeEvent::OwnershipReleased {
            by: "daemon".to_string(),
        },
        NativeEvent::OwnershipUnverified {
            detail: "d".repeat(1000),
        },
    ];
    for event in settlement.clone() {
        if let Err(error) = log.append(event.clone()) {
            panic!("a full journal refused the settlement record {event:?}: {error}");
        }
    }
    drop(log);
    let size = fs::metadata(dir.join("events.jsonl")).expect("stat").len();
    assert!(
        size <= MAX_RECORD_BYTES,
        "events.jsonl is {size} bytes, past the {MAX_RECORD_BYTES}-byte read cap"
    );
    let events = read_events(&dir).expect("a full journal must stay readable");
    let tail: Vec<NativeEvent> = events[events.len() - 3..]
        .iter()
        .map(|record| record.event.clone())
        .collect();
    assert_eq!(tail, settlement);
}
