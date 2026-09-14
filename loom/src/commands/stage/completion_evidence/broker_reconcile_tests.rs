use super::super::broker_tests::{CompleteResult, FakeTransport, Fixture};
use super::BrokerOutcome;
use crate::handoff::load_session_checkpoint;
use crate::models::stage::StageStatus;

#[test]
fn lost_ack_with_matching_receipt_is_reconciled() {
    let fixture = Fixture::new(StageStatus::Completed);
    let mut transport = FakeTransport::new(&fixture, CompleteResult::Error);
    transport.persist = true;
    transport.receipt_on_error = true;

    let outcome = fixture.run(false, &fixture.output(), &transport);

    assert_eq!(outcome, BrokerOutcome::AcceptedReconciled);
}

#[test]
fn lost_ack_with_forged_receipt_is_not_reconciled() {
    let fixture = Fixture::new(StageStatus::Completed);
    let mut transport = FakeTransport::new(&fixture, CompleteResult::Error);
    transport.forge_receipt_on_error = true;

    let outcome = fixture.run(false, &fixture.output(), &transport);

    assert_eq!(
        outcome,
        BrokerOutcome::Uncertain("accepted receipt is missing".into())
    );
}

#[test]
fn lost_ack_while_executing_records_one_actionable_blocker() {
    let fixture = Fixture::new(StageStatus::Executing);
    let mut transport = FakeTransport::new(&fixture, CompleteResult::Error);
    transport.persist = true;
    let output = fixture.output();

    let first = fixture.run(false, &output, &transport);
    let second = fixture.run(false, &output, &transport);

    assert_eq!(first, BrokerOutcome::VerifiedPendingAck);
    assert_eq!(second, BrokerOutcome::VerifiedPendingAck);
    let checkpoint =
        load_session_checkpoint(&fixture.stage.id, &fixture.session.id, &fixture.work_dir)
            .unwrap()
            .unwrap();
    assert!(checkpoint.is_actionable());
    assert_eq!(checkpoint.repeat_count(), 1);
}

#[test]
fn lost_ack_replay_counts_each_distinct_producer_output_once() {
    let fixture = Fixture::new(StageStatus::Executing);
    let mut transport = FakeTransport::new(&fixture, CompleteResult::Error);
    transport.persist = true;
    let first_output = fixture.output();

    fixture.run(false, &first_output, &transport);
    fixture.run(false, &first_output, &transport);
    assert_eq!(checkpoint_repeat_count(&fixture), 1);

    fixture.run(false, &fixture.output(), &transport);
    assert_eq!(checkpoint_repeat_count(&fixture), 2);
}

fn checkpoint_repeat_count(fixture: &Fixture) -> u32 {
    load_session_checkpoint(&fixture.stage.id, &fixture.session.id, &fixture.work_dir)
        .unwrap()
        .unwrap()
        .repeat_count()
}
