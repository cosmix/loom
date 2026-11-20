# D2: `loom target status|accept`, the `init --clean` refusal, loom's own plan commits

Stage `guard-integration`, tier sonnet, in parallel with D1 and D3 (disjoint files). Read
`../common.md` first. `loom::git::target_guard` exists; read `loom/src/git/target_guard/mod.rs`.

You own `loom/src/cli/types.rs`, `loom/src/cli/types_target.rs` (new),
`loom/src/cli/dispatch.rs`, `loom/src/commands/mod.rs`, `loom/src/commands/target/mod.rs`,
`loom/src/commands/target/status.rs`, `loom/src/commands/target/accept.rs`,
`loom/src/commands/target/tests.rs` (all new except the first, third and fourth),
`loom/src/commands/init/execute.rs`, `loom/src/commands/clean/mod.rs`,
`loom/src/commands/run/plan_inputs.rs`, its test module `loom/src/commands/run/plan_inputs/tests.rs`
and `loom/src/fs/plan_lifecycle/commit.rs`.

Frozen, read-only: `loom/tests/target_cli_contracts.rs`. Read it first. It `#[path]`-includes
`tests/integration/helpers.rs`, whose `create_worktree` call D1 updates; a compile error there
is D1's.

Your one check, run once at the end, from `loom/`: `cargo test --test target_cli_contracts`.
The library's own test build does not compile until D1 adds `Orchestrator::check_target_guard`
(the frozen daemon contracts call it), so the main agent runs your unit tests after D1 returns.

Sibling plans: `PLAN-web-host-graft-followthrough` (stage `implement-host-and-context`) splits
`dispatch` into `cli/dispatch_commands.rs` and removes its ledger line, and `PLAN-model-router-hooks`
(stage `router-core`) adds `Commands::Router`. The `dispatch` facts below were read at
`066e7667`; if either plan merged first, re-read `cli/dispatch*.rs`, `cli/types.rs` and the
ledger, route `Commands::Target` wherever `Commands::Worktree` routes then, and keep every file
at or under 400 lines.

## 1. CLI

- `cli/types_target.rs`: `#[derive(Subcommand)] pub enum TargetCommands { Status, Accept {
  #[arg(long)] to: String } }`, doc comments as `--help` text: "Show the target branch guard:
  the accepted tip, the current tip, and any hold" / "Accept the current target tip after
  reviewing a move loom did not make; --to must name the current tip". Follow the layout of an
  existing `cli/types_*.rs` file. Declare it from `cli/types.rs` the way `types_project.rs` is
  (`#[path = "types_target.rs"] mod target; pub use target::TargetCommands;`, about lines
  18-20), so `cli/mod.rs` (owned by a sibling plan) needs no change.
- `cli/types.rs` (381 lines; stays at or under 400): `Commands::Target { #[command(subcommand)]
  command: TargetCommands }` with a doc comment, placed beside the operator commands
  (`Worktree`, `Sessions`).
- `cli/dispatch.rs`: `dispatch` is at its ledgered 86 lines and its comment says the top-level
  match is at its line ceiling. Measured: adding `| Commands::Target { .. }` to the
  `dispatch_tools` arm (about line 323) pushes it past 100 columns, rustfmt wraps it, and
  `dispatch` grows to 88, failing `cargo test --test maintainability`. Instead give
  `Commands::Target` its own one-line arm calling a new `dispatch_target(command)` helper, and
  make room by moving another arm's multi-line body into a helper (for example the
  `Commands::Run { .. }` arm, about line 298, as `cmd @ Commands::Run { .. } =>
  dispatch_run(cmd),`), so `dispatch` does not grow; if it shrinks, report the exact count (the
  main agent lowers the ledger line). `Commands` is matched exhaustively only in `dispatch.rs`.
- `commands/mod.rs`: `pub mod target;`.

## 2. `loom target status` (`commands/target/status.rs`)

Work dir: `crate::commands::common::work_dir_path()`; repo root: the work dir's
`WorkDir::main_project_root()` (as `commands/stage/merge.rs` does); target:
`crate::fs::resolve_target_branch_from_config(&work_dir, &repo_root)`. Read-only: it never
writes the record. Print:

