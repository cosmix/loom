# Model And Effort Config

> [pressure]/[models] sections, precedence chain, value types

## Two Configurable Sections

`[pressure]` (`~/.loom/config.toml` / `.loom/work/config.toml`) sets the model and effort for
the three `loom pressure` steps. `[models]` sets the model and effort each stage type's
main-agent session launches with. Both are opt-in: `loom init` never writes either section into
a project config.

## Precedence Chain (per key, model and effort resolve independently)

1. Explicit per-invocation value — a plan stage's `model` / `reasoning_effort` field for stage
   sessions, a CLI flag (`--claude-model`, `--claude-effort`, etc.) for `loom pressure`.
2. Project config `<repo>/.loom/work/config.toml`.
3. User config `~/.loom/config.toml`.
4. Built-in default.

## `[pressure]` Keys and Defaults

`pressure.claude_model` opus, `pressure.claude_effort` xhigh, `pressure.codex_model`
gpt-5.6-sol, `pressure.codex_effort` xhigh, `pressure.address_model` opus,
`pressure.address_effort` high.

## `[models]` Keys and Defaults

`models.standard_model` opus / `models.standard_effort` high (`standard`);
`models.knowledge_model` opus / `models.knowledge_effort` medium (`knowledge`);
`models.knowledge_distill_model` sonnet / `models.knowledge_distill_effort` high
(`knowledge-distill`); `models.integration_verify_model` opus /
`models.integration_verify_effort` xhigh (`integration-verify`). Merge and base-conflict
sessions stay pinned at opus/high; adjudication keeps its own `[adjudication] model`; neither
is configurable through `[models]`.

## KEY-Level vs SECTION-Level Fallback

There is no split any more. All four project-backed sections — `[pressure]`, `[models]`,
`[terminal]` and `[context]` — resolve **per key**: a project section that omits a key lets that
key fall through to the user config, then the built-in. `[terminal]` and `[context]` used to be
SECTION-level (a present section won whole, and keys it omitted derived built-ins); the operator
ruled that a defect on 2026-09-13, and it was removed from the runtime readers
(`fs/work_dir/config_sections.rs`) and from `/api/config` (`config_api/workspace.rs`).

One qualification: a project `[context]` that sets `model_window_tokens` supplies
`ceiling_tokens` (derived from that window) even when it omits `ceiling_tokens`, so a user
ceiling sized for a 1M window cannot override a plan's smaller window. The raw-layer merge in
`ContextConfig::resolve_with_user_ceiling` (`fs/work_dir/context_config.rs`) is the one place that
predicate is decided. `subagent_ceiling_tokens` and `model_window_tokens` have no user-tier key.
`loom init` writes only the `[context]` keys a plan sets.

## Resolution Code

Stage sessions resolve through `crate::fs::work_dir::resolve_stage_model_effort`; the three
`loom pressure` slots resolve through `commands::pressure`'s own resolver; the user-tier lookup
for both goes through `crate::user_config`.

## Plan Doctrine

A plan stage omits `model` and `reasoning_effort` by default, so the stage type's configured
default applies. Set either field only as a deliberate override, and state why in the stage
description.

## Value Types Are a Separate Concern

This file's `[pressure]`/`[models]` keys are string-valued (model names, effort levels) and
resolved through the precedence chain above. The registry's typed read path — `ConfigValue`,
`ValueKind`, per-surface threading through CLI/TUI/web/TS/React — is a cross-cutting seam documented
separately: see [Typed Config Values](../architecture/config-value-types.md).
