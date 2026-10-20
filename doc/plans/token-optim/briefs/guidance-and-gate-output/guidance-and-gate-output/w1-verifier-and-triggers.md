# W1: verifier gate recipe and Go skill triggers

Tier: haiku (`loom-software-engineer` with `model: haiku`). Every edit below is exact text. Working directory for cargo: `loom/`. Paths below are repo-relative.

## Files you own

- `agents/loom-verifier.md`
- `loom/src/orchestrator/signals/cache/blocks.rs` (the `VERIFIER_GATE` constant only)
- `skills/loom-golang/SKILL.md` (the `triggers:` list only)

Read-only: `loom/tests/guidance_gate_contracts.rs` (frozen contract file; never edit it), `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` (`only_the_standard_prefix_sends_the_gate_to_a_loom_verifier`), `loom/src/commands/skill_index.rs` (`STOPWORDS`).

Two codex units ran before you. C1 wrote `loom/src/orchestrator/signals/tests_guidance_manifest.rs`, which looks for the needle `2>&1; echo "exit=$? log=` in `agents/loom-verifier.md`, so the recipe line below must keep that exact text. C2 wrote `go_generic_words_do_not_qualify_golang_on_a_rust_prompt` in `loom/tests/integration/hooks_skill_trigger_evidence.rs`, which copies the real `skills/loom-golang/SKILL.md` into a fake catalog and fails until Part C lands. Do not edit either file.

## Why

A gate step whose output passes the Bash tool's 12,000-character cap (`BASH_MAX_OUTPUT_LENGTH=12000`, `resource_limit_env` in `loom/src/orchestrator/terminal/native/wrapper.rs:286`) is truncated, so the failure list may never reach the verifier, and a misread round costs a fresh verifier. `<command> | tail` hides the command's exit status behind `tail`'s. The verifier therefore writes each step's full output to a log outside the tree and prints only the exit code, the log path and the failure lines.

## Part A: `agents/loom-verifier.md`

Re-read the file first. Plan 7 of this programme edits `agents/*.md` after this plan, so change only the lines named here.

1. Frontmatter `description:` (line 3, anchored by the text `Writes nothing and never fixes.`): replace that sentence with `Writes only its gate logs under $LOOM_SCRATCH_DIR and never fixes.` Leave `name`, `tools`, `model`, `effort` and `maxTurns` unchanged.
2. `## What You Run`: after the numbered list (items 1-3) and before the paragraph that starts `Run each command once.`, insert this paragraph and block. The four-backtick `markdown` fence below only marks the text to insert: insert what lies between its fences, unindented, with a blank line before and after. The two `bash` block lines are exact; the frozen contract extracts the first fenced `bash` block that contains `gate-<step>.log` and runs its lines with its own values for the three placeholders.

   ````markdown
   Run every gate step and every criterion through this recipe:

   ```bash
   <command> > "$LOOM_SCRATCH_DIR/gate-<step>.log" 2>&1; echo "exit=$? log=$LOOM_SCRATCH_DIR/gate-<step>.log"
   rg -n -m 20 -B 2 -A 8 '<failure pattern>' "$LOOM_SCRATCH_DIR/gate-<step>.log"
   ```

   `<step>` is a short step name (`build`, `test`, `doctest`, `clippy`, `fmt`, `doc`, `criterion-<n>`). `<failure pattern>` is the runner's failure marker; for cargo, `FAILED|panicked at|^error|^warning:`. The log keeps the full output, because the Bash tool cuts output past 12,000 characters and a step printed whole can lose its failure list. Never pipe a step into `tail` or `head`: the pipe reports the last command's status, so a failing step reads as exit 0. When the excerpt does not explain a failure, read more of the log with `rg` or by line range.
   ````