Build the text in `pub(crate) fn report(repo_root: &Path, work_dir: &Path, target: &str) ->
Result<String>` and print it, so tests assert exact lines (the CLI contract
`target_status_prints_review_and_restore_commands` checks `Attestation: off (`, the accept
command and both restore commands with full ids):

- `Target: <branch>`; `Accepted: <id or "not recorded yet (the daemon records it at start)">`;
  `Current: <id>`; `Attestation: on` or `Attestation: off (<reason>)` from `attestation_mode`;
  when it is off and `target_guard::attestation_latched(work_dir, target)` is `Ok(true)`, the
  same line continues `; this run recorded it on, so every move without a ledger line holds
  until loom target accept` (plan section 3.2).
- State: `in sync`; or, with a recorded hold, `HELD since <since>` and one line per reason
  (`HoldReason`'s `Display`), then `Review: git log --oneline <accepted>..<observed>` and
  `git diff --stat <accepted> <observed>`, then `Accept: <accept_command>`, then
  `Restore:` and each line of `restore_commands`; or, moved with no recorded hold, `moved
  <a12>..<t12>, not yet evaluated` followed by what `pending_hold` reports (a hold it would
  record, or `would be accepted`).
- Exit 0 in every case (it reports).

## 3. `loom target accept --to <commit>` (`commands/target/accept.rs`)

1. When `LOOM_SESSION_ID` is set and non-empty, bail with: `loom target accept is the
   operator's review of a move loom did not make, and this is loom session <id>; a stage agent
   cannot accept it`. A guard rail only: the real boundary is that no session can write
   `.loom/`. Do not reuse `admin_proof::refuse_operator_inside_a_session`: its message sends the
   agent to `loom stage dispute-criteria`, which is wrong here.
2. `target_guard::accept(&repo_root, &work_dir, &target, &to)?` (it takes the merge lock and
   refuses a `--to` that is not the current tip).
3. Print `Accepted <target> at <to12> (was <from12>).` then `Attestation: on` or `Attestation:
   off (<reason>)` from `attestation_mode`: the accept also records that mode as the run's
   attestation policy (plan section 3.2), so the operator sees the policy they just set.
4. When the target is checked out in the main checkout (`git symbolic-ref -q HEAD` there is
   `refs/heads/<target>`) and `git diff --cached --quiet <to>` fails there (the checkout's index
   is not at the accepted tip), print: `Your checkout of <target> still holds files from before
   the move. Bring it along (keeps your local edits): git read-tree -m -u <from> <to>`.
   Never run it.

## 4. `loom init --clean` and `loom clean --state|--all` refuse while the target is held

One shared helper in `commands/target/mod.rs`: `pub(crate) fn refuse_unreviewed_move(repo_root:
&Path) -> Result<()>`. It resolves the state directory the way its callers do
(`resolve_state_dir`), and when `target-guard.json` exists, reads only the record, never
`config.toml` (a malformed config must not block the recovery path): for every target key of
the record (`guarded_refs(work_dir)` with `refs/heads/` stripped), refuse when
`target_guard::recorded_hold` is `Some` or `target_guard::pending_hold` is `Some`:
`bail!("the target branch {t} has a move loom did not accept ({reason summary}); review it with
'loom target status', then accept or restore it before deleting loom's state")`. A record that
cannot be read refuses and names the file. A missing record proceeds as today.

Call sites, each BEFORE anything is stopped, reaped or deleted:

- `commands/init/execute.rs::stop_daemon_and_prune` (about line 205), first statement when
  `clean`: `execute` stops the daemon and reaps live sessions (`IncludeLiveBeforeClean`) before
  `cleanup_work_directory` runs, so a refusal there would come after the destructive steps.
  `stop_daemon_and_prune` is not ledgered; `execute` must not grow.
- `commands/clean/mod.rs::execute` (about line 46), when `all || state`, before
  `stop_daemon_before_destroying_state` (`clean_state_directory` removes `.loom/work` with
  `remove_dir_all`; `loom clean` then `loom init` is the normal flow between plans, and the
  `init` refusal text itself recommends it).

Deleting the record otherwise would make the next run trust an unreviewed move.

## 5. Loom's own commits on the main checkout

Loom's git runs with hooks disabled, so the hook never attests loom's own two plan commits.
Attest them yourself when, and only when, every path the commit touches is one of the plan's
own files or under the knowledge prefix (the `allow` value `target_guard` writes,
`doc/loom/knowledge/`). NOT all of `doc/plans/`: `commit_post_completion_changes` runs `git add
-u` over the whole checkout and commits the whole index (which a session can also write), so a
session's edit of another pending plan (its YAML sandbox, provision commands) would be swept in
and attested onto the target; `doc/plans/` is not a control path.

- One helper in `fs/plan_lifecycle/commit.rs`: `pub(crate) fn attest_plan_commit(work_dir:
  &Path, repo_root: &Path, allowed: &[&Path]) -> Result<()>` (commands call into fs, never the
  reverse). After a successful commit read the new `HEAD` (`after`) and use `after^` as `from`
  (a `HEAD` read before the commit can be stale), and the branch
  (`crate::git::branch::current_branch`). When `refs/heads/<branch>` is among
  `target_guard::guarded_refs(work_dir)` and every path of `git diff --name-only -z --no-renames
  <after>^ <after>` is in `allowed` or starts with `doc/loom/knowledge/`, call
  `target_guard::append_attestation(work_dir, &branch_ref, &from, &after)`. Otherwise leave it
  unattested: the next evaluation holds it and the operator reviews it. A failed attestation
  write is a warning, not an error.
- `fs/plan_lifecycle/commit.rs::commit_post_completion_changes` (about line 15): `allowed` is the
  old plan path, the new plan path and `doc/plans/REVIEW-<plan id>.md` (written by
  `fs/plan_review/mod.rs`).
- `commands/run/plan_inputs.rs::commit_rename` (about line 107, `git commit --only` of the plan
  rename): `allowed` is the old and new plan path. In practice no record exists yet when it runs
  (the daemon records the target after `loom run` renames the plan), so this attests nothing
  today; wire it anyway so a restart that renames later is covered.
- The work dir: `plan_inputs.rs` and `commit.rs` already receive or resolve it (`&WorkDir`);
  read their signatures and thread it through the smallest change.

## 6. Tests (`commands/target/tests.rs`, plus cases in the existing test modules of the files

you own)

