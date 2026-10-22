---
---
# Security And Isolation

> 4-layer worktree defense

## Worktree Isolation (4-Layer Defense)

1. **Git layer** -- Separate worktrees at `.worktrees/<stage-id>/` with branch `loom/<stage-id>`. Symlinks: `.loom/work` -> shared state, .claude/CLAUDE.md -> instructions, root `CLAUDE.md` -> project guidance.
2. **Sandbox layer** -- `MergedSandboxConfig` (`sandbox/config.rs`) generates `settings.local.json` with filesystem deny/allow policy, network domains, and fail-closed sandbox availability. Plan-configured `excluded_commands` are rejected; generated settings do not grant broad executable exemptions. Knowledge writes use the narrow Loom control path rather than direct file edits. The OS sandbox confines Bash only; the file tools are confined by the hook layer, and other tools are listed in [Sandbox and Confinement Gaps](../concerns/sandbox-and-confinement-gaps.md), "Tool Routes That Run Outside the Bash Sandbox".
3. **Signal layer** -- Four stage-type-specific stable prefix generators in cache.rs (standard, knowledge, integration-verify, knowledge-distill). Include isolation rules and subagent restrictions.
4. **Hook layer** -- commit-guard.sh blocks exit without commit. worktree-file-guard.sh and credential-guard.sh confine the file tools. commit-filter.sh blocks subagent git operations and subagent-verify-guard.sh blocks subagent full-suite verification, both gated on `loom_is_subagent()` (live-ancestor `LOOM_MAIN_AGENT_PID`, then a payload-first classification via `loom_payload_agent_verdict` — the intervening-Claude-process walk is only the fallback — not a PPID comparison).

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
isolation (`worktree.bgIsolation`) in every settings document it generates. Claude
Code's default (`"worktree"`) blocks Edit/Write in the checkout until
`EnterWorktree`, which would push subagents into nested worktrees on top of loom's
— leaving stray branches and tangled checkouts. Loom emits `"none"` so subagents
edit the loom worktree directly (Claude Code v2.1.143+; older versions ignore it).

Two write sites:

- **Loom-spawned sessions** — `sandbox/settings.rs::generate_settings_json` emits a
  top-level `"worktree": { "bgIsolation": "none" }` block, and `sandbox::build_settings`
  carries it into the session capsule
  (`orchestrator/terminal/native/session_settings/contents.rs::capsule_settings`).
  `merge_existing_permissions` only touches `permissions.*`, so the block survives it.
- **Interactive sessions in the main repo** —
  `fs/permissions/settings.rs::ensure_loom_hooks_local` sets it idempotently in
  `.claude/settings.local.json` (never the committed `settings.json`, to avoid
  imposing on non-loom teammates), alongside the agent-teams env var.

## Where a Session's Write Grants Come From

Loom's spawn path writes no `.claude/settings.local.json`; only `loom repair` writes the main checkout's file (via `ensure_loom_hooks_local`). Every session kind launches from a generated
**capsule**, `W/capsules/<session-id>.settings.json` (`orchestrator/terminal/native/session_settings.rs`),
built by the pure `sandbox::settings::build_settings`. Approved permissions live in a loom-owned
list, `W/permissions/approved.json` (`fs/permissions/approved.rs`), rendered into every later
session's capsule. Home-directory control surfaces are spelled `~/...` in both the sandbox and
permission layers of every capsule; repo and executable-dir surfaces are absolute in `denyWrite` and
`//abs` in `Edit` rules. `~/.codex/hooks`, `~/.codex/hooks.json` and `~/.codex/config.toml` are all
denied in every capsule, whether or not the codex lane is licensed for that session. `T/.loom` is
denied in the sandbox layer only — `Edit(.loom/**)` already covers the Claude Code file tools there,
so the sandbox-only deny is defense in depth against a tool that bypasses `Edit`. Knowledge-stage
setup (`orchestrator/core/spawn_setup.rs`) only checks: `validate_knowledge_sandbox` and
`require_knowledge_hooks` write no settings file.

