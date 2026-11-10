# W3: language-neutral doctrine text (R2.2, R2.3) and the field-check script (R3)

Agent: `loom-software-developer`. Read `doc/plans/briefs/full-suite-at-gate/residual-fixes/common.md` first.

You have two separate jobs. Finish part 1, run its checks, then do part 2.

## Files you own

- `loom-hooks/_subagent-preamble.txt` (BLOCK-A's three lines only)
- `loom/src/orchestrator/signals/cache.rs` (`INTEGRATION_VERIFY_OVERRIDE` only)
- `loom/src/orchestrator/signals/cache/blocks.rs` (fns `append_no_verify_block` and `append_review_dimension_details` only)
- `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` (const `BLOCK_A` only)
- `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` (the assertions named below)
- `scripts/full-runs-field-check.py` (new)

Never edit `loom-hooks/subagent-verify-guard.sh`: W2 owns it, BLOCK-A copy included.

## Part 1: the doctrine text

Your specification is the plan's R2.2 (both parts) and R2.3, in `### R2. Guard and doctrine text`. Every change is a replacement of quoted text with quoted text.

1. **`INTEGRATION_VERIFY_OVERRIDE`** in `cache.rs`. The constant is a Rust string built from many lines joined by `\` continuations. Make the two text changes R2.2 lists inside the existing lines. A line may get longer; never add or remove a line. `wc -l loom/src/orchestrator/signals/cache.rs` prints 511 before and after.
   - The parenthesis after `it carries the COMPLETE suite` is replaced.
   - The sentence `On one tree the first request runs; covered requests join it; later requests hit the pass cache.` is split across two source lines. Replace its words across those same two lines and keep the `\` continuations where they are. Watch the spaces at each line break: a `\` continuation in Rust drops the newline and the leading whitespace of the next line, so the space between two words must stay before the `\`.
2. **BLOCK-A** in three copies: the three `content.push_str` lines in `append_no_verify_block` (`blocks.rs`), lines 12-14 of `_subagent-preamble.txt`, and lines 13-15 of `BLOCK_A` (`tests_doctrine_blocks.rs`). Replace R2.2's three quoted lines with its three replacement lines. Each copy keeps its line count and its own wrapping: in `blocks.rs`, each line sits inside a `content.push_str("...\n");` call, so keep that call shape and change only the text inside it. Compare the three copies afterwards: the visible text must be identical.
3. **R2.3** in `append_review_dimension_details` (`blocks.rs`): one string literal replaced.
4. **`tests_doctrine_prefixes.rs`**, three changes:
   - in `integration_verify_prefix_assigns_one_canonical_gate_owner`, replace `assert!(prefix.contains("cargo clippy -- -D warnings"));` with the two assertions R2.2 gives;
   - in the same function's needle list (the one ending with `"stale evidence never waives a failing gate"`), add `"its verdict can list criteria it did not select",`;
   - in the needle list whose failure message is "IV prefix lost review guard", add `"**Functional** — spawn this one as `loom-software-engineer`",` after the Build/test/sandbox needle. That em dash is U+2014, as in the source string.
5. `rustfmt --edition 2021` on the Rust files you changed.

Check, from `loom/`:

```bash
LOG="${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}"
cargo test --lib orchestrator::signals:: -- --skip block_a_agrees_across_every_surface > "$LOG/w3-signals.log" 2>&1; echo "exit=$?"
rg -n -m 20 -M 300 'FAILED|panicked|^error|test result' "$LOG/w3-signals.log"
wc -l src/orchestrator/signals/cache.rs
cd .. && rg -n 'rustfmt --edition 2021 <files>|cargo clippy -- -D warnings`, `cargo fmt' --glob '!doc/**' --glob '!target' --glob '!loom/target' --glob '!loom/tests/fixtures/**' --glob '!loom-hooks/subagent-verify-guard.sh'; echo "rg exit=$?"
```

`exit=0`, zero failed, `511`, and `rg exit=1`: the old text is gone everywhere except the guard, which W2 changes. `block_a_agrees_across_every_surface` is skipped because it also reads W2's copy. If the `rg` finds another live copy, report it.

## Part 2: the field-check script

Your specification is the plan's `### R3. Field-check script`.

1. Copy `doc/plans/briefs/full-suite-at-gate/full_runs.py` to `scripts/full-runs-field-check.py`. Never edit the original: it is the record.
2. Add a module docstring line saying the script is temporary and is deleted after the field check in the plan's Acceptance item 4.
3. Replace the `sys.argv` handling with `argparse`: positional `days` (float, default 14), `--since YYYY-MM-DD` (when given, the cutoff is that date at 00:00 UTC, through `datetime.datetime.strptime(..., "%Y-%m-%d").replace(tzinfo=datetime.timezone.utc).timestamp()`), and `--root DIR` (default `Path.home() / ".claude/projects"`).
4. Keep `RUNNERS`, `FULL` and `SCOPED_FLAGS` exactly as they are.
5. Kind per project directory: `iv` when its name contains `integration-verify`, `knowledge` when it contains `knowledge`, else `standard`.
6. Outcome per hit. `scan` reads each JSONL file once. Collect the Bash `tool_use` blocks that hit, keyed by their `id`. From entries with `type == "user"`, collect each `message.content[]` item with `type == "tool_result"` by its `tool_use_id`. Its `content` is a string, or a list of `{"type": "text", "text": ...}` items: join their texts. A hit whose result text contains `[subagent-verify-guard/` is `blocked`, else `ran` (a hit with no result is `ran`). `scan` returns `(command, outcome)` pairs.
7. Output: one line per `(repo, kind, role, outcome)` key with its count, where `role` is `main` or `sub`, sorted by key. Then the examples, as the old script prints them, each prefixed with its kind and outcome.
8. If `--root` does not exist or is empty, print the empty counts and exit 0.

Check, from the repo root (your session can read `~/.claude/projects`; the stage's own acceptance cannot):

```bash
python3 -I scripts/full-runs-field-check.py 30 > "$LOG/w3-new.txt"; echo "exit=$?"
python3 -I doc/plans/briefs/full-suite-at-gate/full_runs.py 30 > "$LOG/w3-old.txt"; echo "exit=$?"
python3 -I scripts/full-runs-field-check.py 1 --since 2026-11-09; echo "exit=$?"
d=$(mktemp -d "${TMPDIR:-/tmp}/fieldcheck.XXXXXX") && python3 -I scripts/full-runs-field-check.py 30 --root "$d"; echo "exit=$?"
```

Every run exits 0. The new script's counts summed over every key equal the old script's `sub` plus `main` totals (the first line of the old output). Report both totals.