- `report` on a repo with no record, with a clear record, with a hold (both restore commands
  when the target is checked out, one when it is not; assert the exact lines), and with a move
  not yet evaluated, and with a latched record while `attestation_mode` is `Off` (the
  `Attestation: off (<reason>); this run recorded it on, ...` line);
- `accept` refuses with `LOOM_SESSION_ID` set; prints the read-tree note when the checkout's
  index is behind (build the note in a `checkout_note(..) -> Result<Option<String>>` and assert
  it). Every in-process accept test is `#[serial]` and calls
  `std::env::remove_var("LOOM_SESSION_ID")` first (or sets it, for the refusal test): the
  stage's acceptance runs inside a loom session, which exports it;
- `refuse_unreviewed_move` refuses with a recorded hold, refuses when `pending_hold` reports
  one, refuses on an unreadable record, proceeds when clear or unrecorded, and ignores a
  malformed `config.toml`; `loom clean --state` and `init --clean` call it before anything
  destructive (a test per call site that asserts the state directory still exists after the
  refusal);
- `commit.rs` gets its own `#[cfg(test)] mod tests` (its existing tests live in
  `plan_lifecycle_tests.rs`, which is not yours): a plan-done commit touching only the plan
  files and `doc/loom/knowledge/` writes `attest <after^> <after> refs/heads/main` and the next
  `target_guard::check` is `Clear`; one that also carries `src/x.rs` is not attested; one that
  also carries an edit to another `doc/plans/PLAN-*.md` is not attested;
- `plan_inputs/tests.rs` (the existing test module of `plan_inputs.rs`): with a record for `main` (`target_guard::check`
  first), `commit_rename` of a plan file writes `attest <after^> <after> refs/heads/main` and the
  next `target_guard::check` is `Clear`; without a record it writes no ledger line. This is the
  behavioural proof behind the `attest_plan_commit(` wiring entry, which only proves the call
  is present.

## Traps

- `status` must not take the merge lock or write anything; `pending_hold` writes nothing.
- `status` and `accept` resolve the target with `resolve_target_branch_from_config`, which
  propagates a malformed config: surface it, do not default. The clean refusal reads only the
  record (section 4).
