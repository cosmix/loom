# exits-and-guards / w6-negated-search-lint: plan validation warns on a criterion that negates a search over a command's output

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Finding

A plan carried, on eight stages, the acceptance criterion

```text
! cargo test --test maintainability 2>&1 | rg -q "grew from|new violation at|scan failed|invalid maintainability baseline|could not compile"
```

Criteria run under `sh -c` without `pipefail`, so the pipeline's status is `rg`'s, and `!`
inverts it: the criterion passes whenever `cargo` dies with any message the pattern does not
list (a missing test target, a link error, a toolchain fault), and it writes an empty log.
`loom plan verify --strict` accepted it: `masks_exit_status` in
`loom/src/plan/schema/validation/criterion_hazards/masked_exit.rs` flags only a forced-success
ending.

## Decision

A new hazard, `Hazard::NegatedSearch`, a WARNING (so `--strict` fails on it and `loom init`
does not). It flags the last top-level statement of a criterion when it negates a search over
another command's output:

- Shape A: the statement is `! <pipeline>`, the pipeline has at least one top-level `|` or
  `|&`, and the command of its last segment is a matcher.
- Shape B: the statement is `test $? -eq 1`, `test $? -ne 0`, `[ $? -eq 1 ]` or `[ $? -ne 0 ]`,
  and the statement before it is a pipeline with at least one top-level `|` or `|&` whose last
  segment's command is a matcher.

A matcher is a command whose name (after `Word::command_name()`) is `rg`, `grep`, `egrep`,
`fgrep` or `ag`. Where the statement holds `&&` or `||`, the part after the last top-level
`&&`/`||` is the one judged.

Not flagged, and each needs a test that says so: a pipeline into a matcher that is not negated
(`cargo test | rg -q "test result: ok"`: a dead command prints nothing and the criterion
fails); a negated matcher that reads files and has no pipe (`! rg -q TODO src/lib.rs`), which
many plans use for an absence check and which is out of scope here; a `|` inside quotes
(`! rg -q "a|b" file`).

Message (`Hazard::message`), exactly:

```text
negates a search over a command's output (`! <command> | rg ...`), so it passes whenever that command dies without printing a matching line; assert on the command's own exit status, or have it write a file and search the file
```

## Files

Own (write):
- `loom/src/plan/schema/validation/criterion_hazards.rs`
- `loom/src/plan/schema/validation/criterion_hazards/masked_exit.rs`
- `loom/src/plan/schema/validation/criterion_hazards/negated_search.rs` (new)
- `loom/src/plan/schema/tests/negated_search_tests.rs` (new)
- `loom/src/plan/schema/tests/mod.rs`

Read only: `loom/src/plan/schema/validation/shell_lex.rs` (`Token`, `Word`, `lex`,
`simple_commands`: it splits at every control token and forgets which one, so it cannot tell a
pipe from `&&`; walk the tokens yourself), `loom/src/plan/schema/validation/search_args.rs`,
`loom/src/plan/schema/tests/criterion_hazard_tests.rs` (case style: `assert_flags`,
`assert_clean`, and the two preflight tests), `loom/src/plan/schema/validation.rs` (the callers
`push_hazard_errors` and `push_hazard_warnings`; do not edit: the file has an exact ledger row).
Frozen (never edit): `loom/src/plan/schema/tests/negated_search_contract_tests.rs`.

## Changes

1. `negated_search.rs`: `pub(super) fn negates_search(tokens: &[Token]) -> bool` implementing
   shapes A and B over the token stream. Reuse the statement splitter of `masked_exit.rs`
   (`statements`, `strip_parens`): raise its visibility to `pub(super)` instead of copying it.
   `!` is an ordinary `Word` in this lexer (it is listed in `KEYWORDS` in
   `criterion_hazards.rs`): shape A holds when it is the first word of the judged part.
2. `criterion_hazards.rs`: the variant `NegatedSearch` in `Hazard`; its message; `is_error`
   stays false for it; in `scan_into`, beside the `masks_exit_status` call and under the same
   `tail` condition, insert the hazard when `negates_search(&tokens)`. The file is 363 lines:
   keep the addition under ten lines.
3. No change to which command kinds are scanned: the hazard applies wherever `MaskedExit` does
   (acceptance, setup, wiring tests).

## Tests (write them; the orchestrator runs them)

`negated_search_tests.rs`, registered in `tests/mod.rs`, using `scan` and `Hazard` as
`criterion_hazard_tests.rs` imports them:

- Flags: the finding's criterion verbatim; `cd loom && ! cargo test 2>&1 | grep -q FAILED`;
  `! make lint |& rg -q warning`; `cargo test 2>&1 | rg -q "grew from"; test $? -eq 1`;
  `cargo build 2>&1 | grep -c error; [ $? -ne 0 ]`.
- Clean: `cargo test | rg -q "test result: ok"`; `! rg -q TODO src/lib.rs`;
  `rg -q -F x file; test $? -eq 1`; `! rg -q "a|b" file`; `! cargo test`.
- `MaskedExit` is unaffected: `cargo test || true` still flags it and only it.
- Preflight: a stage whose acceptance is the finding's criterion yields a warning containing
  "negates a search" from `validate_structural_preflight`, and `validate` reports no error for
  it (follow `preflight_reports_hazard_warnings_for_every_command_kind` and
  `preflight_keeps_hazard_errors_out_of_warnings`).

Contracts to turn green: the two tests in `negated_search_contract_tests.rs`.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- Flagging every pipeline that ends in a matcher would reject the fail-closed form
  `<command> | rg -q <expected>`, which is the common way to assert on a command's output.
- Looking only at the first word of the whole command misses `cd dir && ! cmd | rg -q x`.
- A pending plan that carries the finding's criterion will start to warn under `--strict`. That
  is intended; do not add an exemption.
