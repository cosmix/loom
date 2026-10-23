# PLAN: gpt-6.1-sol and codex model fallback for `loom pressure`

## Goal

1. Add `gpt-6.1-sol` (OpenAI, released 2026-09-29, rolling out by plan tier) to the codex
   model vocabulary and make it the default `pressure.codex_model`.
2. When the codex model `loom pressure` asks for is unavailable to the account, retry with the
   closest model **of the same family**: the nearest newer version first, working upward, and
   then the nearest older version, working downward. When no model in the family works, fail
   and print the codex error. Tiers (astra / sol / terra / luna) are never swapped.

Scope: `loom pressure` only. The forwarder lane (`codex-forward.sh`, terra/luna by doctrine)
only gets `gpt-6.1-sol` added to its allowlists, with no fallback.

## Facts the design rests on

- Probed locally with codex-cli 0.159.1 (`codex exec -m gpt-9-nonexistent ...`): exit code 1
  within seconds, before any generation. The log contains
  `ERROR: {"type":"error","status":400,"error":{"type":"invalid_request_error","message":"The 'gpt-9-nonexistent' model is not supported when using Codex with a ChatGPT account."}}`.
  The same run first prints a non-fatal `warning: Model metadata for ... not found`, so that
  warning is **not** an unavailability signal.
- The other reported form is `404 Not Found: Model not found <model>` (backend route).
- The codex model catalog (`/model`, `models_cache.json`) is known to be stale in both
  directions: models are missing from the list yet callable by name. A pre-flight catalog
  check would give wrong answers, so the design retries when a run fails instead.
- Resolved model/effort slots come from `PressureModels::resolve`
  (`loom/src/commands/pressure/models.rs`). The allowed values come from
  `crate::codex::CODEX_MODELS` (`loom/src/codex.rs:17`), which also feeds clap's
  `PossibleValuesParser` (`cli/types_pressure.rs:44`), the config registry `ValueKind::Enum`
  (`user_config/keys.rs:94`, and through it the TUI and web settings), the project-config
  reader (`fs/work_dir/config_sections/pressure_config.rs:56`) and shell completions
  (`completions/dynamic/commands.rs:86`). Adding the id to `CODEX_MODELS` covers all of them.
- The codex review runs **in the background at the same time as** the foreground Claude TUI
  (`run_pressure_step`, `commands/pressure/mod.rs:206`). A fallback must not print while
  Claude owns the terminal.

## Family ordering rule

Parse each `CODEX_MODELS` id as `gpt-<version>-<family>`. `<version>` is dotted numeric
(`6`, `6.1`, `5.6`) and compares segment-wise, with a missing segment counting as 0, so
`6 < 6.1`.

For a requested model `R` in family `F`:

```text
candidates(R) = [R]
              ++ family members with version > R, ascending (nearest newer first)
              ++ family members with version < R, descending (nearest older first)
```

Examples with the new vocabulary:

```text
gpt-6.1-sol   -> [gpt-6.1-sol, gpt-6-sol]
gpt-6-sol     -> [gpt-6-sol, gpt-6.1-sol]
gpt-6-astra   -> [gpt-6-astra]
gpt-5.6-terra -> [gpt-5.6-terra]
gpt-6-luna    -> [gpt-6-luna]
```

## Changes

### 1. Vocabulary and default: `loom/src/codex.rs`

- `CODEX_MODELS = ["gpt-6-astra", "gpt-6.1-sol", "gpt-6-sol", "gpt-5.6-terra", "gpt-6-luna"]`.
- `DEFAULT_PRESSURE_CODEX_MODEL = "gpt-6.1-sol"`.
- Add `pub fn codex_model_candidates(requested: &str) -> Vec<&'static str>`, which implements
  the rule above over `CODEX_MODELS`. A requested id that is not in `CODEX_MODELS` returns
  just `[requested]`. That cannot happen after validation, but the function stays total.
  Unit tests cover the five examples above and the version compare (`6 < 6.1`, `5.6 < 6`).
  If `codex.rs` (303 lines) would pass 400 lines with the tests, put the tests in a sibling
  `codex_tests.rs` via `#[cfg(test)] #[path = ...] mod tests;`.

### 2. Unavailability detection: new `loom/src/commands/pressure/fallback.rs`

- `pub(super) fn model_unavailable(log: &str, model: &str) -> bool`. It is true when a single
  line of the log contains the model id and, case-insensitively, one of
  `model is not supported`, `model not found`, `model_not_found`, or `does not exist`.
  Requiring the id on the same line keeps an unrelated error from triggering a fallback, and
  the `Model metadata ... not found` warning does not match, because it lacks those phrases.
- Unit tests: the exact 400 line captured above → true; the 404 form → true; the metadata
  warning alone → false; the phrase naming a different model → false; an empty log → false.

### 3. Retry loop: `loom/src/commands/pressure/spawn.rs` + `mod.rs`

- Replace the direct `spawn_codex_background` / `wait_codex(Child)` pair with a codex
  **runner thread**:
  `spawn_codex_with_fallback(codex_path, repo_root, skill, log_path, candidates, effort) -> CodexRun`.
  The thread tries each candidate in order. Each attempt re-creates `log_path` (the existing
  spawn body) and waits on the child. On a non-zero exit where
  `model_unavailable(log, candidate)` holds and another candidate remains, it copies the
  failed attempt's log tail into a note and moves to the next candidate. Otherwise it returns
  `CodexOutcome { status, model_used, notes }`.
- `CodexRun` wraps the `JoinHandle`. `wait_codex` keeps its spinner, polling
  `JoinHandle::is_finished()` instead of `Child::try_wait()`, then joins.
