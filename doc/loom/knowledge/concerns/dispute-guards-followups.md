# Dispute Guards Followups

> open review suggestions: trial, overlay undo, narrowing, prompt text

## Dispute Guards: Open Follow-Ups

Review suggestions and gaps left by `PLAN-contract-and-criterion-dispute-guards`. Design: [Contract Trial](../architecture/contract-trial.md), [Criterion Narrowing](../architecture/criterion-narrowing-evidence.md), [Frozen Contracts Change Only Through `--replacement`](../conventions/dispute-and-adjudication.md#frozen-contracts-change-only-through---replacement). Test-environment items are in [Host-Job Test Gaps](sandbox-and-confinement-gaps.md#host-job-test-gaps).

### Contract trial and overlay

- **A receipt binds HEAD and the files only.** `TrialReceipt` (`verify/contracts/trial.rs:33`) is not tied to the freeze generation, so a receipt from before an accepted re-freeze still admits the same bytes at the same HEAD against a different harness; `JobRecord.tree_digest` could also bind uncommitted implementation edits made after the trial.
- **A receipt is lost once three newer applied jobs exist** (`host_verify/store.rs` `KEPT_JOB_DIRS`); the trial-passed text (`wait_report.rs:19`) could tell the agent to file before running other checks.
- **A same-session contracts or at-base request supersedes the running criteria check** instead of waiting, because the waiting slot holds only a criteria selection (`host_verify/submit.rs:129`, `:363`). Let it wait, or document the asymmetry.
- **Overlay undo gaps.** `frozen_restore.rs:85` does not skip the undo when the stage's current job is Running under another id, so retiring a stale held job could revert a newer trial's live overlay of the same bytes (reachable only if `runner.rs:154` errors while `queued_records` succeeds); `child_contracts.rs:246 live_overlay` skips a stale job's undo by file name only; `signals.rs:114 retire_historical_groups` undoes a non-current job only when it has recorded groups. `frozen_restore.rs:102` reads the mode from a path-based `symlink_metadata` while the write is no-follow; take it from the opened fd. `fs/safe_replace.rs:66` leaves `.<name>.<rand>.tmp` after a SIGKILL mid-publish.
- **Missing tests.** `recover_after_restart` restoring a frozen file left overlaid by a trial Running at restart (`signals.rs:107`; `tests_child_overlay_undo.rs` only calls `kill_groups` on a Cancelled record, and its mode assertion is weak because `fs::write` keeps an existing file's mode); a panic inside `run` with a real `flock` relock, pinning the unwind order of `with_admitted_overlay` (restore declared before the admission guard); `gate_dedup_contracts.rs:35` asserting the contract file's frozen hash survives the re-freeze.
- **Dead and duplicated code.** `trial::with_overlay` is used only by `trial_contract_tests.rs`; production calls `with_admitted_overlay`. `(file, sha256)` pairs at `dispute_admission.rs:242` and `prompt/contract_replacement.rs:66` duplicate `trial::trial_files`; `receipt_covers(Some(&receipt), &receipt.head, ..)` makes its head check a tautology. The three name forms of each replaced file in `child_contracts.rs:153` duplicate `freeze.rs:191-202`. `AuditRow::field_name` (`plan/amendment/journal.rs:89`) and `apply_accept.rs:195 field_name` spell `wiring_tests` and `wiring-tests` differently.
- **Environment markers are applied even with a parsed summary**, so a contract whose expected message contains `connection refused` cannot be frozen (recorded in [Contract Trial](../architecture/contract-trial.md)).

### Dispute filing and re-freeze

- **`apply_contract_notice.rs:64`**: the all-reject notice without a replacement still tells the agent to restore each contract, although admission already refused drift; drop it or make it conditional.
- **Prompt rustdoc.** `prompt/contract_frozen_set.rs:3` and `:27-28`, and the line at `prompt/contract_verdict.rs:41`, still say an accept re-freezes the changed and new files and drops the deleted ones; an accept now puts changed files back and keeps deleted ones. `trial.rs:9-11` and the `with_overlay` doc say every earlier byte comes back whatever the run did; the restore is conditional.
- **Stage-session guard.** `commands/stage/amend.rs` checks `LOOM_WORKTREE_PATH`, `LOOM_STAGE_ID` and `LOOM_SESSION_ID`; the real boundary is the session's `.loom` write denies (`sandbox/control_surfaces/session_denies.rs:81-82`), which no test here verified for a stage session.
- **Reachability.** The `reachable` check does not resolve a call written `stage::x(...)` through a re-export (`dispatch_amend -> stage::amend` left no edge); a plain `use ...::amend::amend` import does.

### Criterion narrowing

- **Counts, not names.** See the limits in [Criterion Narrowing](../architecture/criterion-narrowing-evidence.md); a base run failing one of 500 executed tests lets the judge drop the whole criterion and hand off one test.
- **`at_current_base` errors when evidence exists and the base cannot be read**, even for a `replace` without `leaves_out`, which needs no evidence (`verdict_narrowing.rs:114`); it only causes a needs-more-evidence round. Call it only when a `leaves_out` entry is present.
- **`stage_cache/base_checkout.rs:221 write_git_file`** checks `.git`, then opens `.git/<name>` by path with `O_NOFOLLOW` on the last component only; open `.git` with the no-follow walk and `openat` beneath it.
- **`commands/status/ui/tui/handoffs.rs:64`**: the `Paragraph` has no `Wrap`, so long hand-off lines are cut at the terminal width.
- **Test gaps.** `retry_recovery_tests.rs:98` could assert exactly one hand-off for dispute `"1"` after the retry (the idempotent replay path); `sandbox_probe.rs:234`'s macOS branch has no unit test.

### Contract signal and prompt text

- **The contract signal prints "No scaffold edits: every contract file is already collected"** when every contract's `test` is writer-chosen, because `StageTree::scaffold` skips contracts whose test is unset. Four writers hit it ([mistake](../mistakes/dispute-guards-delivery.md#the-contract-signal-said-no-scaffold-edits-for-writer-chosen-contracts)); the signal should say the scaffold is derived at freeze. A hook or `loom plan verify` check cannot see this; the fix is the signal text.
- **Size ceilings.** `skills/loom-orchestration/SKILL.md` and the standard-signal boilerplate floor are pinned to exact byte ceilings (`signals/tests_size.rs`); any stage-text addition needs a matching trim. Contract prompt headings repeat once per file, so `prompt/truncate.rs::section_body` skips an already-halved section.
- **Struct growth.** Adding a field to `JobRecord`, `HostJobRequest`, `WaitSite` or `RequestState` breaks about 25 struct literals in test files outside `host_verify` (`rg '\b(WaitSite|JobRecord|HostJobRequest|RequestState) \{'` lists them before ownership is planned).

## Open Items of the Memory Journals Grant

- The bubblewrap side (`confine_protected.rs::bind_memory_journals`, the second bind where the worktree's link reaches the journals, `tests_child_state_root.rs` under bubblewrap) has run only in CI, never locally.
- `loom memory pending --strict` finds the state root through the worktree's `.loom/work` link and exits 0 with `no memory journals` when it finds none (`commands/memory/handlers/pending.rs`). A stage that removes or repoints the link passes the criterion without its memories being read. No environment variable names the state root to the CLI today; `LOOM_WORK_DIR` is read by hooks only.
- `memory_journals_grant` canonicalizes `<state root>/memory` and follows it when it is a symlink. Only the operator and the daemon can write there.
- On the operator's `loom stage complete` and `loom check` run from inside a worktree, the grant comes from the state root that worktree's link names, as the stage record does.
