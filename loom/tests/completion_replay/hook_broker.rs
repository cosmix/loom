use loom::handoff::{
    check_definition_hash, current_blocker, load_session_checkpoint, worktree_head_commit,
    CompletionPhase,
};
use loom::models::stage::StageStatus;

use super::support::ReplayFixture;

fn run_through_hook(fixture: &ReplayFixture) -> (std::process::Output, std::process::Output) {
    let producer = fixture.run_completion();
    let producer_stdout = String::from_utf8_lossy(&producer.stdout);
    assert!(
        producer.status.success(),
        "producer failed\nstdout:\n{producer_stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&producer.stderr)
    );
    let hook = fixture.run_hook_post(&fixture.pinned_command(), &producer_stdout, false);
    (producer, hook)
}

fn diagnostics(producer: &std::process::Output, hook: &std::process::Output) -> String {
    format!(
        "broker input stdout:\n{}\nbroker input stderr:\n{}\nhook stdout:\n{}\nhook stderr:\n{}",
        String::from_utf8_lossy(&producer.stdout),
        String::from_utf8_lossy(&producer.stderr),
        String::from_utf8_lossy(&hook.stdout),
        String::from_utf8_lossy(&hook.stderr),
    )
}

fn load_checkpoint(fixture: &ReplayFixture) -> loom::handoff::CompletionCheckpoint {
    load_session_checkpoint(
        &fixture.stage_id,
        &fixture.session_id,
        &fixture.worktree.join(".loom/work"),
    )
    .expect("load replay checkpoint")
    .expect("broker must persist a replay checkpoint")
}

#[test]
fn installed_hook_persists_verified_attempt_when_daemon_is_offline() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let (producer, hook) = run_through_hook(&fixture);
    let output = diagnostics(&producer, &hook);
    let hook_stderr = String::from_utf8_lossy(&hook.stderr);
    assert!(
        hook.status.code() == Some(2)
            && hook_stderr.contains("verification passed but the daemon acknowledgement was lost"),
        "expected verified_pending_ack hook rejection\n{output}"
    );

    let stage = fixture.reload_stage().expect("reload replay stage");
    let checkpoint = load_checkpoint(&fixture);
    let latest = checkpoint.latest.as_ref().expect("latest evidence");
    let commit = worktree_head_commit(&fixture.worktree).expect("read worktree HEAD");
    assert!(
        stage.status == StageStatus::Executing
            && latest.commit == commit
            && latest.check_definition_hash == check_definition_hash(&stage)
            && latest.exact_command == fixture.pinned_command()
            && latest.phase == CompletionPhase::VerifiedPendingAck
            && latest.external_failure_code.as_deref() == Some("daemon_transport")
            && checkpoint.repeat_count() == 1
            && current_blocker(&checkpoint, &stage, Some(&commit)).is_some(),
        "persisted checkpoint did not retain actionable authority\n{output}\ncheckpoint: {checkpoint:#?}"
    );
}

#[test]
fn duplicate_delivery_is_idempotent_but_new_nonce_increments_repeat_count() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let (first_producer, first_hook) = run_through_hook(&fixture);
    let first_stdout = String::from_utf8_lossy(&first_producer.stdout);
    let duplicate = fixture.run_hook_post(&fixture.pinned_command(), &first_stdout, false);
    let duplicate_output = diagnostics(&first_producer, &duplicate);
    assert!(
        duplicate.status.code() == Some(2) && load_checkpoint(&fixture).repeat_count() == 1,
        "duplicate delivery changed the repeat count\n{duplicate_output}"
    );

    let (second_producer, second_hook) = run_through_hook(&fixture);
    let second_output = diagnostics(&second_producer, &second_hook);
    assert!(
        first_hook.status.code() == Some(2)
            && second_hook.status.code() == Some(2)
            && load_checkpoint(&fixture).repeat_count() == 2,
        "distinct producer nonce did not increment repeat count\n{}\n{second_output}",
        diagnostics(&first_producer, &first_hook)
    );
}

#[test]
fn checkpoint_readback_does_not_authorize_a_successor_session() {
    let fixture = ReplayFixture::new(&["true"]).expect("create replay fixture");
    let (producer, hook) = run_through_hook(&fixture);
    let output = diagnostics(&producer, &hook);
    let checkpoint = load_checkpoint(&fixture);
    let readback = load_checkpoint(&fixture);
    assert_eq!(
        checkpoint, readback,
        "checkpoint changed on fresh read\n{output}"
    );

    fixture
        .rewrite_stage_session("session-completion-successor")
        .expect("write successor session");
    let successor = fixture.reload_stage().expect("reload successor stage");
    let commit = worktree_head_commit(&fixture.worktree).expect("read worktree HEAD");
    assert!(
        hook.status.code() == Some(2)
            && current_blocker(&readback, &successor, Some(&commit)).is_none(),
        "predecessor checkpoint authorized successor session\n{output}\ncheckpoint: {readback:#?}"
    );
}
