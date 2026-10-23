# W3: in-session `loom check` through a host job (stage host-jobs)

Read `doc/plans/briefs/host-verification/common.md` first. Sections 2 (relay facts), 3.5 (`tree_digest`) and 3.7 (the session-side protocol and its exit codes) pin your interfaces. You call W1's `submit::submit` (3.3) from the drain handler. Paths are relative to `loom/`.

## Territory

You own:

- `src/host_verify/digest.rs`, `wait.rs`, `tests_digest.rs`, `tests_wait.rs`;
- edits to `src/relay/kind.rs`, `src/relay/payload.rs`, `src/relay/matrix.rs`, `src/relay/mod.rs` (the `pub use payload::{..}` line), `src/relay/tests_matrix.rs`, and `src/relay/matrix_host.rs` (new);
- edits to `src/orchestrator/core/inbox_drain.rs` (the module root: one `mod check;` line), `src/orchestrator/core/inbox_drain/apply.rs` (one arm), `src/orchestrator/core/inbox_drain/test_support.rs` and `src/orchestrator/core/inbox_drain/tests_matrix.rs` (one arm each in their exhaustive kind matches);
- `src/orchestrator/core/inbox_drain/check.rs` (new);
- edits to `src/commands/verify.rs`, `src/commands/verify_relay.rs` (new), `src/sandbox/settings.rs` (`STATE_READ_DIRS`) and `src/sandbox/settings/tests.rs`;
- the visibility of `completion_scaffold` in `src/handoff/completion/content.rs` (one word);
- `../loom-hooks/loom-relay.sh` and `../loom-hooks/tests/loom-relay-kinds.sh`.

W1 declares `pub mod digest; pub mod wait;` and `#[cfg(test)] mod tests_digest; mod tests_wait;` in `src/host_verify/mod.rs`; do not add `#[path] mod tests` to your files.

## Steps

1. **Relay kind.**
   - `RequestKind::Check`: wire name `check`; appended LAST in `all()` (the return type becomes `[RequestKind; 11]`; update the doc comment's count and the module doc's "ten request kinds"); not a control kind.
   - No assertion in `kind.rs` changes: `is_control_matches_the_seven_control_kinds` filters control kinds, and `Check` is not one. Leave every existing test as it is.
2. **Payload.** In `relay/payload.rs`:
   - Add `#[serde(deny_unknown_fields)] pub struct HostJobRequest { pub head: String, pub tree_digest: String, pub no_cache: bool }`.
   - Add `RequestPayload::Check(HostJobRequest)`.
   - Decode: `RequestKind::Check => Ok(RequestPayload::Check(decode(payload, "check")?))`.
   - Add `HostJobRequest` to the `pub use payload::{..}` line of `relay/mod.rs` (`payload` is a private module).
   - A unit test: a payload with an extra key fails to decode.

   Stage 2 reuses `HostJobRequest` for `complete`; name it exactly so.
3. **Matrix.** `relay/matrix.rs` is 350 lines and stage 2 adds another kind, so the new rows go in a new file: `src/relay/matrix_host.rs` holds `pub(super) const HOST_JOB_MATRIX: &[(SessionType, RequestKind, MatrixVerdict)]`, and `matrix.rs` declares it and makes `verdict` search `MATRIX` and then `HOST_JOB_MATRIX` (it still panics on a missing pair).
   - `Check` is `Apply` for `SessionType::Stage`, `Knowledge` and `Adjudication` (judges run `loom check <stage> --no-cache`), `Refuse` for `Merge`, `BaseConflict` and `Contract`.
   - In `relay/tests_matrix.rs`, add a `CHECK_COLUMN` test beside `COMMIT_COLUMN`. Never edit `EXPECTED` or its length assertion.
   - `inbox_drain/tests_matrix.rs::classify_applied` and `inbox_drain/test_support.rs::payload_for` match `RequestKind` exhaustively: add a `Check` arm to each (`payload_for` returns a valid `HostJobRequest` JSON). `inbox_drain/tests_matrix.rs` zips `RequestKind::all()` with its fixed rows, which is why `Check` is appended last.
