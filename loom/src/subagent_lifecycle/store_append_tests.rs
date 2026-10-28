use anyhow::Result;

use super::store::{append_all_locked, replay, AppendOutcome};
use super::tests::{codex_record, set_codex_id, Fixture};
use super::{LifecycleRecord, WorkerOutcome};

/// Records appended in one call are checked against each other: the second
/// copy of a record is a duplicate of the first, appended under the same lock.
#[test]
fn append_all_dedupes_within_one_call() -> Result<()> {
    let fixture = Fixture::new("append-all")?;
    let mut authorization = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", false)?;
    let mut terminal = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", true)?;
    set_codex_id(&mut authorization)?;
    set_codex_id(&mut terminal)?;

    let outcomes = append_all_locked(
        &fixture.work,
        &[&authorization, &terminal, &authorization],
        |journal| {
            assert!(journal.is_empty());
            Ok(true)
        },
    )?;

    assert_eq!(
        outcomes,
        [
            AppendOutcome::Appended,
            AppendOutcome::Appended,
            AppendOutcome::Duplicate
        ]
    );
    assert_eq!(
        replay(&fixture.work)?.outcome(&authorization.identity),
        WorkerOutcome::Succeeded
    );
    Ok(())
}

/// `admit` sees the journal under the lock; refusing appends nothing and
/// reports every record as a duplicate, and its error aborts the append.
#[test]
fn append_all_admission_sees_the_locked_journal() -> Result<()> {
    let fixture = Fixture::new("append-admit")?;
    let mut authorization = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", false)?;
    let mut terminal = codex_record(&fixture.root, "forwarder-a", "unit-a", "job-a", true)?;
    set_codex_id(&mut authorization)?;
    set_codex_id(&mut terminal)?;
    append_all_locked(&fixture.work, &[&authorization], |_| Ok(true))?;

    let refused = append_all_locked(&fixture.work, &[&authorization, &terminal], |journal| {
        assert_eq!(journal, std::slice::from_ref(&authorization));
        Ok(false)
    })?;
    assert_eq!(refused, [AppendOutcome::Duplicate; 2]);
    assert_eq!(journal_lines(&fixture, &authorization)?, 1);

    let error = append_all_locked(&fixture.work, &[&terminal], |_| anyhow::bail!("not now"))
        .expect_err("admission error aborts the append");
    assert_eq!(error.to_string(), "not now");
    assert_eq!(journal_lines(&fixture, &authorization)?, 1);
    Ok(())
}

fn journal_lines(fixture: &Fixture, record: &LifecycleRecord) -> Result<usize> {
    let path = fixture
        .work
        .join("subagents")
        .join(record.identity.stage_id())
        .join("lifecycle.jsonl");
    Ok(std::fs::read_to_string(path)?.lines().count())
}
