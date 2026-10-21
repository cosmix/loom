# W1: verifier gate recipe and Go skill triggers

Tier: sonnet (`loom-software-engineer`). Parts A and C are exact text. Part B is a rule-bound rewrite of a constant whose current text plan B (`doc/plans/PLAN-host-verification.md`) set. Working directory for cargo: `loom/`. Paths below are repo-relative.

Plan B ran before this plan:

- `loom check <stage-id>` in a session relays a request to a daemon host job, prints `Verification requested ...` and exits 3. Run again, it waits and prints the result.
- For each failing criterion it prints a `full log: <path>` line naming the criterion's whole output.
- Its relay line must stay in the Bash output for the relay hook (`loom-hooks/loom-relay.sh`) to forward it. Redirecting or piping `loom check` loses the request.
- Plan B also rewrote item 2 of the verifier definition's "What You Run" (criteria through `loom check`, its exit codes, SESSION-ENV and RUNNER-ENV) and set `VERIFIER_GATE`'s current text. Re-read both first.

## Files you own

- `agents/loom-verifier.md`
- `loom/src/orchestrator/signals/cache/blocks.rs` (the `VERIFIER_GATE` constant only)
- `skills/loom-golang/SKILL.md` (the `triggers:` list only)

Read-only: `loom/tests/guidance_gate_contracts.rs` (frozen contract file; never edit it), `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` (`only_the_standard_prefix_sends_the_gate_to_a_loom_verifier`), `loom/src/commands/skill_index.rs` (`STOPWORDS`).

Two codex units ran before you. C1 wrote `loom/src/orchestrator/signals/tests_guidance_manifest.rs`, which looks for the needle `2>&1; echo "exit=$? log=` in `agents/loom-verifier.md`, so the recipe line below must keep that exact text. It also pins `` `loom check <stage-id>` ``, `RUNNER-ENV` and `SESSION-ENV` in the standard stable prefix, which `VERIFIER_GATE` (Part B) carries. C2 wrote `go_generic_words_do_not_qualify_golang_on_a_rust_prompt` in `loom/tests/integration/hooks_skill_trigger_evidence.rs`, which copies the real `skills/loom-golang/SKILL.md` into a fake catalog and fails until Part C lands. Do not edit either file.

## Why

A gate step whose output passes the Bash tool's 12,000-character cap (`BASH_MAX_OUTPUT_LENGTH=12000`, `resource_limit_env` in `loom/src/orchestrator/terminal/native/wrapper.rs:286`) is truncated, so the failure list may never reach the verifier, and a misread round costs a fresh verifier. `<command> | tail` hides the command's exit status behind `tail`'s. The verifier therefore writes each step's full output to a log outside the tree and prints only the exit code, the log path and the failure lines.

## Part A: `agents/loom-verifier.md`

Re-read the file first. Change only the lines named here; plan B's text elsewhere in the file stays as you find it.

1. Frontmatter `description:` (anchored by the text `Writes nothing and never fixes.`): replace that sentence with `Writes only its gate logs under $LOOM_SCRATCH_DIR and never fixes.` Leave `name`, `tools`, `model`, `effort` and `maxTurns` unchanged.
2. `## What You Run`: directly after the numbered list (items 1-3), before whatever paragraph follows it, insert this paragraph, block and paragraph. Anchor on the end of the numbered list, never on the next paragraph's text, which plan B may have reworded.

   The four-backtick `markdown` fence below only marks the text to insert: insert what lies between its fences, unindented, with a blank line before and after. The two `bash` block lines are exact. The frozen contract extracts the first fenced `bash` block that contains `gate-<step>.log` and runs its lines with its own values for the three placeholders.

   ````markdown
   Run every gate step through this recipe:

   ```bash
   <command> > "$LOOM_SCRATCH_DIR/gate-<step>.log" 2>&1; echo "exit=$? log=$LOOM_SCRATCH_DIR/gate-<step>.log"
   rg -n -m 20 -B 2 -A 8 -M 300 --max-columns-preview '<failure pattern>' "$LOOM_SCRATCH_DIR/gate-<step>.log" | head -c 6000
   ```

   `<step>` is a short step name (`build`, `test`, `doctest`, `clippy`, `fmt`, `doc`). `<failure pattern>` is the runner's failure marker; for cargo, `FAILED|panicked at|^error|^warning:`. The log keeps the full output, because the Bash tool cuts output past 12,000 characters and a step printed whole can lose its failure list. The first line reports the step's own exit code and the log path. The excerpt is bounded in lines and bytes, so one huge failing line cannot flood the report. Never pipe the step itself into `tail` or `head`: the pipe reports the last command's status, so a failing step reads as exit 0. When the excerpt does not explain a failure, read more of the log with `rg` or by line range.

   Criteria are not run through the recipe. Run `loom check <stage-id>` bare, as its own Bash call with stdout unfiltered: it relays a request, and a redirect or pipe hides the line the relay needs. On exit 3 run it again. For each failing criterion it prints `full log: <path>`; read that file with the recipe's `rg ... | head -c 6000` line.
   ````