3. `## Constraints`, the first bullet (anchored by `**Writes no files**`): replace the whole bullet line with this line (the `text` fence is not part of it):

   ```text
   - **Writes only its gate logs**: one `$LOOM_SCRATCH_DIR/gate-<step>.log` per step, outside the tree. No Edit, no Write, no other redirection, no formatter or linter in fix mode (`cargo fmt` runs as `cargo fmt --check`), no dependency changes. Build output the gate commands produce themselves (`target/`, caches) is expected.
   ```

4. `## Output Format`: in the `STEP` block add the line `log: <the step's log path>` directly after `exit: <code>`.

Nothing else in the file changes.

## Part B: `VERIFIER_GATE` (`loom/src/orchestrator/signals/cache/blocks.rs:94`)

Replace the constant's string with exactly:

```text
**The gate runs in a `loom-verifier`**: once every implementer returned, spawn ONE by agent type, without `name`, briefed with this stage's acceptance criteria. It writes only logs in `$LOOM_SCRATCH_DIR` and reports PASS/FAIL per step with command, exit code and failing excerpt. Delegate fixes, then spawn a FRESH verifier for the next round.\n\n
```

(`\n\n` is the Rust escape already at the end of today's string.) The text is 345 bytes against today's 348. It must not grow: on 6fee4ecd the standard stage's stable-plus-semi-stable floor measured 10,228 bytes against `STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` 10,240 (`loom/src/orchestrator/signals/tests_size.rs:72`), and the stable prefix 8,091 against 8,192. The needle `**The gate runs in a`loom-verifier`**` stays: `only_the_standard_prefix_sends_the_gate_to_a_loom_verifier` (`tests_doctrine_prefixes.rs:62-74`) pins it. `(opus, effort xhigh)` goes because the agent definition pins model and effort. Do not change the doc comment or `append_verifier_completion_header`.

## Part C: `skills/loom-golang/SKILL.md` triggers (lines 4-31)

`_rank` in `loom-hooks/skill-trigger.sh:265-279` qualifies a skill on two single-word hits (score 1 each) or one phrase hit (score 2, `_score_keywords`, `:194-201`). Today `struct` plus `map` qualifies loom-golang on a Rust prompt. Replace the `triggers:` list with exactly this list, in this order, and change nothing else in the file:

```yaml
triggers:
  - golang
  - goroutine
  - in go
  - go code
  - go package
  - go module
  - go channel
  - go interface
  - go struct
  - go pointer
  - go slice
  - go map
  - go defer
  - go context
  - error
  - gin
  - echo framework
  - fiber
  - cobra
  - viper
  - gorm
  - sqlx
  - go mod
  - go test
  - effective go
  - errgroup
  - sync
  - go mutex
  - waitgroup
  - race detector
```

The bare words `go`, `channel`, `interface`, `struct`, `pointer`, `slice`, `map`, `defer`, `mutex` and `echo` are gone. `error` and `sync` stay as they are: both are in `STOPWORDS` (`loom/src/commands/skill_index.rs:34-43`), so the indexer already drops them. `STOPWORDS` itself is untouched: `stopwords_match_between_shell_and_rust_index` (`hooks_skill_trigger_evidence.rs:151-172`) pins the shell and Rust copies equal.

## Your one check

After all parts: `cargo test --test guidance_gate_contracts verifier_recipe_reports_exit_log_and_excerpt`, run once, from `loom/`. The main agent runs C2's Go negative case and the gate. If the contract fails because its expectations differ from this brief, report the exact assertion; never edit the contract file.

## Traps

- The recipe block must be the first fenced `bash` block in the file that contains `gate-<step>.log`. Use only the placeholders `<command>`, `<step>` and `<failure pattern>`.
- `rustfmt` leaves long string literals alone; run `cargo fmt` on your Rust files and keep `VERIFIER_GATE` on one line.
- `RETIRED_PHRASES` (`loom/src/orchestrator/signals/tests_doctrine_blocks.rs`) are asserted absent from every `agents/*.md`; the gate's `cargo test --lib signals::` catches a hit.
