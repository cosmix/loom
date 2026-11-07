# criteria-authoring W1b: writer choices at the daemon, in the freeze job, in the record and at completion

Tier: opus. Wave 2, with W1a, W2 and W3. Read `common.md` first, then the knowledge sections it lists
("Where a Contract Runs" especially: it is the trust rule you enforce). Frozen contracts:
`tests/writer_choices_contracts.rs` (never edit it; it calls `parse_choices`, `merge_choices`,
`validate_choices` and `store::effective_contract` through the public API).

## Why

A writer-chosen test, runner or directory is agent text that moves where and how a host job runs a command, and
that completion later trusts from the freeze record. Every choice is validated by the daemon against the stage's
base commit, stored where only the daemon writes, applied to the plan's specs in one place, and recorded.

## Files you own

`src/verify/contracts/choices.rs` (new), `src/verify/contracts/choices_tests.rs` (new),
`src/verify/contracts/completion.rs`, `src/verify/contracts/run_dir.rs`, `src/verify/contracts/collection/mod.rs`,
`src/stage_service/contracts.rs`, `src/stage_service/contracts_choices.rs` (new),
`src/stage_service/contracts_choices_tests.rs` (new), `src/host_verify/freeze.rs`, `src/host_verify/tests_freeze.rs`,
`src/host_verify/tests_freeze_static.rs` (new), `src/host_verify/freeze_refuse.rs`,
`src/verify/contracts/refusal.rs`, `src/verify/contracts/refusal/static_return.rs` (new),
`src/verify/contracts/refusal/static_return_tests.rs` (new), `src/orchestrator/core/event_handler/freeze_refusal.rs`,
`src/orchestrator/core/event_handler/freeze_refusal_tests.rs`,
`src/orchestrator/core/event_handler/freeze_refusal_returned_tests.rs` (new).

Sizes that decide where code goes (the maintainability test fails a file over 400 lines or a function over 50, and
none of these is in the ledger): `stage_service/contracts.rs` is 394 lines and `handle_freeze_contracts` 43, so the
admission of choices lives in `contracts_choices.rs`; `event_handler/freeze_refusal_tests.rs` is exactly 400 lines,
so every new tick test goes in `freeze_refusal_returned_tests.rs`; `host_verify/tests_freeze.rs` is 320 lines, so
the new static-refusal tests of `host_verify` go in `tests_freeze_static.rs`. Check each with `wc -l` first.

## `choices.rs` (declared `pub mod choices;` by W0; signatures are in `common.md`)

- `parse_choices(tests, runners, cwds)`: each entry is `<contract-id>=<value>`, split at the FIRST `=` (a test
  name may contain `=`). The id passes `crate::validation::validate_id`; the value is non-empty after trimming
  and has no `char::is_control`; limits: 64 distinct ids, test at most 512 bytes, runner at most 64, cwd at most
  512, and at most 16 KiB of values in all (a freeze request may not pass `stage_service::MAX_REQUEST_BYTES`,
  64 KiB). The same id given twice for the same flag is an error naming the flag. The three flag lists merge into
  one map by id. An empty input gives an empty map.