3. `## Constraints`, the first bullet (anchored by `**Writes no files**`): replace the whole bullet line with this line (the `text` fence is not part of it):

   ```text
   - **Writes only its gate logs**: one `$LOOM_SCRATCH_DIR/gate-<step>.log` per step, outside the tree. No Edit, no Write, no other redirection, no formatter or linter in fix mode (`cargo fmt` runs as `cargo fmt --check`), no dependency changes. Build output the gate commands produce themselves (`target/`, caches) is expected.
   ```

4. `## Output Format`: in the `STEP` block add the line `log: <the step's log path>` directly after `exit: <code>`.

Nothing else in the file changes.

## Part B: `VERIFIER_GATE` (`loom/src/orchestrator/signals/cache/blocks.rs`)

Re-read the constant: plan B set its current text. Rewrite its string by these rules, then report the old and new byte lengths:

1. It keeps, as exact substrings:
   - `**The gate runs in a`loom-verifier`**` (`only_the_standard_prefix_sends_the_gate_to_a_loom_verifier`, `tests_doctrine_prefixes.rs`, pins it);
   - `` `loom check <stage-id>` ``;
   - `RUNNER-ENV`;
   - `SESSION-ENV`.

   The relocation manifest pins the last three: once this plan removes the orchestration skill's gate paragraph, they are the main agent's only rule for the verifier's environment reports.
2. It says the verifier writes only gate logs in `$LOOM_SCRATCH_DIR`, in place of any "writes nothing" wording.
3. It drops `(opus, effort xhigh)`, because the agent definition pins model and effort.
4. Its byte length is no greater than the base's. The standard stable prefix and floor sit near their ceilings (`STABLE_PREFIX_MAX_BYTES` 8,192 and `STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` 10,240 in `tests_size.rs`). Shorter is better.
5. It ends with the `\n\n` escape the string already ends with.

A text in the right shape, measured at 463 bytes and to be trimmed to the base's length:

```text
**The gate runs in a `loom-verifier`**: once every implementer returned, spawn ONE by agent type, without `name`, briefed with the criteria. It writes only gate logs in `$LOOM_SCRATCH_DIR`, reports PASS/FAIL per step with exit code and failing excerpt, and runs criteria via `loom check <stage-id>`: pass there, fail in its shell is SESSION-ENV (no FAIL); fail only from the confined runner is RUNNER-ENV, yours to dispute. Delegate fixes; spawn a FRESH verifier.\n\n
```

Do not change the doc comment or `append_verifier_completion_header`. Keep the constant on one line.

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

- The recipe block must be the first fenced `bash` block in the file that contains `gate-<step>.log` (plan B added no fenced block). Use only the placeholders `<command>`, `<step>` and `<failure pattern>`.
- The recipe contains no `| tail`. The contract checks that; the excerpt line pipes into `head -c` only.
- Never write a recipe or an example that redirects or pipes a `loom` command.
- `rustfmt` leaves long string literals alone; run `cargo fmt` on your Rust files and keep `VERIFIER_GATE` on one line.
- `RETIRED_PHRASES` (`loom/src/orchestrator/signals/tests_doctrine_blocks.rs`) are asserted absent from every `agents/*.md`; the gate's `cargo test --lib signals::` catches a hit.
