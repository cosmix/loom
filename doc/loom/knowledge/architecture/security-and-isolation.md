---
---
# Security And Isolation

> 4-layer worktree defense, security model, settings.local.json sites.

## Worktree Isolation (4-Layer Defense)

1. **Git layer** -- Separate worktrees at `.worktrees/<stage-id>/` with branch `loom/<stage-id>`. Symlinks: `.work` -> shared state, .claude/CLAUDE.md -> instructions, root `CLAUDE.md` -> project guidance.
2. **Sandbox layer** -- `MergedSandboxConfig` (`sandbox/config.rs`) generates `settings.local.json` with filesystem deny/allow policy, network domains, and fail-closed sandbox availability. Plan-configured `excluded_commands` are rejected; generated settings do not grant broad executable exemptions. Knowledge writes use the narrow Loom control path rather than direct file edits.
3. **Signal layer** -- Four stage-type-specific stable prefix generators in cache.rs (standard, knowledge, integration-verify, knowledge-distill). Include isolation rules and subagent restrictions.
4. **Hook layer** -- commit-guard.sh blocks exit without commit. commit-filter.sh blocks subagent git operations and subagent-verify-guard.sh blocks subagent full-suite verification, both gated on `loom_is_subagent()` (live-ancestor `LOOM_MAIN_AGENT_PID`, then a payload-first classification via `loom_payload_agent_verdict` — the intervening-Claude-process walk is only the fallback — not a PPID comparison).

## Security Model

- **ID validation**: Alphanumeric + dash/underscore, max 128 chars, no path traversal (validation.rs)
- **Acceptance criteria**: Runs arbitrary shell commands (trusted model)
- **Socket**: Mode 0o600 (owner only), max 100 connections, 10MB message limit, Unix only
- **Self-update**: minisign signature verification for binaries; `agents.zip`, `skills.zip`, and `CLAUDE.md.template` ARE SHA256-verified against the release checksums asset (self-update refuses to install an asset with no checksum entry). The asset-name mismatch an earlier version of this bullet described is fixed: `commands/self_update/mod.rs::checksum_asset` fetches `SHA256SUMS.txt`, the name the release workflow publishes (corrected 2026-09-03; `PLAN-embed-assets-and-complete-self-update` removes the config-asset download path entirely)
- **Shell escaping**: escape_shell_single_quote(), escape_applescript_string() in emulator.rs
- **permission_mode field** (`SandboxConfig` / `StageSandboxConfig`): Resolves as stage > plan > stage-type default. Default by stage type: ALL four stage types → `auto` (Knowledge, KnowledgeDistill, Standard, IntegrationVerify) — loom stages run autonomously with no human to answer prompts, so the agent auto-accepts actions its heuristics deem safe; the sandbox deny/allow rules are the safety boundary. Override to a stricter mode (`accept-edits`, `plan`) at plan or stage level if needed. **Delivery:** the resolved mode is passed as the `--permission-mode` CLI flag by `build_claude_command` at spawn — NOT via `permissions.defaultMode` in the worktree's `settings.local.json`, which Claude Code v2.1.142+ ignores for `auto` (a repo cannot grant itself auto mode; only the CLI flag or user/managed settings are honored). Auto mode itself requires a supporting account/model (Opus 4.6+/Sonnet 4.6+); loom's job is only to request it correctly. See entry-points.md and mistakes.md.

## Per-Worktree Gitignore for settings.local.json

After worktree creation, `.claude/settings.local.json` is appended (idempotently) to `<worktree>/.git/info/exclude`. Uses per-worktree exclude to avoid polluting the repo's `.gitignore`.

- Standard/IntegrationVerify/KnowledgeDistill: append to `<worktree>/.git/info/exclude`
- Knowledge stages: append to main repo's `.git/info/exclude` (no worktree created)
- The per-worktree exclude file lives at `<worktree>/.git/info/exclude` — NOT at `<worktree-dir>/.git/info/exclude` (the latter is a FILE pointing at the real gitdir, not a directory; the real exclude is at `<repo>/.git/worktrees/<stage-id>/info/exclude`)

## Claude Code Worktree Isolation Disabled in Generated Settings

Loom owns the per-stage git worktree, so it disables Claude Code's _own_ worktree
isolation (`worktree.bgIsolation`) in every settings file it generates. Claude
Code's default (`"worktree"`) blocks Edit/Write in the checkout until
`EnterWorktree`, which would push subagents into nested worktrees on top of loom's
— leaving stray branches and tangled checkouts. Loom emits `"none"` so subagents
edit the loom worktree directly (Claude Code v2.1.143+; older versions ignore it).

Two write sites, both targeting `settings.local.json` (never the committed
`settings.json`, to avoid imposing on non-loom teammates):

- **Worktree stage sessions** — `sandbox/settings.rs:generate_settings_json()`
  emits a top-level `"worktree": { "bgIsolation": "none" }` block. Survives the
  `merge_existing_permissions()` step, which only touches `permissions.*`.