- After Claude finishes, `run_pressure_step` prints each fallback note (nothing is printed
  while the Claude TUI is up), in the form:
  `! codex model gpt-6.1-sol is unavailable to this account — fell back to gpt-6-sol`.
- When every candidate is unavailable, the last attempt's non-zero status flows into the
  existing `should_stop("codex", ...)`, which already prints the log tail. That tail contains
  the codex error, which covers "fail and show the codex error". Add one line naming every
  model tried.
- The effort is unchanged across attempts.
- The run header and dry-run preview (`print_run_header`, `mod.rs:~180`) show the chain when
  it has more than one entry, e.g. `codex gpt-6.1-sol (fallback: gpt-6-sol) / xhigh`.
  Update the dry-run expectation in `commands/pressure/tests.rs:163` (`-m gpt-6-sol` →
  `-m gpt-6.1-sol`).
- Keep `spawn.rs` under 400 lines. If the runner does not fit, it goes in `fallback.rs` next
  to the detector.

### 4. Forwarder-lane allowlists (no fallback)

Add `gpt-6.1-sol` to the model `case` in:

- `loom-hooks/codex-forward.sh:38`
- `loom-hooks/codex-forward-guard.sh:118`
- `loom-hooks/codex-forward-result.sh:25`

Also update the `CODEX_MODELS` doc comment, which says it is "kept in step with" these.

### 5. Tests and fixtures touched by the vocabulary

- `loom/src/completions/dynamic/tests/tests_commands.rs`: assert `gpt-6.1-sol` is completed
  for prefix `gpt-6` and **not** for `gpt-6-`.
- `loom/src/commands/pressure/models.rs` tests: the all-defaults case now expects
  `gpt-6.1-sol`.
- `web/src/api/fixtures/config.json:136-149`: variants gain `gpt-6.1-sol`, and the default
  becomes `gpt-6.1-sol`.
- Leave fixtures that pin `gpt-6-sol` as a recorded model (`codex_lifecycle`,
  `codex_evidence`, `token_optimization`, `subagent_owned_wait`) alone. They record history,
  not the default.

### 6. README.md

- Line ~447: the `--codex-model` accepted list gains `gpt-6.1-sol`. Add a sentence on the
  fallback: an unavailable model retries within its family, nearest newer version first and
  then nearest older, never another tier, and fails with the codex error when the family is
  exhausted.
- Line ~453 defaults table: `pressure.codex_model` → `gpt-6.1-sol`.
- Line ~605 config reference row: default `gpt-6.1-sol`; the accepted values gain
  `gpt-6.1-sol`.
- `rg -n 'gpt-6-sol' README.md` shows no remaining claim that it is the default.

### 7. Knowledge (outside a stage: write through the CLI, then `loom knowledge check`)

- `conventions/model-and-effort-config.md`: use `replace-section` on "`[pressure]` Keys and
  Defaults" (default `gpt-6.1-sol`), and add a "Codex Model Fallback" section (family rule,
  detection strings, pressure-only scope, and why a catalog pre-flight was rejected). Note
  there that Codex "Ultrafast" is a speed mode (like Claude Code's `/fast`), not a model id,
  so it never belongs in `CODEX_MODELS`.
- `architecture/remote-control.md` (the `loom pressure` children paragraph): replace-section.
  The default becomes `gpt-6.1-sol` with in-family fallback. Also fix the stale claim that
  codex "Reasoning effort stays pinned (`CODEX_REASONING_EFFORT` in pressure/spawn.rs)". The
  tree resolves it through `pressure.codex_effort` / `--codex-effort` (`CODEX_EFFORTS`,
  `DEFAULT_PRESSURE_CODEX_EFFORT` in `codex.rs`).
- `architecture/codex-plugin.md` "Do NOT confuse" paragraph: `gpt-6-sol`
  (`commands/pressure/mod.rs:245`) → `gpt-6.1-sol` (default, falls back to `gpt-6-sol`),
  pointing at `DEFAULT_PRESSURE_CODEX_MODEL` in `codex.rs` instead of a stale line number.
- Run `loom knowledge sync` if any file is touched with the Edit tool, then
  `loom knowledge check`.

## Acceptance criteria

```bash
cd loom
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --no-fail-fast 2>&1 | tail -50        # all green
cargo test codex_model_candidates                 # family-order unit tests
cargo test model_unavailable                      # detector unit tests
```

- `loom pressure --dry-run <plan>` header shows `gpt-6.1-sol (fallback: gpt-6-sol)`.
- `loom config -k pressure.codex_model gpt-6.1-sol` is accepted, and `gpt-7-sol` is rejected
  with the allowed list.
- Hook tests under `loom-hooks/tests/` pass (`codex-forward-*`).
- Manual check (needs codex installed): set `pressure.codex_model` to `gpt-6.1-sol` on an
  account without access, or temporarily add a fake `gpt-6.9-sol` to `CODEX_MODELS` in a
  scratch build. The run prints the fallback note after Claude ends, and codex continues on
  `gpt-6-sol`.
- `rg -n 'gpt-6\.1-sol' README.md doc/loom/knowledge` shows the default and the fallback rule
  documented.
- File sizes: every touched `.rs` file ≤ 400 lines and every function ≤ 50 lines.

## Out of scope

- Fallback in the forwarder lane (`codex-forward.sh`), per the operator's decision.
- Remembering an unavailable model between runs. A failed attempt costs a few seconds and no
  tokens, and a cache would go stale as rollout reaches the account.
- GPT-6.1 Sol Ultrafast. It is a Codex speed mode for the same model, like Claude Code's
  `/fast`, not a separate model id, so it never enters `CODEX_MODELS` or the fallback chain.
