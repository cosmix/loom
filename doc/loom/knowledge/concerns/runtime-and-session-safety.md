# Runtime And Session Safety

>

## `evaluate_new_session` Fails a Working Spawn on a Benign `~/.tmux.conf` Warning

The rule "any stderr with exit 0 is a failure" is a plan mandate, pinned by unit tests and carried by
an explicit stage decision — so it was **deliberately left as-is**. But tmux prints `~/.tmux.conf`
deprecation warnings to stderr _while creating the session fine_, so one benign warning now: fails the
spawn, kills a **working** server via the abort path, and returns `Err` — which blocks the stage
(`FailureType::InfrastructureError`) instead of retrying on another lane. There is no
`.loom/work/terminal-backend-fallback` marker, so the consequence is a single Blocked
stage rather than tmux being disabled repo-wide.

The `has-session` probe that immediately follows is the authoritative signal and would distinguish the
two cases. Gating the stderr rule on that probe is a design call for the plan owner, not an
integration-verify defect.

## `loom attach` Overview Panes Are Live, Writable Agent Terminals

The overview's panes are full interactive attach clients (no `-r`), so a stray keystroke goes into a
live autonomous agent's input, and `C-b x` kills the **stage's** pane — the inner servers have no
`remain-on-exit`, only the viewer window does.

Left as-is deliberately: the plan mandates the exact pane string. If the overview is meant to be a
_viewer_ rather than N live terminals, adding `-r` to `attach-session` on the **overview path only**
(never on `loom attach <stage-id>`, which is the intentionally interactive path) is a one-word change.

## Overview Viewer Retirement

The overview viewer has a repository-specific retirement path used by explicit clean and init. Generic socket enumeration still excludes viewer sockets to avoid treating other repositories’ viewers as session sockets.

## PreToolUse File Guards Cannot Eliminate Path-Swap Races

The canonical `worktree-file-guard.sh` now covers Read, Write, Edit, Glob, and Grep; canonicalizes
paths; compares path components; and rejects both leaf and parent symlinks. This closes the concrete
absolute-path, common-prefix, and final-symlink escapes found in the security review.

The guard is still a check before a separate built-in tool performs the real open. A hostile process
can replace a parent path after the hook returns, so the guard cannot bind its decision to the inode
the tool later uses. Treat it as defense in depth; the host OS sandbox remains the authoritative
boundary. A race-free design requires a dedicated file-operation broker that performs traversal and
the actual open relative to an already-open worktree directory using no-follow semantics. Do not
describe the current hook boundary as TOCTOU-free.

## `loom attach <stage-id>` Dies With Its Stage

**Direct attach dies with its stage by design.** `loom attach <stage-id>` `exec`s into the stage's
OWN server (`commands/attach/mod.rs`), whose lifetime is the stage's (default `exit-empty`;
`completion_handler.rs` `kill_session` is only a backstop). The client gets `[exited]` on
completion; no follow/loop exists. Candidate design if this matters: make direct attach a
`select-pane` + `resize-pane -Z` focus on the long-lived overview viewer, so the operator is attached
to the viewer's lifetime, not the stage's (note `split-window` unzooms, and the build should reuse a
healthy viewer instead of `kill-session`-ing it). Daemon log for any attach/reconcile question:
`.loom/work/orchestrator.log`, level fixed by `RUST_LOG` in the shell BEFORE `loom run`.

## Orphan Adoption Only Runs at Daemon Startup

`adopt_orphaned_agents` is called from `recover_orphaned_sessions`, which
`orchestrator.rs` invokes exactly ONCE, at daemon startup — not on the poll loop, despite the
"every tick" phrasing that appeared in the brief that requested it. An agent orphaned mid-run
therefore stays invisible until the next `loom run`. That is enough for the incident it was written
for (a killed daemon is restarted by definition), and the spawn-time guard in `start_stage` closes
the duplicate-spawn hole independently, so this is a narrower reach rather than a hole. Making it
per-tick is a one-line addition to the scheduler loop; the pass is already idempotent and pinned as
such by a test, so the only question is cost — it scans `.loom/work/pids/` per Executing stage.

## Memory commands use shared state discovery

Memory commands resolve state through `commands::common::work_dir_path` (`commands/memory/handlers/work_dir.rs:76-77`), which delegates to `WorkDir::new` (`commands/common/mod.rs:23-34`). `fs/work_dir.rs:130-174` searches within the nearest repository and temporary-directory boundary, with direct workspace and state-root handling. `find_worktree_root_from_cwd` retains substring-based detection for its other callers; it is not the memory read resolver. Review those other consumers separately rather than changing read-only memory recovery based on this helper.

## `commands::memory` Tests Mutate the Process-Global Working Directory

Those tests call `env::set_current_dir` and rely on `#[serial]`, which only serializes a test
against OTHER `#[serial]` tests — anything unmarked in the same binary runs concurrently with a
foreign cwd in effect. In each test the `TempDir` is also declared AFTER its `EnvGuard`, so it is
deleted FIRST and there is a window where the process cwd points at a removed directory.

Neither of these caused the 77-failure incident (an ambient impostor `.git` did, see above), which
is why they were not changed. They remain a live hazard: any future test in that binary that
resolves a relative path while a sibling holds a foreign cwd will fail in a way that looks
unrelated to its own subject. Fix shape: inject the working directory instead of mutating the
process's, as `orchestrator/merge_lifecycle/tests.rs::cleanup_defers_while_the_cwd_is_inside_the_worktree_it_would_remove` already does deliberately for this exact
reason.

## Guard Hooks: Four Design Questions Deliberately Left Open

