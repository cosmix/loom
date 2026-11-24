# exits-and-guards / w8-ledger-tighten: this repository's size-ledger test can lower its own ledger

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

This unit changes this repository's own tooling (its size-ledger test). It and w8b-ledger-hooks
are the two units of the plan that are specific to this repository.

## Finding

`loom/tests/maintainability.rs` (with `loom/tests/maintainability/baseline.rs`) fails in four
cases: a ledgered unit grew, a new unit exceeds a limit, a ledger row is stale ("no longer
violates its limit; remove it"), a ledgered unit shrank ("lower the entry to ..."). The last two
are bookkeeping. The pre-commit hook runs this test, so every change that improves ledgered code
is refused until someone hand-edits `loom/maintainability-baseline.txt`. In four stages of one
plan that hand-edit was the step that stopped the stage.

## Decision

The test gains a tighten mode, on only when the environment variable `LOOM_LEDGER_TIGHTEN` is
exactly `1`: before validating, it rewrites the ledger so that stale rows are removed and shrunk
rows are lowered. It never adds a row and never raises a value. Growth and new violations still
fail. Worker w8b-ledger-hooks makes the pre-commit hook run the test in this mode; the variable
name and the value are the interface between you.

Lowering or removing a row is a tightening: loom's ratchet rule
(`loom/src/verify/integrity/ratchet_diff.rs`) raises nothing for it.

## Files

Own (write):
- `loom/tests/maintainability.rs`
- `loom/tests/maintainability/baseline.rs`
- `loom/tests/maintainability/tighten.rs` (new)

Read only: `loom/tests/maintainability/scanner.rs`, `loom/maintainability-baseline.txt` (its
layout: a comment header, `file` rows, a blank line, `function` rows).

## Changes

### 1. `tighten` (`loom/tests/maintainability/tighten.rs`)

Declared in `maintainability.rs` with `#[path = "maintainability/tighten.rs"] mod tighten;`, as
its siblings are.

```rust
/// `source` with every stale row removed and every shrunk row lowered to its
/// measured size; every other line, comments and blank lines included, is
/// kept byte for byte. Never adds a row and never raises a value. Returns the
/// new text and how many rows changed.
pub fn tighten(source: &str, violations: &[Violation]) -> (String, usize)
```

- Work line by line. A line that `baseline::parse_line` reads as a row (make that function
  `pub(super)` or `pub`; it is private today) is looked up in `violations` by its
  `BaselineKey`: no violation with that key drops the line; a violation measured below the
  recorded value replaces the line with `baseline::render_entry(&key, measured)`; anything else
  (equal, or grown) keeps the line unchanged.
- A line that does not parse is kept unchanged: `baseline::parse` reports it afterwards.
- Keep the trailing newline of the original text.

### 2. The mode (`loom/tests/maintainability.rs`)

In `repository_maintainability_debt_does_not_grow_or_go_stale`, after the scan and the read:
when `std::env::var("LOOM_LEDGER_TIGHTEN").as_deref() == Ok("1")`, compute
`current_violations`, call `tighten`, and when the row count is non-zero write the new text to
the ledger path and print `maintainability: lowered or removed {n} ledger row(s) in {path}` to
stderr. Then parse and validate the text now on disk, exactly as today. With the variable unset
or set to anything else, nothing is written. The test function must stay under 50 lines: move
the mode into a helper.

## Tests (write them; the orchestrator runs them)

In `tighten.rs` (a `#[cfg(test)] mod tests`), with hand-built `Violation` values the way
`baseline.rs`'s tests build theirs:

- a stale `file` row and a stale `function` row are removed, and the header comment and the
  blank separator line survive;
- a shrunk row is lowered to the measured value and counted;
- a grown row and an exact row are byte-identical afterwards;
- a violation with no row adds nothing;
- text with nothing to change is returned unchanged with a count of 0.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- A rewrite that regenerates the ledger from the scan records new violations and grown units.
  The mode edits existing rows only.
- `loom/tests/` is scanned by the same test: `tighten.rs` and every function in it obey the
  400 and 50 line limits. `baseline.rs` is 313 lines; do not grow it past 360.
- The mode writes a tracked file of the checkout the test runs in. It must do so only under the
  exact variable value, so that CI and the pre-push hook, which do not set it, stay read-only.