Three groups of denies are added to every capsule beyond the state root and the control surfaces:

- **Git administrative directories.** `<common>/hooks` and `<common>/config`, every sibling worktree's administrative directory `<common>/worktrees/<name>` whole, and the session's own `config.worktree`, for the worktrees registered at spawn (`session_denies` and `deny_worktree_admin_dirs` in `sandbox/control_surfaces/session_denies.rs`, on the split from `partition_admin_dirs`; [Loom's `config.worktree` Deny and Check](#looms-configworktree-deny-and-check)).
- **Credential reads.** `sandbox.filesystem.denyRead` carries the credential directories, nine credential files and, without the codex lane, `~/.codex/auth.json` (`policy::mandatory_deny_read(codex_licensed)`; [Sandbox and Confinement Gaps](../concerns/sandbox-and-confinement-gaps.md#credential-reads-denied-files-plain-paths-and-what-stays-readable)).
- **Tool routes.** `permissions.deny` names `LSP`, `RemoteTrigger`, `CronCreate` and `PushNotification` in every capsule, and `Workflow` in every capsule except the Stage session of an `ultracode: true` stage (`SESSION_TOOL_DENIES` and `capsule_deny_rules` in `orchestrator/terminal/native/session_settings/contents.rs`; `launch.rs::workflow_licensed`). Bare tool names rather than `--disallowedTools`, which is variadic and would consume the positional prompt ([Tool Routes That Run Outside the Bash Sandbox](../concerns/sandbox-and-confinement-gaps.md#tool-routes-that-run-outside-the-bash-sandbox)).

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
[Accepted Gaps From the State-Confinement Work](../concerns/sandbox-and-confinement-gaps.md#accepted-gaps-from-the-state-confinement-work).

The main checkout's `.claude/settings.local.json` is shared: knowledge-stage spawns, merge and
adjudication sessions, and the operator's own interactive sessions all read it. Loom never writes
it, but whatever it holds (including Claude Code's own approval writes and any `env.LOOM_WORK_DIR`)
reaches those sessions. See [Live State Pollution](../mistakes/live-state-pollution.md) for the
exposure this caused.

## Session Requests Travel Through a Hook-Written Inbox

A sandboxed session cannot write loom state directly (owner decision 3). Every write request goes by reference: the CLI writes a ticket into `$LOOM_SCRATCH_DIR` and prints one line ending `LOOM_RELAY_V1` naming the ticket id and its SHA-256, with no path in the line, so fixture output, docs and echoed transcripts that merely print the line do nothing. The twelve relayed kinds (`relay/kind.rs`) are `memory`, `block`, `dispute`, `handoff`, `merge-resolved`, `verdict`, `telemetry`, `freeze-contracts`, `file-dispute`, `commit`, `check` and `complete`. Nine of them, every kind except `memory`, `telemetry` and `check`, are control kinds that only the session's lead process may relay (`RequestKind::is_control`). `commit` carries a session's staged index (message, the HEAD and tree it saw) for the daemon to commit and sign; see [Daemon-Owned Commits](daemon-owned-commits.md).

**Stage completion is a host job.** The completion command in a session relays a `complete` ticket (`commands/stage/complete_relay.rs`). The daemon's inbox drain (`orchestrator/core/inbox_drain/complete.rs`) queues a `Complete` host job for it and settles the ticket with the job id. The daemon's runner starts `loom stage verify-host` on the host, outside the session sandbox, and the daemon completes the stage only from that job's passing run (`host_verify/apply.rs`). No token, socket credential or nonce carries completion. The trusted PreToolUse pin `loom-hooks/loom-control-complete.sh` allows only the one exact completion command, holds no verdict and writes the completion heartbeat. A legacy session is refused, and an operator's shell completes locally. Details: [Host Verification Jobs](verification-v2-gates.md).

`loom-hooks/loom-relay.sh` (PostToolUse, matcher `Bash`) runs a fast bash check on the command's shell tokens, then hands off to `loom hook relay`, which proves the request came from the session it claims by process ancestry, verifies the ticket against the printed hash, and writes the entry to `W/inbox/<session-id>/`. The daemon's `drain_session_inboxes` applies each entry at most once against a per-session ledger.

**Leftover tickets are swept.** A ticket's line can fail to reach the hook: stdout redirected or piped through `tail`, a script file, a background call, or more than `MAX_LINES_PER_CALL` (16) lines in one output. `loom hook relay` therefore relays its own lines and then every other `<id>.req` still in the scratch directory whose kind is in `--allowed-kinds` and is NOT a control kind (`commands/hook/relay/sweep.rs`), so only the non-control kinds (`memory`, `telemetry`, `check`) are swept. A control ticket stays bound to its own line: one a subagent wrote is refused, and sweeping it during a later main-agent call would relay the subagent's block, handoff or verdict under the main agent's authority. A swept ticket has no announced hash; `ticket_check::read_unlisted` requires the same file shape (single-link regular file, no symlink, owner uid, size cap) and a decoded id equal to the file stem. The hash never bound a ticket to anything the session could not produce itself; the session proof and the command-derived kinds authorise a relay, and both apply to the sweep. The hook's fast path lets any payload containing ` memory ` through so the helper runs even when the line was hidden, and stays silent unless a relay line was present. History: [Memory Relay Drain Gap](../mistakes/memory-relay-drain-gap.md).

Modules: `loom/src/relay/*` (protocol: kind, line, ticket, payload, inbox, matrix, scratch), `fs/inbox/*` (layout, ledger, dedupe), `commands/hook/relay*` (the `loom hook relay` CLI), `orchestrator/core/inbox_drain/*` (the daemon side), `commands/request/*` (`loom request status`, for polling a request's outcome). See [Inbox Ledger Has Two Rows Per Request Id](../mistakes/concurrency-and-locking.md#inbox-ledger-has-two-rows-per-request-id--a-lookup-must-take-the-latest-2026-09-13) before writing any code that looks a request up by id.

## Loom's `config.worktree` Deny and Check

>
> config.worktree deny and check

`partition_admin_dirs` (`git/worktree/config_worktree.rs`) lists `<common>/worktrees/*` when the capsule is built and splits the entries by the session's location: the entry whose `gitdir` records `<worktree>/.git` is the session's own, every other entry is a sibling. `deny_worktree_admin_dirs` (`sandbox/control_surfaces/session_denies.rs`) then denies each sibling directory whole and the own entry's `config.worktree` alone. The own entry's `HEAD`, `index` and `logs/` stay writable, because `git add` writes its `index`; `config.worktree` is denied because, with `extensions.worktreeConfig`, git reads it and it could name a command the daemon's pinned git runs. Nothing is denied beneath a sibling directory: bubblewrap cannot create a mount point inside a read-only bind, and a sibling's entry vanishes when its stage merges. The common directory, `refs/` and the object store stay writable.

The split follows the session's cwd, never its kind. A session whose cwd is a stage worktree (Stage, Contract and Merge sessions; an adjudication judge while its disputed stage's `.worktrees/<id>` exists, `judge_cwd` in `orchestrator/adjudication/session.rs`) gets the own entry plus the siblings. A checkout-rooted session (knowledge, base-conflict, and a judge once the worktree is gone: `.git` is a directory and Claude Code denies only `.git/hooks` and `.git/config`) has no own entry, so every entry is a sibling and every `config.worktree` is denied inside a whole-directory deny. For a linked-worktree session the own `config.worktree` deny duplicates Claude Code's own `worktrees/*/config.worktree` deny. `sandbox/settings.rs::target_is_worktree` decides whether a capsule is worktree-rooted, from the path and the state-root symlink.

A worktree-rooted capsule whose worktree no registry entry names, or that two entries name, refuses the spawn: the capsule cannot tell which directory is the session's own. A registry with no entries denies nothing. The residuals are in [Residuals of the Sibling Administrative Directory Denies](../concerns/sandbox-and-confinement-gaps.md#residuals-of-the-sibling-administrative-directory-denies).

A worktree registered after a session spawned is covered by none of these denies, so the daemon checks before running git there: `WorktreeGit::run` (`git/worktree/pinned.rs`) calls `check_worktree_config`. When the main repository enables `extensions.worktreeConfig` and `<admin>/config.worktree` is non-empty, the check lists its keys as data (`git config --file .. --name-only --list` in the main repository) and refuses unless every key is one git itself writes there: `core.sparseCheckout`, `core.sparseCheckoutCone`, `index.sparse`, and the keys `git worktree add` copies from a sparse main worktree. An empty regular file is accepted without spawning git; a symlink goes through the full check. The remaining race is recorded in [Sandbox and Confinement Gaps](../concerns/sandbox-and-confinement-gaps.md).

The srt confinement e2e (`orchestrator/terminal/native/tests_confinement_e2e.rs`, `tests_confinement_srt.rs`) emulates Claude Code's linked-worktree grant and its denies, so stage probes run against what production grants; the bubblewrap probe `tests_capsule_denies::bwrap` proves the sibling-directory denies and the own-directory control on the emitted capsule ([Sandbox and Confinement Gaps](../concerns/sandbox-and-confinement-gaps.md#sandbox-denial-has-no-end-to-end-ci-canary)).

## Worktree Membership Is Anchored to the END of the Path

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

## Spawn Preflight and the Merge Gate

`sandbox::validate_config` (`sandbox/config.rs:167`) refuses three settings unconditionally, with no
plan-author acknowledgement possible: `permission_mode: bypass-permissions`, `sandbox.enabled: false`
and `sandbox.allow_unsandboxed_escape: true`. The error is a typed `SandboxPreflightRefusal`
(`sandbox/config/preflight.rs`), so a spawn it stops is recorded as `SandboxSetupFailure`
(`crash_classification::spawn_failure_type`), not a generic error.

`loom init` refuses an unconfined plan the same way, through `refuse_unconfined_sandbox`, which
calls the same `sandbox::preflight::sandbox_policy_refusals` `loom run` uses; `init` asks for no
interactive confirmation.

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

The merge gate is the `MergeGate` argument of `git::merge::merge_stage`: under `MergeLock`,
`MergeGate::Enforce` holds a branch whose merge-base or landing diff touches `.claude/**`,
`.mcp.json`, `.loom/**` or the git hooks directory (`MergeResult::Held`), and the caller routes the
stage to `NeedsHumanReview` naming the paths. `orchestrator/core/merge_handler/merge_gate.rs` adds
fail-open pre-filters before an automatic merge and before a merge-resolution session is spawned. A
preflight refusal during a merge-resolution spawn routes the stage to `NeedsHumanReview` with
`SandboxSetupFailure` (`merge_spawn_block_reason`, `report_merge_spawn_failure`); other spawn errors
retry — `route_to_human_review` takes an `Option<FailureType>`, and the gate and the phantom-merge
hold both pass `None`. Every CLI merge path enforces the gate: `loom stage merge <id>`
(`commands/stage/merge.rs` and `merge/landing.rs` pass `MergeGate::Enforce`), local `--resolved`,
and the progressive merge of a CLI stage completion (`commands/stage/progressive_complete.rs`).
Only `loom stage human-review --force-complete` passes `MergeGate::Bypass`, the operator's override.
Full rules: [The merge gate](merge-flow.md#the-merge-gate).

The hooks directory comes from a SCOPED read of `core.hooksPath` (local, then global, then system) —
loom's own git runner prepends `-c core.hooksPath=/dev/null -c core.fsmonitor=false` to every git call
it makes (`git/runner.rs` `NO_HOOKS_ARGS`, owner decision 10), which silently wins over an UNSCOPED read of the same key. See
[`-c core.hooksPath=/dev/null` Silently Overrides a Scoped Read](../mistakes/sandbox-tooling-and-network.md#-c-corehookspathdevnull-silently-overrides-a-scoped-corehookspath-read-2026-09-13).
