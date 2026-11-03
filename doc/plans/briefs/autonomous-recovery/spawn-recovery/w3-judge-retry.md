# spawn-recovery W3: a failed judge spawn is retried, repaired or escalated with its full error

Tier: opus. Wave 2, after W1 and W2. Read `doc/plans/briefs/autonomous-recovery/common.md`
first, then knowledge `mistakes/adjudication-autonomy-deadlock.md` ("Escalation Left the
Disputing Agent Alive, and Approve Re-attached It"). Frozen contracts:
`src/orchestrator/adjudication/spawn_retry_contract_tests.rs`.

## Why

`AdjudicatorRegistry::start_pending_adjudications` (`src/orchestrator/adjudication/session.rs`)
escalates the stage to a human on the FIRST spawn error
(`Escalation::adjudicator_unavailable`, `escalation.rs`), and `claim_session_slot`
(`mod.rs`) spends one of the dispute's `MAX_ADJUDICATION_ATTEMPTS` before the spawn, so a
failed spawn also burns a judge attempt. The session record is saved only after the backend
spawned (`spawn_for`), so a daemon that dies between the two can hand the next daemon a stage
with a live judge and no record: a second judge.

## Interface (pinned)

New `src/orchestrator/adjudication/spawn_retry.rs`:

```rust
pub(crate) enum SpawnClass {
    /// Retry with backoff: any error no typed rule below claims.
    Transient,
    /// The spawn path repaired something (hooks reinstalled) or a daemon restart is pending
    /// (`RefusalKind::StaleDaemon`): retry without spending the transient budget.
    Repairable,
    /// Escalate now: a preflight refusal of kind WritableInstall, WritableHookTool, Policy or
    /// NoHooksDir, or `crate::claude::ClaudeBinaryMissing`.
    Permanent,
}
pub(crate) fn classify(error: &anyhow::Error) -> SpawnClass;
pub(crate) const MAX_TRANSIENT_SPAWN_FAILURES: u32 = 3;
pub(crate) const MAX_REPAIRABLE_SPAWN_FAILURES: u32 = 5;
pub(crate) enum RetryDecision { RetryAt(DateTime<Utc>), Escalate(Escalation) }
/// Persist one failure in `disputes/<stage>/<n>/spawn_failures.json`
/// (`{ transient, repairable, last_error, next_attempt_at }`, atomic write) and decide.
/// Transient: backoff `orchestrator::retry::calculate_backoff(count, 30, 300)`; the third
/// transient failure escalates. Repairable: 30 s; the fifth escalates. Permanent: escalates.
pub(crate) fn record_failure(work_dir: &Path, stage_id: &str, dispute_id: u32, class: SpawnClass, error: &anyhow::Error, now: DateTime<Utc>) -> RetryDecision;
/// False while `next_attempt_at` is in the future. Read from disk every call.
pub(crate) fn may_attempt(work_dir: &Path, stage_id: &str, dispute_id: u32, now: DateTime<Utc>) -> bool;
```

## Edits (you own these files)

1. `spawn_retry.rs` as above; the escalation's reason is
   `No adjudication session could be started (<class>): <remedy>` on its first line, then the
   full error chain, one cause per line (`error.chain()`), control characters stripped, the
   whole reason cut at 4096 bytes on a char boundary. Remedies: Permanent preflight
   "the host boundary refused the spawn; read the causes below"; ClaudeBinaryMissing
   "needs a host install: claude on PATH"; budget spent "the spawn kept failing; the last
   error is below".
2. `src/orchestrator/adjudication/mod.rs`: `claim_session_slot` checks
   `spawn_retry::may_attempt` BEFORE `record_attempt`, and declares `spawn_retry` (module
   lines only).
3. `src/orchestrator/adjudication/session.rs`:
   - on a spawn error, `classify`, then `record_failure`; `RetryAt` un-spends the judge attempt
     (`attempts` back to its value before the claim) and pushes nothing; `Escalate` pushes the
     escalation (still written by `Orchestrator::escalate_dispute`, which retires the disputing
     agent first);
   - `spawn_for` saves the adjudication session record (no pid yet) BEFORE the backend spawn
     and removes it when the spawn fails;
   - `live_adjudication_session` also counts an adjudication record of the stage with no
     verified process identity created less than 60 s ago (mirror
     `orchestrator/core/recovery_guards.rs`' young-session rule): a judge spawning right now
     is not dead. Confirm the orphan recovery (`recover_orphaned_sessions`) and the judge
     watchdog (`orchestrator/monitor/judge_watchdog.rs`) read such a record the same way; if
     either would close or recover it, change it to match (you own both files).
4. `src/orchestrator/adjudication/escalation.rs`: `adjudicator_unavailable` takes the composed
   reason and keeps `failure_info.evidence` as the reason's lines (the full chain).
5. `src/claude.rs`: `find_claude_path` returns `anyhow::Error::new(ClaudeBinaryMissing {
   checked: Vec<PathBuf> })` (a `pub struct` implementing `Display` with today's message and
   `std::error::Error`), so classification is by type.
6. Tests beside each module (`session_tests.rs`, `escalation_tests.rs`, a
   `spawn_retry_tests.rs`): a stage reset (status no longer `NeedsAdjudication`) during
   backoff gets no judge; a closed dispute (`closed.marker`) gets none; the retry file
   survives a fresh registry (daemon restart).

## Interplay to keep

- The stage's per-kind dispute budget is spent at FILING; spawn retries never touch it.
- `MAX_ADJUDICATION_ATTEMPTS` bounds judges that STARTED and produced no verdict; a spawn
  failure never spends one.
- Escalation still goes through `Orchestrator::escalate_dispute` (retire first, then write).

## Done

Your one check after the stage compiles: `cargo test --lib orchestrator::adjudication::`.
