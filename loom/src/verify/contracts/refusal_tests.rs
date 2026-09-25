//! A refused freeze on record, the standing it gives the writer's stage, and
//! the wait it parks that stage in.

use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus};
use crate::relay::ensure_dir_0700;
use crate::verify::contracts::test_support::{contract_stage, write_test_freeze};
use crate::verify::transitions::{create_stage, load_stage};

const STAGE: &str = "s1";

/// A work dir holding [`contract_stage`] under a running writer of
/// `session_type`, and that writer's 0700 scratch directory.
struct Fixture {
    _tmp: TempDir,
    work_dir: PathBuf,
    scratch_root: PathBuf,
    scratch_dir: PathBuf,
    session_id: String,
}

fn fixture(session_type: SessionType) -> Fixture {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let mut writer = Session::new_contract(STAGE);
    writer.session_type = session_type;
    writer.status = SessionStatus::Running;
    save_session(&writer, &work_dir).unwrap();
    create_stage(&contract_stage(STAGE, &writer.id), &work_dir).unwrap();
    let scratch_root = tmp.path().join("scratch");
    let scratch_dir = scratch_root.join(&writer.id);
    ensure_dir_0700(&scratch_dir, uid()).unwrap();
    Fixture {
        _tmp: tmp,
        work_dir,
        scratch_root,
        scratch_dir,
        session_id: writer.id,
    }
}

fn uid() -> u32 {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

fn standing_of(fx: &Fixture) -> Standing {
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    standing_in(&fx.work_dir, &stage, Some(fx.scratch_root.as_path()), uid())
}

fn problems() -> Vec<String> {
    vec![
        ".bashrc is neither a contract file nor matched by a `harness` glob".to_string(),
        "contract `rejects-x` passes before implementation".to_string(),
    ]
}

#[test]
fn a_recorded_refusal_reads_back_until_cleared() {
    let fx = fixture(SessionType::Contract);

    record(&fx.scratch_dir, &problems()).unwrap();
    assert_eq!(load(&fx.scratch_dir).unwrap(), Some(problems().join("\n")));

    clear(&fx.scratch_dir).unwrap();
    assert_eq!(load(&fx.scratch_dir).unwrap(), None);
    clear(&fx.scratch_dir).unwrap();
}

#[test]
fn a_refusal_is_never_read_through_a_symlink() {
    let fx = fixture(SessionType::Contract);
    let outside = fx.scratch_root.join("outside.txt");
    std::fs::write(&outside, "planted").unwrap();
    std::os::unix::fs::symlink(&outside, fx.scratch_dir.join(REFUSAL_FILE)).unwrap();

    assert!(load(&fx.scratch_dir).is_err());
    assert_eq!(standing_of(&fx), Standing::Other);
}

#[test]
fn only_an_unfrozen_writer_with_a_refusal_on_record_stands_refused() {
    let fx = fixture(SessionType::Contract);
    assert_eq!(standing_of(&fx), Standing::Other);

    record(&fx.scratch_dir, &problems()).unwrap();
    assert_eq!(standing_of(&fx), Standing::Refused(problems().join("\n")));

    write_test_freeze(&fx.work_dir, STAGE, &fx.session_id);
    assert_eq!(standing_of(&fx), Standing::Frozen);
}

#[test]
fn an_implementation_session_is_never_a_refused_writer() {
    let fx = fixture(SessionType::Stage);
    record(&fx.scratch_dir, &problems()).unwrap();

    assert_eq!(standing_of(&fx), Standing::Other);
}

#[test]
fn a_parked_stage_waits_with_the_problems_until_its_wait_ends() {
    let mut stage = contract_stage(STAGE, "writer");

    park(&mut stage, &problems().join("\n")).unwrap();

    assert_eq!(stage.status, StageStatus::WaitingForInput);
    let reason = stage.review_reason.clone().unwrap();
    assert!(reason.starts_with("contract freeze refused"), "{reason}");
    assert!(reason.contains(&format!("{}; {}", problems()[0], problems()[1])));

    end_wait(&mut stage).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.review_reason, None);
}

#[test]
fn a_refused_relay_parks_only_the_executing_unfrozen_writer_stage() {
    let fx = fixture(SessionType::Contract);
    let refused = |session_id: &str| {
        park_refused_relay(&fx.work_dir, STAGE, session_id, "too many files").unwrap()
    };

    assert!(!refused("another-session"));
    assert!(refused(&fx.session_id));
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::WaitingForInput);
    assert!(stage.review_reason.unwrap().contains("too many files"));

    assert!(
        !refused(&fx.session_id),
        "a waiting stage is not parked twice"
    );
}

#[test]
fn a_refused_relay_leaves_a_frozen_stage_to_its_handover() {
    let fx = fixture(SessionType::Contract);
    write_test_freeze(&fx.work_dir, STAGE, &fx.session_id);

    assert!(!park_refused_relay(&fx.work_dir, STAGE, &fx.session_id, "already frozen").unwrap());
    assert_eq!(
        load_stage(STAGE, &fx.work_dir).unwrap().status,
        StageStatus::Executing
    );
}

#[test]
fn a_missing_scratch_directory_means_no_refusal() {
    let fx = fixture(SessionType::Contract);
    std::fs::remove_dir(&fx.scratch_dir).unwrap();
    let stage = load_stage(STAGE, &fx.work_dir).unwrap();

    assert_eq!(
        read_standing(&fx.work_dir, &stage, Some(fx.scratch_root.as_path()), uid()).unwrap(),
        Standing::Other
    );
}
