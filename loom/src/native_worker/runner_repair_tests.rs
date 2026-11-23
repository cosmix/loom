use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use super::super::tests::record;
use super::super::{run_pass, Pass, Step};
use super::*;
use crate::native_worker::ids::mint_invocation_id;
use crate::native_worker::model::NativeManifest;
use crate::native_worker::store::{create_invocation, read_events};

const OLD_STAGE: &str = "recover-unit";
const TORN: &[u8] = br#"{"seq":2"#;

fn pass_steps(work: &Path) -> Vec<Step> {
    let none = HashSet::new();
    let pass = Pass {
        work_dir: work,
        held: &none,
        spawned: &none,
        may_spawn: false,
    };
    run_pass(&pass, &mut HashSet::new(), &mut HashSet::new())
}

fn append_torn(dir: &Path) {
    let mut file = OpenOptions::new()
        .append(true)
        .open(dir.join("events.jsonl"))
        .unwrap();
    file.write_all(TORN).unwrap();
}

fn has_sidecar(dir: &Path) -> bool {
    fs::read_dir(dir).unwrap().any(|entry| {
        let name = entry.unwrap().file_name();
        name.to_string_lossy().starts_with("events.jsonl.torn-")
    })
}

fn warned(steps: &[Step]) -> bool {
    steps.iter().any(|step| matches!(step, Step::Warn(_)))
}

#[test]
fn a_torn_tail_of_a_gone_supervisor_is_repaired_and_settled() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = record(tmp.path());
    append_torn(&dir);
    fs::write(dir.join("supervisor.json"), br#"{"pid":0,"start":""}"#).unwrap();
    let steps = pass_steps(tmp.path());
    assert!(steps.iter().any(|step| matches!(step, Step::Settled(_))));
    assert!(read_events(&dir).is_ok());
    assert!(has_sidecar(&dir));
}

#[test]
fn a_torn_tail_under_a_live_supervisor_is_left_alone() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = record(tmp.path());
    append_torn(&dir);
    let pid = std::process::id();
    let start = crate::process::process_start_time(pid).expect("own start time");
    let claim = format!(r#"{{"pid":{pid},"start":"{start}"}}"#);
    fs::write(dir.join("supervisor.json"), claim).unwrap();
    let before = fs::read(dir.join("events.jsonl")).unwrap();
    let steps = pass_steps(tmp.path());
    assert!(warned(&steps));
    assert_eq!(fs::read(dir.join("events.jsonl")).unwrap(), before);
    assert!(!has_sidecar(&dir));
}

#[test]
fn a_corrupt_middle_line_is_refused_not_truncated() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = record(tmp.path());
    let path = dir.join("events.jsonl");
    let mut body = b"broken\n".to_vec();
    body.extend_from_slice(&fs::read(&path).unwrap());
    body.extend_from_slice(TORN);
    fs::write(&path, &body).unwrap();
    fs::write(dir.join("supervisor.json"), br#"{"pid":0,"start":""}"#).unwrap();
    let steps = pass_steps(tmp.path());
    assert!(warned(&steps));
    assert_eq!(fs::read(&path).unwrap(), body);
    assert!(!has_sidecar(&dir));
}

/// A record old enough for `supervisor_is_gone` to tombstone it.
fn old_record(work: &Path) -> (String, PathBuf) {
    let invocation_id = mint_invocation_id().expect("id");
    let manifest = NativeManifest {
        v: 1,
        stage_id: OLD_STAGE.to_string(),
        unit_id: "unit-a".to_string(),
        invocation_id: invocation_id.clone(),
        owner_session_id: "session-lead".to_string(),
        workspace_root: work.to_path_buf(),
        brief_path: work.join("brief.md"),
        brief_sha256: crate::utils::sha256_hex(b"brief"),
        model: "gpt-6.1-terra".to_string(),
        effort: "xhigh".to_string(),
        deadline_secs: 1500,
        created_at: Utc::now() - chrono::Duration::hours(1),
    };
    let dir = create_invocation(work, &manifest, b"brief").expect("record");
    (invocation_id, dir)
}

fn repair(work: &Path, held: &HashSet<String>) -> (bool, Vec<Warning>) {
    let mut found = Vec::new();
    let repaired = repair_lost_tails(work, OLD_STAGE, held, &mut found);
    (repaired, found)
}

#[test]
fn a_held_torn_record_and_a_clean_sibling_are_not_touched() {
    let tmp = tempfile::TempDir::new().unwrap();
    let (held_id, held_dir) = old_record(tmp.path());
    let (_, clean_dir) = old_record(tmp.path());
    append_torn(&held_dir);
    let before = fs::read(held_dir.join("events.jsonl")).unwrap();
    let held = HashSet::from([held_id]);
    let (repaired, found) = repair(tmp.path(), &held);
    assert!(!repaired);
    assert!(found.is_empty());
    assert_eq!(fs::read(held_dir.join("events.jsonl")).unwrap(), before);
    assert!(!has_sidecar(&held_dir));
    assert!(!held_dir.join("supervisor.json").exists());
    assert!(!clean_dir.join("supervisor.json").exists());
}

#[test]
fn only_the_torn_record_is_tombstoned_and_repaired() {
    let tmp = tempfile::TempDir::new().unwrap();
    let (_, torn_dir) = old_record(tmp.path());
    let (_, clean_dir) = old_record(tmp.path());
    append_torn(&torn_dir);
    let (repaired, _) = repair(tmp.path(), &HashSet::new());
    assert!(repaired);
    assert!(has_sidecar(&torn_dir));
    assert!(torn_dir.join("supervisor.json").exists());
    assert!(read_events(&torn_dir).is_ok());
    assert!(!clean_dir.join("supervisor.json").exists());
}
