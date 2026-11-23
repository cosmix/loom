# Planning Sandbox Delivery

>

## Assumed a Read-Denied Directory Is Read-Only Under srt

**What happened:** `check_state`'s "parent not writable" rule and the confinement probe
`the_planning_root_and_runtime_are_write_denied` assumed the planning root is read-only inside the
sandbox. On a real host run, `touch <planning root>/probe` exited 0 and `LOOM_PLAN_STATE` was refused
as a planning root because its parent was writable.

**Why:** srt 0.0.78 (`linux-sandbox-utils.js` `pushReadDenyDirMounts`) covers a read-denied directory
with a writable `--tmpfs` per command and re-binds the `allowWrite`/`allowRead` carve-outs into it; a
`denyWrite` outside `allowWrite` is skipped as already read-only. The write lands in the tmpfs.

**Prevention:** on Linux treat a `denyRead` directory as a private writable tmpfs whose writes never
reach the host. Test host-side effects (no probe file on the host, the server lock keeps its content),
never the shell's exit code. `check_parent` accepts mounted grants for this reason
([Planning Document Model](../architecture/planning-document-model.md#runtime-directory)).

**Fix:** `runtime.rs` `grants_mounted`; the probe is `writes_to_the_planning_root_and_runtime_never_reach_the_host`.

## A Skipped Real-Sandbox Test Leaves the Sandbox Unproven

**What happened:** `tests/plan_session_confinement.rs` skips inside a stage (nested sandbox, no Unix
sockets, srt shim fails), so three defects survived every stage gate: the probe above, the
`LOOM_PLAN_STATE` check, and `loom map` failing on `.loom/cache/context-v1` ("Read-only file system")
because `ContextStore::ensure` ran before the in-memory fallback.

**Why:** a test that skips passes. Only the host run with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` exercises
the carve-outs.

**Prevention:** a sandbox change is unproven until that host run passes; a stage records the run
(`confinement: ran, PASS, srt <version>, <platform>`) or states that it could not run. Code that writes a
cache must let a denied write fall back to memory instead of creating directories up front.

## `Edit(//root/doc/*)` Covers the Directory `doc/loom`

**What happened:** the authoring allow list used `Edit(//<root>/doc/*)` for the top-level files of
`doc/`. The unit test used the `glob` crate and could not see the overreach.

**Why:** Claude Code matches `Edit` paths with gitignore semantics, so `doc/*` matches the directory
`doc/loom` and everything below it whenever the guard hook does not answer.

**Prevention:** emit one literal `Edit` rule per existing top-level regular file and test the doc/loom
directories themselves (`no_kind_allows_an_edit_under_doc_loom`). Never write a rule path with a glob
over a directory that holds a path the session must not write.

## The State-Root `.ignore` Does Not Make an Ancestor Search Safe

**What happened:** the plan 6 brief said a Grep/Glob root that is only an ancestor of the project token
files is allowed because the state-root `.ignore` keeps ripgrep out.

**Why:** the `.ignore` lists only `STATE_ROOT_SECRET_FILES` (not `terminal-history`), ripgrep does not
prune an explicitly named root, a Grep `glob` overrides ignore files, and native Grep runs outside the OS
sandbox.

**Prevention:** the planning guard ports `credential-guard.sh` `walks_into_state_root`
(`guard/search.rs`) instead of trusting an ignore file. Verify a brief's safety claim against the
tool's behavior before it becomes a rule.

## Smaller Slips

- **Formatter edition.** Running `rustfmt --edition 2024` on files of a crate on 2021
  (`loom/Cargo.toml`) rewrote untouched test bodies. Use `cargo fmt` or the crate's edition.
- **Reading one file less than the hook needs.** The terminal recovery in `main.rs` was assumed to write
  ANSI bytes before the plan hook's JSON deny; `write_terminal_sequence` (`utils.rs`) writes only to a
  terminal and the hook's stdout is a pipe. Read the writer before reasoning about its output.
- **`loom check` while subagents write.** A check whose tree digest differs from the running job starts
  a new request that cancels the running one ("superseded by ..."). Run it once after the subagents return.
- **Test-integrity text matching.** An edit to a line containing `tests_assert` raised a `TI-edit`
  event because the check counts it as an assertion line; leave such a doc line alone or dispute the
  event (see [Plan Server Delivery](plan-server-delivery.md) for the split-file rule).

## Summary Store Anchored at a Read-Denied Directory on macOS

**What happened:** every `loom plan summary` command inside a macOS planning session failed with `Failed to open dirfd at <loom home>/plan/<key>: Operation not permitted`; the same sessions worked on Linux.

**Why:** the session's `denyRead` covers the planning root and `allowRead` re-grants only `drafts/`, `claims/`, `summaries.lock` and `attachments/`. Linux srt covers the denied directory with a tmpfs and re-binds the grants, so the runtime directory itself still opens. macOS srt emits Seatbelt subpath rules, so opening the runtime directory returns EPERM while an open of a grant succeeds. The summary store anchored at `safe_open_dirfd(&paths.dir)`, and `open_safely` on macOS walked every intermediate directory. The Linux-only confinement test never caught it.

**Prevention:** a session-side open never anchors at or walks through a directory the session reaches only through carve-outs; open the grant in one symlink-refusing syscall. Never grant the runtime directory: it holds the server token in `lock`, plus `tokens/` and `sessions/`. On macOS, `O_NOFOLLOW | O_NOFOLLOW_ANY` together return EINVAL; pass `O_NOFOLLOW_ANY` alone.

**Fix:** `fs/safe_fs/single_open.rs` (openat2 on Linux, `O_NOFOLLOW_ANY` on Darwin 20+); the store anchors at the loom user directory through `RuntimePaths::base`/`below_base` (`planning/runtime.rs`); `ensure_runtime_dir` accepts an openable grant. Regression: `tests/planning_summary_seatbelt.rs`.

## Summary Sessions Denied Their Own Seal Commands

**What happened:** About 88 Bash calls in summary sessions, `seal` and `release` among them, were refused with `SUMMARY_DENY`. It looked intermittent because each retry that went through used a different command string.
**Why:** The summary instructions named the commands but not their shape. The installed CLAUDE.md template tells every session to append `; echo "exit=$?"`; a `$` expansion, a file redirect or `cd` matches no allow rule, so the Manual-mode session sent it to the PermissionRequest hook.
**Prevention:** A session kind with a narrow allow list carries instructions that pin the exact command form and outrank user and project command habits. When the deny log fills, compare the denied strings with their allowed retries before changing a rule.
**Fix:** A command-form paragraph in `planning/guidance/text/summary_brief.md`, `plan-plugin/skills/summaries/SKILL.md` and `plan-plugin/agents/summary.md`, guarded by `summary_instructions_keep_bash_to_the_allowed_command_form`.

## Sandbox Route and Autonomy Entries Recorded in Other Topics

A metaplan that denied the `Workflow` tool every ultracode stage runs on, a sibling-directory deny nested above a `config.worktree` deny that aborts bubblewrap, a probe control the Knowledge capsule does not grant, and metaplan reasons that went stale against the tree: [Sandbox Paths and Settings](sandbox-and-settings.md). A `rustfmt` edition and recursion, a filtered `loom check` that hid the relay line, and the scratch directory a read-only reviewer cannot read: [Brief and Dev-Server Gotchas](typed-config-values-process.md). An integration-verify suite criterion with no extended timeout: [Cargo Churn and Stall Kills](completion-runtime.md).

- [contract-phase-human-blocking](contract-autonomy-delivery.md#a-contract-freeze-refusal-waited-on-the-operator): a plan defect in the contract phase parked the stage on the operator. Rule: no stage waits on a human for something loom or an agent can resolve; a human is reached only after the automatic budgets run out.

- [codex-lane-rogue-wrapper](codex-lane-rogue-wrapper.md#direct-mode-codex-forwards-are-invisible-to-lifecycle-reconcile-and-status): direct-mode codex forwards (Linux nested-bubblewrap fallback) leave no companion job, so reconcile, lifecycle and the status model display all miss them.
