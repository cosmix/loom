---
---
# Security Sandbox And Hooks

> Hooks, input validation, sandbox config

## Hook Patterns

Hooks receive data via **stdin JSON**. Read with `timeout 1 cat`. Response: exit 0 = allow, exit 2 = block (stderr shown). Advanced JSON response supports `permissionDecision: allow/deny/ask` with `updatedInput`.

**Key hooks**: commit-guard.sh (Stop) blocks exit without commit; commit-filter.sh (PreToolUse:Bash) blocks subagent commits; subagent-verify-guard.sh (PreToolUse:Bash) blocks subagent full-suite verification; plans-path-guard.sh (PreToolUse:Edit/Write) blocks plan writes outside `doc/plans/`; prefer-modern-tools.sh blocks grep/find; post-tool-use.sh updates heartbeat; pre-compact.sh triggers handoff; session-start/end.sh handle lifecycle.

**Subagent detection**: Wrapper script exports `LOOM_MAIN_AGENT_PID`. `loom_is_subagent()` requires that PID to be a live ancestor, then classifies the caller payload-first via `loom_payload_agent_verdict` (`.agent_type`/`.transcript_path`); an intervening-Claude-process walk is only the fallback for a payload-less or unrecognized caller — it is not a `$PPID` comparison. Subagents are blocked from git mutation and stage completion.

Hook installation: scripts embedded via `include_str!()` in constants.rs, installed to `~/.claude/hooks/loom/`, config in `.claude/settings.local.json`.

## Security Patterns

**Input validation**: `validate_id()` - alphanumeric + dash/underscore, max 128 chars, reserved names blocked. `safe_filename()` strips traversal. **Shell escaping**: `escape_shell_single_quote()` and `escape_applescript_string()` in emulator.rs. **Self-update**: minisign signature verification (50MB binary, 4KB sig), atomic install via temp->backup->rename->rollback. **Env var expansion**: positional replacement to handle overlapping names ($FOO vs $FOOBAR).

## Permission Sync Pattern

Three-component: path transformation (absolute->relative, parent traversal resolved), merge-not-overwrite (union+dedup), sync before acceptance. File locking via fs2 crate; always write to the locked handle.

## Sandbox Config Merging

Plan-level `SandboxConfig` merges with stage-level policy, with stage values overriding plan values. Plan-configured `excluded_commands` are rejected outright; sandbox disablement and unsandboxed escape require explicit policy acknowledgement or are rejected. Generated settings emit OS-level `denyRead` for sensitive paths and set `failIfUnavailable: true` whenever the sandbox is enabled, and `loom run` refuses to start on Linux/WSL when `bwrap`/`socat` are missing or the kernel is WSL1, since that setting would otherwise make every session exit at startup. A settings-write failure blocks the stage before spawn. Loom, Git, interpreters, build tools, and package managers are never granted prefix-wide unsandboxed Bash access.

## Sandbox permission_mode Resolution

`permission_mode` resolves: stage-level > plan-level > stage-type default.

| Stage type        | Default permission_mode |
| ----------------- | ----------------------- |
| Standard          | `auto`                  |
| IntegrationVerify | `auto`                  |
| Knowledge         | `auto`                  |
| KnowledgeDistill  | `auto`                  |

All four stage types default to `auto` as of 2026-07-01 (previously `accept-edits`). Loom stages execute autonomously with no human at the terminal, so the agent auto-accepts actions its heuristics deem safe; the sandbox filesystem deny/allow rules and hooks are the safety boundary. Override at plan or stage level with a stricter `permission_mode` (e.g. `accept-edits`, `plan`) if needed.

YAML key is `permission_mode` (snake_case), values are kebab-case: `"auto"`, `"accept-edits"`, `"plan"`, `"default"`.

## One Flattener for an Untrusted Value Rendered on Many Surfaces

`context/untrusted.rs::inline_safe` is the single definition now shared by THREE surfaces: the two
agent-facing renderers — the Knowledge Brief (`orchestrator/signals/format/brief.rs`) and `loom knowledge context`'s
stdout (`commands/knowledge/context.rs`) — plus one operator-facing surface added later, the `loom
status` payload (`commands/status/data/sanitize.rs`, applied via `commands/status/data/collector.rs:364` to every untrusted
string reaching `StatusData`: stage/session ids, `model`, `last_tool`, `last_activity`, evidence lines).
Its docstring states outright that a second copy would duplicate a security rule that must never drift.

## Ask Which Surfaces Render the Type, Not Who Copied the Helper

`context/untrusted.rs:5-8` names its call sites in a doc comment — "this has exactly two
call sites, do not add a third copy". That does not prevent a THIRD SURFACE from having
ZERO copies. `loom map` was rewritten into an agent-facing renderer of the same
graph-derived strings — scopes, paths, ids, and a `ParseError` detail built from a raw
line of the offending source file — and flattened none of them.

**Rule:** when a new command renders values an existing renderer flattens, the review
question is "which surfaces render this TYPE?", not "did anyone copy the helper?" — grep
for the type's FIELDS, not for the helper's name. And when you do flatten, route every
variant through it, not only the one that is currently attacker-controlled: uniform
treatment is free (`inline_safe` passes fixed-format strings through unchanged by its own
contract) and it avoids an asymmetry that will catch out whoever adds the next variant.

