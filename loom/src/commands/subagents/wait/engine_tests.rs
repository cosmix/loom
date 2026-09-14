use std::time::Duration;

use super::engine::{wait_for_workers, POLL_INTERVAL};
use super::lease::deadline_after;
use super::model::{TerminalOutcome, WaitIdentity};
use super::tests::{identity, FakeClock, FakeEvidence, FakeSleeper};
use crate::subagent_lifecycle::WorkerOutcome;

fn evidence(identity: &WaitIdentity, scripts: Vec<Vec<WorkerOutcome>>) -> FakeEvidence {
    FakeEvidence::new(
        identity
            .workers
            .iter()
            .zip(scripts)
            .map(|(worker, script)| (worker.worker.id.clone(), script)),
    )
}

#[test]
fn long_replay_succeeds_after_901_sleeps() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let mut script = vec![WorkerOutcome::Active; 901];
    script.push(WorkerOutcome::Succeeded);
    let evidence = evidence(&identity, vec![script.clone(), script]);
    let deadline = deadline_after(&clock, Duration::from_secs(3_600)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::Succeeded);
    assert_eq!(result.polls, 901);
    assert_eq!(sleeper.sleeps(), 901);
}

#[test]
fn deadline_with_live_workers_is_not_proof_of_death() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let evidence = evidence(&identity, vec![vec![WorkerOutcome::Active]; 2]);
    let deadline = deadline_after(&clock, Duration::from_secs(4)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::TimedOut);
    let detail = result.detail.unwrap();
    assert!(detail.contains("claude:claude-a"));
    assert!(detail.contains("codex:codex-a"));
    assert!(detail.contains("deadline expiry is not proof that a worker died"));
}

#[test]
fn failed_beats_cancelled_and_cancelled_beats_pending() {
    let identity = identity();
    let clock = FakeClock::new();
    let deadline = deadline_after(&clock, Duration::from_secs(30)).unwrap();
    let sleeper = FakeSleeper::new(clock.clone());
    let failed = evidence(
        &identity,
        vec![
            vec![WorkerOutcome::Cancelled("cancelled".into())],
            vec![WorkerOutcome::Failed("failed".into())],
        ],
    );
    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &failed,
        POLL_INTERVAL,
    );
    assert_eq!(result.outcome, TerminalOutcome::Failed);

    let cancelled = evidence(
        &identity,
        vec![
            vec![WorkerOutcome::Active],
            vec![WorkerOutcome::Cancelled("cancelled".into())],
        ],
    );
    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &cancelled,
        POLL_INTERVAL,
    );
    assert_eq!(result.outcome, TerminalOutcome::Cancelled);
}

/// A hung worker ends the wait on its own evidence, without waiting out the
/// deadline the way `TimedOut` does.
#[test]
fn stalled_worker_ends_the_wait_before_the_deadline() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let evidence = evidence(
        &identity,
        vec![
            vec![WorkerOutcome::Active],
            vec![WorkerOutcome::Stalled(
                "codex worker codex-a: no progress for 2580s".into(),
            )],
        ],
    );
    let deadline = deadline_after(&clock, Duration::from_secs(3_600)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::Stalled);
    let detail = result.detail.unwrap();
    assert!(detail.contains("codex:codex-a stalled"), "{detail}");
    assert!(detail.contains("no progress for 2580s"), "{detail}");
    assert_eq!(result.polls, 0);
    assert_eq!(sleeper.sleeps(), 0);
}

/// Exact terminal evidence outranks inferred stall evidence.
#[test]
fn failed_beats_stalled() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let evidence = evidence(
        &identity,
        vec![
            vec![WorkerOutcome::Stalled("no transcript growth".into())],
            vec![WorkerOutcome::Failed("companion failed".into())],
        ],
    );
    let deadline = deadline_after(&clock, Duration::from_secs(30)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::Failed);
    assert!(result.detail.unwrap().contains("companion failed"));
}

/// Exact terminal evidence outranks inferred stall evidence for a cancellation
/// too, not only a failure.
#[test]
fn cancelled_beats_stalled() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let evidence = evidence(
        &identity,
        vec![
            vec![WorkerOutcome::Stalled("no transcript growth".into())],
            vec![WorkerOutcome::Cancelled("companion cancelled".into())],
        ],
    );
    let deadline = deadline_after(&clock, Duration::from_secs(30)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::Cancelled);
    assert!(result.detail.unwrap().contains("companion cancelled"));
}

#[test]
fn unknown_at_deadline_stays_unknown() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let evidence = evidence(
        &identity,
        vec![
            vec![WorkerOutcome::Unknown("missing journal".into())],
            vec![WorkerOutcome::Active],
        ],
    );
    let deadline = deadline_after(&clock, Duration::ZERO).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::Unknown);
    assert!(result.detail.unwrap().contains("missing journal"));
}

#[test]
fn success_that_reverts_to_active_is_not_terminal() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let evidence = evidence(
        &identity,
        vec![
            vec![WorkerOutcome::Succeeded, WorkerOutcome::Active],
            vec![WorkerOutcome::Active, WorkerOutcome::Succeeded],
        ],
    );
    let deadline = deadline_after(&clock, Duration::from_secs(2)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::TimedOut);
}

#[test]
fn boot_change_mid_wait_interrupts() {
    let identity = identity();
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    sleeper.change_boot_on_next_sleep("boot-b");
    let evidence = evidence(&identity, vec![vec![WorkerOutcome::Active]; 2]);
    let deadline = deadline_after(&clock, Duration::from_secs(30)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence,
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::Interrupted);
    assert_eq!(result.polls, 1);
}
