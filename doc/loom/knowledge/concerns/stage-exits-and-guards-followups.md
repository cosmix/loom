# Stage Exits And Guards Followups

> Out-of-scope rows, open reviewer suggestions, proposed checks after exits plan

## Out of Scope of the Stage-Exits Plan: Open Items and Revisit Conditions

Each row is a decision to leave something out, with the condition that reopens it. Mechanisms: [stage-exit-recording](../architecture/stage-exit-recording.md).

- **No daemon nudge into a live stage session, no shorter stall line for an ended turn.** No mechanism sends text into a stage session, and the daemon cannot tell a session waiting on its own background command from an idle one. The idle-stop block reaches the agent inside its own session. Revisit if sessions still end idle after three blocks.
- **The daemon does not commit an accepted contract replacement or complete the stage without a session.** A contract verdict starts a fresh session (`orchestrator/adjudication/apply_contract_rulings.rs`); with the feedback accumulation and the frozen-file cover that session stages one file, commits and completes. Revisit if that session proves costly.
- **No completion gate on edits outside a stage's `files:` list.** `files:` is an ownership declaration and stages legitimately touch module roots and lockfiles; observed out-of-list edits were ratchet lowerings, which loom allows. A product decision.
- **A freeze does not prove a contract can pass.** A build failure is an accepted red; a contract that could never pass was fixed by a dispute within minutes once the session could file one.
- **No warning for `! rg -q <pattern> <file>`.** Fail-open only when the file is unreadable, and many plans use the form; `NegatedSearch` flags only the piped form.
- **Two daemon warnings, each seen once in a rotated log and not reproduced.** `criterion group may survive retirement` (`host_verify/signals.rs:47`): revisit when it appears in a current daemon log; read the error it carries and the group's retirement path. `Graph disagrees with Executing stage file; forcing node` (`orchestrator/core/recovery_sync.rs:65`): revisit when it recurs; capture the stage id, graph node state and stage file status at that tick.

## Stage-Exits Plan: Unimplemented Reviewer Suggestions

**Idle-stop block** (`commands/hook/idle_stop.rs`, `orchestrator/core/loop_recovery/child_disposition.rs`, `loom-hooks/loom-relay-stop.sh`)

- A stop with `stop_hook_active` set is skipped, so an unattended session gets at most one block per turn and the three-block cap only counts across turns. Revisit if sessions end idle with blocks still unspent.
- The decision replays the lifecycle journal of every stage of the plan on every Stop of an owned Executing stage session. The daemon does the same per tick; a stage-scoped replay would bound the hook's cost to the stage. `session_child_disposition_in` only wraps the free function; making that function `pub(crate)` removes the layer.
- `LOOM_SCRATCH_DIR` is used for the marker without `check_scratch_dir`/`validate_session_dir` (`commands/hook/relay.rs`, `relay/scratch.rs`), which the relay applies.
- A nested headless `claude -p` started from the stage's Bash inherits `LOOM_*` and passes every check, so it would be told to commit and complete the outer stage. Consider a main-agent identity check; whether hooks load in that process is unverified.

**Main-checkout rule** (`loom-hooks/worktree-isolation.sh`, `loom-hooks/_containment.sh`). Best effort by design; the OS sandbox enforces writes.

- Glob or variable tokens into the main checkout (`/repo/src/*.rs`), `~user/` forms, colon-joined values (`PYTHONPATH=.:/repo/x`), `@/repo/x`, combined short flags (`tar -xC/repo`), multi-letter attached options (`-isystem/repo/include`) and `-Wl,-rpath,/repo` pass the rule.
- Relative tokens are not resolved against `LOOM_TOKEN_CWD`: `cd ~ && rg foo src/loom/loom/src` reaches the main checkout unblocked.
- `loom_path_in_main_checkout` recomputes the repository, home and scratch paths with several subshells per absolute token on every Bash call; compute them once before the token loop.

**Pre-commit ledger block** (`loom/.githooks/pre-commit`)

