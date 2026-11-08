# w1: Doctrine text, one rule on every surface

Plan: `doc/plans/PLAN-full-suite-at-gate.md` (sections "Decision" and "Changes 1 and 3").
Scope: prose and the Rust constants and tests that pin it. No behavior code.

Implementation note: exact replacement prose below is a draft. Final wording is trimmed to fit the existing doctrine size caps; the rule and pinned surfaces remain as described here.

Rule being installed, in one paragraph:

- Standard stages: a subagent runs the tests for the code it wrote and its module when it is confident
  the code is ready or the fix is correct. It never runs the whole suite. The orchestrator runs the
  full suite only through `loom check`. A repeated `loom check` is never refused.
- Integration-verify (IV) is exempt: its orchestrator and reviewers may run the full suite, always
  through `loom check integration-verify [--criterion <n>]`, never a bare runner in the shell.
- No duration ("ten minutes") appears anywhere.

## Decisions made while writing this brief

1. **BLOCK-A heading stays** (`VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:`). About ten tests and
   the hook match it. Only the bullets change.
2. **Compile bullet loses its tail.** "No full test suite, no linter, no `cargo fmt`, and never a
   repeated or looping check" becomes "No linter and no `cargo fmt`." The whole-suite ban moves to the
   new run bullet, and the repeat ban goes away.
