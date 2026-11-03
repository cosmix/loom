# live-checks R1: `loom check --live`, the live job and the receipt

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first ("Receipts and
completion", "Entry verdict"). Frozen contracts: `tests/live_checks_contracts.rs` (with
`tests/live_checks_contracts/**`) and
`src/orchestrator/core/inbox_drain/live_check_contract_tests.rs`; read them first, never edit
them. The stage description's PUBLIC SURFACE is authoritative. The orchestrator's foundation
step already declared `pub mod completion; pub mod receipt; pub mod stage_run;` in
`src/live/mod.rs`, `pub mod check;` in `src/commands/live/mod.rs`, and created each file with one
`//!` line. R2 writes `src/live/completion.rs` against your receipt types in parallel.

## Files (yours alone)

`src/live/receipt.rs`, `src/live/stage_run.rs`, `src/live/tests_receipt.rs` (declare it from
`receipt.rs` as `#[cfg(test)] #[path = "tests_receipt.rs"] mod tests;`),
`src/commands/live/check.rs`, `src/host_verify/mod.rs`, `src/host_verify/child.rs`,
`src/host_verify/child_live.rs` (new), `src/host_verify/submit.rs`, `src/host_verify/apply.rs`,
`src/relay/payload.rs`, `src/orchestrator/core/inbox_drain/check.rs`,
`src/orchestrator/core/inbox_drain/complete.rs`, `src/orchestrator/core/inbox_drain/commit.rs`,
`src/relay/matrix_host.rs`, `src/relay/tests_matrix.rs`, `src/commands/verify.rs`,
`src/commands/verify_relay.rs`, `src/cli/dispatch.rs`. Also frozen for you:
`src/orchestrator/core/inbox_drain/live_check_contract_tests.rs`'s
`resolver_may_request_only_its_own_live_run`.

## The receipt (`src/live/receipt.rs`)

Types and functions exactly as the PUBLIC SURFACE gives them; serde with `deny_unknown_fields`,
`ReceiptOutcome` tagged `outcome` (`passed`, `failed`, `unavailable`).

- `current_binding(checkout)`: `crate::verify::criteria::content_digest(checkout)` gives
  `Content`; when it is `None`, `git status --porcelain=v1 --untracked-files=all` through the
  checkout's pinned git (`crate::verify::review::fingerprint::checkout_git`) must be empty, and
  then `git rev-parse HEAD^{tree}` gives `CommitTree`; a dirty tree gives `Ok(None)`.
- `save(work_dir, receipt)`: validate the stage id (`crate::validation::validate_id`), write
  `verification/<stage>/live-receipt.json` with no-follow opens below the state root
  (`crate::fs::safe_fs::safe_open_dirfd` and `safe_write_with_mode_in_workdir`, the pattern
  `plan_session/launch.rs` `write_script` uses), mode 0644, creating the stage directory when
  missing. `load` reads it with a no-follow open, at most 1 MiB; absent is `Ok(None)`.

## The run (`src/live/stage_run.rs`)

`run_stage_entries_with(work_dir, stage, checkout, job_id, runner)`:

1. `current_binding(checkout)`; `Ok(None)` is an error: "the tree holds a symlink, a
   submodule or a flagged index entry, so loom cannot fingerprint its content while it has
   uncommitted changes: commit, then run loom check --live again".
2. `crate::live::credential::load(work_dir)`. `None` (or a refused file) gives every entry
   `Unavailable { Credential, "no live credential is configured: run loom live credential set
   --kind api-key|oauth-token from the repository root" }` (a refused file names its rule) and
   never calls `runner`.
3. Otherwise `runner(index, entry, &credential)` for each entry in order; an `Err` from the
   runner is that entry's `Failed { reason: "the live run could not start: <error>" }` and the
   next entry still runs.
4. `current_binding(checkout)` again; a different value leaves `binding: None` (the tree
   changed during the run, so the receipt never matches).
5. Build the receipt (each entry's `definition_hash` from `live_entry_hash(entry)`, platform
   `<std::env::consts::OS>-<std::env::consts::ARCH>`, `crate::version::VERSION`), `save` it,
   return it.

