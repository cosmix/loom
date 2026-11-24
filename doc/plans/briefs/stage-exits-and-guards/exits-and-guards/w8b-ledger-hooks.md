# exits-and-guards / w8b-ledger-hooks: this repository's commit hook lowers the size ledger and stages it

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

This unit changes this repository's own git hooks. It and w8-ledger-tighten are the two units
of the plan that are specific to this repository.

## Finding

`loom/.githooks/pre-commit` runs `cargo test --quiet --test maintainability`, which fails on a
stale or shrunk row of `loom/maintainability-baseline.txt` as well as on growth. A change that
improves ledgered code is refused until someone hand-edits the ledger; in four stages of one
plan that was the step that stopped the stage. The hook's error text then reads "a ledgered
file or function grew", which is wrong for a stale or shrunk row.

## Interface (pinned; another worker implements the other side)

Worker w8-ledger-tighten gives the test a tighten mode: with the environment variable
`LOOM_LEDGER_TIGHTEN=1`, `cargo test --test maintainability` first rewrites the ledger in place
(stale rows removed, shrunk rows lowered, nothing added or raised) and then validates it;
growth and new violations still fail. Without the variable the test writes nothing.

## Files

Own (write):
- `loom/.githooks/pre-commit`
- `loom/.githooks/pre-push`
- `scripts/test-pre-commit-partial-staging.sh`
- `CONTRIBUTING.md`

Frozen (never edit): `loom/tests/ledger_hook_contracts.rs`.

## Changes

The ledger path is `loom/maintainability-baseline.txt`, relative to the repository root.

### 1. `loom/.githooks/pre-commit`

- Before the hook formats anything, and after the partial-staging guard has passed, record
  whether the ledger has unstaged changes (`git diff --quiet -- ":(literal)$LEDGER"`; exit 1
  means it has) and its content hash (`git hash-object -- "$LEDGER"`, empty when the file is
  absent).
- Replace the maintainability block's command with
  `LOOM_LEDGER_TIGHTEN=1 cargo test --quiet --test maintainability 2>&1`. The argv is unchanged
  on purpose: `scripts/test-pre-commit-partial-staging.sh` matches the fake cargo's log line
  `cargo test --quiet --test maintainability`.
- When the test fails, print:

  ```text
  ERROR: maintainability gate failed - a ledgered file or function grew,
  or a new file/function exceeds the limits (see the lines above).
  Refactor (extract into a new module); never raise or add a row in
  loom/maintainability-baseline.txt. Stale and shrunk rows were lowered for you.
  ```

  and exit 1.
- When the test passes and the ledger's hash changed: if the ledger had unstaged changes before
  the hook ran, print `ERROR: loom/maintainability-baseline.txt has unstaged edits and rows that
  had to be lowered. Stage or discard your edits, then commit again.` to stderr and exit 1;
  otherwise `git add -- ":(literal)$LEDGER"` and print `Staged the lowered ledger rows in
  loom/maintainability-baseline.txt.`.
- Rewrite the block's header comment to state both behaviours.

### 2. `loom/.githooks/pre-push`

The command is unchanged (strict, no variable). Replace its failure text with one that names
the cases: growth or a new violation is refactored, never recorded; a stale or shrunk row is
fixed by running `LOOM_LEDGER_TIGHTEN=1 cargo test --test maintainability` from `loom/` (the
pre-commit hook does this) and committing the ledger.

### 3. `scripts/test-pre-commit-partial-staging.sh`

The script's fake `cargo` logs its argv. Extend the fake so that, when `LOOM_LEDGER_TIGHTEN` is
`1` and a marker file in the fixture asks for it, it removes one row from the fixture's ledger.
Add two cases in the script's existing style:

- a fully staged source change plus a ledger the fake cargo lowers: the hook exits 0 and the
  ledger is staged;
- the same with an unstaged edit already in the ledger: the hook exits 1, and the index and the
  working tree are unchanged apart from the fake cargo's own rewrite.

The six existing cases keep passing unchanged.

### 4. `CONTRIBUTING.md`

Where it describes the maintainability gate, add that the pre-commit hook lowers stale and
shrunk rows and stages the ledger, that growth is never recordable, and the manual command.

## Tests

The two script cases above. Contracts to turn green: the two tests in
`loom/tests/ledger_hook_contracts.rs`, which run this hook against a fake `cargo`.

## Proof

`bash -n loom/.githooks/pre-commit`, `bash -n loom/.githooks/pre-push`,
`bash -n scripts/test-pre-commit-partial-staging.sh`.

## Traps

- `loom stage commit` runs this hook inside the stage session, and loom's daemon commits the
  staged tree afterwards: a ledger the hook lowered but did not stage is left out of the commit,
  and the next commit fails on it.
- The partial-staging guard exists because the hook's re-stage loop once swept unstaged edits
  into a commit. The new `git add` must not reintroduce that for the ledger; the unstaged-edits
  refusal is the guard.
- The hook runs under Bash 3.2 and from any directory inside the repository; it already `cd`s
  to `$REPO_ROOT`. Use the `:(literal)` pathspec form the hook uses elsewhere, and keep the
  `unset GIT_LITERAL_PATHSPECS ...` line first.
- A repository state with no ledger file (a fixture) must not fail the hook on the hash or the
  diff commands.