3. **The IV prefix drops BLOCK-A and its intro line** ("The spawn guard prepends ... Paste BLOCK-A
   verbatim"). IV reviewers still get the preamble from `spawn-guard.sh` on typed spawns, and its
   EXCEPTION bullet covers them.
4. **`loom-code-reviewer` has no Bash** (`tools: Read, Glob, Grep`). Review dimension 3 needs a
   subagent that can run `loom check`, so the new dimension 3 text names `loom-software-engineer`.
5. **Codex lane (`codex-forward.sh`, `AGENTS.md.template`) gets the same rule**, with the sandbox
   caveat that a test the sandbox cannot run is skipped and named in the report. Drop step 9 if the
   operator wants the codex lane unchanged.
6. **BLOCK-B, BLOCK-E, `CODE_STAGE_GATE`, `format/codex.rs:213` and `helpers.rs:236` stay as they are.**
   They say "the gate is the orchestrator's single `loom check`" or "run `loom check` once beside the
   final review". That describes placement, not a limit, and each is pinned in 2-4 places.

## Size limits (Rule 17, enforced by `cargo test --test maintainability`)

| File | Lines now | Rule |
| --- | --- | --- |
| `loom/src/orchestrator/signals/cache.rs` | 511 | Over 400 and recorded exactly in `loom/maintainability-baseline.txt` line 16 (`file src/orchestrator/signals/cache.rs 511`). The final implementation remains at 511 lines and preserves that baseline; any mismatch fails. |
| `loom/src/orchestrator/signals/cache/blocks.rs` | 142 | Fine. Net +1 to +3 lines. |
| `loom/src/orchestrator/signals/tests_doctrine.rs` | 369 | Keep under 400. The new assertion adds about 8 lines. |
| `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` | 155 | Fine. |
| `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` | 308 | Fine. |
| `loom/src/orchestrator/signals/tests_doctrine_gate.rs` | 141 | Fine. |
| `loom-hooks/subagent-verify-guard.sh` | 606 | Over 400; `w4-install-and-guard.md` splits it. Do not grow it. |
| `loom-hooks/codex-forward.sh` | 412 | Over 400. Edit in place, no net growth. |
| `loom-hooks/commit-filter.sh` | 463 | Over 400. Edit in place, no net growth. |
| `loom-hooks/_subagent-preamble.txt` | 41 | Spawn guard prepends it to every typed spawn; net +2 lines at most. |
| `CLAUDE.md.template`, `skills/loom-orchestration/SKILL.md`, `AGENTS.md.template`, agents | 233, 288, 144, 93 | Prose, no limit; keep each edit to the lines named. |

No function limit is at risk: `generate_integration_verify_stable_prefix` is 38 lines and shrinks by 1.

## Traps

- **Byte-identical BLOCK-A.** `tests_doctrine.rs::block_a_agrees_across_every_surface` does
  `text.contains(BLOCK_A)`. After step 1 there are four exact copies: the preamble, `blocks.rs`
  (assembled from `push_str` lines), `tests_doctrine_blocks.rs::BLOCK_A`, and the guard `block()` heredoc.
  Copy the text below verbatim, including the two leading spaces on continuation lines. No trailing
  newline in `BLOCK_A` (it ends at `MAIN AGENT fixes.`); the `push_str` copy ends with `\n\n`.
- **Apostrophes.** `MAIN AGENT'S` and `stage's` sit inside Rust `"..."` literals (fine, no escape) and
  inside the guard heredoc `<<'EOF'` (quoted, so no expansion and no escape). Do not switch the heredoc
  to unquoted `<<EOF`: backticks would then run as command substitution.
- **Rust `\` continuation** in `INTEGRATION_VERIFY_OVERRIDE` strips the next line's indentation, so every
  source line must end with a space before `\`. Needles in tests match the compiled text, one single
  space across each break.
- **`cache.rs` baseline** (see table). The old assertion at `cache.rs:381` is replaced one for one, but
  the override text changes the line count.
- **Guard file is split by `w4`.** `block()` and the header comment sit above the runner matchers
  (`check_cargo` through `check_command`), so they stay in `subagent-verify-guard.sh` when `w4` moves
  the matchers into `_verify_matchers.sh`. Apply step 8 after `w4`'s split and find the text by anchor,
  not by line number (`rg -n 'VERIFICATION IS THE MAIN' loom-hooks`). If `block()` is moved into the
  sourced file, `tests_doctrine.rs::HOOK_SUBAGENT_VERIFY_GUARD` (an `include_str!` of the main file)
  must point at the file that holds it. I could not read `w4`'s brief when writing this one; confirm
  against `doc/plans/briefs/full-suite-at-gate/w4-install-and-guard.md`.
- **Hook test snippet.** `loom/tests/integration/hooks_subagent_verify_guard.rs:203`
  (`BLOCK_MESSAGE_SNIPPET`) also asserts the block message; it changes in step 8.
- **CHANGELOG.** Do not edit `CHANGELOG.md`; the `/changelog` skill writes it at release.
- **Installed copies.** `~/.claude/hooks/loom/_subagent-preamble.txt` and `~/.claude/CLAUDE.md` are
  copies. They update only through `dev-install.sh`. Nothing in this brief edits them.
- **Do not touch** the lines `**Review order (plan v2):** run `loom check` once beside the final
  review ...` in `SKILL.md:232`, `v2_section_review.rs:20`, `v2_section_tests.rs:13`,
  `tests_doctrine_v2.rs:38`. They are byte-pinned in four places and stay true.
- **Knowledge docs** (`doc/loom/knowledge/conventions/guidance-channels-and-plugin-scope.md`,
  `architecture/verification-v2-gates.md`) are updated by the main agent after the stage, not here.

## Final texts

### BLOCK-A (new)

```text
VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:
- Compile the files YOU wrote as often as you need (`cargo check`,
  `cargo build`, `tsc --noEmit`, `bun run build`) and format them with
  `rustfmt --edition 2021 <files>`. No linter and no `cargo fmt`.
- Run the tests for the code YOU wrote, and its module, when you are
  confident your code is ready or your fix is correct. Never the whole
  test suite: the stage's gate runs it.
- Report instead: files changed, assumptions made, anything unresolved.
  The stage's gate compiles, tests, and lints; the MAIN AGENT fixes.
```

Old text, for reference (this is what each site holds today):

```text
VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:
- Compile the files YOU wrote as often as you need (`cargo check`,
  `cargo build`, `tsc --noEmit`, `bun run build`) and format them with
  `rustfmt --edition 2021 <files>`. No full test suite, no linter, no
  `cargo fmt`, and never a repeated or looping check.
- AT MOST ONE narrowly-scoped check over the files YOU wrote (e.g.
  `cargo test <your_module>::`), run ONCE. Skip it if you are unsure.
- Report instead: files changed, assumptions made, anything unresolved.
  The stage's gate compiles, tests, and lints; the MAIN AGENT fixes.
```

## Steps

Each step ends with one acceptance command. Run `rg` from `/home/dkaponis/src/loom`.

### Step 1. BLOCK-A in its four byte-identical copies

**1a. `loom-hooks/_subagent-preamble.txt` lines 11-24.** Replace lines 12-24 (everything after the
`VERIFICATION IS ...` heading) with:

```text
- Compile the files YOU wrote as often as you need (`cargo check`,
  `cargo build`, `tsc --noEmit`, `bun run build`) and format them with
  `rustfmt --edition 2021 <files>`. No linter and no `cargo fmt`.
- Run the tests for the code YOU wrote, and its module, when you are
  confident your code is ready or your fix is correct. Never the whole
  test suite: the stage's gate runs it.
- Report instead: files changed, assumptions made, anything unresolved.
  The stage's gate compiles, tests, and lints; the MAIN AGENT fixes.
- EXCEPTION - integration-verify stages are exempt from the test rule
  above. Their orchestrator and reviewers may run the full suite, only
  through `loom check integration-verify [--criterion <n>]`, never as a
  bare runner in the shell.
- A subagent never runs the stage's gate, except in integration-verify
  stages (above); the main agent re-checks one criterion with
  `loom check <stage-id> --criterion <n>`.
```

Current lines 20-24 being replaced by the last two bullets:

```text
- EXCEPTION - integration-verify stages: the orchestrator runs loom check
  integration-verify itself; reviewers run only targeted checks, never
  the full suite.
- A subagent never runs the stage's gate; the main agent re-checks one
  criterion with `loom check <stage-id> --criterion <n>`.
```

Blank line 25 and the `CONTEXT CEILING` block stay.

**1b. `loom/src/orchestrator/signals/cache/blocks.rs` lines 43-51** (`append_no_verify_block`). Replace
the six `push_str` calls on lines 44-49 with:

```rust
    content.push_str("- Compile the files YOU wrote as often as you need (`cargo check`,\n");
    content.push_str("  `cargo build`, `tsc --noEmit`, `bun run build`) and format them with\n");
    content.push_str("  `rustfmt --edition 2021 <files>`. No linter and no `cargo fmt`.\n");
    content.push_str("- Run the tests for the code YOU wrote, and its module, when you are\n");
    content.push_str("  confident your code is ready or your fix is correct. Never the whole\n");
    content.push_str("  test suite: the stage's gate runs it.\n");
```

Lines 43 (heading push), 50-51 (Report bullet) stay. Also extend the doc comment above the function
(line 36-37) with one sentence: "The standard prefix only: the integration-verify prefix carries
`INTEGRATION_VERIFY_OVERRIDE` instead (`cache.rs`)."

**1c. `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` lines 11-19.** Replace the `BLOCK_A` literal
body with the BLOCK-A (new) text above. Keep `pub(super) const BLOCK_A: &str = "VERIFICATION ...` as the
first line and the closing `MAIN AGENT fixes.";`. Also update the doc comment on lines 8-10 only if it
mentions "every surface": change "Every surface must carry this text byte for byte" to "Every surface
except the integration-verify prefix must carry this text byte for byte".

Add to `RETIRED_PHRASES` (same file, near `concat!("hard ceiling on any", " single check"),` at line 113),
using `concat!` so this file never holds the phrase contiguously:

```rust
    // Retired with the full-suite-at-the-gate rule: a subagent runs the tests
    // for its own code and module, with no count limit.
    concat!("AT MOST ONE narrowly", "-scoped"),
    concat!("never a repeated or ", "looping check"),
```

**1d. Guard `block()` heredoc** is step 8 (it moves after `w4`). Until step 8 lands,
`block_a_agrees_across_every_surface` fails on the guard; that is expected.

Acceptance after 1a-1c:

```bash
rg -n "AT MOST ONE narrowly|repeated or looping|No full test suite" loom-hooks/_subagent-preamble.txt loom/src/orchestrator/signals/cache/blocks.rs
```

Expected: no output.

### Step 2. IV prefix stops carrying BLOCK-A; new override text

**2a. `loom/src/orchestrator/signals/cache.rs` line 152.** Delete the line
`    append_no_verify_block(&mut content);` inside `generate_integration_verify_stable_prefix`. Keep line 104
(the standard prefix call). The import at line 12 stays (the standard prefix still uses it). After the
delete the IV block reads:

```rust
    append_execution_rules_header(&mut content);
    content.push_str(LOAD_ORCHESTRATION_SKILL);
    content.push_str(INTEGRATION_VERIFY_OVERRIDE);
    append_subagent_ceiling_block(&mut content);
```

The ceiling block opens "Also paste the ceiling rule:". The word "Also" now follows the override. That is
acceptable; do not touch `blocks.rs:64`, it is shared.

**2b. `cache.rs` lines 115-126.** Replace the whole `INTEGRATION_VERIFY_OVERRIDE` constant with:

```rust
const INTEGRATION_VERIFY_OVERRIDE: &str = "⚠️ **INTEGRATION-VERIFY OVERRIDE — the standard-stage test rule does NOT apply here:**\n\n\
         This stage may run the full suite, and every full run goes through \
         `loom check integration-verify [--criterion <n>]`, never a bare test runner in the shell. \
         The IV orchestrator runs `loom check integration-verify`; it carries the COMPLETE suite \
         (e.g. `cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`) \
         and the orchestrator reads all stderr for each immutable tree, environment, and criterion \
         contract. Give reviewers the check summaries and evidence pointers. \
         Reviewers inspect independently and run `loom check integration-verify` themselves, with \
         `--criterion <n>` for the criteria they need. On one tree the first request runs, a request \
         the running job covers joins it, and a later one hits the pass cache. Reviewers may add \
         targeted discriminating security/functional checks; those are additional evidence, never \
         a substitute for the canonical gate. Each check summary records command, real exit, \
         criterion verdict, input/contract identity, elapsed time, and an evidence pointer when \
         available. A missing receipt means run the check. After any review fix, the orchestrator \
         re-evaluates invalidated checks; only the repaired criteria cache decides reuse, and stale \
         evidence never waives a failing gate.\n\n";
```

The sentence "On one tree the first request runs, a request the running job covers joins it, and a
later one hits the pass cache" describes the job joining that `w3-host-jobs.md` implements. Keep it.

**2c. `cache.rs:381`.** The standard-prefix test asserts the old phrase. Replace the line

```rust
        assert!(prefix.contains("AT MOST ONE narrowly-scoped check"));
```

with

```rust
        assert!(prefix.contains("Run the tests for the code YOU wrote, and its module,"));
```

**2d. `loom/maintainability-baseline.txt` line 16.** Run `wc -l loom/src/orchestrator/signals/cache.rs`
and write the exact count into `file src/orchestrator/signals/cache.rs <count>`. (Do this last in the
step, after 2a-2c.)

Acceptance:

```bash
rg -n "append_no_verify_block" loom/src/orchestrator/signals/cache.rs
```

Expected: exactly two lines (the `use` on line 12 and the standard-prefix call near line 104).

### Step 3. Review dimension 3 (`blocks.rs::append_review_dimension_details`, line 116)

Current:

```rust
    content.push_str("3. **Build/test/sandbox** — inspect the `loom check` summaries and log pointers supplied by the orchestrator, including ALL stderr and warnings even when tests pass; any \"blocked\", \"denied\", \"connection refused\" or \"failed to download\" is a BLOCKER, not a workaround. Exit code 0 does NOT mean success.\n");
```

Replacement:

```rust
    content.push_str("3. **Build/test/sandbox** — spawn this one as `loom-software-engineer`, since `loom-code-reviewer` has no Bash. It runs `loom check integration-verify` (add `--criterion <n>` for the criteria it needs; never a bare test runner in the shell) and inspects the summaries and log pointers, including ALL stderr and warnings even when tests pass; any \"blocked\", \"denied\", \"connection refused\" or \"failed to download\" is a BLOCKER, not a workaround. Exit code 0 does NOT mean success.\n");
```

Pinned fragments that survive: `Build/test/sandbox`, `warnings even when tests pass`,
`is a BLOCKER, not a workaround`, `Exit code 0 does NOT mean success`.

Acceptance:

```bash
rg -n "supplied by the orchestrator" loom/src
```

Expected: no output.

### Step 4. `ORCHESTRATOR_GATE` (`blocks.rs:73`)

Current:

```rust
const ORCHESTRATOR_GATE: &str = "**The gate is `loom check`**: once every implementer has returned, run the formatter, then `loom check <stage-id>` once plus the steps this stage lists outside acceptance, beside the final review. A fix round runs one `loom check <stage-id> --criterion <n> ...` with every failed criterion plus the lint and format criteria. Never re-run what the check covers.\n\n";
```

Replacement:

```rust
const ORCHESTRATOR_GATE: &str = "**The gate is `loom check`**: once every implementer has returned, run the formatter, then `loom check <stage-id>` plus the steps this stage lists outside acceptance, beside the final review. A fix round runs `loom check <stage-id> --criterion <n> ...` with every failed criterion plus the lint and format criteria. The full suite runs only through `loom check`, never as a bare runner in the shell; an unchanged tree hits the pass cache, so a repeated check costs little.\n\n";
```

Pins that must survive: `**The gate is `loom check`**`, `` `loom check <stage-id>` ``, `--criterion <n>`
(`tests_doctrine_prefixes.rs:65`, `tests_doctrine_gate.rs:51`, `tests_guidance_manifest.rs:138,168`).

Add one test to `tests_doctrine_gate.rs`, after `gate_block_and_orchestration_skill_state_the_check_flow`
(line 55):

```rust
#[test]
fn gate_block_keeps_the_full_suite_behind_loom_check() {
    let prefix = generate_stable_prefix();
    assert!(
        prefix.contains("The full suite runs only through `loom check`"),
        "the standard prefix must send the full suite through `loom check`"
    );
    assert!(
        !prefix.contains("Never re-run what the check covers"),
        "the retired re-run ban must not return: a repeated check is cheap, not refused"
    );
}
```

Acceptance:

```bash
rg -n "Never re-run what the check covers" loom/src --glob '!tests_doctrine_gate.rs'
```

Expected: no output.

### Step 5. Pin tests

**5a. `tests_doctrine.rs::block_a_agrees_across_every_surface` (lines 112-133).** Replace the function with:

```rust
#[test]
fn block_a_agrees_across_every_surface() {
    let signal_prefix = generate_stable_prefix();
    let iv_prefix = generate_integration_verify_stable_prefix();

    for (label, text) in [
        ("signal stable prefix", signal_prefix.as_str()),
        (PREAMBLE_LABEL, HOOK_SUBAGENT_PREAMBLE),
        (
            "loom-hooks/subagent-verify-guard.sh",
            HOOK_SUBAGENT_VERIFY_GUARD,
        ),
    ] {
        assert!(
            text.contains(BLOCK_A),
            "{label} does not carry BLOCK-A verbatim. The no-verify rule must be \
             byte-identical on every surface an agent reads it from; reword one \
             and you must reword all of them. Expected to find:\n{BLOCK_A}"
        );
    }

    // Integration-verify is exempt from the test rule: its full runs go through
    // `loom check integration-verify`, so its prefix carries the override and
    // never BLOCK-A.
    assert!(
        !iv_prefix.contains("VERIFICATION IS THE MAIN AGENT'S JOB")
            && !iv_prefix.contains("Run the tests for the code YOU wrote"),
        "the integration-verify prefix must not carry BLOCK-A"
    );
}
```

**5b. `tests_doctrine_prefixes.rs`.**

- `integration_verify_prefix_retains_required_quality_and_isolation_guards` (needle list, line 162): delete
  the line `        "VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS",`. Keep `"INTEGRATION-VERIFY OVERRIDE"` and
  `"does NOT apply here"` (the new heading still contains both).
- `integration_verify_prefix_retains_required_review_and_doctrine_guards` (needle list, lines 184-192): add
  `"**Build/test/sandbox** — spawn this one as `loom-software-engineer`",`.
- `integration_verify_prefix_assigns_one_canonical_gate_owner` (needle list, lines 224-236): replace three
  needles. Old, then new:

```text
"runs `loom check integration-verify` ONCE",
"Reviewers inspect independently and run only targeted",
"targeted checks are additional evidence, never a substitute",
```

```rust
        "The IV orchestrator runs `loom check integration-verify`; it carries the COMPLETE suite",
        "every full run goes through `loom check integration-verify [--criterion <n>]`, never a bare test runner",
        "Reviewers inspect independently and run `loom check integration-verify` themselves",
        "those are additional evidence, never a substitute",
```

  After the list, next to `assert!(prefix.contains("cargo clippy -- -D warnings"));`, add:

```rust
    assert!(!prefix.contains("AT MOST ONE narrowly"), "IV carries no standard-stage test rule");
```

- Rename and rewrite `subagent_preamble_limits_iv_reviewers_to_targeted_checks` (lines 249-273). Doc comment:
  "The subagent preamble reaches every typed spawn, integration-verify reviewers included: it exempts
  them from the test rule and routes their full runs through `loom check integration-verify`." New
  function name `subagent_preamble_routes_iv_full_runs_through_loom_check`; needles:

```rust
    for needle in [
        "- EXCEPTION - integration-verify stages are exempt from the test rule",
        "through `loom check integration-verify [--criterion <n>]`, never as a",
        "- A subagent never runs the stage's gate, except in integration-verify",
    ] {
```

  and keep the two `assert!`s after the loop, adding to the first one:
  `assert!(!HOOK_SUBAGENT_PREAMBLE.contains("reviewers run only targeted checks"), "...");`.
  The last assert (`generate_integration_verify_stable_prefix().contains("loom check integration-verify")`)
  stays as is.

Acceptance:

```bash
rg -n "run only targeted|integration-verify. ONCE|limits_iv_reviewers" loom/src/orchestrator/signals
```

Expected: no output.

### Step 6. Orchestration skill and CLAUDE.md.template

**6a. `skills/loom-orchestration/SKILL.md:121`** (coordinator preamble, inside a `text` fence). Current:

```text
- Compile your workers' files freely; AT MOST ONE narrowly-scoped check over the files your workers wrote (e.g. `cargo test <your_module>::`), run ONCE; skip it if you are unsure. The stage's gate runs after you return; the MAIN AGENT owns fixes.
```

Replacement:

```text
- Compile your workers' files freely. Run the tests for the code your workers wrote, and its module, when you are confident it is ready; never the whole test suite, the stage's gate runs it after you return. In an integration-verify stage a full run goes through `loom check integration-verify [--criterion <n>]`, never a bare runner. The MAIN AGENT owns fixes.
```

Line 122 ("Return files changed, verification command + result ...") stays. Line 176 (Rule 7 point 3) and line
232 stay (see Traps). The coordinator preamble is not byte-pinned, but `SKILL.md` is scanned by the
`RETIRED_PHRASES` sweep, so the old phrase must be gone.

**6b. `CLAUDE.md.template` line 96** (Rule 5). Current:

```text
Subagents write code and report; the main agent owns fixes and all git operations (hard stop 3). A subagent compiles its own files freely and runs at most one narrowly scoped check, once. A standard stage's gate is the main agent's single `loom check`; a fix round re-checks only the failed criteria with `--criterion <n>`; integration-verify keeps its own. `loom-hooks/spawn-guard.sh` prepends the subagent preamble, `loom-hooks/_subagent-preamble.txt` (git, verification, ceilings, memory), to every typed spawn whose prompt lacks it; never paste it. Codex subagents get theirs from `loom-hooks/codex-forward.sh`.
```

Replacement:

```text
Subagents write code and report; the main agent owns fixes and all git operations (hard stop 3). A subagent compiles its own files freely and runs the tests for the code it wrote and its module when it is confident the code is ready or the fix is correct, never the whole suite. A standard stage's gate is the main agent's `loom check`, the only place the full suite runs; a fix round re-checks only the failed criteria with `--criterion <n>`. Integration-verify is exempt: its orchestrator and reviewers may run the full suite, always through `loom check integration-verify [--criterion <n>]`, never a bare runner. `loom-hooks/spawn-guard.sh` prepends the subagent preamble, `loom-hooks/_subagent-preamble.txt` (git, verification, ceilings, memory), to every typed spawn whose prompt lacks it; never paste it. Codex subagents get theirs from `loom-hooks/codex-forward.sh`.
```

`tests_guidance_manifest.rs` needs only `_subagent-preamble.txt` from this line (R5 relocation); the line
is not in `FROZEN_LINES`. Rule 7 text in the template (line 125) names no test limit; no change.

Acceptance:

```bash
rg -n "AT MOST ONE narrowly|at most one narrowly|run ONCE" skills/loom-orchestration CLAUDE.md.template
```

Expected: no output.

### Step 7. Agent definitions

**7a. `agents/loom-senior-software-engineer.md:87`.** Replace only the sentence
`At most ONE narrowly-scoped check over files you touched directly, run ONCE, skipped if unsure — verification beyond that belongs to the stage's gate.`
with
`Run the tests for the code you wrote and its module when you are confident it is ready; never the whole suite, which the stage's gate runs.`
The opening clause "not by running the test suite or any linter (compiling your own files is fine)"
becomes "not by running a linter (compiling your own files is fine)". Resulting start of the line:

```text
Before reporting work done, review the diff — not by running a linter (compiling your own files is fine) — across the same six-dimension adversarial review ...
```

**7b. `agents/loom-software-engineer.md:72`.** Current: `This pass runs no build, suite or linter:`.
Replacement: `This pass runs no linter and no whole-suite run:`. Line 81 ("The stage's gate compiles, tests, and
lints ...") stays.

**7c.** `agents/loom-software-developer.md` has no verification text; no change. `loom-code-reviewer.md`
line 32 ("Read-only review does not claim tests were run") stays.

Acceptance:

```bash
rg -n "ONE narrowly|runs no build, suite" agents
```

Expected: no output.

### Step 8. Hook guard text (apply after `w4`'s split)

All four edits are in `loom-hooks/subagent-verify-guard.sh`. Find each by anchor.

**8a. `block()` heredoc** (anchor `[subagent-verify-guard/subagent-verify]`). Replace the BLOCK-A lines
under the `BLOCKED:` framing line with the BLOCK-A (new) text. The framing line and blank line before
`VERIFICATION IS ...` stay. The heredoc is quoted (`<<'EOF'`); keep it quoted.

**8b. `block()` main-agent message** (anchor `gate-once] ⛔ BLOCKED`). Current:

```text
[subagent-verify-guard/gate-once] ⛔ BLOCKED: the stage gate runs once, in loom check $STAGE_ID; re-check one criterion with loom check $STAGE_ID --criterion <n>; run your own work with a filtered test
```

Replacement (unquoted heredoc, so `$STAGE_ID` expands; no backticks):

```text
[subagent-verify-guard/gate-once] ⛔ BLOCKED: the full suite runs only through loom check $STAGE_ID; re-check one criterion with loom check $STAGE_ID --criterion <n>; run your own work with a filtered test
```

The tag `gate-once` (also the `loom_record_block` id) stays: renaming it is outside this plan.
`hooks_subagent_verify_guard_main.rs:62` asserts `loom check {STAGE}` and `--criterion`; both remain.

**8c. Header comment, lines 10-13.** Replace `(the stage gate runs once, in `loom check`)` with
`(the full suite runs only through `loom check`)`. Update the matching comment at
`loom/tests/integration/hooks_subagent_verify_guard_main.rs:4`: `(the stage gate runs once, in `loom check`)`
becomes `(the full suite runs only through `loom check`)`.

**8d. Header comment, lines 30-33 (carve-out).** Replace
`(its orchestrator runs `loom check`; the reviewers run only targeted checks, which is doctrine in the subagent preamble, not enforced here)`
(the text is wrapped over lines 32-33) with
`(its orchestrator and reviewers run the full suite through `loom check integration-verify`, which is doctrine in the subagent preamble, not enforced here)`.
The hook logic stays unchanged.

**8e. `loom/tests/integration/hooks_subagent_verify_guard.rs:203`.** Replace

```rust
const BLOCK_MESSAGE_SNIPPET: &str = "AT MOST ONE narrowly-scoped check";
```

with

```rust
const BLOCK_MESSAGE_SNIPPET: &str = "Run the tests for the code YOU wrote, and its module,";
```

It is also used by `hooks_subagent_verify_guard_carveout.rs:178` through the parent module; no edit there.

Acceptance:

```bash
rg -n "AT MOST ONE narrowly|repeated or looping|gate runs once" loom-hooks/subagent-verify-guard.sh loom-hooks/_verify_matchers.sh loom/tests/integration
```

Expected: no output (`_verify_matchers.sh` may not exist; `rg` then reports it and exits 2, which is fine).

### Step 9. Other copies of the rule

- **`loom-hooks/commit-filter.sh` lines 308-311** (subagent-git block message, in place, same line count).
  Current lines 308-309:

```text
2. Run AT MOST ONE narrowly-scoped check covering only what you changed
   (`cargo test <filter>`, `cargo test --test <name>`) - never the full suite
```

  Replacement:

```text
2. Run the tests for the code you wrote and its module (`cargo test <filter>`,
   `cargo test --test <name>`) - never the full suite, the stage's gate runs it
```

- **`loom-hooks/codex-forward.sh` lines 99-101** (codex preamble, in place, same 3 lines). Current:

```text
DO NOT VERIFY. No full build, no test suite, no linter, no formatter, no type-checker, and never
a repeated or looping check. At most ONE narrowly-scoped check over the files you changed, run
once; skip it if you are unsure. The orchestrator compiles, tests, lints, and fixes.
```

  Replacement:

```text
DO NOT VERIFY THE WHOLE TREE. No full build, no whole test suite, no linter, no formatter, no
type-checker. Run the tests for the code you wrote and its module when you are confident it is
ready; skip a test the sandbox cannot run and say so. The orchestrator compiles, tests, lints, fixes.
```

  Line 90 ("Test execution follows the verification boundary below.") stays.

- **`AGENTS.md.template` lines 54-57.** Current:

```text
Do not perform verification for a loom stage. This includes full builds, test suites, linters,
formatters, and type-checkers. Never run a repeated or looping check. At most, run one
narrowly-scoped check over the files you changed, exactly once, and skip it when unsure. The
orchestrator performs integration verification.
```

  Replacement:

```text
Do not verify the whole tree for a loom stage. This includes full builds, whole test suites,
linters, formatters, and type-checkers. Run the tests for the code you wrote and its module when
you are confident it is ready, and skip a test the sandbox cannot run, saying so in your report. The
orchestrator performs integration verification.
```

- **`README.md:965`** (section "Implementers Do Not Verify"). Replace the first sentence
  `An implementing subagent may run **at most one narrowly-scoped check** covering the files it just changed.`
  with
  `An implementing subagent runs the tests for the code it wrote and its module when it is confident the code is ready or the fix is correct, never the whole suite.`
  In the same paragraph replace `the orchestrator runs one `loom check` on the finished tree, and an `integration-verify` stage keeps its own gate, as its signal directs`
  with `the orchestrator runs `loom check` on the finished tree, and an `integration-verify` stage is exempt: its orchestrator and reviewers run the full suite through `loom check integration-verify [--criterion <n>]``.

- **`README.md:967`**, in the second paragraph: replace `because the gate runs once in `loom check`` with
  `because the full suite runs only through `loom check``.

- **`README.md:971`** (carve-out bullet). Replace the sentence
  `That stage type keeps its own gate: its orchestrator runs `loom check integration-verify` once, and its reviewers run only targeted checks, never the full suite. The hook does not enforce that limit; it only lets those subagents through.`
  with
  `That stage type is exempt from the test rule: its orchestrator and reviewers may run the full suite, always through `loom check integration-verify [--criterion <n>]`, never a bare runner. The hook does not enforce that; it only lets those subagents through.`

- **`README.md:1103`**: replace `In a `standard` stage the gate is the orchestrator's single `loom check`` with
  `In a `standard` stage the gate is the orchestrator's `loom check`, the only place the full suite runs`.

- **`CONTRIBUTING.md:96`.** Replace
  `a subagent may run at most one narrowly scoped check over the files it changed. Whole-project verification belongs to the stage's gate: the orchestrator's single `loom check` in a `standard` stage; an `integration-verify` stage keeps its own.`
  with
  `a subagent runs the tests for the code it wrote and its module, never the whole suite. The full suite runs only through `loom check`: the orchestrator's gate in a `standard` stage, and `loom check integration-verify` in an `integration-verify` stage, whose reviewers may run it too.`

- **`skills/loom-plan-writer/references/parallelization.md:5`.** Replace
  `an OPTIONAL per-coordinator `Verify:` line — AT MOST ONE narrowly-scoped check over the files that coordinator's workers wrote, run ONCE, skipped if the coordinator is unsure; it is not a substitute`
  with
  `an OPTIONAL per-coordinator `Verify:` line — the tests for the code that coordinator's workers wrote and its module, never the whole suite; it is not a substitute`.
  `w2-skills.md` does not cover this file, so w1 owns it.

- **`skills/loom-plan-writer/references/parallelization.md:52-53`** (the example `EXECUTION PLAN` block). Replace each
  `verify (optional, ONE scoped check, skip if unsure):`
  with
  `verify (optional; the tests for its workers' code and module, never the whole suite):`
  keeping the `Coordinator A` and `Coordinator B` prefixes and the trailing commands as they are.

- **`skills/loom-plan-writer/references/codex-implementers.md:23`**, Claude-lane cell. Replace
  `One narrowly scoped check, run once, fixture-backed tests included`
  with
  `The tests for the code it wrote and its module, fixture-backed tests included, never the whole suite`.
  The codex-lane cell stays (the sandbox mounts TMPDIR read-only).

Acceptance:

```bash
rg -n "AT MOST ONE narrowly|narrowly-scoped check|narrowly scoped check|repeated or looping|run ONCE" --glob '!target' --glob '!doc/**' --glob '!web/dist/**' --glob '!CHANGELOG.md' .
```

Expected: no output. (Unrelated hits that remain are fine only if they are not this rule: the
`narrowly-scoped capability` text in `README.md:958`, `loom/src/sandbox/settings/tests.rs:236` and
`loom/src/commands/stage/tests/admin_proof.rs:328` are about other things; ignore them.)

### Step 10. Final sweep and verification

Sweep, from `/home/dkaponis/src/loom`:

```bash
rg -n "AT MOST ONE narrowly-scoped" --glob '!target' --glob '!doc/**'
```

Expected: no output (plan acceptance line).

Tests that pin this doctrine, run once at the end by the main agent:

```bash
cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::
cargo test --manifest-path loom/Cargo.toml --test integration hooks_subagent_verify_guard
cargo test --manifest-path loom/Cargo.toml --test maintainability
cargo test --manifest-path loom/Cargo.toml --test reviewer_brief_guard_contracts
```

What each covers:

| Target | Pins |
| --- | --- |
| `orchestrator::signals::tests_doctrine` | BLOCK-A across the signal prefix, preamble and guard; absence from the IV prefix; `RETIRED_PHRASES` sweep over `CLAUDE.md.template`, skills, preamble, agents |
| `orchestrator::signals::tests_doctrine_prefixes` | IV override needles, the renamed preamble test, `**The gate is `loom check`**` only in the standard prefix |
| `orchestrator::signals::tests_doctrine_gate` | gate needles and the new `gate_block_keeps_the_full_suite_behind_loom_check` |
| `orchestrator::signals::tests_guidance_manifest` | template relocation needles (`_subagent-preamble.txt`, `--criterion <n>`, `` `loom check <stage-id>` ``) |
| `orchestrator::signals::cache` | standard-prefix assertion at the old line 381 |
| `integration hooks_subagent_verify_guard` | block message snippet, main-agent refusal still names `loom check <stage>` and `--criterion` |
| `maintainability` | `cache.rs` exact line count in `loom/maintainability-baseline.txt` |

`rustfmt --edition 2021 --check` on `blocks.rs`, `cache.rs` and the touched test files is part of the
main agent's `cargo fmt --check` gate. The longest new `push_str` line is about 97 columns, under the
100 limit; the dimension 3 line is a single long string literal like its neighbors and rustfmt leaves it.

## Sites summary (count: 20 files, 33 edits)

| # | Path | Anchor | Step |
| --- | --- | --- | --- |
| 1 | `loom-hooks/_subagent-preamble.txt` | lines 12-24 | 1a |
| 2 | `loom/src/orchestrator/signals/cache/blocks.rs` | `append_no_verify_block` 44-49, doc comment 36-37 | 1b |
| 3 | `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` | `BLOCK_A` 11-19, `RETIRED_PHRASES` | 1c |
| 4 | `loom/src/orchestrator/signals/cache.rs` | line 152 call, 115-126 override, 381 assert, baseline | 2 |
| 5 | `loom/maintainability-baseline.txt` | line 16 | 2d |
| 6 | `blocks.rs` | `append_review_dimension_details` line 116 | 3 |
| 7 | `blocks.rs` | `ORCHESTRATOR_GATE` line 73 | 4 |
| 8 | `loom/src/orchestrator/signals/tests_doctrine_gate.rs` | new test after line 55 | 4 |
| 9 | `loom/src/orchestrator/signals/tests_doctrine.rs` | `block_a_agrees_across_every_surface` 112-133 | 5a |
| 10 | `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` | lines 162, 184-192, 224-236, 249-273 | 5b |
| 11 | `skills/loom-orchestration/SKILL.md` | line 121 | 6a |
| 12 | `CLAUDE.md.template` | line 96 | 6b |
| 13 | `agents/loom-senior-software-engineer.md` | line 87 | 7a |
| 14 | `agents/loom-software-engineer.md` | line 72 | 7b |
| 15 | `loom-hooks/subagent-verify-guard.sh` | `block()` heredocs, header comments (4 edits) | 8a-8d |
| 16 | `loom/tests/integration/hooks_subagent_verify_guard.rs` | line 203 | 8e |
| 17 | `loom/tests/integration/hooks_subagent_verify_guard_main.rs` | line 4 | 8c |
| 18 | `loom-hooks/commit-filter.sh`, `loom-hooks/codex-forward.sh`, `AGENTS.md.template` | 308-309, 99-101, 54-57 | 9 |
| 19 | `README.md` (965, 967, 971, 1103), `CONTRIBUTING.md:96` | prose | 9 |
| 20 | `skills/loom-plan-writer/references/parallelization.md:5`, `codex-implementers.md:23` | prose | 9 |

`loom-hooks/spawn-guard.sh` embeds no rule text (it only reads the preamble file and matches its first
line), so it needs no edit.
