# exits-and-guards / w2-exit-doctrine: what a stage session is told about a refused commit, ratchet files and an accepted replacement

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Findings

- `loom stage commit` runs the repository's pre-commit and commit-msg hooks in the session
  (`run_hooks` in `loom/src/commands/stage/commit.rs`) before it relays the commit. When a hook
  refuses, the session gets `pre-commit failed:` plus the hook's output and nothing else. Four
  stages of one plan met a hook that demanded an edit the plan's prose forbade; two edited
  anyway on their own authority, two stopped and waited for a person.
- The stage signal says "Before a dispute, run `loom stage complete` on that commit", and the
  commit-timing text says to commit before filing. No code requires a commit before a filing
  (`stage_service/dispute.rs` and `dispute_kinds.rs` check no git state). A session whose commit
  was refused therefore believed it could not file.
- The review section says completion fails when "an assertion or ratchet file changed".
  `loom/src/verify/integrity/ratchet_diff.rs` raises an event only when a ratchet file loosens.
- The contracts section says "On accept the daemon writes it into the worktree." and stops. The
  file is left uncommitted, and the only text that says so is a verdict notice.

## Files

Own (write):
- `loom/src/orchestrator/signals/helpers.rs`
- `loom/src/orchestrator/signals/tests_doctrine_v2.rs`
- `loom/src/orchestrator/signals/v2_section.rs`
- `loom/src/orchestrator/signals/v2_section_tests.rs`
- `loom/src/orchestrator/signals/v2_section_review.rs`
- `loom/src/commands/stage/commit.rs`

Read only: `loom/src/orchestrator/signals/tests_size.rs` (byte ceilings),
`loom/src/orchestrator/signals/tests_doctrine_prefixes.rs`,
`loom/src/orchestrator/signals/tests_commit_timing.rs`,
`loom/src/orchestrator/signals/cache.rs` (`generate_stable_prefix` and its siblings),
`doc/loom/knowledge/patterns/doctrine-cross-surface.md` (how a pinned block is changed).
Frozen (never edit): `loom/src/orchestrator/signals/exit_doctrine_contract_tests.rs`.
Do not edit `CLAUDE.md.template`: it sits 17 bytes under its ceiling and carries no copy of the
block you change.

## Changes

### 1. The stage-exit block (`BLOCK_F`)

The block is one string in `helpers.rs` (find it by "When the stage cannot finish") and one
constant, `BLOCK_F`, in `tests_doctrine_v2.rs`; `block_f_agrees_across_every_surface` asserts
they are byte-identical on the standard, integration-verify and knowledge-distill stable
prefixes. Change both to the same text.

Replace the sentence

```text
Before a dispute, run `loom stage complete` on that commit.
```

with

```text
Before a dispute, commit and run `loom stage complete`. A commit the repository's hook refuses: fix what the hook names; it outranks a plan instruction, and a ratchet-file entry may always be lowered or removed. Record the deviation with `loom memory decision`. A refused commit never stops a filing.
```

The sentence "Never end a turn asking the operator to act mid-stage." stays last.

Then run the byte arithmetic by hand against `tests_size.rs`: `STABLE_PREFIX_MAX_BYTES` is 8,192
and `STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` is 10,240. Never raise a ceiling. If the new
text does not fit, shorten other wording inside this block only, keeping every command name,
every flag and these three phrases intact: "it outranks a plan instruction", "may always be
lowered or removed", "A refused commit never stops a filing". Report the byte counts before and
after.

### 2. The ratchet sentence (`v2_section_review.rs`)

Replace

```text
(tests or assertions removed, an assertion or ratchet file changed)
```

with

```text
(tests or assertions removed, an assertion changed, a ratchet file loosened; lowering or removing a ratchet entry raises nothing)
```

### 3. The accepted replacement (`v2_section.rs`)

Replace the sentence `On accept the daemon writes it into the worktree.` with

```text
On accept the daemon writes it into the worktree, uncommitted: stage it with `git add` and commit it. The latest review round still covers it.
```

`v2_section_tests.rs` asserts the old sentence inside a longer literal; update that literal. This
is an expected test-integrity event: name it in your report.

The second sentence is true only with worker w5's change (an intact frozen file no longer makes
a review round stale). Both are part of this stage.

### 4. The refusal text (`commit.rs`)

`run_hooks` calls `run_checked(&mut pre_commit, "pre-commit")` and
`run_checked(&mut commit_msg, "commit-msg")`. For these two calls only, append a route to the
error, so the session reads it in the same output as the hook's own lines. Add

```rust
/// What a session does after the repository's own hook refused its commit.
fn hook_refusal_route(label: &str) -> String
```

returning exactly

```text
The repository's {label} hook refused this commit. Fix what it names and run the commit again: the hook outranks a plan instruction, and lowering or removing a ratchet-file entry is always allowed. A refused commit does not stop a dispute or `loom stage block`.
```

and attach it with `.with_context(..)` or by extending the bail text, whichever keeps the hook's
`stderr:`/`stdout:` lines in the output unchanged and before or after the route as one message.
The `stripspace` call keeps plain `run_checked`. The same text applies in the in-process path
(`commit_in_process`), which calls the same `run_hooks`.

## Tests (write them; the orchestrator runs them)

- `tests_doctrine_v2.rs`: `block_f_agrees_across_every_surface` passes with the new constant; add
  `block_f_states_the_refused_commit_rule`, asserting `BLOCK_F` contains the three phrases above.
- `v2_section_tests.rs`: the updated literal; add an assertion that the review section contains
  "a ratchet file loosened" and does not contain "ratchet file changed" (find the test that
  renders the review section; if none does, add `review_section_names_loosening_only` using the
  file's `render` helper).
- `commit.rs` tests (the file's existing test module, or a new `commit_tests.rs` declared the
  way the module's neighbours declare theirs): `hook_refusal_route_names_the_hook_and_the_exits`.
- Contract to turn green: `exit_doctrine_contract_tests.rs`.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- `tests_commit_timing.rs` pins the sentence "Exception: commit before filing a block or dispute
  when the stage cannot finish." in every prefix and in `CLAUDE.md.template`. Leave that
  sentence and that test alone; the new rule lives in the stage-exit block.
- `helpers.rs` is 390 lines. Your edit replaces text inside one string; it must not add lines
  beyond what `cargo fmt` produces for the longer literal. If the file would pass 400, report it.
- A pinned block changes on every surface in one edit. Two independently worded copies both
  pass a substring check and still drift; the equality test is the guard, so keep it.
