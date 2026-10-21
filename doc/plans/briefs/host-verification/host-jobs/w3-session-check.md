# W3: in-session `loom check` through a host job (stage host-jobs)

Read `doc/plans/briefs/host-verification/common.md` first. Sections 3.5 (`tree_digest`) and 3.7 (the session-side protocol) pin your interfaces. You call W1's `submit::submit` (3.3) from the drain handler. Paths are relative to `loom/`.

## Territory

You own:

- `src/host_verify/digest.rs`, `wait.rs`, `tests_digest.rs`, `tests_wait.rs`;
- edits to `src/relay/kind.rs`, `src/relay/payload.rs`, `src/relay/matrix.rs`;
- edits to `src/orchestrator/core/inbox_drain/apply.rs` (one arm) and `mod.rs` (declaration);
- `src/orchestrator/core/inbox_drain/check.rs` (new);
- edits to `src/commands/verify.rs` and `src/sandbox/settings.rs` (`STATE_READ_DIRS`).

W1 declares `pub mod digest; pub mod wait;` in `src/host_verify/mod.rs`; add `#[cfg(test)]` test-module declarations inside your own files (`#[cfg(test)] #[path = "tests_digest.rs"] mod tests;`).

## Steps

1. **Relay kind.**
   - `RequestKind::Check`: wire name `check`; added to `all()` (the array grows to 11; update the doc comment's count); not a control kind.
   - Update the module doc's "ten request kinds".
   - Update the tests in `kind.rs` that count kinds. This raises `TI-edit-loom/src/relay/kind.rs`, which the main agent disputes. Keep every existing kind's assertion.
2. **Payload.** In `relay/payload.rs`:
   - Add `#[serde(deny_unknown_fields)] pub struct HostJobRequest { pub head: String, pub tree_digest: String, pub no_cache: bool }`.
   - Add `RequestPayload::Check(HostJobRequest)`.
   - Decode: `RequestKind::Check => Ok(RequestPayload::Check(decode(payload, "check")?))`.

   Stage 2 reuses `HostJobRequest` for `complete`; name it exactly so.
3. **Matrix** (`relay/matrix.rs::verdict`). Read how rows are written:
   - `Check` is `Apply` for `SessionType::Stage`, `Knowledge` and `Adjudication` (judges run `loom check <stage> --no-cache`).
   - `Refuse` for every other session type.
   - Add the row-level tests the file's pattern uses.
4. **Drain handler** (`inbox_drain/check.rs`, plus one arm in `apply.rs::apply`: `RequestPayload::Check(request) => check::apply_check(&work_dir, stage_id, record, &request, <relay request id>)`).
   - `apply.rs` is 360 lines; keep the arm to a few lines.
   - Find how the relay request id reaches `apply` (the `InboxEntry` has an id; thread it through `Admitted` if it is not there yet, line-neutrally).
   - Admission rules in `apply_check`:
     - A `Stage` or `Knowledge` session must own the stage: reuse `apply.rs::require_owner`.
     - An `Adjudication` session must be the running judge of that stage: its session record's `stage_id` equals the stage and its status is running. Find how the judge watchdog identifies the stage's judge (`orchestrator/monitor/judge_watchdog.rs`) and reuse that predicate.
   - Validate:
     - `head` is 40 or 64 lowercase hex;
     - `tree_digest` is 64 lowercase hex;
     - the stage's checkout HEAD equals `head`. The checkout is the stage worktree, or the repository root for a knowledge stage; read HEAD with `git::worktree` helpers or `git rev-parse HEAD` pinned to that directory, as `inbox_drain/commit.rs::pinned_worktree` does. Otherwise refuse with `the checkout's HEAD is <x>, not <head>: run loom check again`.
   - Do not recompute the digest here: the child checks it on the host before running anything.
   - Call `host_verify::submit::submit` with kind `Check`. Settle:
     - `Queued` → `Settle::Applied(Some("host job <id> queued"))`;
     - `Joined` → `Applied(Some("joined host job <id>"))`;
     - `Superseded` → `Applied(Some("host job <id> queued; <old> cancelled"))`.

   A submit error settles `Refused` with the error.
5. **Tree digest** (`digest.rs`), exactly common.md 3.5. Run git through `std::process::Command` with `current_dir(checkout)`, `.env_remove("GIT_DIR").env_remove("GIT_INDEX_FILE").env_remove("GIT_WORK_TREE")`. Parse porcelain v2 `-z`:
   - ordinary `1` and rename `2` entries (the rename carries its original path as a separate NUL field);
   - unmerged `u`;
   - untracked `?`;
   - ignored `!` does not appear without `--ignored`.

   Hash each listed current path's content; read files with a size cap of 64 MiB, refusing larger with an error. A symlink contributes its target string, not the pointee.
6. **Wait protocol** (`wait.rs`), common.md 3.7. It holds the logic both `loom check` and, in stage 2, `loom stage complete` use. Shape it as:

   ```rust
   pub struct WaitSite<'a> { pub work_dir: &'a Path, pub stage_id: &'a str, pub checkout: &'a Path, pub kind: JobKind, pub no_cache: bool, pub scratch_dir: &'a Path, pub budget: Duration }
   pub enum WaitOutcome { Requested, Pending { done: usize }, Finished(JobRecord, Option<JobResult>) }
   pub fn wait_or_request(site: &WaitSite<'_>, relay: &mut dyn FnMut(&HostJobRequest) -> Result<String /*request id*/>, out: &mut dyn Write) -> Result<WaitOutcome>;
   pub fn exit_code(outcome: &WaitOutcome) -> i32;   // 3, 3, 0/1/4
   pub fn budget_for(stage: &Stage) -> Duration;      // common.md 3.7 step 4
   ```

   - The `relay` closure is how the caller emits the ticket (`RelayContext::check` + `emit` with the kind's `RequestKind`). That keeps `wait.rs` testable without a scratch session.
   - The scratch state file: `<scratch_dir>/host-verify/<stage>-<kind>.json`. Write it with mode 0600. Delete it after printing a terminal outcome.
   - `JobKind` serializes to snake_case for the file name.
   - Printing: as common.md 3.7, to `out`. `✓`/`✗` lines in label order.
7. **`loom check` in a session** (`commands/verify.rs::execute`):
   - In `relay::emit::mode(&EnvSnapshot::from_process_env())`'s `Relay(context)` case, call `context.check(RequestKind::Check, Some(stage_id), &cwd, uid)` (uid as `commands/stage/commit.rs` gets it).
   - The checkout is `context.worktree_path`, or the repository root for a knowledge session.
   - Then `wait::wait_or_request` with a closure that emits the ticket, print, and `std::process::exit(wait::exit_code(..))` when the code is not 0.
   - `Operator` and `Legacy` keep today's local run, unchanged.
   - `--suggest` is irrelevant in Relay mode: print `--suggest needs a local run; ignored in a session` once if given.
   - `--no-cache` (if `loom check` has it; `rg -n 'no_cache' src/cli`) sets `no_cache`.
8. **State read access.** Add `"verification"` to `sandbox/settings.rs::STATE_READ_DIRS`. `sandbox/settings.rs` is 438 lines and ledgered: the edit must be line-neutral (the const is one array line, so add the element on it). Update any test that pins the list's exact contents (`rg -n 'STATE_READ_DIRS|"reviews"' src/sandbox`). `sandbox/settings/tests.rs` is ledgered at exactly its current length, so keep such an edit line-neutral too.

## Tests you write

- `tests_digest.rs`: clean tree digest is stable; a tracked edit changes it; an untracked file changes it; a rename changes it; two different contents of the same path differ; an ignored file does not change it.
- `tests_wait.rs`, with a temp state root and scratch dir, writing records through W1's store:
  - no state → relay called once, `Requested`, state written;
  - state plus a running job that turns `Passed` mid-wait → `Finished`, exit 0, state deleted;
  - `Failed` → exit 1 and the failed summaries printed;
  - `Cancelled` → exit 4;
  - budget elapses → `Pending`, exit 3;
  - a state whose head differs from the checkout → a new request.
- Unit tests in `check.rs` for each admission refusal: wrong owner, judge of another stage, malformed head, HEAD mismatch.

## Proof (run once, from `loom/`)

```bash
cargo test --lib host_verify::tests_digest host_verify::tests_wait relay:: orchestrator::core::inbox_drain:: 2>&1 | tail -30
```

If the build fails only on W1's or W2's files not existing yet, report that and stop.

## Report

Files changed; how the request id reaches the handler; the judge predicate you reused; the proof output.