`pub(crate) fn run_stage_entries(work_dir, stage, paths, job_id, record)` (crate-internal:
`StageExecutionPaths` is `pub(crate)`, and a `pub` signature naming it fails clippy's
`private_interfaces`) calls it with the real runner: one
`run_entry` per entry with `loom_bin` from `std::env::current_exe()`, `tree` =
`paths.worktree_root` (else the acceptance dir), `working_dir` = `paths.acceptance_dir`,
`hosts` = `entry.hosts`, `timeout` = `entry.timeout_secs`, `upstream` = `Upstream::public()`,
`claude` = `profile::find_claude().ok()`, `zero_test_guard` = `stage.plan_version == 2`; and
hands each `EntryReport` to `record` (the caller's sink) before turning it into a
`ReceiptEntry`.

## The job

- `host_verify/mod.rs`: `JobKind::Live` (`live`), `name()`; `mod child_live;`; the
  `CriterionOutcome::label` doc lists `live-<n>`.
- `host_verify/child.rs` `run_job`: `JobKind::Live => child_live::run_live_job(work_dir,
  &record),`. The file is exactly 400 lines: take the line back by condensing a doc comment in
  the same file; it must stay at or under 400.
- `host_verify/child_live.rs`: `pub(super) fn run_live_job(work_dir: &Path, record:
  &JobRecord) -> JobResult`: load the stage, `JobTree::resolve`, refuse a tree that differs
  from the request (as `check` does), then `run_stage_entries` with a sink that records each
  entry as `CriterionOutcome { label: "live-<n>", passed: outcome is Passed, summary: "<✓|✗|?>
  live #<n> `<command>`: <outcome>", log }` through `Progress::record(outcome,
  Some(stdout+stderr))`; then `tree.differences(record)` and `finish(record, outcomes, checked,
  None)`.
- `submit.rs`: a `Live` request is refused behind a running `Complete` or `Freeze` job with the
  same message a check gets; `covers` joins a `Live` request only to a `Live` job; a `Complete`
  never covers a `Live` request.
- `apply.rs`: `JobKind::Live => Applied::Ignored("a live run changes no stage state; its receipt
  is the record")`.
- `relay/payload.rs` `HostJobRequest`: `#[serde(default, skip_serializing_if =
  "std::ops::Not::not")] pub live: bool,` with a doc line. The file is 379 lines.
- `inbox_drain/check.rs`: `pub(super) fn job_kind_for(request: &HostJobRequest, stage: &Stage)
  -> Result<JobKind, String>` as the PUBLIC SURFACE gives it; `apply_check` submits with that
  kind. `inbox_drain/complete.rs`: a completion request with `live: true` is refused ("a
  completion never runs live entries; run loom check --live <stage>").

## The command

- `commands/verify.rs` `CheckArgs`: `#[arg(long, conflicts_with = "criterion")] pub live:
  bool` with doc "Run the stage's live entries in the live profile and record a receipt; never
  part of `loom stage complete`". Do not touch `execute` (an exact ledger row).
- `cli/dispatch.rs` `run_check`: `if args.live { ... return crate::commands::live::check::execute(&stage_id, suggest); }`
  before the cache toggle; `--suggest` with `--live` is an error ("--suggest does not apply to
  --live").
- `commands/live/check.rs` `execute(stage_id, suggest)`: from a session
  `crate::commands::verify_relay::run_live(stage_id, &work_dir)` relays and exits (it never
  returns there); in the operator's shell it returns and the local run follows: load the stage,
  refuse one with no live entries, resolve its paths
  (`acceptance_runner::resolve_stage_execution_paths`), `run_stage_entries` with a sink that
  prints each outcome, then print the receipt summary and exit 1 unless every entry passed.
- `verify_relay.rs`: `pub(crate) fn run_live(stage_id, work_dir) -> Result<()>` shares
  `request_or_wait` through a `kind: JobKind` parameter (`WaitSite.kind`, and the payload's
  `live: kind == JobKind::Live`); `run` keeps its signature and passes `JobKind::Check`.

## A merge resolver's live run (plan Decision 16)

A resolver of a stage with required live entries runs `loom check --live <stage>` after
committing the merge (R2 writes the signal text). Today the relay refuses every `check` from a
Merge session three times over: the matrix row (`relay/matrix_host.rs`, Merge x Check is
`Refuse`, which `RelayContext::check` and `inbox_drain/apply.rs` `admit` both read), and
`check.rs` `admit_session`, whose `require_owner` wants `stage.session` to be the record (a
resolver never owns its stage). Admit it for the resolver's own stage only:

- `relay/matrix_host.rs`: Merge x Check becomes `MatrixVerdict::Apply`; rewrite the doc
  comment: a Merge session's check is admitted only as the live run of the stage it resolves,
  which `inbox_drain/check.rs` enforces. `relay/tests_matrix.rs` `CHECK_COLUMN`: the Merge row
  becomes `Apply`; rename `check_is_applied_for_stage_knowledge_and_adjudication_only` to
  `check_is_applied_for_stage_knowledge_adjudication_and_merge` and fix its doc line. The changed
  row is data, not an assertion line, and the `#[test]` count is unchanged, so no integrity
  event arises.
- `inbox_drain/commit.rs`: move the resolver rule at the top of `prepare_merge` (record
  `Running`; `merge_source_branch` equal to `branch_name_for_stage(stage_id)`; stage
  `MergeConflict` or `MergeBlocked`; the same three refusal texts) into
  `pub(super) fn require_resolver(work_dir: &Path, stage_id: &str, record: &Session) ->
  Result<Stage, String>`, and call it from `prepare_merge`. Commit behaviour and
  `commit_tests*.rs` stay unchanged.
- `inbox_drain/check.rs` `admit_session` takes the request. A `SessionType::Merge` record:
  refuse when `request.live` is false ("a merge resolver requests only loom check --live
  <stage>; loom stage merge --resolved reruns every completion gate"), else
  `super::commit::require_resolver(host.work_dir(), stage_id, record)`. Adjudication and the
  other kinds keep their rules. `validate` and `job_kind_for` then run unchanged:
  `checkout_head` reads the stage worktree, which is the tree the resolver works in.
- Nothing else changes on the client side. With the matrix row applied, `RelayContext::check`
  passes for a resolver: its checkout boundary is the project root (`relay/emit/cwd.rs`), its
  `LOOM_STAGE_ID` is its stage (the wrapper exports it for every kind), and the daemon's
  `session_pass.rs` refuses an entry whose stage is not the record's. A resolver's plain
  `loom check` is refused by the daemon, and `host_verify::wait` reports that refusal.

## Traps

- `host_verify/child.rs` 400, `relay/payload.rs` 379, `host_verify/runner.rs` 387,
  `inbox_drain/commit.rs` 310: count lines.
- A `Live` job's deadline is `job_deadline_secs(stage)`, which the live-profile stage extended;
  change nothing there.
- Never cache a live run; never run acceptance in a live job.
- `cargo fmt`; your one check: `cargo test --lib host_verify:: --no-run`.

## Report

Files changed, the final line counts of `child.rs` and `payload.rs`, and every exhaustive
`match` on `JobKind` the compiler named outside your files.
