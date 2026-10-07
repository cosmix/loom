# Target Guard

> Accepted-tip record, hook, holds

## Target Guard

A stage session can move the target branch (normally `main`) itself, because Claude Code grants a linked-worktree session write access to the whole git common directory ([G2](../concerns/agent-rule-bending-hardening.md)). The guard makes such a move visible and holds every merge until the operator reviews it. It never restores the ref itself. The target is whatever the plan config's `[plan] base_branch` names; nothing is specific to this repository.

Code: `loom/src/git/target_guard/` (`mod.rs` verdicts and `accept`, `record.rs` record and ledger I/O, `evaluate.rs` the move evaluation, `attestation.rs` the hook mode, `text.rs` operator text), `loom/src/commands/target/status.rs` and `loom/src/commands/target/accept.rs`, `loom/src/orchestrator/core/target_hold.rs` (the daemon side), `loom-hooks/git-reference-transaction-hook.sh`.

## State Files (`.loom/work/`)

| File | Writer | Content |
| --- | --- | --- |
| `target-guard.json` (`RECORD_FILE`) | host loom only | per target key (short branch name, a leading `refs/heads/` is stripped by `target_key`): `accepted` tip, current `hold`, `attestation` latch |
| `target-guard.refs` (`REFS_FILE`) | rewritten with every record write | `ref <refs/heads/..>` lines naming the refs the hook guards, plus an `allow <prefix>` line (the knowledge prefix) |
| `target-guard.ledger` (`LEDGER_FILE`) | the hook | `attest <from> <to> <ref>` when git prepares a move, `abort ...` when git aborts it |

The files last one plan run. Record writers: `merge_stage` after its own advance (`record_advance`), the evaluation, and `loom target accept`. The daemon creates a missing entry from the current tip at startup (trust on first use, `trust_on_first_use`); `merge_stage` does the same for an operator CLI merge. Loom's own plan commits (`fs/plan_lifecycle/commit.rs`, `attest_plan_commit`) are attested only when every path of the commit's own diff is the plan file's old or new path, `doc/plans/REVIEW-<plan id>.md` or under `doc/loom/knowledge/`; a wider rule would launder a session's edit of another pending plan.

## Attestation (the Hook)

`loom-hooks/git-reference-transaction-hook.sh` is a git-side script installed as `.git/hooks/reference-transaction` by `loom init`, `loom repair` (check and install; a foreign hook is reported, never overwritten) and daemon startup (`start_target_guard`). It is not in `LOOM_HOOKS`; the installer is `git/hooks/reference_transaction.rs` (`install_reference_transaction_hook`, marker `HOOK_MARKER`). It rewrites the file by staged write plus rename and sets mode 0755, because git silently skips a non-executable hook. Per update of a guarded ref:

- `prepared`: it appends `attest <current> <new> <ref>` to the ledger (it reads the current value itself, since git passes the all-zero id when the caller gave none). When the append fails (a sandboxed session cannot write `.loom/`) and `LOOM_SESSION_ID` is set, it refuses the update unless the move fast-forwards and changes only paths under the `allow` prefix (knowledge stages commit to the target from their sandbox). With no session id and no write access it allows the move without a line.
- `aborted`: it appends a matching `abort` line.
- It skips silently when it cannot read `target-guard.refs`. A future `denyRead` of `.loom/work` for sessions would turn the refusal off; only `tests_confinement_target` under srt would notice.
- It exports `GIT_NO_REPLACE_OBJECTS=1` and `GIT_GRAFT_FILE=/dev/null/loom-no-grafts`. `git`, `sed` and `dirname` resolve through the session's `PATH`; a fake one only leaves a ledger gap, which the daemon holds.

A session that skips the hook (`-c core.hooksPath=`, a raw ref write) leaves no line, so the ledger is a trust input only for host-side moves. `attestation_mode` is `Active` only when loom's hook is installed and executable (`access X_OK`), `core.hooksPath` is unset at the worktree, local, global and system scopes (each read with `--includes`; a scoped `git config --get` skips `include.path` files), and the hook can find the state directory. Husky-style `core.hooksPath` turns it `Off`; this repository's `loom/.githooks` does, so its record starts unlatched.

