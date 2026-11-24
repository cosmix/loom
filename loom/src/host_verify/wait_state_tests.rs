use std::fs::OpenOptions;
use std::sync::mpsc;
use std::time::Duration;

use super::*;
use crate::host_verify::store::{save_current, save_result, stage_dir};
use crate::host_verify::tests_wait::{fixture, result, OLD_JOB, STAGE};
use crate::host_verify::wait::{exit_code, release, WaitOutcome};
use crate::host_verify::{JobKind, JobState};

const NEW_JOB: &str = "22222222222222222222222222222222";

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
        at_base: false,
        criteria: None,
        contracts: None,
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

/// Write `text` into the FIFO at `path` once a reader has it open (ENXIO before).
fn hand_over_through_fifo(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let mut open = std::fs::OpenOptions::new();
    open.write(true).custom_flags(libc::O_NONBLOCK);
    let mut fifo = loop {
        match open.open(path) {
            Ok(file) => break file,
            Err(error) => {
                let waiting = error.raw_os_error() == Some(libc::ENXIO);
                assert!(waiting && std::time::Instant::now() < deadline, "{error}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    };
    fifo.write_all(text.as_bytes()).unwrap();
}

#[test]
fn a_superseded_job_is_followed_through_its_own_record_and_reported_stopped() {
    let fx = fixture();
    fx.request("r1");
    fx.ledger(
        "r1",
        LedgerOutcome::Applied,
        &format!("host job {OLD_JOB} queued"),
    );
    let mut old = fx.job(OLD_JOB, JobState::Cancelled, &["r1"]);
    old.detail = Some(format!("superseded by {NEW_JOB}"));
    save_current(&fx.work_dir, &old).unwrap();
    save_current(&fx.work_dir, &fx.job(NEW_JOB, JobState::Queued, &["r2"])).unwrap();
    let (waited, text) = fx.call(Duration::from_secs(5), false, "r9");
    assert!(matches!(&waited, WaitOutcome::Finished(job, _) if job.id == OLD_JOB));
    assert_eq!(exit_code(&waited), 4);
    assert!(text.contains(NEW_JOB), "{text}");
}

#[test]
fn a_cancelled_job_exits_four() {
    let fx = fixture();
    fx.request("r1");
    let mut cancelled = fx.job(OLD_JOB, JobState::Cancelled, &["r1"]);
    cancelled.detail = Some("takedown".to_string());
    save_current(&fx.work_dir, &cancelled).unwrap();
    let (waited, text) = fx.call(Duration::from_secs(5), false, "r9");
    assert_eq!(exit_code(&waited), 4);
    assert!(text.starts_with("Host job: stopped (check, "), "{text}");
    assert!(text.contains("takedown"), "{text}");
    assert!(
        text.contains("Run the same command again to request a new run."),
        "{text}"
    );
}

#[test]
fn a_job_whose_directory_was_pruned_is_stopped() {
    let fx = fixture();
    fx.request("r1");
    fx.ledger(
        "r1",
        LedgerOutcome::Applied,
        &format!("host job {OLD_JOB} queued"),
    );
    std::fs::create_dir_all(stage_dir(&fx.work_dir, STAGE).unwrap()).unwrap();
    let (waited, _) = fx.call(Duration::from_secs(5), false, "r9");
    assert_eq!(exit_code(&waited), 4);
}

#[test]
fn a_refused_ledger_entry_is_reported_and_clears_the_state() {
    let fx = fixture();
    fx.request("r1");
    fx.ledger("r1", LedgerOutcome::Refused, "not the active session");
    let (waited, text) = fx.call(Duration::from_secs(5), false, "r9");
    assert_eq!(
        waited,
        WaitOutcome::Refused("not the active session".to_string())
    );
    assert_eq!(exit_code(&waited), 1);
    assert!(text.starts_with("Host job: refused (check, "), "{text}");
    assert!(text.contains("not the active session"), "{text}");
    assert!(!fx.state_file().exists());
}

#[test]
fn a_changed_no_cache_or_tree_starts_a_new_request() {
    let fx = fixture();
    fx.request("r1");
    let (waited, text) = fx.call(Duration::from_secs(5), true, "r2");
    assert_eq!(
        waited,
        WaitOutcome::Requested {
            request_id: "r2".into()
        }
    );
    assert!(text.starts_with("Host job: requested"), "{text}");
    assert!(std::fs::read_to_string(fx.state_file_for(true))
        .unwrap()
        .contains("r2"));

    std::fs::write(fx.checkout.join("new.txt"), "x\n").unwrap();
    let (waited, _) = fx.call(Duration::from_secs(5), true, "r3");
    assert_eq!(
        waited,
        WaitOutcome::Requested {
            request_id: "r3".into()
        }
    );
    assert!(std::fs::read_to_string(fx.state_file_for(true))
        .unwrap()
        .contains("r3"));
}

#[test]
fn a_terminal_outcome_leaves_a_newer_request_the_other_waiter_wrote() {
    let fx = fixture();
    fx.request("r1");
    save_current(&fx.work_dir, &fx.job(OLD_JOB, JobState::Running, &["r1"])).unwrap();
    let passed = fx.job(OLD_JOB, JobState::Passed, &["r1"]);
    let (work_dir, state_file) = (fx.work_dir.clone(), fx.state_file());
    let first = std::fs::read_to_string(&state_file).unwrap();
    // A FIFO in the state's place: r2 lands only after the waiter's one read.
    std::fs::remove_file(&state_file).unwrap();
    nix::unistd::mkfifo(&state_file, nix::sys::stat::Mode::S_IRWXU).unwrap();
    let other_waiter = std::thread::spawn(move || {
        hand_over_through_fifo(&state_file, &first);
        std::fs::remove_file(&state_file).unwrap();
        std::fs::write(&state_file, first.replace("r1", "r2")).unwrap();
        save_result(&work_dir, STAGE, &result(&passed, Vec::new())).unwrap();
        save_current(&work_dir, &passed).unwrap();
    });
    let (waited, _) = fx.call(Duration::from_secs(10), false, "r9");
    other_waiter.join().unwrap();
    assert!(matches!(waited, WaitOutcome::Finished(..)));
    assert!(std::fs::read_to_string(fx.state_file())
        .unwrap()
        .contains("r2"));
}
