//! A refused freeze on record: the writer's scratch file, the standing it
//! gives the writer's stage, and the daemon's `refused.json`.

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
    standing(&fx.work_dir, &stage)
}

fn problems() -> Vec<String> {
    vec![
        ".bashrc is neither a contract file nor matched by a `harness` glob".to_string(),
        "contract `rejects-x` passes before implementation".to_string(),
    ]
}

#[test]
fn a_recorded_refusal_reads_back_with_its_time_until_cleared() {
    let fx = fixture(SessionType::Contract);
    let before = SystemTime::now() - std::time::Duration::from_secs(5);

    record(&fx.scratch_dir, &problems()).unwrap();
    let (lines, modified) = local_refusal(&fx.scratch_root, &fx.session_id)
        .unwrap()
        .unwrap();
    assert_eq!(lines, problems());
    assert!(modified >= before, "{modified:?}");

    clear(&fx.scratch_dir).unwrap();
    assert_eq!(
        local_refusal(&fx.scratch_root, &fx.session_id).unwrap(),
        None
    );
    clear(&fx.scratch_dir).unwrap();
}

#[test]
fn a_refusal_is_never_read_through_a_symlink() {
    let fx = fixture(SessionType::Contract);
    let outside = fx.scratch_root.join("outside.txt");
    std::fs::write(&outside, "planted").unwrap();
    std::os::unix::fs::symlink(&outside, fx.scratch_dir.join(REFUSAL_FILE)).unwrap();

    assert!(local_refusal(&fx.scratch_root, &fx.session_id).is_err());
}

#[test]
fn a_missing_scratch_directory_means_no_local_refusal() {
    let fx = fixture(SessionType::Contract);
    std::fs::remove_dir(&fx.scratch_dir).unwrap();

    assert_eq!(
        local_refusal(&fx.scratch_root, &fx.session_id).unwrap(),
        None
    );
}

#[test]
fn only_a_frozen_contract_writer_stands_frozen() {
    let fx = fixture(SessionType::Contract);
    record(&fx.scratch_dir, &problems()).unwrap();
    assert_eq!(standing_of(&fx), Standing::Other, "a refusal parks nothing");

    write_test_freeze(&fx.work_dir, STAGE, &fx.session_id);
    assert_eq!(standing_of(&fx), Standing::Frozen);
}

#[test]
fn an_implementation_session_is_never_a_frozen_writer() {
    let fx = fixture(SessionType::Stage);
    write_test_freeze(&fx.work_dir, STAGE, &fx.session_id);

    assert_eq!(standing_of(&fx), Standing::Other);
}

#[test]
fn a_waiting_stage_ends_its_wait() {
    let mut stage = contract_stage(STAGE, "writer");
    stage.try_mark_waiting_for_input().unwrap();
    stage.review_reason = Some("asked a question".to_string());

    end_wait(&mut stage).unwrap();

    assert_eq!(stage.status, crate::models::stage::StageStatus::Executing);
    assert_eq!(stage.review_reason, None);
}

#[test]
fn a_daemon_refusal_is_recorded_replaced_declined_and_taken() {
    let fx = fixture(SessionType::Contract);
    assert_eq!(daemon_refusal(&fx.work_dir, STAGE).unwrap(), None);

    record_daemon_refusal(
        &fx.work_dir,
        STAGE,
        "old",
        &problems(),
        RefusalSource::Daemon,
    )
    .unwrap();
    let latest = vec!["infrastructure: the daemon stopped".to_string()];
    record_daemon_refusal(
        &fx.work_dir,
        STAGE,
        &fx.session_id,
        &latest,
        RefusalSource::Writer,
    )
    .unwrap();
    let recorded = daemon_refusal(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(recorded.session_id, fx.session_id, "the latest record wins");
    assert_eq!(recorded.problems, latest);
    assert_eq!(recorded.source, RefusalSource::Writer);
    assert_eq!(recorded.declined, None);

    mark_declined(&fx.work_dir, STAGE, "not admitted").unwrap();
    let declined = daemon_refusal(&fx.work_dir, STAGE).unwrap().unwrap();
    assert_eq!(declined.declined.as_deref(), Some("not admitted"));
    assert_eq!(declined.at, recorded.at);

    take_daemon_refusal(&fx.work_dir, STAGE).unwrap();
    assert_eq!(daemon_refusal(&fx.work_dir, STAGE).unwrap(), None);
    take_daemon_refusal(&fx.work_dir, STAGE).unwrap();
    mark_declined(&fx.work_dir, STAGE, "gone").unwrap();
    assert_eq!(daemon_refusal(&fx.work_dir, STAGE).unwrap(), None);
}

#[test]
fn the_daemon_record_is_the_documented_json() {
    let fx = fixture(SessionType::Contract);
    record_daemon_refusal(
        &fx.work_dir,
        STAGE,
        "w1",
        &problems(),
        RefusalSource::Daemon,
    )
    .unwrap();

    let path = fx
        .work_dir
        .join("contracts")
        .join(STAGE)
        .join("refused.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(json["session_id"], "w1");
    assert_eq!(json["source"], "daemon");
    assert!(json["declined"].is_null());
    assert_eq!(json["problems"][1], problems()[1]);
    assert!(json["at"].is_string());
}

#[test]
fn a_failed_record_write_is_an_error() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let fx = fixture(SessionType::Contract);
    let dir = fx.work_dir.join("contracts").join(STAGE);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let written = record_daemon_refusal(
        &fx.work_dir,
        STAGE,
        "w1",
        &problems(),
        RefusalSource::Daemon,
    );

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(written.is_err());
    assert_eq!(daemon_refusal(&fx.work_dir, STAGE).unwrap(), None);
}

#[test]
fn problem_lines_drop_empty_lines_and_trim() {
    assert_eq!(
        problem_lines("  a \n\n b\n"),
        vec!["a".to_string(), "b".to_string()]
    );
}
