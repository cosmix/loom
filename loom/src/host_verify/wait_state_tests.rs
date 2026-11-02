use std::fs::OpenOptions;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::host_verify::tests_wait::fixture;
use crate::host_verify::wait::release;
use crate::host_verify::{JobKind, JobState};

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
