use std::fs::OpenOptions;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::host_verify::tests_wait::fixture;
use crate::host_verify::wait::release;
use crate::host_verify::{JobKind, JobState};

#[test]
fn each_check_selection_has_its_own_state_file() {
    let fx = fixture();
    let mut paths = std::collections::BTreeSet::new();
    for (criteria, no_cache) in [
        (None, false),
        (Some(&[1][..]), false),
        (Some(&[2][..]), false),
        (None, true),
    ] {
        let site = WaitSite {
            criteria,
            ..fx.site(Duration::ZERO, no_cache)
        };
        let path = state_path(&site).unwrap();
        assert_eq!(path.parent().unwrap().file_name().unwrap(), "host-verify");
        let name = path.file_name().unwrap().to_str().unwrap();
        let key = name
            .strip_prefix("s1-check-")
            .unwrap()
            .strip_suffix(".json")
            .unwrap();
        assert_eq!(key.len(), 16);
        assert!(key
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
        paths.insert(path);
    }
    assert_eq!(paths.len(), 4);
}

#[test]
fn a_completion_or_freeze_keeps_one_state_file_per_kind() {
    let fx = fixture();
    for (kind, name) in [
        (JobKind::Complete, "s1-complete.json"),
        (JobKind::Freeze, "s1-freeze.json"),
    ] {
        for no_cache in [false, true] {
            let site = WaitSite {
                kind,
                ..fx.site(Duration::ZERO, no_cache)
            };
            assert_eq!(state_path(&site).unwrap().file_name().unwrap(), name);
        }
    }
}

fn state(id: &str) -> RequestState {
    RequestState {
        request_id: id.to_owned(),
        head: "head".into(),
        tree_digest: "tree".into(),
        no_cache: false,
        criteria: None,
        submitted_at: Utc::now(),
    }
}

#[test]
fn replacement_waits_for_ownership_check_and_delete() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("request.json");
    write_state(&path, &state("r1"))?;
    let (start_tx, start_rx) = mpsc::channel();
    let (checked_tx, checked_rx) = mpsc::channel();
    let writer_path = path.clone();
    let writer = std::thread::spawn(move || -> Result<()> {
        start_rx.recv()?;
        let lock_path = writer_path.with_extension("json.lock");
        let probe = OpenOptions::new().read(true).write(true).open(lock_path)?;
        let blocked = probe
            .try_lock_exclusive()
            .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock);
        checked_tx.send(blocked)?;
        drop(probe);
        write_state(&writer_path, &state("r2"))
    });
    remove_state_if_owned_inner(&path, "r1", || {
        start_tx.send(()).unwrap();
        assert!(checked_rx.recv_timeout(Duration::from_secs(5)).unwrap());
    })?;
    writer.join().expect("writer panicked")?;
    assert_eq!(read_state(&path).unwrap().request_id, "r2");
    Ok(())
}

#[test]
fn release_keeps_a_replacement_and_removes_its_own_request() -> Result<()> {
    let fx = fixture();
    let mut site = fx.site(Duration::ZERO, false);
    site.kind = JobKind::Complete;
    let path = state_path(&site)?;
    let record = fx.job("job", JobState::Passed, &["r1"]);
    write_state(&path, &state("r2"))?;
    release(&site, &record)?;
    assert_eq!(read_state(&path).unwrap().request_id, "r2");
    write_state(&path, &state("r1"))?;
    release(&site, &record)?;
    assert!(!path.exists());
    Ok(())
}

#[test]
fn state_mutations_report_lock_failures() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("request.json");
    std::fs::create_dir(path.with_extension("json.lock"))?;
    assert!(write_state(&path, &state("r1")).is_err());
    assert!(remove_state_if_owned(&path, "r1").is_err());
    Ok(())
}

#[test]
fn release_reports_lock_failure_and_preserves_state() -> Result<()> {
    let fx = fixture();
    let mut site = fx.site(Duration::ZERO, false);
    site.kind = JobKind::Complete;
    let path = state_path(&site)?;
    let record = fx.job("job", JobState::Passed, &["r1"]);
    write_state(&path, &state("r1"))?;
    let lock = path.with_extension("json.lock");
    std::fs::remove_file(&lock)?;
    std::fs::create_dir(lock)?;
    assert!(release(&site, &record).is_err());
    assert_eq!(read_state(&path).unwrap().request_id, "r1");
    Ok(())
}
