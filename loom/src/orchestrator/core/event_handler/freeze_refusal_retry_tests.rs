//! Filing a freeze refusal: a declined filing stays declined, and a failed one retries.

use super::*;

#[test]
#[serial]
fn a_declined_filing_is_marked_and_never_retried() {
    let mut fx = fixture();
    fx.record(&fx.writer.id);

    settle_filing(
        &fx.work,
        STAGE,
        FreezeRefusalFiling::Declined("not admitted".to_string()),
    )
    .unwrap();
    fx.tick();

    fx.assert_nothing_filed();
    let record = daemon_refusal(&fx.work, STAGE).unwrap().unwrap();
    assert_eq!(record.declined.as_deref(), Some("not admitted"));
}
#[test]
#[serial]
fn an_admission_read_error_after_the_initial_read_retries() {
    let mut fx = fixture();
    fx.record(&fx.writer.id);
    let (stage, _) = fx.orchestrator.refusing_writer(STAGE).unwrap().unwrap();
    let refusal = daemon_refusal(&fx.work, STAGE).unwrap().unwrap();
    let path = fx.work.join("contracts").join(STAGE).join("freeze.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(fx.orchestrator.file_refusal(&stage, &refusal).is_err());
    let pending = daemon_refusal(&fx.work, STAGE).unwrap().unwrap();
    assert_eq!(pending.declined, None);
    fx.assert_nothing_filed();
    std::fs::remove_dir(path).unwrap();
    fx.tick();
    fx.assert_filed_once();
    assert_eq!(fx.stage().tally.freeze_refusal_disputes, 1);
}
#[test]
#[serial]
fn a_tick_retried_after_a_failed_filing_writes_one_request() {
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let mut fx = fixture();
    fx.record(&fx.writer.id);
    // The request is written, then the stage update fails.
    let stages = fx.work.join("stages");
    std::fs::set_permissions(&stages, std::fs::Permissions::from_mode(0o555)).unwrap();
    fx.tick();
    std::fs::set_permissions(&stages, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(fx.stage().status, StageStatus::Executing);
    assert!(
        daemon_refusal(&fx.work, STAGE).unwrap().is_some(),
        "kept for the retry"
    );

    fx.tick();

    fx.assert_filed_once();
}
