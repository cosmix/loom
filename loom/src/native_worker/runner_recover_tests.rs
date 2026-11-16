//! Recovery-pass tests: warnings repeat only after their condition cleared, and a
//! lost supervisor that may have started bubblewrap is never released unproven.

use std::fs;

use super::*;
use crate::native_worker::ids::mint_invocation_id;
use crate::native_worker::model::NativeManifest;
use crate::native_worker::store::{create_invocation, mark_spawn_begun, read_events};

const STAGE: &str = "recover-unit";

fn record(work: &Path) -> PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: STAGE.to_string(),
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

fn warnings(work: &Path, warned: &mut HashSet<WarnKey>) -> usize {
    let none = HashSet::new();
    let pass = Pass {
        work_dir: work,
        held: &none,
        spawned: &none,
        may_spawn: false,
    };
    run_pass(&pass, &mut HashSet::new(), warned)
        .iter()
        .filter(|step| matches!(step, Step::Warn(_)))
        .count()
}

#[test]
fn a_lasting_corrupt_record_warns_once_until_it_clears() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let work = tmp.path();
    let dir = record(work);
    fs::write(dir.join("events.jsonl"), b"broken\n").expect("corrupt");
    let mut warned = HashSet::new();
    assert_eq!(warnings(work, &mut warned), 1);
    assert_eq!(warnings(work, &mut warned), 0);
    fs::remove_dir_all(&dir).expect("remove");
    assert_eq!(warnings(work, &mut warned), 0);
    assert!(warned.is_empty(), "a cleared condition is forgotten");
    let dir = record(work);
    fs::write(dir.join("events.jsonl"), b"broken\n").expect("corrupt");
    assert_eq!(warnings(work, &mut warned), 1);
}

fn tail(dir: &Path) -> Vec<NativeEvent> {
    let events = read_events(dir).expect("events");
    events[events.len() - 2..]
        .iter()
        .map(|record| record.event.clone())
        .collect()
}

#[test]
fn a_lost_supervisor_after_the_spawn_marker_is_failed_and_unverified() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let dir = record(tmp.path());
    mark_spawn_begun(&dir).expect("marker");
    settle_lost_supervisor(&dir).expect("settle");
    let tail = tail(&dir);
    assert!(
        matches!(
            &tail[0],
            NativeEvent::Terminal {
                outcome: Outcome::Failed,
                ..
            }
        ),
        "{tail:?}"
    );
    assert!(
        matches!(&tail[1], NativeEvent::OwnershipUnverified { detail }
            if detail.contains("boundary.json is missing")),
        "{tail:?}"
    );
}

#[test]
fn a_lost_supervisor_without_the_spawn_marker_is_released() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let dir = record(tmp.path());
    settle_lost_supervisor(&dir).expect("settle");
    let tail = tail(&dir);
    assert!(
        matches!(
            &tail[0],
            NativeEvent::Terminal {
                outcome: Outcome::Unknown,
                ..
            }
        ),
        "{tail:?}"
    );
    assert!(
        matches!(&tail[1], NativeEvent::OwnershipReleased { by } if by == "daemon"),
        "{tail:?}"
    );
}

/// `settle_spawn_failure` after bubblewrap started appended `Terminal` but not
/// its ownership event: the daemon must not release it.
#[test]
fn a_started_spawn_failure_whose_settle_append_failed_stays_unverified() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let dir = record(tmp.path());
    mark_spawn_begun(&dir).expect("marker");
    EventLog::open(&dir)
        .expect("log")
        .append(NativeEvent::Terminal {
            outcome: Outcome::Failed,
            reason: "boundary spawn failed: no pid namespace".into(),
        })
        .expect("terminal");
    settle_lost_supervisor(&dir).expect("settle");
    let tail = tail(&dir);
    assert!(
        matches!(&tail[0], NativeEvent::Terminal { reason, .. } if reason.starts_with("boundary spawn failed"))
    );
    assert!(
        matches!(&tail[1], NativeEvent::OwnershipUnverified { .. }),
        "{tail:?}"
    );
}
