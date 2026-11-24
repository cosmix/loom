# Stage Exit Recording

> Stage exits: refused-commit route, idle-stop block, verdict delivery, new guards

## A Stage Session Ends in a State Loom Records

A stage session ends completed, disputed, blocked or waiting on a question, never idle on a message nobody reads. Four parts keep it so: the stage-exit doctrine, the refused-commit route, the idle-stop block and the delivery of every verdict to the next session.

## Stage-Exit Doctrine and the Refused-Commit Route

- `STAGE_EXIT_RULES` (`orchestrator/signals/helpers.rs`, pinned as `BLOCK_F` in `tests_doctrine_v2.rs`) tells the main agent: dispute a wrong check, contract, finding or integrity event; block only for a need only a person can meet; before a dispute, commit and run the completion command. On a commit the repository's hook refuses: fix what the hook names, it outranks a plan instruction, a ratchet-file entry may always be lowered or removed, record the deviation with `loom memory decision`, and a refused commit never stops a filing. `append_commit_timing_rules` in the same prefix carries the matching exception to commit-at-exit: commit before filing a block or dispute when the stage cannot finish.
- `loom stage commit` runs the repository's `pre-commit` and `commit-msg` hooks before committing (`commands/stage/commit.rs::run_checked`). A refusal from either appends `hook_refusal_route`: the same rule in one paragraph. The `stripspace` step adds nothing.
- The v2 signal's review-gate text says a ratchet file raises an integrity event only when loosened (lowering or removing a row of a `<key> <integer>` ledger raises nothing; any change to a ratchet file in another format does), and that an accepted contract replacement is written into the worktree uncommitted: stage it with `git add` and commit it; the latest review round still covers it (`orchestrator/signals/v2_section.rs`, `v2_section_review.rs`).

## Idle-Stop Block

`loom hook idle-stop` (`commands/hook/idle_stop.rs`, a hidden `HookCommands` variant) reads the Stop payload and prints one `{"decision":"block","reason":...}` object or nothing. `loom-hooks/loom-relay-stop.sh` runs it (bounded to 5 s) when its own ticket check has nothing to say and relays the object; no hook file or registration beyond the existing Stop pair exists for it.

`decide` blocks only when ALL hold: the stage is `Executing` and names this session; the session is a stage or knowledge session (not contract, judge or merge); the session's subagents and native workers are all quiet (`ChildDisposition::NoChildren` from `Orchestrator::session_child_disposition_in`, the journal the daemon's quiet-refusal timer reads); no host job of the stage is non-terminal (an unreadable store counts as live); the session heartbeat timestamp changed since the previous block (once per tool call); and fewer than `MAX_IDLE_STOP_BLOCKS` (3) blocks went out. Unreadable evidence, a malformed marker or a payload with `stop_hook_active` set never blocks. The marker `.idle-stop` in `LOOM_SCRATCH_DIR` holds the count and the heartbeat timestamp, written under an exclusive directory lock. The reason lists the recorded exits: finish (commit, complete), dispute, or `loom stage block` / `AskUserQuestion`.

## Verdict Delivery to the Next Session

- **A dispute round** is every dispute filed between one entry into `NeedsAdjudication` and the requeue after the last verdict. `stage_service/dispute.rs::clear_previous_round_feedback` removes `disputes/<stage>/feedback.md` on the first filing of a round (the stage is not yet `NeedsAdjudication`), in the criterion and the contract filing paths alike.
- **Notices accumulate.** `orchestrator/adjudication/feedback.rs` appends each rejection, question and notice as a section through `feedback/section.rs::append_section` (separator `---`; a no-op when the file already holds the section, so a verdict applied twice across retries appends once; past 64 KiB the file is truncated, with a dropped-notices prefix when it had content). The path comes from `models::dispute::feedback_file`.
- **The review reason joins.** `Stage::replace_review_reason` (`models/stage/issue_history.rs`) in `NeedsAdjudication` appends a further reason to the current one, skipping lines it already holds, instead of archiving the first as superseded.
- **A replacement trial's rerun follows its request.** `host_verify/wait.rs::state_matches` skips the tree-digest comparison when the call carries replacement files: the daemon's own overlay changes the digest during the trial. The job still refuses a receipt for a tree that moved, and a filing still needs a receipt for the current tree.
- **An accepted replacement keeps the review.** `verify/review/frozen_cover.rs::only_intact_frozen` (called from `gate.rs`) lets a round cover a tree on the same base whose only changes are frozen files whose digest equals the freeze record; an agent's own edit of a frozen file does not match the record and keeps the round stale. `loom stage review status` reports it as "the only change is an accepted replacement of a frozen file".

## Guards Added With the Exits

- **`NegatedSearch`** (warning, `plan/schema/validation/criterion_hazards/negated_search.rs`): a final `! <command> | <matcher>` (rg, grep), the same in a non-final operand of an all-`&&` chain, or a final check of a search's failure after a statement that pipes to a matcher. `--strict` fails on it; the message names the fail-closed remedy: run the command for its own exit status, and when only some failures count have the tool expose a mode for that. A negated file search without a pipe stays out.
- **`main-checkout`** (`loom-hooks/worktree-isolation.sh::check_main_checkout_tokens`, `_containment.sh::loom_path_in_main_checkout`): an absolute path into the main checkout that classes `outside` (not state, worktree or scratch) is blocked, tilde paths and `NAME=`, `--flag=` and one-short-option (`-C/path`) prefixes included. It is off when the repository root is `/`, the home directory or an ancestor of it. Gaps: [stage-exits-and-guards-followups](../concerns/stage-exits-and-guards-followups.md).
- **Ledger tighten mode:** [git-and-build-workflow](../conventions/git-and-build-workflow.md#the-maintainability-ledger-the-commit-hook-lowers-rows-nothing-raises-them).