## Two Guards That Disagree on the One Behavior That Matters Stay Separate

`loom-hooks/worktree-file-guard.sh` and `loom-hooks/credential-guard.sh` share almost all of their
canonicalization logic but diverge on the one thing that decides correctness: `worktree-file-guard`
must REJECT a symlink leaf, `credential-guard` must FOLLOW one (so a worktree's `.loom/work/admin.token`
resolves through the state-root symlink to the main repo's real file). A shared helper with two
callers that disagree on that behavior would need a mode flag threading through every call site.
When two near-duplicate pieces of logic differ only on the safety-relevant branch, duplicating the
few lines that matter is clearer than parameterizing a shared function to hide the disagreement.

## Every Host-Side Open of an Agent-Writable Worktree Path Must Use `fs::safe_read::open_regular_no_follow` (2026-09-10)

The host daemon runs trusted and outside the sandbox, but polls several paths a sandboxed
stage session can write: `fs/memory/spool.rs` (`drain_spool`), `telemetry/spool.rs`
(`drain_into_events`), and `fs/stage_request/spool.rs` (`drain_spool`) all open a
`<worktree>/*-spool.jsonl` file the sandboxed agent controls, read it, then `set_len(0)` to
truncate it. Opening with plain `OpenOptions` follows symlinks: a stage that plants the spool
path as a symlink gets the trusted daemon to truncate any file its own user can write, every
poll cycle (~5s). All three now open via `fs::safe_read::open_regular_no_follow(worktree_root,
relpath, flags)`, which refuses a planted symlink instead of following it (`fs/safe_fs.rs`'s
`O_NOFOLLOW`-at-every-component primitives back it). `context/refresh/source_graph/enumerate.rs`
and `generation.rs` were fixed the same way, so a tracked or untracked symlink can no longer
make the host read a file outside the worktree into the graph.

Prevention: any new host-side read or write of a worktree-writable path goes through
`fs::safe_read`/`fs::safe_fs`, never `std::fs`/`OpenOptions` directly — the daemon trusts its
own code, not the worktree's contents.

## A Worktree's `.git` File Is Agent-Writable Too, So Daemon-Side Git Never Follows It (2026-09-25)

A linked worktree's `.git` is a plain file naming its administrative directory; the stage agent
that controls the worktree can rewrite it to name a directory of its own choosing, whose
`config` could define a `filter.<name>.clean` git runs on every `diff`/`status` that re-hashes a
file. Any git the daemon starts with default discovery in a stage worktree would run that filter
outside the sandbox. `git/worktree/pinned.rs::WorktreeGit::pinned` closes this the same way
`fs::safe_read::open_regular_no_follow` closes a symlinked spool path above: it never lets the
worktree decide where its own configuration comes from. It resolves the worktree's registered
entry under the main repository's `<common-dir>/worktrees/` itself, then sets `GIT_DIR`,
`GIT_WORK_TREE` and `GIT_COMMON_DIR` from those resolved paths, so repository configuration comes
from the main repository's `config` alone (plus the user's global and system files, keeping
filters such as git-lfs working). `WorktreeGit::discovered` — trusting the worktree's own `.git` —
stays correct only for a process that already runs with the writing agent's own privileges: the
agent's own CLI inside its sandbox, never the daemon or a host-side hook.

**Prevention:** any process outside a stage's sandbox that runs git in a stage worktree pins it
(`WorktreeGit::pinned`/`pinned_in_project_of`), never discovers it; a stage's own process may
discover, since it already has no more privilege than the worktree it would misconfigure.

## A Local Fallback Needs Proof That No Daemon Runs (2026-09-25)

`verify::review::observer::source` computes a stage's change fingerprint itself only when it holds
POSITIVE evidence no daemon owns the project: nothing answers on `.loom/work/orchestrator.sock`
(`DaemonReach::NotListening`) AND the daemon's singleton lock is a regular file this process can
`flock` (`DaemonServer::proven_stopped`). A missing socket alone proves nothing — a sandbox can
hide or deny the path of a socket that exists — and neither does a missing lock file, for the same
reason. Every other outcome, including a permission error opening the socket, is the typed
`DaemonUnreachable` error, and the caller fails closed rather than silently computing a value that
could disagree with the daemon's. The same asymmetry as the credential-narrowing entries in
`mistakes/sandbox-state-channels.md`: absence of evidence for a denial is not evidence of absence.

## Re-Export a `pub(crate)` Helper Into a New Caller Instead of Re-Encoding Its Logic (2026-09-13)

`fs/permissions/drift.rs::flatten_hook_triples` already turned a hooks JSON value into the
`(event, matcher, command)` triples the drift check compares. When the spawn preflight's
`sandbox/config/preflight/hook_commands.rs` needed the same flattening to check installed hook
commands, it imported the helper (`fs/permissions/mod.rs:33` re-exports it as `pub(crate)`) rather
than writing its own walk of the same JSON shape. A visibility bump plus a re-export is cheaper than
a second copy that can drift from the first — see
[Duplicated Extension-to-Language Table](../concerns/code-quality-and-hook-debt.md#duplicated-extension-to-language-table-2026-08-17)
for what happens when nobody does this.