The latch: `.git/config` is a file deny, so once the operator's own `git config` replaces it by rename a running session can write it and set `core.hooksPath` ([Sandbox gaps](../concerns/sandbox-and-confinement-gaps.md)). Each entry therefore carries `attestation`, raised whenever an evaluation finds the mode `Active`; while set, the evaluation walks the ledger even if the mode reads `Off`, so a downgrade becomes an `Unattested` hold. Only `loom target accept` lowers it, to the mode current at the accept. The latch rises only during an evaluation, so an operator who unsets `core.hooksPath` mid-run and a session that re-sets it before any move leave it false ([Hardening concerns](../concerns/agent-rule-bending-hardening.md)).

## Evaluation (`evaluate.rs`)

`check` reads the record, returns `Clear` when the tip equals `accepted` with no hold (no lock), and otherwise takes `MergeLock` and runs `check_locked`. For accepted tip A and observed tip T, `reasons` yields a `HoldReason` list (`NotFastForward`, `ControlPaths`, `Unattested`, `StageWork`, `Unevaluable`):

- T not a descendant of A: `NotFastForward`.
- Control paths (`git/merge/control_paths.rs`) in A..T: `ControlPaths`, naming them.
- Attestation required (mode `Active` or latched): `attestation_gap` walks the ledger chain from A to T; a gap whose diff leaves `doc/loom/knowledge/` is `Unattested` (at most `MAX_GAP_PATHS` = 20 paths named). The walk runs at most 2*steps+2 iterations, drops `from == to` entries (host `git gc`/`pack-refs` writes them), never revisits a tip, and `gap_end` finds candidate ledger `from` values with one `git rev-list --ancestry-path A..T`. When the walk starts at A or a descendant it reports the unrestricted gap, else the forward-only gap: a gap behind A is the reset-and-undo signature that charges accepted commits.
- A `refs/heads/loom/*` commit-typed ref (listed with `%(objecttype)`) carrying commits in T not in A: `StageWork`. This is the main signal when attestation is off.
- No reasons: accepted silently, the record advances.

Ledger lines are untrusted-format input: ids that are not 40 or 64 lowercase hex characters are skipped, and an ancestry test treats any git exit other than 0 as "not an ancestor". A hold memoises on the observed tip (no git work per tick, except an `Unevaluable` hold, which is re-evaluated); a re-recorded hold for the same tip keeps its `since`; a restore (T == A) clears it. A record that cannot be read is never overwritten by a check: the target is held as `Unevaluable`.

## What a Hold Does

- `merge_stage` returns `Blocked(MergeBlock::TargetHeld { target, accepted, observed })` for every gate mode ([Merge Flow](merge-flow.md)); the daemon's retry lands the stage once the hold clears. A `TargetHeld` block is never put in the blocked-retry memo.
- Daemon paths that settle a stage from ancestry test against the accepted tip, never the live ref: `Orchestrator::merged_into_accepted` (`target_hold.rs`) calls `target_guard::merged_into_accepted`; `probe_accepted`/`probe_merged` wrap it for the recovery sweep (`sync_graph_with_stage_files`, phantom-merge revert) and log a probe error once per distinct message; `merge_handler.rs::verify_and_finalize_merge` uses it for the `NoWorktree` finalize. The auto-merge precheck records `TargetHeld` before the gate, the zero-commits route and the finalize.
- New stage worktrees start at the accepted tip (`spawn_setup.rs` `resolve_worktree`), so spawns continue during a hold. A leftover `loom/<id>` branch with no worktree is recreated only when it has no commits beyond that start point. Knowledge-stage spawns (`spawn_hold_reason` returns `BlockReason::TargetHeld`), merge-resolver spawns and blocked retries wait. An unreadable record blocks a code stage's spawn too.
- The daemon check (`check_target_guard`) fails closed. A contended merge lock (another owner holds it) is judged read-only with `pending_hold`: a hold found that way gates but is not printed (a merge may be between moving the target and recording the advance), no hold clears, an error holds. A check error keeps the current hold or builds an in-memory `Unevaluable` hold, so a session holding `merge.lock` cannot freeze the verdict clear; the remembered error resets on every `Clear` or `Held` answer, so one that recurs after the guard recovered is logged again.
- `loom status`, the TUI and the web dashboard show one warning alert per hold, with or without a daemon (`scheduling_report_guard.rs`); a record that cannot be read raises one alert naming the file.