The adversarial review of `read-guard.sh`/`poll-guard.sh`/`_read_discipline.sh` surfaced four
behaviours kept AS SPECIFIED rather than patched, because each needs a spec decision, not a
one-line fix:

1. **The deny threshold may not fit CLAUDE.md's own workflow.** Five identical `git status`
   invocations in one session trigger a deny, yet CLAUDE.md's own commit workflow runs `git
status` before staging, again after committing, and again before completion — a real session
   following the documented workflow can plausibly hit the threshold.
2. **Ledger TSV keys can interleave across processes.** `_loom_ledger_append` uses the whole
   normalised command line as the TSV key with no atomicity guarantee beyond `PIPE_BUF`; a key
   longer than `PIPE_BUF` can interleave between two in-process subagents that both sanitise to
   `agent_id=main`.
3. **`loom_deny_enabled` is a line-oriented `grep`-style check**, so a TOML multi-line string
   VALUE that happens to contain the literal lines `[hooks]` and `deny_enabled = true` would
   enable the switch even though no real config intended it.
4. **`poll-guard`'s rule-2 `cat` branch is unreachable for any pre-existing `.loom/work` file**,
   because rule 3 (repeat-read escalation) fires first for files the ledger already has an entry
   for — effectively dead code on the common path.

None of these are fixed; each is a live behaviour a future stage should either ratify explicitly
or change with an accompanying spec decision, not patch as an incidental side effect of unrelated
hook work.

## `is_ancestor("1")` Cannot Distinguish "Not an Ancestor" From "Walked Off the Top of a Container"

`loom-hooks/_common.sh`'s `is_ancestor()` exits its walk-up-the-process-tree loop as soon as the
current pid becomes `"1"` or `"0"`, WITHOUT checking whether that final value equals the target
pid — so `is_ancestor(target="1")` is a guaranteed, deterministic `false` regardless of the real
process tree, even inside a container where PID 1 genuinely is an ancestor of everything. This is
useful as a test fixture (a non-ancestor `LOOM_MAIN_AGENT_PID` of `"1"` can never flake true), but
it is also a real edge-case correctness gap for any deployment where the loom main agent's PID
could legitimately be 1. Not fixed — recorded because the test-fixture use depends on the same
behaviour that makes it a latent bug elsewhere.

## Dispute Bookkeeping Gaps Around Escalation

`--approve` refuses while a live worker or merge resolver may write to the stage (`commands/stage/human_review.rs::refuse_live_writer`). A filing checks the transition on a loaded snapshot before writing the request; `loom stage reset` closes open disputes (`commands/stage/state/loop_recovery/mod.rs`).

- **A refused filing can leave an open request in a race.** `stage_service/dispute.rs::handle_dispute_criteria_identified` (lines 80–106) and `stage_service/dispute_kinds.rs::file_dispute_inner` (lines 192–219) hold the dispute lock, validate a snapshot, publish `disputes/<stage>/<n>/request.md`, then call `update_stage` under the separate stages-directory lock. A concurrent status change can make the final transition fail after the request is published. Fix the publication/transition transaction at the shared service; preserve the dispute-before-stage lock ordering. `daemon/server/dispute.rs` and `dispute_kinds.rs` are transport adapters.
- **Unverified: a survivor may block the fresh spawn.** A surviving session with no PID identity can read as dead to an approval probe. If it stays in `active_sessions` and `stage.session`, `insert_active_session` (`orchestrator/core/session_lifecycle.rs`) may refuse the fresh session; verify this against the current retirement and adoption paths before changing behavior.

## `failure_info` Outlives the Attempt That Set It

A status writer retires the reasons of the status it leaves: `Stage::try_transition` and `Stage::force_status_with_reason` (`models/stage/methods.rs`) call `retire_issues_of` (`models/stage/issue_history.rs`), which archives `failure_info` and `review_reason` into `issue_history` on entering WaitingForDeps, Queued, Executing, Completed or Skipped, archives `failure_info` on entering MergeConflict, archives a merge-conflict entry (with `merge.conflict_since` as its `detected_at`) on leaving MergeConflict, and drops a merge block labelled by the transition. Writers that record a reason for the new status write it after their transition. A failure overwritten without a status change goes through `Stage::replace_failure`, which archives the old one unless type and evidence are unchanged, so per-tick re-records add nothing. Explicit clears (`stage_service::apply_block`, `human-review --reject`, `loom stage retry --force`, `loom stage reset`, `clear_merge_block`) archive before clearing, so a stage Blocked with no `failure_info` still means `loom stage block` or a rejected review, never a crash (`orchestrator/core/event_handler/blocked.rs` relies on it).

Open gaps: re-blocking a crash-blocked stage (a legal Blocked to Blocked) archives the crash evidence but still cancels a pending auto-retry; `issue_history` is unbounded and ships whole in every status frame.

## Concurrent Daemon-Absence Probes Read Each Other as a Running Daemon

`inspect_lock` (`loom/src/fs/daemon_state.rs::inspect_lock`) proves no daemon runs by taking `flock(LOCK_EX|LOCK_NB)` on `orchestrator.lock`. Two probers at once (threads, or two `loom hook review-harvest` processes with no daemon) can each see the other's probe as a held lock, and `fingerprint::compute` then fails with `DaemonUnreachable`. The window is microseconds, and inside a stage the daemon answers on the socket so the probe never runs. Candidate fix: probe with `LOCK_SH|LOCK_NB` so probers share while the daemon's `LOCK_EX` still excludes them; check every `inspect_lock` caller and a daemon starting mid-probe first.
