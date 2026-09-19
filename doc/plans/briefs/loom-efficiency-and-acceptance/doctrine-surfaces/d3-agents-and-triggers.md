# D3 — agent definitions and skill triggers

Tier: sonnet (`loom-software-engineer`). Starts after D1 returns. Read `../common.md` first.

## Goal

Agent definitions point at skills that resolve and stop granting a tool the doctrine forbids;
skill triggers stop colliding with loom's own vocabulary. Evidence: report sections 4.2 (2e) and
4.4 — `loom-debugging` suggested 40 times and loaded 0; `/code-review` resolves to an unrelated
built-in; both engineer agents grant `Task` while the worker preamble spends tokens forbidding
spawns.

## Files you own (write)

- `agents/loom-software-engineer.md`, `agents/loom-senior-software-engineer.md`
- `skills/*/SKILL.md` frontmatter `triggers:` and `description:` of the skills listed below.
  Not `skills/loom-plan-writer/` (D2) and not `skills/loom-orchestration/` (D1).

## Steps

1. Both engineer agents, "Skills to Leverage" (`loom-software-engineer.md:61-79`,
   `loom-senior-software-engineer.md:57-84`). Replace each bare slash name with a form that
   loads. Core skills (listed in `skills/core-skills.txt`) load as
   `Skill(skill="loom-debugging")`; catalogued ones as
   `Skill(skill="loom-skills", args="loom-refactoring")`. The listed names map to real
   directories once prefixed: `loom-debugging`, `loom-refactoring`, `loom-testing`,
   `loom-error-handling`, `loom-code-review`, `loom-auth`, `loom-background-jobs`,
   `loom-data-validation`, `loom-event-driven`, `loom-feature-flags`; check every name in the
   senior file the same way (`test -d skills/<name>`). Add one sentence: when the brief names
   skills for the stage, load those first.
2. Remove `Task` from the `tools:` line of both files (`loom-software-engineer.md:4`). Remove
   sentences that tell the agent how to spawn subagents, if any.
3. Do not touch BLOCK-A in either file: `tests_doctrine.rs` pins it byte-for-byte in every
   `agents/*.md`. The codex-tier sentence (`:95`, `:107`) stays as it is.
4. Triggers. In these skills remove the single-word triggers that are loom or general
   engineering vocabulary, keeping phrases: `loom-ci-cd` (`stage`, `job`), `loom-auth` (`token`,
   `session`), `loom-react` (`hook`, `state`), `loom-golang` (`context`), `loom-istio`
   (`gateway`, `canary`), `loom-prompt-engineering` (`prompt` alone), `loom-data-visualization`
   (`report`, `graph`), `loom-documentation` (`document`), and `state` in `loom-before-after`,
   `loom-terraform`, `loom-diagramming`, `loom-kubernetes`; `job` in `loom-prometheus`,
   `loom-background-jobs`, `loom-kubernetes`, `loom-logging-observability`; `token` in
   `loom-rate-limiting`, `loom-search`; `event` in `loom-webhooks`, `loom-event-driven`,
   `loom-database-design`. Where a removed word was the skill's only route in, add a specific
   phrase (`ci pipeline stage`, `jwt token`, `react hook`, `go context`). In
   `loom-model-evaluation` replace `model selection` with `ml model selection`.
5. For each skill you touched, end its `description:` with one "Not for:" clause naming the
   nearest confusion (for `loom-model-evaluation`: choosing which LLM tier runs a task).

## Traps

- The hook's index is rebuilt from frontmatter by `loom/src/commands/skill_index.rs`; a trigger
  list left empty drops the skill from matching. Every skill keeps at least three triggers.
- YAML frontmatter: keep list indentation and quoting exactly as each file has it.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib skills::` — run once.