4. **Drain handler** (`inbox_drain/check.rs`, declared in `inbox_drain.rs`, plus one arm in `apply.rs::apply`: `RequestPayload::Check(request) => check::apply_check(..)`).
   - `apply.rs` is 360 lines; keep the arm to a few lines.
   - The relay request id: `Admitted` does not carry it; `InboxEntry.id` is available in `admit`. Thread it through `Admitted` line-neutrally.
   - Admission in `apply_check`:
     - A `Stage` or `Knowledge` session must own the stage: reuse `apply.rs::require_owner`. A `loom-verifier` subagent's request arrives as its stage session's and passes this way.
     - An `Adjudication` session must be the live judge of that stage: the predicate `apply.rs::adjudication_verdict` uses (`record.session_type == SessionType::Adjudication && record.status == SessionStatus::Running && host.session_alive(record)`); `session_pass.rs::validate_attribution` already binds `record.stage_id` to the stage. Extract it into a shared helper rather than copying it.
   - Validate:
     - `head` is 40 or 64 lowercase hex;
     - `tree_digest` is 64 lowercase hex;
     - the stage's checkout HEAD equals `head`. The checkout is the stage worktree (`git::worktree::stage_worktree_path`) for every session type, or the repository root for a knowledge stage; read HEAD pinned to that directory, as `inbox_drain/commit.rs::pinned_worktree` does. Otherwise refuse with `the checkout's HEAD is <x>, not <head>: run loom check again`.
   - Do not recompute the digest here: the child checks it on the host before running anything.
   - Call `host_verify::submit::submit` with kind `Check`. Settle:
     - `Queued` → `Settle::Applied(Some("host job <id> queued"))`;
     - `Joined` → `Applied(Some("host job <id> joined"))`;
     - `Superseded` → `Applied(Some("host job <id> queued; <old> cancelled"))`;
     - `Refused(reason)` → `Refused` with the reason.

   Every `Applied` detail starts with `host job <id>`: the wait protocol reads the job id from it. A submit error settles `Refused` with the error.
5. **Tree digest** (`digest.rs`), exactly common.md 3.5: `tree_digest` and `knowledge_tree_digest`. Run git through `std::process::Command` with the flags and environment pinned there. Raise `handoff/completion/content.rs::completion_scaffold` to `pub(crate)` and drop each untracked entry it accepts. Read files with a size cap of 64 MiB, refusing larger with an error.
6. **Wait protocol** (`wait.rs`), common.md 3.7. It holds the logic both `loom check` and, in stage 2, `loom stage complete` use. Shape it as:

   ```rust
   pub struct WaitSite<'a> { pub work_dir: &'a Path, pub stage_id: &'a str, pub session_id: &'a str, pub checkout: &'a Path, pub knowledge: bool, pub kind: JobKind, pub no_cache: bool, pub scratch_dir: &'a Path, pub budget: Duration }
   pub enum WaitOutcome { Requested, Pending { done: usize }, Finished(JobRecord, Option<JobResult>), Refused(String) }
   pub fn wait_or_request(site: &WaitSite<'_>, relay: &mut dyn FnMut(&HostJobRequest) -> Result<String /*request id*/>, out: &mut dyn Write) -> Result<WaitOutcome>;
   pub fn exit_code(outcome: &WaitOutcome) -> i32;   // 0 requested/pending/passed, 1 failed/refused, 4 stopped
   pub fn budget_for(stage: &Stage) -> Duration;      // common.md 3.7 step 4
   ```

   - The `relay` closure emits the ticket (`RelayContext::check` + `emit` with `RequestKind::Check`, `end_turn: false`). `wait_or_request` prints the `Host job: requested ...` line and its follow-up line to `out` BEFORE calling it, so the relay line stays last. That keeps `wait.rs` testable without a scratch session.
   - The first line of every outcome is `Host job: <state> (<kind>, <short head>)` (common.md 3.7 table).
   - Refusals come from the session's inbox ledger (`fs::inbox::read_ledger`); a job that is no longer current is followed through `store::load_job_record`.
   - The scratch state file: `<scratch_dir>/host-verify/<stage>-<kind>.json`, mode 0600, holding `{request_id, head, tree_digest, no_cache, submitted_at}`, written atomically (temp file and rename in the same directory). Delete it after printing a terminal outcome, and only when it still holds the `request_id` this call read (re-read just before deleting): two verifier subagents of one session share the file.
   - `JobKind` serializes to snake_case for the file name.
   - Printing: as common.md 3.7, to `out`. A failed outcome with a `log` prints `full log: <job dir>/<log>` (absolute) after its summary. `✓`/`✗` lines in label order.
7. **`loom check` in a session** (`commands/verify.rs::execute`). `execute` is ledgered at exactly 63 lines and must stay 63: put the session branch in a new file `src/commands/verify_relay.rs`, declared inside `verify.rs` as `#[path = "verify_relay.rs"] mod session_relay;` (`commands/mod.rs` is W2's), and replace one existing line of `execute` with the call.
   - `relay::emit::mode(&EnvSnapshot::from_process_env())`:
     - `Relay(context)`: call `context.check(RequestKind::Check, Some(stage_id), &cwd, uid)` (uid as `commands/stage/commit.rs` gets it), compute the checkout (common.md 3.7 step 1), call `wait::wait_or_request(` written exactly so (the stage's wiring checks that text in `verify_relay.rs`, and `session_relay::` in `verify.rs`) with a closure that emits the ticket, then `std::process::exit(wait::exit_code(..))` when the code is not 0.
     - `Legacy`: refuse with the common.md 3.7 text, exit 1. A local run there would nest confinement.
     - `Operator`: today's local run, unchanged.
   - `no_cache` is `std::env::var("LOOM_ACCEPTANCE_CACHE").as_deref() == Ok("0")` (`cli/dispatch.rs::run_check` sets it for `--no-cache`; `execute` has no such parameter).
   - `--suggest` is irrelevant in Relay mode: print `--suggest needs a local run; ignored in a session` once if given.
8. **State read access.** Add `"verification"` to `sandbox/settings.rs::STATE_READ_DIRS`. `sandbox/settings.rs` is ledgered at 438 lines: the const is one `#[rustfmt::skip]` array line, so add the element on it. `sandbox/settings/tests.rs` (ledgered at 1565 lines) pins the list twice: the `dirs` string (about line 245) gains `verification/**`, and `assert_eq!(allow.len(), 15)` (about lines 253 and 665) becomes 17. Keep both edits line-neutral. This is the stage's one expected integrity event (`TI-edit-loom/src/sandbox/settings/tests.rs`).
9. **Relay hook.** In `../loom-hooks/loom-relay.sh::relay_kind_at`, add `check:*) echo check ;;`. `check` is not a control kind, so `drop_control_kinds` is unchanged. In `../loom-hooks/tests/loom-relay-kinds.sh`, add cases in the file's style: `loom check s1` and `loom check s1 --no-cache` derive `check` for the main agent and for a subagent, and `cd loom && loom check s1` does too. Run `bash ../scripts/check-hook-syntax.sh` afterwards.

## Tests you write

- `tests_digest.rs` (real repositories): a clean tree's digest is stable; a tracked edit changes it; an untracked file changes it; a rename changes it; two different contents of the same path differ; an ignored file does not change it; an empty untracked `.bashrc` or `.mcp.json` at the root does not change it (sandbox mount point); `knowledge_tree_digest` ignores an untracked file outside the knowledge directory.
- `tests_wait.rs`, with a temp state root and scratch dir, writing records through W1's store:
  - no state → relay called once, `Requested`, state written, the `Host job: requested` line printed before the relay closure ran, exit 0;
  - state plus a running job that turns `Passed` mid-wait → `Finished`, exit 0, state deleted;
  - `Failed` → exit 1 and the failed summaries printed with `full log:`;
  - `Cancelled` → exit 4;
  - budget elapses → `Pending`, exit 0, `Host job: pending` first;
  - a superseded job is followed through `record.json` and reported `stopped`;
  - a ledger `Refused` entry → `Refused`, exit 1, state deleted;
  - a state whose head or `no_cache` differs → a new request;
  - a terminal outcome for request r1 while the state file already holds r2 (another waiter's newer request) → the state is left in place.
- Unit tests in `check.rs`: a stage session that owns the stage is applied and leaves one `Queued` Check record; the same entry marked as a subagent's (the inbox entry's subagent attribution, as `admit` reads it) is admitted and queued too (a `loom-verifier` runs `loom check`); each refusal: wrong owner, judge of another stage, a judge that is not running, malformed head, HEAD mismatch, a `Refused` submission.

## Proof (run once each, from `loom/`)

```bash
cargo test --lib host_verify:: 2>&1 | tail -20
cargo test --lib relay:: 2>&1 | tail -10
cargo test --lib orchestrator::core::inbox_drain:: 2>&1 | tail -10
cargo test --lib sandbox::settings 2>&1 | tail -5
cd .. && bash loom-hooks/tests/loom-relay-kinds.sh 2>&1 | tail -5
```

If the build fails only on W1's or W2's files not existing yet, report that and stop.

## Report

Files changed; how the request id reaches the handler; the judge predicate you shared; the line counts of `execute` and `relay/matrix.rs`; the proof output.
