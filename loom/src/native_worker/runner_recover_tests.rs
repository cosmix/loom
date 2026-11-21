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

/// A macOS identity for `dir`'s invocation, its private root created below
/// `cache` (canonical: the root is walked without following symlinks).
fn mac_boundary(dir: &Path, cache: &Path, boot_id: &str, session: u32) -> PathBuf {
    let invocation = dir.file_name().unwrap().to_str().unwrap().to_string();
    let root = cache.join("native").join(&invocation);
    fs::create_dir_all(root.join("tmp")).expect("private root");
    fs::write(root.join("tmp").join("scratch"), b"x").expect("scratch file");
    let mac = crate::native_worker::model::MacIdentity {
        invocation,
        session,
        leader_start: "1".into(),
        boot_id: boot_id.into(),
        scratch_root: root.clone(),
    };
    let identity = crate::native_worker::model::BoundaryIdentity {
        supervisor_pid: i32::MAX - 7,
        supervisor_start: "0".into(),
        boundary_pid: session as i32,
        pid_namespace: 0,
        boot_id: boot_id.into(),
        mac: Some(mac),
    };
    store::write_boundary(dir, &identity).expect("boundary.json");
    root
}

fn canonical_temp() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let path = tmp.path().canonicalize().expect("canonical temp");
    (tmp, path)
}

#[test]
fn a_lost_macos_supervisor_after_a_boot_change_is_released_and_its_root_removed() {
    let (_tmp, base) = canonical_temp();
    let dir = record(&base);
    mark_spawn_begun(&dir).expect("marker");
    let root = mac_boundary(&dir, &base.join("cache"), "an-earlier-boot", 4242);
    settle_lost_supervisor(&dir).expect("settle");
    let tail = tail(&dir);
    assert!(
        matches!(&tail[1], NativeEvent::OwnershipReleased { by } if by == "daemon"),
        "{tail:?}"
    );
    assert!(!root.exists(), "the private root outlived a proven release");
}

#[test]
fn a_lost_macos_supervisor_whose_session_is_ambiguous_keeps_state_and_scratch() {
    let (_tmp, base) = canonical_temp();
    let dir = record(&base);
    mark_spawn_begun(&dir).expect("marker");
    // The test process's own session: members exist or the census is uncertain.
    let session = u32::try_from(nix::unistd::getsid(None).expect("sid").as_raw()).unwrap();
    let boot = crate::process::boot_id::os_boot_id().expect("boot id");
    let root = mac_boundary(&dir, &base.join("cache"), &boot, session);
    settle_lost_supervisor(&dir).expect("settle");
    let tail = tail(&dir);
    assert!(
        matches!(&tail[1], NativeEvent::OwnershipUnverified { .. }),
        "{tail:?}"
    );
    assert!(
        root.join("tmp").join("scratch").exists(),
        "scratch removed unproven"
    );
}

/// A stale pid names no member, so the proof releases it and removes the
/// private root. Only a macOS census proves that; off macOS a macOS identity
/// stays unverified with its root. On macOS the census must be conclusive,
/// so this needs an unconfined host (skips, or fails under
/// `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, inside a sandbox).
#[test]
fn a_lost_macos_supervisor_with_a_stale_session_releases_only_with_its_root() {
    #[cfg(target_os = "macos")]
    {
        use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
        let name = "a_lost_macos_supervisor_with_a_stale_session_releases_only_with_its_root";
        if skip_unless(
            confined_commands_runnable(),
            name,
            "needs an unconfined host",
        ) {
            return;
        }
    }
    let (_tmp, base) = canonical_temp();
    let dir = record(&base);
    mark_spawn_begun(&dir).expect("marker");
    let stale = crate::test_stand_in::reaped_pid();
    let boot = crate::process::boot_id::os_boot_id().expect("boot id");
    let root = mac_boundary(&dir, &base.join("cache"), &boot, stale);
    settle_lost_supervisor(&dir).expect("settle");
    let tail = tail(&dir);
    if cfg!(target_os = "macos") {
        assert!(
            matches!(&tail[1], NativeEvent::OwnershipReleased { .. }),
            "{tail:?}"
        );
        assert!(!root.exists(), "the private root outlived the release");
    } else {
        assert!(
            matches!(&tail[1], NativeEvent::OwnershipUnverified { .. }),
            "{tail:?}"
        );
        assert!(root.exists(), "scratch removed unproven");
    }
}

#[test]
fn a_failed_root_removal_after_release_keeps_the_record_for_a_retry() {
    let (_tmp, base) = canonical_temp();
    let dir = record(&base);
    mark_spawn_begun(&dir).expect("marker");
    let root = mac_boundary(&dir, &base.join("cache"), "an-earlier-boot", 4242);
    // A root that is not `native/<invocation>` is refused, as a failure would.
    let mut identity = store::read_boundary(&dir).unwrap().unwrap();
    identity.mac.as_mut().unwrap().scratch_root = base.join("elsewhere");
    store::write_boundary(&dir, &identity).expect("boundary.json");
    let error = settle_lost_supervisor(&dir).expect_err("a failed removal settles nothing");
    assert!(
        format!("{error:#}").contains("could not be removed"),
        "{error:#}"
    );
    let events = read_events(&dir).expect("events");
    assert!(
        !events.iter().any(|r| matches!(
            r.event,
            NativeEvent::OwnershipReleased { .. } | NativeEvent::OwnershipUnverified { .. }
        )),
        "{events:?}"
    );
    assert!(root.exists());
}

#[test]
fn a_lost_supervisor_before_identity_publication_keeps_its_scratch() {
    let (_tmp, base) = canonical_temp();
    let dir = record(&base);
    mark_spawn_begun(&dir).expect("marker");
    let invocation = dir.file_name().unwrap().to_owned();
    let root = base.join("cache").join("native").join(invocation);
    fs::create_dir_all(&root).expect("private root");
    settle_lost_supervisor(&dir).expect("settle");
    assert!(matches!(
        &tail(&dir)[1],
        NativeEvent::OwnershipUnverified { .. }
    ));
    assert!(root.exists(), "an unproven boundary lost its scratch");
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