- **Main-repo sessions** (knowledge stages, interactive) —
  `fs/permissions/settings.rs:ensure_loom_hooks_local()` sets it idempotently
  alongside the agent-teams env var.

## Worktree Membership Is Anchored to the END of the Path (2026-09-12)

`loom-hooks/_common.sh`'s `loom_current_worktree` and `loom-hooks/loom-control-complete.sh` used an
unanchored pattern (`.worktrees/[^/]+`) to decide whether a path is inside a loom worktree, and
Rust's `is_loom_worktree_path` only checked that a `.worktrees` segment appeared somewhere in the
path. Because `LOOM_WORKTREE_PATH` is always the worktree root end-to-end (`stage_executor.rs`'s
`resolve_worktree` → `spawn_setup.rs`'s `get_or_create_worktree` → `operations.rs`'s
`repo_root/.worktrees/<stage_id>`, passed as the spawned session's `cwd`), any path _nested
inside_ that root — the repository checked out again a level down, or a `TMPDIR` placed there —
also matched and was miscounted as its own worktree root.

**Fix:** anchor the pattern to the end of the path — `/\.worktrees/[^/]+/?$` in the shell hooks,
and "parent directory is literally named `.worktrees`" in `is_loom_worktree_path`. A path several
segments below a real worktree root no longer counts as one; nested worktree resolution now
selects the innermost stage.

## Where a Session's Write Grants Come From (STALE, corrected 2026-09-13)

This section used to say a generated `.claude/settings.local.json` decides what a session may
write, with `sandbox::write_settings` writing it (worktree for stage sessions, main checkout for
knowledge/merge/adjudication sessions) and `fs/permissions/sync.rs::sync_worktree_permissions`
copying a worktree's allow rules back into the main checkout's file. All three writes are gone
(`doc/plans/PLAN-loom-state-confinement.md`, owner decision 8): `write_settings` is deleted — only a
test helper of the same name remains, in `orchestrator/terminal/native/tests_confinement_srt.rs` —
and `sync_worktree_permissions` no longer writes any `.claude/settings.local.json`.

Every session kind now launches from a generated **capsule**, `W/capsules/<session-id>.settings.json`,
built by the pure `sandbox::settings::build_settings`. Approved permissions live in a loom-owned
list, `W/permissions/approved.json` (`fs/permissions/approved.rs`), rendered into every later
session's capsule. Home-directory control surfaces are spelled `~/...` in both the sandbox and
permission layers of every capsule; repo and executable-dir surfaces are absolute in `denyWrite` and
`//abs` in `Edit` rules. `~/.codex/{hooks,hooks.json,config.toml}` are denied in every capsule,
whether or not the codex lane is licensed for that session. `T/.loom` is denied in the sandbox layer
only — `Edit(.loom/**)` already covers the Claude Code file tools there, so the sandbox-only deny is
defense in depth against a tool that bypasses `Edit`, not a gap. Two knowledge-related functions were
renamed to match the new shape: `write_knowledge_sandbox_settings` became `validate_knowledge_sandbox`
and `install_knowledge_hooks` became `require_knowledge_hooks`, since neither writes a settings file
any more.

Claude Code still writes a "don't ask again" approval to `<canonical git root>/.claude/settings.local.json`
even for a capsule-launched session (destination `localSettings`; confirmed against the 2.1.269
bundle) — loom cannot prevent that write, so it never writes that file itself, and
`fs/permissions/sync.rs`'s fold-back reads it (and the worktree's own settings) after a session ends.
The fold-back, in order:

1. rewrites a single-leading-slash `Edit`/`Read`/`Write`/`NotebookEdit` rule to cwd-relative form
   (`normalize_single_slash_path`) — in a `--settings` file `/p` resolves relative to the settings
   source (`W/capsules/` for a capsule), not the project root, so an unrewritten rule would resolve
   to the wrong place in the next session;
2. drops inert `Write(...)` rules (`is_inert_write_permission`);
3. rewrites worktree-specific paths (`../`, `.worktrees/`) to their portable form
   (`transform_worktree_path`);
4. runs the result through a control-surface filter that drops any rule granting writes to `.loom`,
   `.claude`, `.worktrees`, a hook directory, `~/.loom` or the scratch root, so an approval can never
   widen the next session's sandbox onto loom's own control surfaces.

