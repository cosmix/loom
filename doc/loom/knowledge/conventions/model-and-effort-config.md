# Model And Effort Config

> [models]

## Two Configurable Sections

`[pressure]` (`~/.loom/config.toml` / `.loom/work/config.toml`) sets the model and effort for
the three `loom pressure` steps. `[models]` sets the model and effort each stage type's
main-agent session launches with. Both are opt-in: `loom init` never writes either section into
a project config. A third pair of slots, the plan summary models, is configured through `[plan]`
(user tier) and `[planning]` (project tier); see `[plan]` Keys and Defaults below.

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
`pressure.rounds` (Number, at least 1, default 1) sets the round count absent `--rounds`; a zero is
rejected on write and dropped from a project file. Precedence: flag, project, user, built-in.

## `[models]` Keys and Defaults

`models.standard_model` sonnet / `models.standard_effort` high (`standard`);
`models.knowledge_model` opus / `models.knowledge_effort` medium (`knowledge`);
`models.knowledge_distill_model` sonnet / `models.knowledge_distill_effort` high
(`knowledge-distill`); `models.integration_verify_model` opus /
`models.integration_verify_effort` xhigh (`integration-verify`). Merge and base-conflict
sessions stay pinned at opus/high; adjudication keeps its own `[adjudication] model`; neither
is configurable through `[models]`.

`models.code_reviewer_model` opus / `models.code_reviewer_effort` high are not a stage type: they
set the `loom-code-reviewer` subagent, applied by the spawn guard to every reviewer spawn and
overriding a model the caller passes. They resolve project -> user -> built-in per key through
`fs::work_dir::resolve_code_reviewer_model_effort`, which the hidden `loom hook reviewer-model`
prints as `model=<m> effort=<e>` (non-zero exit on a config parse error).

## KEY-Level vs SECTION-Level Fallback

All five project-backed sections (`[pressure]`, `[models]`, `[terminal]`, `[context]` and `[planning]`) resolve **per key**: a project section that omits a key lets that key fall through to the user config, then the built-in. A keyless section changes nothing (`fs/work_dir/config_sections.rs::remove_key` drops a section once its last key goes). The runtime readers are `read_context_config`, `read_terminal_config` and `read_plan_config` in `fs/work_dir/`, and the web `/api/config` resolves through the same predicate.

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

## `[plan]` Keys and Defaults

Four string keys pick the model and reasoning effort of plan summary sessions (`planning/summary/config.rs`,
`SummaryModels::resolve`): `plan.document_summary_model` haiku and `plan.document_summary_effort` medium for a
document summary; `plan.overview_model` sonnet and `plan.overview_effort` high for an overview and the summary session
that rebuilds them. The user tier is the `[plan]` table of `~/.loom/config.toml` (`user_config/plan.rs`, defaults
`DEFAULT_PLAN_*`, values validated as model and effort enums); the project tier is the `[planning]` table of
`.loom/work/config.toml` (`fs/work_dir/config_sections/plan_config.rs`, `read_plan_config`, `deny_unknown_fields`).
The tables differ because the project `[plan]` table already holds the run identity (`plan_id`, `source_path`, ...), so
the keys sit in `[planning]` there. A project table holding one unknown or malformed key is dropped whole without a log
line, so all four project overrides revert to the user tier; this mirrors `read_pressure_config`.

## Planner Execution Worker Allocation

For planner plans 06–11 and their linked briefs, substantial Codex execution units use `gpt-6.1-sol` with `--effort high`; routine types, fixtures, documentation and mechanical wiring may use `gpt-6.1-terra` at high effort. Each unit names its model explicitly and the stage dispatch instructions follow that allocation. This choice applies to execution subagents; stage orchestrator models and the product pressure-review model remain independently configured.
