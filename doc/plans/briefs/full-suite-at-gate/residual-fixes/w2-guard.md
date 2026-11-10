# W2: the guard fails open on its own breakage (R2.1, R2.4, BLOCK-A in the guard)

Agent: `loom-software-engineer`. Read `doc/plans/briefs/full-suite-at-gate/residual-fixes/common.md` first.

Your specification is in the plan's `### R2. Guard and doctrine text` section:

- R2.1, in full;
- R2.2, only the *BLOCK-A, the subagent rule* part, and only the copy in `loom-hooks/subagent-verify-guard.sh`;
- R2.4.

W3 does the rest of R2.2 and all of R2.3 at the same time. Never touch their files.

## Files you own

- `loom-hooks/subagent-verify-guard.sh`, the whole file: R2.1's two code edits and header edit, plus BLOCK-A's three lines in `block()`'s heredoc
- `loom-hooks/tests/require-jq-blocking.sh` and `loom-hooks/tests/require-jq-advisory.sh` (R2.1's test move)
- `loom/tests/integration/hooks_subagent_verify_guard.rs` (R2.1's new test, R2.4's comment)
- `loom/tests/integration/hooks_subagent_verify_guard_helper.rs`, new, only if the new test would push `hooks_subagent_verify_guard.rs` past 400 lines (it is 376 now)

## What is already there

- **The `jq` contract is frozen.** Loom's contract session wrote `guard-warns-without-jq` in `loom/tests/integration/hooks_guard_fail_open.rs`. It runs the guard with no `jq` on `PATH` and expects exit 1. It fails until you replace `loom_require_jq "subagent-verify-guard.sh"` with R2.1's `loom_warn_no_jq` lines. Never edit that file.
- **The guard's order matters.** `loom_require_jq` (line 54) runs before the role check (`loom_is_subagent`, line 86). The `_verify_matchers.sh` source (line 207) runs after it. So the missing-matchers test has to run the hook as a live subagent: that is why it uses `run_hook_in_tree` and returns early through `skip_unless_gate_visible`, like the other process-tree tests in the file.
- **The submodule pattern.** If you need `hooks_subagent_verify_guard_helper.rs`, declare it at the end of `hooks_subagent_verify_guard.rs` the way the file already declares its others:

  ```rust
  #[path = "hooks_subagent_verify_guard_helper.rs"]
  mod helper;
  ```

  The submodule then reaches the parent's private helpers (`install_hook`, `run_hook_in_tree`, `skip_unless_gate_visible`, the temp-dir helper) through `use super::*;`. Read `hooks_subagent_verify_guard_main.rs` for a working example.

## Steps

1. R2.1's guard edits, in this order: the `loom_require_jq` line becomes the two `loom_warn_no_jq` lines; the two header lines `# Exit codes: ...` and `# installed - fail closed)` become R2.1's two replacement lines; the `_verify_matchers.sh` `source` becomes the `|| { ... }` block. The block's body is indented with one tab, like the rest of the file.
2. BLOCK-A in `block()`'s heredoc: replace the three lines quoted in R2.2 ("Compile the files YOU wrote ...") with R2.2's three replacement lines, byte for byte, keeping the heredoc's existing leading characters on each line. W3 makes the same change in the other three copies, and `block_a_agrees_across_every_surface` compares all four.
3. Move the guard's case from `require-jq-blocking.sh` to `require-jq-advisory.sh`, as R2.1 describes: everything from the comment `# subagent-verify-guard.sh blocks too` through the last `GUARD_OUTPUT` check moves to just before the advisory file's final `echo "PASS"`. In the moved block, change `-ne 2` to `-ne 1` and its failure message's "exit 2" to "exit 1". Keep the control case, the `jq is not installed` check and the empty-stdout check. Rewrite its comment to say the guard only deters, so without `jq` it warns and lets the call through. The advisory file already defines `TMP`, `NOJQ_PATH` and the `trap`, so the moved block reuses them. Update the header comment of each file so it names the hooks it now covers. Leave `run-all.sh` alone.
4. The new Rust test `a_missing_matchers_helper_lets_the_call_through_with_a_warning`, as R2.1 describes, modelled on `check_subagent_case`. It asserts exit code 1, never 2, and stderr containing `loom install-assets`.
5. R2.4's module-doc replacement in `hooks_subagent_verify_guard.rs`.
6. `rustfmt --edition 2021` on the Rust files you changed.

## Your checks

From the repo root:

```bash
bash -n loom-hooks/subagent-verify-guard.sh; echo "exit=$?"
shellcheck -x -P SCRIPTDIR loom-hooks/subagent-verify-guard.sh loom-hooks/_verify_matchers.sh; echo "exit=$?"
./scripts/check-hook-syntax.sh; echo "exit=$?"
bash loom-hooks/tests/require-jq-blocking.sh; echo "exit=$?"
bash loom-hooks/tests/require-jq-advisory.sh; echo "exit=$?"
bash loom-hooks/tests/subagent-gate-payload.sh; echo "exit=$?"
```

Then, from `loom/`, with `NC_TMP` as in `common.md`:

```bash
LOG="${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}"
TMPDIR="$NC_TMP" cargo test --test integration hooks_ > "$LOG/w2-int.log" 2>&1; echo "exit=$?"
cargo test --test gate_dedup_contracts > "$LOG/w2-dedup.log" 2>&1; echo "exit=$?"
cargo test --test hook_false_positive_contracts > "$LOG/w2-fp.log" 2>&1; echo "exit=$?"
rg -n -m 20 -M 300 'FAILED|panicked|^error|SKIP|test result' "$LOG"/w2-*.log
wc -l tests/integration/hooks_subagent_verify_guard*.rs
```

Each prints `exit=0`; both `require-jq` scripts print `PASS`. The `hooks_` filter runs the frozen `jq` contract and your new test. Report any `SKIP` line. Every file is at or under 400 lines. The signals tests that compare the BLOCK-A copies are not yours to run: the orchestrator runs them once W3 is done too.