Accepted gap: the filter reads rule text only, so a rule naming a symlink into a control surface
still passes it — the phase-3 OS deny rules close this in practice, since a deny wins over any allow
and the sandbox resolves symlinks. See
[Accepted Gaps From the State-Confinement Work](../concerns/sandbox-and-confinement-gaps.md#accepted-gaps-from-the-state-confinement-work-2026-09-13).

The main checkout's `.claude/settings.local.json` is still shared and still read by knowledge-stage
spawns, merge and adjudication sessions, and the operator's own interactive sessions. On 2026-09-13
it held the running plan's `allow_write` list and `env.LOOM_WORK_DIR` pointing at the live state
directory, so an operator session inherited both, and anything it ran that honors `LOOM_WORK_DIR`
targeted live state. See [Live State Pollution](../mistakes/live-state-pollution.md). That specific
exposure predates the capsule work above and is unrelated to it — loom's own writes to that file are
gone, but the file's pre-existing contents and Claude Code's own approval writes remain.

## Session Requests Travel Through a Hook-Written Inbox (2026-09-13)

A sandboxed session cannot write loom state directly (owner decision 3). Every write request —
memory notes, knowledge edits, stage completion, adjudication verdicts — goes by reference: the CLI
writes a ticket into `$LOOM_SCRATCH_DIR` and prints one line ending `LOOM_RELAY_V1` naming the
ticket id and its SHA-256, with no path in the line, so fixture output, docs and echoed transcripts
that merely print the line do nothing.

`loom-hooks/loom-relay.sh` (PostToolUse, matcher `Bash`) runs a fast bash check on the command's
shell tokens, then hands off to `loom hook relay`, which proves the request came from the session it
claims by process ancestry, verifies the ticket against the printed hash, and writes the entry to
`W/inbox/<session-id>/`. The daemon's `drain_session_inboxes` applies each entry at most once against
a per-session ledger.

Modules: `loom/src/relay/*` (protocol: kind, line, ticket, payload, inbox, matrix, scratch),
`fs/inbox/*` (layout, ledger, dedupe), `commands/hook/relay*` (the `loom hook relay` CLI),
`orchestrator/core/inbox_drain/*` (the daemon side), `commands/request/*` (`loom request status`,
for polling a request's outcome). See
[Inbox Ledger Has Two Rows Per Request Id](../mistakes/concurrency-and-locking.md#inbox-ledger-has-two-rows-per-request-id--a-lookup-must-take-the-latest-2026-09-13)
before writing any code that looks a request up by id.

## Spawn Preflight and the Merge Gate (2026-09-13)

`sandbox::validate_config` (`sandbox/config.rs:167`) refuses three settings unconditionally, with no
plan-author acknowledgement possible: `permission_mode: bypass-permissions`, `sandbox.enabled: false`
and `sandbox.allow_unsandboxed_escape: true`. The error is a typed `SandboxPreflightRefusal`
(`sandbox/config/preflight.rs`), so a spawn it stops is recorded as `SandboxSetupFailure`
(`crash_classification::spawn_failure_type`), not a generic error.

`loom init` refuses an unconfined plan the same way, through `refuse_unconfined_sandbox`, which
calls the same `sandbox::preflight::sandbox_policy_refusals` `loom run` uses — the interactive
confirmation gate an unconfined plan used to get at `init` time is gone.

`commands/run/confinement.rs::require_confinement`, called from both `loom run` entry points'
shared `run_startup_preflights` (`commands/run/mod.rs:115`), runs checks 1-4 (`validate_config`;
every installed hook script present, `loom-relay.sh` included; `LOOM_BIN`/the hooks directory outside
every writable root; `/bin/bash` and every `hook_path` tool resolve outside every writable root) plus
the `R/loom-hooks/**` rule and the loom-written-keys check `repair --fix` also uses. Every spawn
repeats checks 2-4 in `native/launch.rs` before writing anything, using `HostFacts`
(`sandbox/config/preflight.rs`) — one struct giving a spawn exactly the host facts a launch sees.
`LOOM_BIN` must be owned by the operator or root, not group- or world-writable, and outside every
writable root — an executable-dir deny skips any directory that is, or contains, a session-writable
root: R, T, the scratch root, every grant, compared on canonicalized paths
(`sandbox/control_surfaces/session_denies.rs::is_ancestor_of_writable_root`). The hard-link warning
covers operator-owned executables only.

The merge gate (`orchestrator/core/merge_handler/merge_gate.rs`) runs before an automatic merge and
before a merge-resolution session is spawned: a branch whose diff touches `.claude/**`, `.mcp.json`,
`.loom/**` or the git hooks directory is not merged, and its stage moves to `NeedsHumanReview` naming
the paths. A preflight refusal during a merge-resolution spawn routes the stage to
`NeedsHumanReview` with `SandboxSetupFailure` (`merge_spawn_block_reason`,
`report_merge_spawn_failure`); other spawn errors still retry as before —
`route_to_human_review` takes an `Option<FailureType>`, and the gate and the phantom-merge hold both
pass `None`. `loom stage merge <id>` calls `git::merge::merge_stage` directly and stays the
operator's ungated path, by design.

The hooks directory comes from a SCOPED read of `core.hooksPath` (local, then global, then system) —
loom's own git runner prepends `-c core.hooksPath=/dev/null` to every git call it makes
(`git/runner.rs`, owner decision 10), which silently wins over an UNSCOPED read of the same key. See
[`-c core.hooksPath=/dev/null` Silently Overrides a Scoped Read](../mistakes/sandbox-and-settings.md#-c-corehookspathdevnull-silently-overrides-a-scoped-corehookspath-read-2026-09-13).
