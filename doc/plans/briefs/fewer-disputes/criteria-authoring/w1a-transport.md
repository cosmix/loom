# criteria-authoring W1a: freeze flags and their transport

Tier: sonnet. Wave 2, with W1b, W2 and W3. Read `common.md` first. Frozen contracts:
`tests/writer_choices_contracts.rs` (never edit it).

## Why

The contract writer passes `--test`, `--runner` and `--cwd` on `loom stage contracts freeze`. The request
reaches the daemon by three routes (relay ticket, socket RPC, spool file) that all end in
`handle_freeze_contracts`. You carry a `ContractChoices` map along each route and call the handler with it.
W1b writes `choices.rs` (`parse_choices`) and the handler body; you code against the signatures in `common.md`.

## Files you own

`src/cli/types_stage_contracts.rs`, `src/cli/dispatch_stage.rs`, `src/commands/stage/contracts.rs`,
`src/commands/stage/contracts/freeze.rs`, `src/commands/stage/contracts/freeze_choices_tests.rs` (new; W0b owns
the existing `freeze_tests.rs` and `freeze_run_dir_tests.rs`), `src/stage_service/types.rs`, `src/daemon/protocol.rs`, `src/daemon/protocol_debug.rs`,
`src/daemon/server/stage_control.rs`, `src/daemon/server/contracts.rs`, `src/daemon/server/self_service_tests.rs`,
`src/relay/payload.rs`, `src/fs/stage_request/apply.rs`, `src/fs/stage_request/tests/apply.rs`,
`src/fs/stage_request/tests/replay.rs`, `src/orchestrator/core/inbox_drain/apply.rs`,
`src/orchestrator/core/inbox_drain/stage_request_dispatch.rs`,
`src/orchestrator/core/inbox_drain/tests_matrix.rs`, `src/orchestrator/core/inbox_drain/test_support.rs`,
`src/commands/stage/contracts/freeze_follow.rs` (new), `src/commands/stage/contracts/freeze_follow_tests.rs` (new),
`src/host_verify/wait.rs`, `src/host_verify/tests_wait_freeze.rs` (new),
`../loom-hooks/commit-guard.sh`, `../loom-hooks/tests/commit-guard-contract-session.sh`.

## Edits

1. `ContractsCommands::Freeze` (`src/cli/types_stage_contracts.rs`) gains three repeatable options:
   `#[arg(long = "test", value_name = "ID=TEST")] test: Vec<String>`, `#[arg(long, value_name = "ID=ADAPTER")]
   runner: Vec<String>`, `#[arg(long, value_name = "ID=DIR")] cwd: Vec<String>`. Doc comments: what each names and
   that a later freeze keeps the flags it does not repeat. `dispatch_stage_contracts`
   (`src/cli/dispatch_stage.rs`) passes them to `stage::contracts_freeze(stage_id, test, runner, cwd)`.
