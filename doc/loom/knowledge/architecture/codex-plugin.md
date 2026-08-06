# Codex Plugin

> Topic notes for the architecture knowledge area.

## Install and identity

- Marketplace `openai-codex` from GitHub `openai/codex-plugin-cc`; plugin name `codex`.
- Observed installed version **1.0.6** (`claude plugin list --json`, 2026-08-06), install path
  `~/.claude/plugins/cache/openai-codex/codex/<version>/`.
- Needs the `codex` CLI on PATH (observed `codex-cli 0.146.0`) and an authenticated `~/.codex/auth.json`.
  Codex initialises a sqlite state runtime under `~/.codex`; a sandbox that denies writes there kills
  every run with `failed to initialize state runtime` (exit 1) before any model call is made.

Non-interactive CLI:

```bash
claude plugin marketplace add openai/codex-plugin-cc    # add <source>
claude plugin install codex@openai-codex --scope user   # user|project|local, default user
claude plugin list [--json]
```

## The codex-rescue subagent

Spawn via the Agent tool with `subagent_type: "codex:codex-rescue"`.

Its frontmatter (`agents/codex-rescue.md`) declares `model: sonnet`, `tools: Bash`. **That sonnet is the
THIN FORWARDING WRAPPER, not the implementing model** — the real work runs in Codex behind the companion
script. Do not read that `sonnet` as the quality tier of the result.

Wrapper contract (all from `agents/codex-rescue.md`):

- Exactly ONE Bash call to `node "${CLAUDE_PLUGIN_ROOT}/scripts/codex-companion.mjs" task ...`, and it
  returns that stdout **verbatim**, with no commentary before or after.
- It forwards `--model` / `--effort` **ONLY when the request names them** ("Leave `--effort` unset unless
  the user explicitly requests a specific reasoning effort"; "Leave model unset by default").
  **Therefore model and effort MUST be written into the prompt text you hand the subagent.**
- Valid `--effort` values: `none`, `minimal`, `low`, `medium`, `high`, `xhigh`
  (`VALID_REASONING_EFFORTS`, `scripts/codex-companion.mjs:71`).
- It adds `--write` by DEFAULT unless read-only / review-only behaviour is requested.
- `spark` maps to `--model gpt-5.3-codex-spark`; `--resume` adds `--resume-last`, `--fresh` does not.
- Left to itself it picks **background** for open-ended tasks — so say foreground explicitly.
  See [Codex Concurrency](codex-concurrency.md): background fan-out is forbidden by doctrine.

## Hooks (hooks/hooks.json)

| Event | Script | Timeout |
|--------------|-------------------------------|---------|
| SessionStart | `session-lifecycle-hook.mjs`  | 5s      |
| SessionEnd   | `session-lifecycle-hook.mjs`  | 5s      |
| Stop         | `stop-review-gate-hook.mjs`   | 900s    |

The Stop gate is **OPT-IN**: `main()` returns early unless `config.stopReviewGate` is set
(`stop-review-gate-hook.mjs:154-156`; `defaultState()` sets it `false`), and
`STOP_REVIEW_TIMEOUT_MS = 15 * 60 * 1000` (`:16`). When it does run and fails it emits
`{"decision":"block", ...}` (`:169`). loom also binds Stop via `commit-guard.sh` — **both run**.

## LOOM TRAP — never install at local scope

Settings keys are `enabledPlugins` and `extraKnownMarketplaces`. Scope decides the file:
project → `.claude/settings.json`, local → `.claude/settings.local.json`, user → `~/.claude/settings.json`.

`loom/src/sandbox/settings.rs::write_settings` rebuilds `.claude/settings.local.json` from scratch via
`generate_settings_json`, merges back **only `permissions`** (`merge_existing_permissions`,
settings.rs:187 and :472 — it reads nothing but `existing_settings.get("permissions")`), then overwrites
the whole file (settings.rs:193). Every other top-level key — including `enabledPlugins` and
`extraKnownMarketplaces` — is **DROPPED**. This applies to worktrees *and* the main repo root
(`loom repair --fix` and knowledge-stage spawns both call it).

**Install codex at user or project scope, NEVER local.**
