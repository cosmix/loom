# C2: the stage `model` allowlist (codex unit)

Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first. Paths below are relative to `loom/`.

## Files

Write only these four:

- `src/plan/schema/types_v2.rs`
- `src/plan/schema/types.rs` (the `model` field of `StageDefinition`, its doc comment, and one import)
- `src/plan/schema/tests/stage_model_tests.rs` (new)
- `src/plan/schema/tests/mod.rs` (one line)

Read for the pattern, do not edit: `src/plan/schema/tests/reasoning_effort_tests.rs` (YAML template plus `crate::yaml::from_str::<LoomMetadata>`), `src/claude.rs` (`CLAUDE_MODELS`, the aliases `haiku`, `sonnet`, `opus`, `fable`), `src/fs/work_dir/config_sections/models_config.rs` (the `[models]` config validates against the same list).

Never run git. Never touch a `.loom/` path. Use no network.

## Why

`StageDefinition.model` accepts any string, while `reasoning_effort` is validated when the plan is parsed. The launcher shell-escapes the model (`src/orchestrator/terminal/native/mod.rs`, `build_claude_command`), so a typo is no injection; it fails only when the stage spawns, hours into a run.

## Step 1: the deserializer and the field

In `types_v2.rs`, directly after `deserialize_reasoning_effort`, add a function with the same shape:

```rust
pub(super) fn deserialize_stage_model<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
```

`None` passes. A value in `crate::claude::CLAUDE_MODELS` passes as `Some(value)`. Anything else fails (bring `serde::de::Error` into scope inside the function, as `deserialize_reasoning_effort` does) with `D::Error::custom(format!("invalid model '{s}'. Allowed values: {}", CLAUDE_MODELS.join(", ")))`. Doc comment: the stage model must be one of the aliases the `[models]` config accepts, so a typo fails when the plan is parsed.

In `types.rs`, extend the existing import `use super::types_v2::{deserialize_reasoning_effort, ProvisionEntry};` with `deserialize_stage_model`, and give the `model` field the multi-line attribute that `reasoning_effort` carries:

```rust
#[serde(
    default,
    skip_serializing_if = "Option::is_none",
    deserialize_with = "deserialize_stage_model"
)]
```

Rewrite the field doc comment: the model for the main agent of this stage, one of `haiku`, `sonnet`, `opus`, `fable`; any other value fails when the plan is parsed. The file is 381 lines; it must stay under 400.

## Step 2: the tests

New file `src/plan/schema/tests/stage_model_tests.rs`, modelled on `reasoning_effort_tests.rs`: a version-1 YAML template of one stage (`id: s1`, `name: First Stage`, `working_dir: "."`, `artifacts: ["README.md"]`, `model: "{model}"`) and a second template without the `model` line. Tests:

- every value of `CLAUDE_MODELS` parses, and `stages[0].model.as_deref()` equals it;
- `claude-opus-5`, `opus[1m]`, `opus; rm -rf /` and the empty string each fail, and the error text (`format!("{err}")`) contains `invalid model`;
- the template without `model` parses to `None`.

Register it in `src/plan/schema/tests/mod.rs` with `mod stage_model_tests;`, in alphabetical order (directly after `mod stage_id_tests;`). Keep every function under 50 lines.

## Step 3: check your own text

Run no build, test or formatter: six Claude workers build the same crate in this worktree at the same time, so a cargo command can wait on their build lock past the 540-second forward deadline, and `cargo test --lib --no-run` alone can outlast it. Re-read your four files against the signature, the attribute and the error text above instead. The orchestrator runs `cargo build` and `cargo test --lib plan::schema::tests::stage_model_tests` right after this unit returns; a compile error goes to a fresh Claude worker.

Report the final line count of `types.rs` and the names of the tests you wrote.