- `merge_choices(current, update)`: union of ids; per field, `update`'s value when it has one, else `current`'s.
- `pub fn prune_choices(contracts: &[ContractSpec], saved: &ContractChoices) -> ContractChoices`: the saved
  choices the plan still admits. An id that names no contract is dropped, and so is a saved `runner` or `cwd`
  for a contract whose plan now sets that field (the plan wins; a choice left with no field is dropped). Every
  reader of `choices.json` prunes before it merges (`overlay_with_saved`, `effective_contracts`,
  `lenient_contracts`, admission and the writer's CLI pre-check), so an amendment never leaves a stale choice
  that refuses every later freeze.
- `overlay_with_saved(work_dir, stage_id, contracts, update)`: `merge_choices(&prune_choices(contracts,
  &load_choices(..)?), update)`, then `with_choice` over each spec (the merged choice for its id, or an empty
  one). Read-only and lenient: a spec whose test is still `None` stays so. The writer's CLI pre-check (W1a) and
  `lenient_contracts` both call it, so the overlay rule lives once.
- `load_choices` / `save_choices`: `.loom/work/contracts/<stage>/choices.json`, a JSON object id -> `ContractChoice`,
  read with `read_bounded` (64 KiB) and written with `locked_dir_update` + `atomic_write_locked`, following
  `store.rs`'s `attempts` file (`canonical_work_dir`, `stage_dir`, now `pub(crate)`). A missing file is an empty
  map; a malformed one is an error naming the stage. Only the daemon writes it.
- `effective_contracts(work_dir, stage)`: for each `stage.contracts` spec, `spec.with_choice(choice)` (the
  stage's pruned saved choice for that id, or an empty one), then `required_test()?`; the first contract without
  a test is the error. `load_effective_stage(stage_id, work_dir)`: `load_stage` then replace `contracts` with
  `effective_contracts`. `lenient_contracts(work_dir, stage)`: the same overlay, never failing (an unreadable
  choices file overlays nothing); a missing test stays `None`.
- `validate_choices(contracts, choices, worktree_root, working_dir, base)` is
  `validate_choices_in(&WorktreeGit::discovered(worktree_root), contracts, choices, worktree_root, working_dir,
  base)`; `pub(crate) fn validate_choices_in(git: &WorktreeGit, contracts: &[ContractSpec], choices:
  &ContractChoices, worktree_root: &Path, working_dir: &Path, base: &str) -> Result<()>` holds the rules and runs
  its one git command through `git`. The writer's CLI (W1a) calls `validate_choices`; the daemon calls
  `validate_choices_in` with its pinned `site.git` (contract-phase.md: daemon git is pinned, so a `.git` file the
  writer rewrote cannot choose git's configuration). Both run on `choices` = `merge_choices(&prune_choices(
  contracts, &saved), &update)`: pruning has removed every stale saved entry, so rules 1 and 3 can only fire for
  the choices the call adds. Every contract (not only those in `choices`) must end with a test. Refusals, each an
  `anyhow::Error` whose message names the contract and the flag and says how to fix it:
  1. a key in `choices` that names no contract;
  2. no effective test (`--test <id>=<identifier>`); a test over 512 bytes, empty after trimming, holding a
     control character, or starting with `-` after trimming (the adapters place the test in an option position,
     e.g. `cargo_test.rs::single_test_command` builds `cargo test <test> -- --exact`, so a value such as `--help`
     or `-Zunstable-options` would be read as an option; an identifier never starts with `-`);
  3. a writer `--runner` for a contract whose plan sets `runner`, or a writer `--cwd` for one whose plan sets `cwd`
     (the plan's value wins and the writer is told so, not silently ignored);
  4. an effective runner that no `crate::testrun::registry::by_name` answers to;
  5. a writer `--test` for a contract whose adapter (`resolve_adapter` of the effective spec, a
     `working_dir`-relative file) is `None`: with no adapter the test runs as a shell command on the host, and a
     command may only come from the plan (its `test` hint) or a `--runner` that names an adapter;
  6. a writer `--cwd` that is not a plain relative path (`store::validate_cwd`, so no `..`, no absolute path),
     that does not equal or lie beneath the stage's `working_dir` taken relative to `worktree_root`, or that is
     not an ancestor of the contract's file (`working_dir/file`); implement the prefix test here on normalized
     component lists (the plan-side twin is `plan/schema/validation/v2_fields.rs::cwd_problem`, which W2 owns and
     you do not edit);
  7. a writer `--cwd` whose adapter has no `run_root_markers()`, or none of whose markers is a regular tracked
     file in `base` at that directory: read `git ls-tree -z <base> -- <cwd>/` (the root listing for `.`) through
     `git.run(..)` (checking its exit status; rule 6 runs first, so `<cwd>` is made of components of the plan's
     own file path), accept a mode starting `100`
     (not `120000`, a symlink), and match each entry name against the markers the way `run_dir.rs::has_marker`
     does (`*.ext` entries match by extension). A marker present only in the worktree, or only untracked, does
     not count. This is the rule that stops a file the writer planted from moving a run.

## Wiring the daemon (`src/stage_service/contracts.rs`, `src/stage_service/contracts_choices.rs`)

`contracts_choices.rs` is a submodule of `contracts.rs`, declared there as `#[path = "contracts_choices.rs"] mod
contracts_choices;` (the path is relative to `src/stage_service/`), with its tests wired from it as
`#[cfg(test)] #[path = "contracts_choices_tests.rs"] mod tests;`. It holds:

```rust
/// The request's choices merged with the pruned saved ones, validated against the stage's base commit with the
/// daemon's pinned git; `Err` is the refusal text.
pub(super) fn admit_choices(work_dir: &Path, stage: &Stage, site: &Site, update: &ContractChoices)
    -> Result<ContractChoices>;
/// Inside the `update_stage` closure: `Some(message)` when a pending `Freeze` job for the same head and tree runs
/// under other saved choices; otherwise saves `merged` when it differs from the file, and answers `None`.
pub(super) fn save_admitted(work_dir: &Path, stage_id: &str, head: &str, tree_digest: &str,
    merged: &ContractChoices) -> Result<Option<String>>;
```

1. `handle_freeze_contracts` gains the last parameter `choices: &ContractChoices` (see `common.md`). After
   `admit` (keep the stage it returns) and before the job is submitted, call `locate(&work_dir, &stage)` and then
   `contracts_choices::admit_choices(` exactly once. Inside it: `base = changes::stage_base(&site.git,
   work_dir)?`, `saved = prune_choices(&stage.contracts, &load_choices(..)?)`,
   `merged = merge_choices(&saved, update)`, then `validate_choices_in(&site.git, &stage.contracts, &merged,
   &site.worktree_root, &site.working_dir, &base)` written with exactly that first argument (the stage's wiring
   pins `validate_choices_in(&site.git`). On an `Err` from `locate` or `admit_choices` the handler returns
   `Ok(refused(format!("{error:#}")))` and writes nothing. A request with no flags still validates (the saved choices and the plan's hints must cover
   every contract).
   Then, inside the existing `update_stage` closure and under its stage lock, before `submit`, call
   `contracts_choices::save_admitted`: it reads `host_verify::store::load_current(&work_dir, stage_id)?`; when
   that holds a non-terminal `JobKind::Freeze` job with the same `head` and `tree_digest` as this request
   (`submit` would join it, whatever the choices) and `merged` differs from the pruned saved choices, it answers
   `Some("a freeze run for this tree is in progress under other flags; wait for its Host job result, then run the
   freeze again with the new flags")` and saves nothing, and the handler answers `Submission::Refused(message)`.
   The writer's CLI refuses this case before it relays (W1a); the daemon keeps the check because a saved change
   would reach the running job through `load_effective_stage`, break its `contracts_digest` binding and fail the
   job as a non-static failure, which files a dispute at once. Otherwise it writes `merged` with `save_choices`
   when it differs from the file as read (pruning alone can change it; a pruned entry never changes an
   effective spec, since `with_choice` lets the plan win and a stale id is never looked up), and `submit`
   follows, so a queued job always loads the choices it was requested with. A request for another head or tree
   supersedes the running job (`submit` cancels it), so changed flags are allowed there. `handle_freeze_contracts`
   stays under 50 lines and `contracts.rs` under 400.
2. `prepare` (the apply path) works on the effective stage: after `admit`, set `stage.contracts =
   choices::effective_contracts(work_dir, &stage)?`. Then `check_reports`, `check_changes`, `read_files` and
   `frozen_contracts` run on specs that all carry a test. `frozen_contracts` records `test:
   Some(<effective test>)` beside `file` and `cwd` (`resolve_adapter` and `resolve_run` already honour the
   writer's runner and cwd on the effective spec).
3. `freeze_with_reports` keeps its signature.

## Wiring the job (`src/host_verify/freeze.rs`)

- `run`: load the stage with `choices::load_effective_stage` (also `reloaded` after `run_contracts`, and the
  `load_stage` inside `apply_passed`), so `contracts_digest` covers the writer's choices and a job that started
  under other choices fails its binding check ("the stage's contract definitions changed") instead of freezing
  them. The writer's CLI and the handler's refusal above keep that check from firing for a join; it stays as the
  backstop.
- The module comment at the top of the file (about lines 11-14) says the refusal is filed by the tick step
  because "the relayed request told the writer to end its turn". That is false after W1a (the writer's session
  waits for the job and its application). Rewrite those lines to current behaviour: applying a failed job
  returns a static refusal to the waiting writer (`refuse_failed`), an interrupted or daemon-stopped run is
  returned the same way (`return_infrastructure`), and any other refusal is recorded for the tick to file as a
  `freeze-refusal` dispute. Rewrite the comment on the `Cancelled` arm of `apply_freeze_result` ("The shutdown's
  cancel leaves no successor job, so no one else would act on the writer's ended turn") the same way: the
  waiting writer reads `Host job: stopped` and runs the freeze again.
- After `run_contracts`, compute `let written = crate::verify::contracts::freeze::written_paths(&contract_site,
  &site.git)?;` and `let compiles = reports.iter().all(|r| r.outcome != BUILD_FAILED && r.outcome != UNVERIFIED);`
  (both constants are in `verify::contracts::freeze`), and call `format_outcome(&contract_site, &site.git, confinement,
  &written, compiles)` (W2 changes that function and `written_paths`; it lists the untracked paths before the
  static gate and removes the ones the gate created, so a lint cache cannot move the tree digest that the
  binding check below compares. Keep the order: `format_outcome`, then `reloaded`, then the second `capture`).
- `tests_freeze.rs` calls `handle_freeze_contracts` once; add `&ContractChoices::new()`. Its
  `stage.contracts[0].test = command.to_string()` (about line 57) wraps the value in `Some(..)`. Those are its
  only changes; every new `host_verify` test goes in `tests_freeze_static.rs`, wired from `host_verify/freeze.rs`
  as `#[cfg(test)] #[path = "tests_freeze_static.rs"] mod static_tests;` (it reaches the `tests_freeze` fixture as
  `crate::host_verify::tests_freeze::..`, as `tests_freeze_refusal.rs` does through `super::`).

## A static refusal returns to the writer (Decision 2)

Grounded today: a failed freeze job (outcome `format` red) reaches `apply_freeze_result`, whose `JobState::Failed`
arm calls `refuse`, which writes `contracts/<stage>/refused.json` with `record_job_refusal`; the next tick's
`dispute_refused_freezes` files it as a `freeze-refusal` dispute at once and a fresh writer replaces the session.
W1a makes the writer's session follow the job, so `Host job: failed` plus the failed outcome's summary (the
failing output, as W2 writes it) reaches the live writer. You decide which refusals the writer may fix itself.

- A job is a static failure when its saved result (`store::load_result`) has no `error`, a failed outcome labelled
  `format` and every other outcome passed (the `contract:<id>` outcomes are red-as-required, so they pass). W2's
  gate fails `format` only for a pure check whose output names a file the writer wrote, or a formatter check.
  Add `pub(super) fn static_failure(work_dir: &Path, record: &JobRecord) -> bool` to `freeze_refuse.rs`.
- The count lives in `refused.json`, the record that already holds a session's refusal: `DaemonRefusal` gains
  `#[serde(default, skip_serializing_if = "is_zero")] pub returned: u32`, the number of static refusals of this
  writer session returned to it and not yet filed (0 = a standing refusal the tick files at once). Both
  `DaemonRefusal` literals in `refusal.rs` gain `returned: 0`. No new file, no configuration key.
- In `refusal/static_return.rs` (declared `mod static_return;` in `refusal.rs`, re-exported with `pub use`):

  ```rust
  pub const STATIC_RETURN_LIMIT: u32 = 3;
  pub enum StaticRefusal { Returned { count: u32 }, Filed, Superseded }   // Debug, Clone, PartialEq, Eq
  pub fn record_static_refusal(work_dir: &Path, stage_id: &str, session_id: &str, origin: JobRefusalOrigin,
      problems: &[String]) -> Result<StaticRefusal>;
  ```

  It runs under the same `locked_dir_update` lock and follows `record_job_refusal`'s ordering rules (the same
  `origin` again is idempotent and reports the record's state without counting again; an older
  `submission_order` is `Superseded`). The count is `prior + 1`, where `prior` is the `returned` of the existing
  record of the SAME session (0 when none, or another session's). Below `STATIC_RETURN_LIMIT` it writes the
  record with `returned = count` and answers `Returned { count }`; at the limit (the third refusal of the
  session) it writes the record with `returned = 0` and answers `Filed`, which the tick then files as today.
- `freeze_refuse.rs` gains `pub(super) fn refuse_failed(work_dir, record) -> Result<Applied>`: with
  `static_failure` it calls `record_static_refusal` and answers `Applied::FreezeRefused(message)` (a `Returned`
  message starts `returned to the contract writer (<count> of 3): `), otherwise it is `refuse` as today.
  Move `refuse` and `refuse_infrastructure` from `freeze.rs` into `freeze_refuse.rs` as `pub(super)` (they keep
  their bodies) so `freeze.rs` shrinks, and change the `JobState::Failed` arm of `apply_freeze_result` to
  `refuse_failed`.
- An interrupted or daemon-stopped run is returned the same way, because the waiting writer is told to rerun
  (W1a keeps the wait's `Host job: stopped ... Run the same command again` for these states). `freeze_refuse.rs`
  gains `pub(super) fn return_infrastructure(work_dir, record, message) -> Result<Applied>`: it calls
  `record_static_refusal` with the problems `refuse_infrastructure` would record (first problem prefixed
  `INFRASTRUCTURE_PREFIX`, so a filed one still tells the adjudicator to have the freeze rerun) and answers like
  `refuse_failed` (`Returned`, `Filed`, or `Ignored` for `Superseded`). The `JobState::Interrupted` arm and the
  `JobState::Cancelled` arm whose detail is `DAEMON_STOPPED` call it; they share the count and the limit with the
  static refusals of the same session. The infrastructure refusals of `apply_passed` (reports that cannot be read,
  a stage that cannot be checked against its run) keep `refuse_infrastructure`. Every other arm, and every
  refusal that is neither a static failure nor one of these two runs, stays an immediate filing.
  The message is what the waiting writer reads in the job's `applied` marker (`describe` in `apply.rs` prefixes
  it `refused the contract run: `, and W1a's wait branches on it): a returned one starts exactly
  `returned to the contract writer (<count> of 3): fix what the output above names and run loom stage contracts
  freeze <stage-id> again`, and a filed one (the third, or any other failed job) starts `filed as a
  freeze-refusal dispute: ` so the writer can tell the two apart.
- `freeze_refusal.rs` `dispute_refused_freeze`: after the `declined` check, `if refusal.returned > 0 &&
  !self.writer_is_quiet(&stage.id, &session_id, SystemTime::from(refusal.at), now) { return Ok(()); }`. Write the
  condition with the literal text `.returned > 0` (the stage's wiring pins it; no helper, no `!= 0`). A returned
  refusal waits while the writer is working (its refusal and heartbeat younger than `REFUSAL_QUIET_SECS`, or a
  subagent running); once the writer has gone quiet it is filed as an ordinary dispute, so a writer that gives up
  after a returned refusal does not stall the stage. The words, identical in every brief and the doctrine: a
  static refusal is returned to the writer; it is filed as a `freeze-refusal` dispute on the third static
  refusal in the session, or when the writer stays quiet 5 minutes after a refusal. Update the module doc comment (bullet list) to say so.
- Traps: `refused.json` is replaced by the next job's refusal of the same session (that is how the count
  carries); a refusal for another session is dropped as stale by the tick, so the count never crosses sessions.
  `Applied::FreezeRefused` still means the stage stays Executing. A returned refusal is not a dispute: the
  stage's `freeze-refusal` budget is untouched.

## Completion and execution paths

- `completion.rs` `check_with`: the loop becomes `for contract in &stage.contracts { let effective =
  store::effective_contract(&record, contract); let recorded = store::recorded_cwd(&record, contract); ...
  run_contract(&effective, recorded, ..) }`. Replace each `contract.test` read with the effective spec's
  (`required_test()`); a spec with no test anywhere is a `cannot be run` failure of its own contract.
- `run_dir.rs` `resolve_run`: `let test = contract.required_test()?;` after the id binding, and use `test` where
  `contract.test` was used (`rust_scope`, the `command` closure, the adapterless command).
- `collection/mod.rs`: `scaffold()` and `chain_problems()` skip a contract whose `test` is `None` (it contributes
  no scaffold pair and no chain check; the daemon's check runs on effective specs).

## Traps

- `admit` loads the plain stage on purpose: the first request arrives before any choice is saved, when contracts
  without a hint have no test.
- A freeze job already running for the same tree is joined by a second request (`submit` joins on head, tree and
  kind), and a changed `choices.json` would break the running job's binding. The writer's CLI refuses such a
  rerun before it relays (W1a); `save_admitted` refuses changed choices while that job is non-terminal for a
  request that reaches the daemon anyway (Wiring the daemon, item 1). The digest check stays as the backstop.
- `store::effective_contract` fills `runner` from the record's `adapter`; at completion `resolve_adapter` then
  uses the frozen adapter instead of re-detecting it from the implementer's tree. Keep the `is_rust` special case
  in `completion.rs` consistent with that (it already prefers the record).
- Nothing removes `choices.json`; it stays beside `freeze.json`. Admission rewrites it pruned.
- `ContractChoices` values are agent text: never put one in a log at `info`, never build a shell string from
  one, and quote it with `run_output_line` or `single_line` where it enters a refusal message.
- The freeze job's deadline (`job_deadline_secs`) already sums every acceptance timeout, so a lint run by W2's
  gate fits.

## Tests

Static refusals. `refusal/static_return_tests.rs`: `record_static_refusal` returns `Returned { count: 1 }`,
`Returned { count: 2 }`, then `Filed` for three jobs of one session with `returned` 1, 2, 0 on the record; the same
origin twice counts once; an older order is `Superseded`; another session starts again at 1.
`host_verify/tests_freeze_static.rs`: `apply_freeze_result` on a failed job with a failed `format` and passing
contracts returns `FreezeRefused` with the returned prefix and records `returned = 1`; a failed `contract:<id>`
outcome, a result with an `error`, and a missing result record `returned = 0`; an `Interrupted` job and a
`Cancelled` job whose detail is `DAEMON_STOPPED` record `returned = 1` with a first problem starting
`INFRASTRUCTURE_PREFIX`, and a third such run of the session (static or not) records `returned = 0`; the messages
carry the returned prefix for counts 1 and 2 and the `filed as a freeze-refusal dispute: ` prefix at the third
and for a failed contract outcome; a job that started under other choices still fails its binding check
(backstop). `event_handler/freeze_refusal_returned_tests.rs` (wired from `freeze_refusal.rs` as `#[cfg(test)]
#[path = "freeze_refusal_returned_tests.rs"] mod returned_tests;`, reusing the fixture of
`freeze_refusal_tests.rs`, which stays at 400 lines): the tick leaves a `returned > 0` record alone while the writer
is not quiet, files it once quiet, and files a `returned = 0` record at once.

`choices_tests.rs` (git fixtures with a committed `pkg/Cargo.toml`): parse (first `=`, duplicate, 65 ids,
control character, 513-byte test, more than 16 KiB of values), merge, save/load round trip and a malformed file,
`prune_choices` (an id the plan no longer has is dropped; a saved `runner` for a contract whose plan now sets
`runner` is dropped and its saved `test` kept), effective/lenient overlays, and every `validate_choices` refusal
above (a test of `--help` or `-Zunstable-options` included) plus the accepting case; the marker rule three ways
(committed marker accepted, marker added after `base` refused, symlinked marker refused). `overlay_with_saved`:
saved test and cwd for `a` survive an update that sets only a runner for `b`; a spec with no hint and no saved test
stays `None`.

Handler (`stage_service/contracts_choices_tests.rs`, wired from `contracts_choices.rs`): a freeze request whose
choices fail validation returns `Response::Error` and leaves no `choices.json`; one that passes saves it; a saved
choice for a contract the plan no longer has refuses nothing and is gone from the saved file; the applied freeze
records `test` in `freeze.json`. With a non-terminal `Freeze` job for head H and tree T on the stage (save a
`JobRecord` through `host_verify::store`), a request for H and T whose choices differ from the saved ones returns
`Response::Error` and leaves `choices.json` byte for byte as it was; the same request with equal choices joins and
answers `Response::Ok`; a request for another tree with changed choices saves them and supersedes.

## Done

Your one check once the crate compiles: `cargo test --lib verify::contracts::choices`.