## Operator Commands

- `loom target status` (`commands/target/status.rs`): accepted and current tip, attestation state, hold reasons, review commands, the accept command and the restore commands (`git update-ref refs/heads/<t> <A> <O>`, plus `git read-tree -m -u <O> <A>` in the main checkout when the target is checked out there). A target branch that does not resolve and an unreadable record print as text with exit 0; a malformed plan config or a git failure still errors.
- `loom target accept --to <commit>` (`commands/target/accept.rs`): refuses inside a loom session (a guard rail; the boundary is the `.loom/` deny) and unless `--to` names the current tip; replaces an unparseable record only through `read_record_for_accept` (an I/O error propagates); prints `git read-tree -m -u <A> <O>` when the checkout was left behind (an error computing that note is a warning, exit 0).
- `loom init --clean` and `loom clean --state|--all` refuse while a recorded target is held or would be held (`begin_clean`, `refuse_unreviewed_move`), before stopping the daemon, printing, pruning or confirming a branch deletion. The refusal reads only the record (`recorded_targets`), never the plan config; an unreadable record refuses and names the file; a restored target stops refusing.

## Trust Inputs and Limits

- The record, refs and ledger sit in `.loom/`, write-denied to every session; the hook, `core.hooksPath` and `.git/config` sit in `.git`, denied as files (see the latch).
- Loom's git ignores replace refs, grafts and the commit-graph ([Sandbox gaps](../concerns/sandbox-and-confinement-gaps.md), "Session-Planted Replace Refs and Grafts"). Every one of loom's guard git calls runs through `crate::git::runner`.
- What stays open is in [Hardening concerns, G2](../concerns/agent-rule-bending-hardening.md): a crafted knowledge-only commit, an attestation-off repository, other refs and objects, a TOCTOU after an accept, and the residuals listed there.
- Test fixtures: `git/target_guard/test_support.rs` (`cfg(test)`, `pub(crate)`) and `loom/tests/target_ref_hook_support/mod.rs` (shared by the hook test files through `#[path]`).

## Residuals