- With unstaged ledger edits and a failing gate the hook exits before printing the maintainability-failure message.
- After a commit that left the lowered ledger unstaged, a later commit of the shrunk file alone passes silently, because the ledger hash does not change in that run. A warning when the ledger diff is unstaged and the hash is unchanged would cover it before CI's stale-row check.
- The lowered ledger is staged before the gate verdict, so a refused commit leaves an index entry the user did not stage. Intended, and worth one line in the refusal output.

**Feedback file, review reason and trial rerun** (`orchestrator/adjudication/feedback.rs`, `feedback/section.rs`, `stage_service/dispute.rs`, `models/stage/issue_history.rs`, `host_verify/wait.rs`)

- `clear_previous_round_feedback` removes the previous round's feedback before the adjudication request and the stage save; a failed save withdraws the request but the feedback is gone.
- `already_holds` reopens the file by name instead of reading through the append descriptor, leaving a window between the two opens, and a non-UTF-8 feedback file fails every append up to the apply cap.
- `append_completion_failures` bypasses `MAX_FEEDBACK_BYTES`; the file can exceed 64 KiB by about 6.5 KiB (20 lines of 300 bytes).
- The whole-line window dedup in `issue_history.rs` drops a distinct one-line reason that equals a line of an earlier reason in the round (`retry`); the dispute's request record keeps the authoritative copy, so only the stage view loses it.
- A trial rerun after a real (non-frozen) edit follows and reports the old request's result for the earlier tree. The filing is still refused (`trial_receipt.rs`), so this costs a round trip; the report could say the tree moved.

**Review gate frozen-file cover** (`verify/review/frozen_cover.rs`). `frozen_digests` joins the working directory lexically and never matches a frozen file outside it (`../x`) or under a symlinked working directory; those cases fail closed and cost a review round. Reuse `working_dir_prefix`/`from_working_dir` (`verify/contracts/changes.rs`).

**`NegatedSearch`** (`plan/schema/validation/criterion_hazards/negated_search.rs`)

- An all-`||` chain with a negated output search in a non-final operand is equally fail-open (`! cargo test 2>&1 | rg -q FAILED || exit 1`); treat it like the all-`&&` chain and leave mixed chains unflagged.
- A parenthesised chain `(! cargo test | rg -q FAILED && echo ok)` is not flagged: the `&&` is below depth 0 and `judged_part` keeps only the final operand.

**Doctrine text and size pins**

- `orchestrator/signals/tests_size.rs:40` quotes about 7,570 bytes for the block; update it to the measured size.
- `tests_doctrine_v2.rs::block_f_agrees_across_every_surface` has no recovery-signal case; recovery reaches `BLOCK_F` only through `stable_prefix_for` (`recovery_format.rs`).
- The refused-commit rule reads differently in `BLOCK_F` ("never stops a filing"), `commands/stage/commit.rs::hook_refusal_route` ("does not stop a dispute or `loom stage block`") and the idle-stop text; align the wording.
- `CONTRIBUTING.md:153` says `CLAUDE.md.template` is capped at 20,480 bytes while `tests_size.rs` pins 20,388.
- `CLAUDE.md.template:90` ("commit before filing a block or dispute") carries no refused-commit exception; the template sits at its byte ceiling. Leave it unless sessions keep stalling on a refused commit.

## Recurrences: Proposed Checks Instead of Another Rule

- **A stage adds a test file that runs git with a hand-written identity or runner.** `loom/tests/test_git_guard_contracts.rs` fails on it only in the full suite, and a stage's gate runs filtered targets, so the guard never ran in the stage ([testing-and-lint](../mistakes/testing-and-lint.md), [consolidation-drift](../mistakes/consolidation-drift.md)). Proposal: a `loom plan verify` warning when a stage's `files:` names a `loom/tests/*.rs` file and no criterion runs `cargo test --test test_git_guard_contracts`.
- **The tree is edited while a `loom-code-reviewer` runs.** The reviewer's stop hook skips the round (`review_harvest_failed.worktree_changed`) and a full re-review follows ([planner-follow-ups-delivery](../mistakes/planner-follow-ups-delivery.md), [ci-gate-rollout](../mistakes/ci-gate-rollout.md)). Proposal: a PreToolUse warning on Edit and Write in a stage session while the reviewer lifecycle journal shows a live `loom-code-reviewer`.