2. `commands::stage::contracts::freeze::freeze` (and `run_freeze`, `request_run`) take the three `Vec<String>`s.
   `request_run` calls `crate::verify::contracts::choices::parse_choices(&test, &runner, &cwd)` right after
   `ContractSite::load`, so a malformed flag is a CLI refusal in the writer's own session (it goes through
   `session_refusal` and `keep_outcome` like any other CLI refusal). It then sets `site.stage.contracts =
   choices::overlay_with_saved(&site.work_dir, stage_id, &site.stage.contracts, &parsed)?` (W1b writes it:
   read-only load of the daemon's saved `choices.json`, merged with this call's flags, laid over the plan's
   specs), so `check_site_changes` derives the scaffold from the tests the writer named in this call AND in
   earlier freezes: a later freeze that repeats no `--test` keeps them. A contract with neither a hint nor a
   saved or passed test is skipped by the scaffold check (the daemon's check is authoritative). Then
   `check_site_changes`, HEAD, and `send_freeze` with the PARSED map of this call only (the daemon merges it
   with the saved ones; never send the merged map).
3. `send_freeze` (`src/commands/stage/contracts.rs`) gains `choices: &ContractChoices` and puts it on
   `StageRequest::FreezeContracts { head, tree_digest, choices }` and `Request::FreezeContracts { .., choices }`.
   The relay payload serialises `StageRequest` already, so the ticket carries it with no further change; the
   spool route appends the same value.
4. `StageRequest::FreezeContracts` (`src/stage_service/types.rs`) and `Request::FreezeContracts`
   (`src/daemon/protocol.rs`) gain `#[serde(default, skip_serializing_if = "BTreeMap::is_empty")] choices:
   ContractChoices`. Update `protocol_debug.rs` (`debug_freeze` prints `choices` as a count only; the values are
   writer text). Update the match arms and constructors: `daemon/server/stage_control.rs`
   (`serve_freeze_contracts` takes `choices: &ContractChoices`; it generates the request id as today),
   `daemon/server/contracts.rs` (the RPC adapter forwards the new last argument),
   `fs/stage_request/apply.rs` (`dispatch_freeze` forwards it), `orchestrator/core/inbox_drain/apply.rs` and
   `stage_request_dispatch.rs` (`settle_freeze` forwards it), and the test constructors in
   `src/daemon/server/self_service_tests.rs`, `src/fs/stage_request/tests/{apply,replay}.rs`,
   `src/orchestrator/core/inbox_drain/{tests_matrix,test_support}.rs`, `src/relay/payload.rs` (its decode test).
   Every call of `handle_freeze_contracts` ends with the new `&choices` argument (W1b changes the handler).
5. Bound the map where it enters: `decode_stage_request` in `relay/payload.rs` already refuses unknown keys;
   add to the freeze arm a refusal of more than 64 entries or any key failing
   `crate::validation::validate_id` (`choices keys must be contract ids`). W1b's `validate_choices` checks
   the ids against the stage's contracts again; this is the cheap wire bound.

6. The writer's session waits for the host run, so a refusal that names its own file comes back to it
   (Decision 2; W1b decides which refusals come back, `host_verify/freeze.rs` and `verify/contracts/refusal.rs`).
   Today `send_freeze` prints `RUN_REQUESTED` ("End your turn now") and returns, so a failed freeze job reaches
   only an adjudicator. In `RelayMode::Relay` replace that with the follow `loom check` uses
   (`commands/verify_relay.rs::request_or_wait` is the model): a `WaitSite { work_dir, stage_id, session_id:
   &context.session_id, checkout: &site.worktree_root, knowledge: false, kind: JobKind::Freeze, no_cache: false,
   criteria: None, scratch_dir: &context.scratch_dir, budget: wait::budget_for(&site.stage) }`, a `TicketSink`,
   and a relay closure that serialises `StageRequest::FreezeContracts { head: request.head.clone(),
   tree_digest: request.tree_digest.clone(), choices: choices.clone() }` and emits it with
   `RequestKind::FreezeContracts` (`context.emit(.., "stage contracts freeze", false, &mut sink)`, returning
   `line.id`; `end_turn` is `false` because the session waits, as in `loom check`; the notice would otherwise tell it to end its turn), then `wait::wait_or_request(&site, &mut relay, out)`, `sink.publish_request(..)` on `Requested`,
   and `std::process::exit(wait::exit_code(&outcome))` after flushing. Keep `RelayMode::Operator` (socket RPC,
   `handle_freeze_response`) and the spool route as they are. The pre-checks (`context.check`, `ContractSite::load`,
   `parse_choices`, `check_site_changes`, HEAD and tree digest) still run first and still end through
   `session_refusal` and `keep_outcome`; once the ticket is sent the attempt reached the daemon, so call
   `refusal::clear(scratch_dir)` there and not `refusal::record` for the host run's own failure.
   The `Host job: requested` lines `wait_or_request` prints replace `RUN_REQUESTED` in the relay route (the
   relay line stays the last line of the first call's stdout); `RUN_REQUESTED` stays, unchanged, for the
   operator and spool routes, which have no waiting session.

   **The wait follows the daemon's application of the job.** The writer must be able to tell a refusal returned
   to it from one that was filed, and only the application (the daemon's `apply_freeze_result`, which writes the
   job's `applied` marker last) decides that. Today `host_verify/wait.rs::awaits_application` is true only for a
   passed `Complete` job, so a finished freeze job is reported as soon as it ends. (a) In `wait.rs`
   `awaits_application` becomes true also for a `JobKind::Freeze` job that finished `Passed` or `Failed` (a
   `match` on `site.kind`; every other kind and state keeps its answer). (b) In `drive`, a Freeze job that awaits
   application is REPORTED before the early return (`report(site, &head, &outcome, out)?`: the heading
   `Host job: failed (freeze, ..)` and the failing outcomes' output), because the caller prints only the
   application line; a `Complete` job is unchanged (the caller reports it). The request state stays until the
   caller `release`s it. (c) New `commands/stage/contracts/freeze_follow.rs`:
   `pub(super) fn follow_application(site: &WaitSite<'_>, record: &JobRecord, budget: Duration, out: &mut dyn
   Write) -> Result<i32>`, modeled on `commands/stage/complete_relay.rs::follow_application`: poll
   `store::load_job_file(site.work_dir, site.stage_id, &record.id, APPLIED_MARKER)` (a short sleep between
   polls) until `budget` (the site budget less the time already waited) ends.
   - Marker found: `wait::release(site, record)?`, then print the marker's note as one line and an ending line,
     and return the exit code. A note starting `froze the contracts` prints `Stop now; loom ends this session.`
     and returns 0. A note starting `refused the contract run: returned to the contract writer` (the word `returned`
     is W1b's prefix, with `(<n> of 3)`) returns 1 and adds nothing: the failing output above it says what to
     fix and the writer freezes again. Any other note (a refusal that is filed, an ignored application) adds
     `This refusal is filed for an adjudicator; stop and let loom end this session.` and returns 1.
   - No marker at the end of the budget: print `wait::heading(JobKind::Freeze, "pending", &record.head)` followed
     by `: the run finished and the daemon has not applied it yet. Run the same command again to keep waiting.`
     and return 0 with the request state kept, so the next call follows the same job.
   `send_freeze` calls it when `wait_or_request` returns `Finished(record, _)` with a Passed or Failed record
   and `std::process::exit`s with its code after flushing; every other outcome keeps `wait::exit_code`.
7. `../loom-hooks/commit-guard.sh`, the contract-session branch of the completion checklist (the
   `Contract session for stage` block of `remind_contract_freeze`, about lines 100-113): it still tells the
   writer that a refusal waits for an adjudicator after 5 quiet minutes and that it must stop after the freeze
   request. Replace those lines (both the `contract_freeze_refused` branch and the closing `After the freeze
   request, stop; loom ends this session.` line) so they say: the freeze waits for its host job and for the
   daemon's application of it; on `Host job: failed`, fix what the output names and run
   `loom stage contracts freeze <stage-id>` again with the same flags; the wait ends with a line that says
   whether the refusal was returned to the writer (`n of 3`) or filed; the exact rule is that a static refusal is
   returned to the writer, and it is filed as a `freeze-refusal` dispute (an adjudicator takes over and a fresh
   writer starts with the verdict) on the third static refusal in the session, or when the writer stays quiet
   5 minutes after a refusal with no subagent running; once the wait prints `Host job: passed` and the line
   `froze the contracts`, stop. (`Host job: completed` is printed by `loom stage complete` only; the freeze
   never prints it.) Keep `printf '%s\n' ... >&2` per line and the `contract_freeze_refused` branch structure.
   `../loom-hooks/tests/commit-guard-contract-session.sh` lines 90-98 pin four strings of the refused branch
   (`the latest freeze was refused: fix what it names`, `loom stage contracts freeze $STAGE_ID again, or stop`,
   `quiet for 5 minutes with no subagent running`, `refusal goes to an adjudicator and a fresh writer starts
   with`); you own that file now: change those expected strings to the words you wrote (each must still be a
   substring of one `printf` line, not split across two), keep the `unexpected` list and the no-loom-call check,
   and add one expectation for `Host job: passed` and one for `froze the contracts`. Run
   `bash ../loom-hooks/tests/commit-guard-contract-session.sh` once after the edit.

## Traps

- The wait's request state is keyed by HEAD and tree digest, so a rerun with the same tree while the first job is
  pending follows the first job; the daemon refuses a rerun whose flags differ from the saved ones while that
  job is non-terminal (W1b), so the follow never reports a job run under other flags. The state now outlives a
  finished freeze job until `follow_application` releases it. Say this in the doc comment of the new function;
  do not try to key the state on the flags.
- The follow runs in the writer's sandbox: it only reads the job directory under the state root (as `loom
  check` does) and the relay scratch directory.

- `StageRequest` is `deny_unknown_fields`; the new field has `#[serde(default)]`, so an older spool line decodes.
- A writer who passes no flags sends an empty map, which is skipped on serialisation: the wire form of a plan
  whose contracts all carry a hint is unchanged.
- The CLI must not print or echo a flag value into the relay line (the relay line stays the last line of the
  first call's stdout).
- Do not validate `--cwd` or `--runner` here (the daemon owns that, with the base commit); `parse_choices`
  only splits and bounds.
- `src/commands/stage/contracts/freeze.rs` has `#[cfg(test)]` imports of `ContractSpec` and friends: keep them.
- `wait.rs` serves `loom check` and `loom stage complete` too: the `Check` and `Complete` paths must behave as
  before (tests in `tests_wait*.rs` and `complete_relay` must stay green; do not edit them).
- A returned refusal whose writer stays quiet for 5 minutes is still filed (W1b's tick), so a writer that wakes
  later may find its freeze refused at admission; print that refusal as the existing code does.

## Tests (new file `freeze_choices_tests.rs`, wired in `freeze.rs` with `#[cfg(test)] #[path = ".."] mod`)

- `Cli::try_parse_from(["loom","stage","contracts","freeze","s1","--test","a=x::y","--test","b=z","--runner",
  "a=cargo-test","--cwd","a=loom"])` parses to the three vectors in order; `--test` alone still parses.
- The relay closure's payload (a small `freeze_payload(request, choices)` helper you factor out for this) for a
  head, tree digest and choices decodes back to the same
  `StageRequest::FreezeContracts`.
- `StageRequest::FreezeContracts` with an empty map serialises without a `choices` key, with entries round
  trips, and a line with a `choices` key decodes through `decode_payload(RequestKind::FreezeContracts, ..)`.
- 65 entries and a key `../x` are refused by the relay decode.
- Re-freeze without repeating `--test` (`freeze_choices_tests.rs`): a work-directory fixture with a saved
  `choices.json` holding a test for a contract that has no plan hint; the pre-check path (factor the overlay of
  `request_run` into a small function the test calls) derives its scaffold from the saved test, and a second
  call with a flag for another contract keeps the first's test.
- `tests_wait_freeze.rs` (wired from `wait.rs` with `#[cfg(test)] #[path = "tests_wait_freeze.rs"] mod
  freeze_tests;`): a finished `Failed` freeze job leaves the request state in place, prints `Host job: failed`
  and the failing output; a finished `Check` job and a failed `Complete` job still remove it; `release` removes it.
- `freeze_follow_tests.rs`: with a marker `froze the contracts (2 files)` the function returns 0 and releases;
  `refused the contract run: returned to the contract writer (1 of 3): ..` returns 1 with no filing line; any
  other note returns 1 with the filing line; no marker and a zero budget prints the `pending` heading and
  returns 0 with the state kept.

## Done

Your one check once the crate compiles: `cargo test --lib commands::stage::contracts::` (the hook test above is your only other run).
