# Codex Plugin

> Topic notes for the architecture knowledge area.

## Install and identity

- Marketplace `openai-codex` from GitHub `openai/codex-plugin-cc`; plugin name `codex`.
- Observed installed version **1.0.6** (`claude plugin list --json`, 2026-08-06), install path
  `~/.claude/plugins/cache/openai-codex/codex/<version>/`.
- Needs the `codex` CLI on PATH (observed `codex-cli 0.146.0`) and an authenticated `~/.codex/auth.json`.
  Codex initialises a sqlite state runtime under `~/.codex`; a sandbox that denies writes there kills
  every run with `failed to initialize state runtime` (exit 1) before any model call is made — and
  some wrapper paths still exit 0, so read stderr, not the exit code.

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

`codex` echoes its own configuration banner (workdir / model / provider / approval / **sandbox** /
reasoning effort / session id) to **stderr** on every healthy run. Loom's silent-failure detector
flags that `sandbox` substring as a possible block; it is benign. Judge a run by the echoed model
line and the actual reply, not by the presence of `sandbox` in stderr.

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

## What loom shipped for this lane

The per-stage `implementers` field names which agent lanes a stage may spawn subagents from.
Three moving parts:

1. **The field.** `Implementer` enum (`models/stage/types.rs:135`), `claude` | `codex`,
   `#[serde(rename_all = "kebab-case")]`, closed — an unknown value is a parse error. It is held by
   `Implementers` (`models/stage/types.rs`), a `#[serde(transparent)]` newtype over
   `Vec<Implementer>` whose `Default` is `vec![Claude]`. Carried on `StageDefinition`
   (`plan/schema/types.rs`) and `Stage` (`models/stage/types.rs`), both `#[serde(default)]`; copied
   at `commands/init/plan_setup.rs` and onto the signal's `EmbeddedContext` at
   `orchestrator/signals/generate.rs`. Query it through `includes_codex()` / `includes_claude()` /
   `preferred()` / `is_mixed()` — never by comparing the whole list, which is what made the original
   scalar design wrong.
2. **ORDER IS THE PREFERENCE, MEMBERSHIP IS THE LICENSE.** These are two different questions and
   the code must not conflate them:
   - `preferred()` (first element) = the lane routine implementation reaches for.
   - `includes_codex()` = whether the codex safety doctrine must be emitted.

   Gating doctrine on the *preference* rather than on *membership* is a real bug: a stage listing
   `["claude", "codex"]` may still spawn a codex subagent, and would then run with none of the
   blast-radius rules. That is exactly the hole the original `implementer == Codex` equality check
   left, and `tests_cache.rs` now pins both the mixed-signal and mixed-recovery cases against it.

   Validation (`plan/schema/validation.rs`) REJECTS an empty list and a repeated lane (order would
   be ambiguous), and WARNS when codex is listed on a knowledge, knowledge-distill, or
   integration-verify stage — on membership, not preference, for the same reason.
3. **The gated signal block.** `format_codex_implementers_section(&Implementers)`
   (`orchestrator/signals/format/sections.rs`) emits `## Codex Implementers`, gated on
   `includes_codex()`. It names every licensed lane, and on a mixed stage tells the orchestrator to
   choose the lane PER SUBAGENT and to keep ONE file-ownership table across lanes (cross-lane
   collisions lose work exactly as same-lane ones do). It lives in the **semi-stable** section
   (regenerated per stage), NOT the stable prefix — the stable prefix only forward-references it
   (`cache.rs`). The recovery path emits it too (`recovery_format.rs`), which was a real gap fixed
   by `d1530e0c`; without it a recovered or retried codex stage loses its whole doctrine block.
   Model/effort are interpolated from `CODEX_IMPLEMENTER_MODEL = "gpt-5.6-luna"` and
   `CODEX_IMPLEMENTER_EFFORT = "xhigh"` (`loom/src/codex.rs:7,10`) rather than hardcoded, and
   `tests_doctrine.rs` asserts BLOCK-B contains both — so changing `codex.rs` without updating the
   prose surfaces fails the build.
4. **Settings carry-forward.** `PRESERVED_SETTINGS_KEYS` / `preserve_unowned_keys`
   (`sandbox/settings.rs:580,587`) — see the scope section below.

Do NOT confuse `gpt-5.6-luna` (`codex.rs:7`, the implementer lane) with `gpt-5.6-sol`
(`commands/pressure/mod.rs:245`, the `loom pressure` review driver). Two features, two models, both
correct; a grep for `gpt-5` returns both.

## Install scope: user or project, not local

Scope decides the file: project → `.claude/settings.json`, local → `.claude/settings.local.json`,
user → `~/.claude/settings.json`. The relevant keys are `enabledPlugins` and `extraKnownMarketplaces`.

`sandbox::write_settings` (`sandbox/settings.rs:77`) REBUILDS `.claude/settings.local.json` from
scratch on every stage spawn, in worktrees and in the main repo root alike. **Since `df7d1060`
(2026-08-07) that no longer erases plugin installs:** `preserve_unowned_keys` (`:587`) carries a
two-key allowlist forward —

```rust
const PRESERVED_SETTINGS_KEYS: [&str; 2] = ["enabledPlugins", "extraKnownMarketplaces"];
```

— so a local-scope install now survives regeneration (verified by driving the real `write_settings`
over a seeded file twice). **Prefer user or project scope anyway**: the carve-out is exactly two
keys, so local scope is safe only for plugins and only by special case, while every other top-level
key in that file is still dropped. `/loom-plan-writer` states the user/project rule as the
requirement. See [Sandbox & Settings](../mistakes/sandbox-and-settings.md) for the rebuild mechanics.