What the guard does not close. The wider surface (other refs, objects, `R`'s index, `HEAD` and `MERGE_HEAD`, a planted `commondir`, host-side git, daemon-owned commits) is G2 in [Agent Rule-Bending Hardening](../concerns/agent-rule-bending-hardening.md).

- **Knowledge-stage exception.** An agent can land a crafted commit that touches only `doc/loom/knowledge/**`: the hook and the evaluation let knowledge-only fast-forwards through, because knowledge stages commit to the target from their sandbox.
- **Attestation off.** Where `core.hooksPath` is set (this repository's `loom/.githooks`, until the operator unsets its local value), a crafted fast-forward with no control path and no `loom/*` commits passes, as in any design without a trusted author signal. A record that started with attestation on keeps requiring it until an accept, but the latch rises only during an evaluation: an operator who unsets `core.hooksPath` mid-run and a session that re-sets it before any move leave it false.
- **Net-diff blindness.** In `Active` mode a session can complete its stage at commit C, push D (a revert of C) or a same-tree merge of C onto the target, and repoint its own `loom/<id>` to the accepted tip. The net diff is empty, so the hook's `knowledge_only` test and the evaluation's gap check pass, and the settle marks the stage merged with none of its work landed. Nothing unreviewed lands; work is silently dropped. Closing it needs a per-commit path check over A..T instead of the net diff, a design change.
- **Restore resets the ref only.** The rejected commits stay in the object store; a stage agent can merge or re-author them into its own branch, which reaches the target only through `merge_stage`'s gate and the stage's review.
- **Timing.** Between a move and the next tick (up to 5 s) anything reading the target sees the moved tip. A resolver already running when a hold starts may merge the held tip into its stage branch. Object replacement after an accept is a time-of-check window.
- **Operator commands use the live ref.** `loom worktree remove` and `--assume-merged` mark a stage merged by ancestry against the live ref.
- **Fallback ref.** `resolve_target_branch(None, ..)` falls back to the agent-writable `refs/remotes/origin/HEAD`; `loom init` always writes `base_branch`, so a run never reaches it.
- **Git outside the runner.** Production git spawned with `Command::new("git")` outside `crate::git::runner` honors replace refs, grafts and the commit-graph; none decides ancestry or a merge ([list](../concerns/sandbox-and-confinement-gaps.md)). `<common>/shallow` is not neutralised either.
- **Hook read access.** The hook skips silently when it cannot read `.loom/work/target-guard.refs`, so a session's refusal depends on the capsule leaving `.loom/work` readable; a future `denyRead` of it would turn the refusal off.

## Unimplemented Review Suggestions

Open findings the plan's reviewers left, none a security gap beyond the residuals above.

**Behavior**

- Ancestry tests against the live target ref remain outside the daemon's settle paths: `verify/transitions/state.rs` (two sites, `is_ancestor_of(.., branch_ref(target_branch), ..)`), `git/cleanup/orphans.rs`, `orchestrator/merge_lifecycle/containment.rs`. They run for stages already marked merged through a guarded path; moving them to `merged_into_accepted` is open.
- `record_merge` (`git/merge/mod.rs`) only warns when `record_advance` fails, so loom's own merge commit is evaluated against the old accepted tip next tick and can raise a `StageWork` or `ControlPaths` hold on loom's own merge; one retry or a louder message is open.
- A contended-lock hold is not announced, but `record_merge_block` (`merge_handler/auto_merge_outcome.rs`) persists and prints it, so a CLI merge between `update-ref` and `record_advance` can make a concurrent daemon auto-merge record a false `MergeBlocked` for one tick. Skipping the tick without persisting is open.
- `fmt_target_held` (`git/merge/tree.rs`) says the record could not be read for any empty `accepted`; `error_hold` also leaves it empty when the record is readable with no entry and the check failed for another reason.
- The hook's `core.quotePath=true` makes it refuse a knowledge-only move whose paths are non-ASCII (fail closed: a knowledge-stage commit of such a file is refused).
- The greedy "latest in ledger order" step choice in `evaluate.rs` can dead-end at a gap where an earlier forward step would clear: a false hold, never an unsound clear.
- `stage_work` keeps only `commit`-typed `loom/*` refs, so an annotated-tag-typed one is skipped; `changed_paths` passes revisions without `--`, so a committed file named like a ref fails closed.
- `loom repair`'s hook check is marker-only (`commands/repair/hooks.rs`): a non-executable or stale loom hook reads healthy while `attestation_mode` reports it off; using the installer's `UpToDate`/`Installed` judgement would agree.
- The attestation latch could also be raised in `start_target_guard` or at a low rate per tick (see Residuals).
- `worktree_start_point` (`spawn_setup.rs`) also runs when the stage worktree exists, so a resume is held while the record is unreadable; it shares `spawn_skip_logged` with the scheduling-skip path, so one cause suppresses the other's warning, and returns `Option<Option<String>>` where a small enum (`UseTip`/`UseBase`/`Wait`) would read better.
- `clean` prunes base-graph cache layers and `init` runs startup repairs before their guard refusal; harmless caches, not literally "before anything".
- `commands/target/status.rs` reports any failed `rev_parse` of the target as "does not resolve" with exit 0, and reads the record five times; a single snapshot accessor in `target_guard` would remove the torn view. `Hold.accepted` as a `String` with an empty sentinel would be safer as `Option<String>`; `accept_command` takes an unused `_target`.

**Layering and duplication**

- `git/target_guard/record.rs` imports `crate::sandbox::KNOWLEDGE_WRITE_GLOB` while `sandbox` imports `git::worktree`, and `git::hooks`, `git::merge` and `git::target_guard` import each other. Moving `HOOK_MARKER` beside the hook installer and a `KNOWLEDGE_PREFIX` const beside `KNOWLEDGE_WRITE_GLOB` removes two of the cycles.
- `install_pre_commit_hook` returns `Result<bool>` while the reference-transaction installer returns `Result<HookInstall>`; the pre-commit installer could move to the enum.
- The hook script's header says `loom init` installs it; the daemon at start and `loom repair` install it too.

**Test gaps**

- Hook level: `--ignore-submodules=none` (a `.gitmodules` `ignore = all` hiding a gitlink change; the precedent is `git/merge/control_paths/tests.rs`), a session recreating an absent target, a refs file with no `allow` line, and the refusal text in `sandboxed_session_update_of_the_target_is_refused`.
- `accept()` with another owner holding the merge lock (`ACCEPT_LOCK_TIMEOUT`), a rewrite that is both `NotFastForward` and `ControlPaths`, the `checkout_note` error arm of `accept`, the `try_exists` stat-error branch of `refuse_unreviewed_move`, `begin_clean(.., false)` (a regression would block `loom clean --worktrees` while held), and an `alerts(work_dir, true)` test of alert order with several targets.
- `attestation_tests.rs` read the real global and system git config through the runner, so a global `core.hooksPath` makes `a_missing_or_foreign_hook_turns_attestation_off` fail with a misleading reason; `the_installed_hook_with_this_state_directory_is_active` tests a stand-in hook, not the installer.
- `tests_confinement_target` passes by early return when `srt` cannot run; the operator runs it after the plan.
- Fixture copies: a sixth `CwdGuard` in `commands/init/execute/tests.rs`, `git()` in `tests/target_cli_tests.rs` duplicates `tests/target_ref_hook_support`, and `target_hold_recovery_tests.rs` re-declares fixtures `target_hold_tests.rs` exports.

## Test Pins

Each guard behavior has a test that goes red when the behavior is mutated (the integration-verify mutation list). Mutated behavior and the pinning test:

- A control-path move is held (`control_path_move_is_held`, `tests/target_guard_contracts.rs`).
- An agent fast-forward to its own branch is not marked merged (`agent_fast_forward_to_own_branch_is_not_marked_merged`).
- A sandboxed session's update of the target is refused (`sandboxed_session_update_of_the_target_is_refused`); `tests_confinement_target` (`orchestrator/terminal/native/`) runs probe 1 under srt, requiring the hook's refusal text on stderr besides a non-zero exit and an unchanged tip, and probe 2 as the control (same move with `core.hooksPath=/dev/null`, which succeeds).
- A replace ref and a graft do not hide an unattested change (`replace_ref_does_not_hide_an_unattested_change`).
- New stage worktrees start at the accepted tip while held (`resolve_worktree_starts_at_the_accepted_tip_while_held`).
- A conflicted stage gets no resolver while held (`a_conflicted_stage_gets_no_resolver_while_held`, using a `MergeConflict` stage with a missing branch so the held check is the only difference).
- An attestation downgrade still holds an unattested move (`attestation_downgrade_still_holds_an_unattested_move`).
- A knowledge stage is not started while held; a held respawn keeps an orphaned stage branch; a contended `NoWorktree` finalize does not settle (`contended_no_worktree_finalize_does_not_settle`).

Hook tests share `loom/tests/target_ref_hook_support/mod.rs` through `#[path]`. Replace, graft and gitlink semantics could not be probed from a subagent (the commit filter blocks `git commit` strings), so those tests assert their own preconditions (git reads the replace ref or the graft) before asserting the refusal.
