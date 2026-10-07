# Model And Effort Config

> [models] precedence, value types

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
gpt-6.1-sol (in-family fallback to gpt-6-sol, see Codex Model Fallback below),
`pressure.codex_effort` xhigh, `pressure.address_model` opus, `pressure.address_effort` high.

## `[models]` Keys and Defaults

`models.standard_model` opus / `models.standard_effort` medium (`standard`);
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

## Codex Model Fallback

`loom pressure` retries an unavailable codex model within its family. `codex_model_candidates`
(`codex.rs`) parses each `CODEX_MODELS` id as `gpt-<version>-<family>`, compares versions
segment-wise (missing segment = 0, so 6 < 6.1), and orders the chain: the requested id, then
same-family newer versions ascending, then same-family older versions descending.
`gpt-6.1-sol` → `[gpt-6.1-sol, gpt-6-sol]`; astra, terra and luna have one member each. Tiers
(astra / sol / terra / luna) are never swapped.

A run counts as unavailable only on a non-zero exit whose log has one codex error line (it
starts with `ERROR`) naming the model id and, case-insensitively, `model is not supported`,
`model not found`, `model_not_found` or `does not exist` (`unavailable_line` in
`commands/pressure/fallback.rs`). The `ERROR` prefix is required because the log also carries
codex's tool output and prose: a review that prints plan text or `fallback.rs` itself quotes
these phrases next to model ids, and an unrelated failure would otherwise rerun the whole review
on the next model. The 404 form arrives as
`ERROR: unexpected status 404 Not Found: Model not found <id>`. Codex also prints a
non-fatal `Model metadata for <id> not found` warning; it matches none of those phrases and never
triggers a retry. The first attempt spawns on the caller's thread, so a spawn failure errors
before Claude starts; the runner thread then retries silently while the Claude TUI owns the
terminal, carrying each failed attempt's matched error line into a note because every attempt
recreates the log. `run_pressure_step` prints the notes after Claude ends and names the model
that wrote the review. When the family is exhausted it prints the models tried, and the last
status flows into `should_stop`, which prints the codex log tail.

Scope: `loom pressure` only. The forwarder lane (`codex-forward.sh`) validates ids against its
own allowlists and never falls back. There is no pre-flight catalog check: the codex model
catalog (`/model`, `models_cache.json`) is stale in both directions, listing models that fail
and omitting models that work, so only a real run answers the question. Unavailability is not
cached between runs, since rollout reaches accounts over time.

Codex "Ultrafast" (for example GPT-6.1 Sol Ultrafast) is a speed mode for the same model, like
Claude Code `/fast`; it is not a model id and never belongs in `CODEX_MODELS`.
