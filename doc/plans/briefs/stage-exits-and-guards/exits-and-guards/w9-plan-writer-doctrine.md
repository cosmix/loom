# exits-and-guards / w9-plan-writer-doctrine: the plan-writer skill states where commit hooks run, how ratchet files are owned and the new lint

Tier: codex terra (`gpt-6.1-terra`, effort high)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

Prose edits only, in four markdown files. Every sentence to add is given below; place each one
where the brief says and change nothing else.

## Finding

A plan told eight stages "nobody in this stage edits maintainability-baseline.txt" and gave
them a criterion that tolerated stale ledger rows, while the repository's pre-commit hook
refused a commit with a stale row. The plan-writer doctrine never says that a repository's
commit hook is a gate every stage must pass, says the hooks run on the daemon's side (they run
in the stage session: `run_hooks` in `loom/src/commands/stage/commit.rs` runs before the commit
is relayed, and the daemon's own commit uses `core.hooksPath=/dev/null`), and does not warn
against a criterion that negates a search over a command's output.

## Files

Own (write):
- `skills/loom-plan-writer/SKILL.md`
- `skills/loom-plan-writer/references/v2-contracts.md`
- `skills/loom-plan-writer/references/verification-rules.md`
- `skills/loom-plan-writer/references/sandbox.md`

## Changes

### 1. `SKILL.md`, Section 6, the "Warnings:" bullet

Append to the list in that bullet, before its final period:

```text
; a negated search over a command's output (`! <command> | rg ...`, or the same pipeline followed by `test $? -eq 1`), which passes whenever the command dies without printing a matching line
```

### 2. `SKILL.md`, Section 6, "Three rules no check enforces"

Rename the lead-in to `**Four rules no check enforces:**` and add as the first bullet:

```text
- **A commit hook is a gate every stage must pass.** `loom stage commit` runs the repository's pre-commit and commit-msg hooks before anything is committed. Read them before writing a stage. Put each check a hook runs into the stage's `acceptance` with its plain exit status, so it fails at `loom check` and not at the commit. Never tell a stage to leave for a later stage a change the hook demands (a lowered ledger row, a reformatted file): a stage that cannot commit cannot finish.
```

### 3. `SKILL.md`, Section 8, the environment inventory bullet that starts "The repository's git hooks:"

Replace the whole bullet with:

```text
- The repository's git hooks: `loom stage commit` runs the pre-commit and commit-msg hooks in the stage session, inside its sandbox, and the daemon then writes the commit without hooks. Every tool a hook calls must exist in the session, and a registry a hook fetches from needs its domain in the stage's `allowed_domains`; `loom plan verify` errors when it is missing.
```

### 4. `references/sandbox.md`, the section "## Commit hooks"

Replace the section's body (the paragraph under the heading) with:

```text
`loom stage commit` runs the repository's pre-commit and commit-msg hooks in the stage session,
inside the session's sandbox, before the commit is relayed; the daemon then writes the commit
with hooks disabled. A hook therefore sees the session's tools, the session's network grants and
the session's write limits. `loom plan verify` errors when a stage's sandbox omits a registry
that the repository's pre-commit hook fetches from. A hook that rewrites files (a formatter, a
ledger it lowers) must stage what it rewrote: the daemon commits the staged tree.
```

### 5. `references/v2-contracts.md`, the section "### `ratchet_files`"

Append one paragraph at the end of that section:

```text
A stage that shrinks code a ledger records lowers or removes its own rows in the same commit; no stage owns a ledger on behalf of the others, because the repository's commit hook may refuse a commit with a stale row. Never write "do not edit" for a ratchet file in a stage description: tightening is always allowed, and the event above already stops a loosening.
```

### 6. `references/verification-rules.md`, under "**3. jq and shell hygiene inside `acceptance`**"

Add one bullet after the bullet that starts "**One assertion per criterion.**":

```text
- **Never negate a search over a command's output.** `! <command> | rg -q <failure text>` passes whenever the command dies with any other message, and `loom plan verify` warns on it. Run the command for its own exit status; when only some failures count, have the tool expose a mode for that and test the mode's exit status.
```

## Constraints

- `loom/src/orchestrator/signals/tests_doctrine.rs` and `tests_doctrine_v2.rs` read
  `skills/loom-plan-writer/SKILL.md` and assert these substrings; each must still be present,
  unchanged: the Rule 7 link `../loom-orchestration/SKILL.md#rule-7--model-allocation`, the
  phrase `lets the daemon complete the stage without a session`, `loom project detect`, and
  `references/v2-contracts.md`.
- Keep every existing list item and sentence you do not replace. Keep line wrapping as it is in
  each file: `SKILL.md` and `v2-contracts.md` use one line per paragraph or bullet; `sandbox.md`
  wraps near 100 columns.
- Every fenced code block in these files names a language. You add none.
- Edit the files under `skills/` in the repository only. Installed copies elsewhere are not
  yours.

## Proof

`rg -n -F 'A commit hook is a gate every stage must pass' skills/loom-plan-writer/SKILL.md`
prints one line; `rg -c -F 'commits relay to the daemon host path' skills/loom-plan-writer`
prints nothing. You run no other command.

## Traps

- Section numbers are the skill's own ("## 6. Verification Fields", "## 8. Sandbox & Execution
  Environment"). Find each place by its quoted lead-in text, never by line number.
- The lint's message is owned by another worker; quote nothing from it here beyond what this
  brief gives.
