# exits-and-guards / w7-main-checkout-guard: a stage session's Bash commands cannot reach the main checkout by absolute path

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Finding

In a stage worktree session, `cd <main checkout>/loom && rg -n ... src; git diff -- src/...` ran
twice, unblocked. The session then decided an edit from the main checkout's sources, which
differ from its worktree's. The same session's `../.loom/work/...` path was blocked as
`[worktree-isolation/path-traversal]`.

Cause: `loom-hooks/worktree-isolation.sh` checks three things: git overrides, tokens that
contain `..` (`check_path_traversal_resolved`), and tokens that name another stage's worktree
(`check_cross_worktree_tokens`). An absolute path with no `..` and no `.worktrees/` component
is never looked at, and `_loom_cwd_segment` in `loom-hooks/_containment.sh` tracks a
`cd <absolute>` as the new working directory without classing it. The file tools are already
confined (`worktree-file-guard.sh` blocks Read, Glob and Grep outside the worktree); only Bash
is open. The OS sandbox does not deny reads of the main checkout either, so this hook is the
only guard.

## Decision

Block a Bash command that names, by absolute path, a location inside the main repository's
checkout that is neither the session's worktree, nor loom's state, nor scratch. System paths,
the home directory's tool installs and everything else outside the repository stay untouched.

## Files

Own (write):
- `loom-hooks/worktree-isolation.sh`
- `loom-hooks/_containment.sh`
- `loom-hooks/tests/worktree-isolation-main-checkout.sh` (new)
- `loom-hooks/tests/containment-cwd-resolve.sh`
- `loom-hooks/tests/run-all.sh`

Read only: `loom-hooks/_worktree_policy.sh`, `loom-hooks/worktree-policy.json`,
`loom-hooks/_git-command.sh` (other callers of `loom_path_class`; they test for `worktree`,
`scratch` and nothing else), `loom-hooks/tests/worktree-isolation-traversal-resolve.sh` (the
fixture and the `allow`/`block`/`warn` helpers to copy),
`loom/tests/hook_false_positive_contracts.rs`.
Frozen (never edit): `loom/tests/hook_main_checkout_contracts.rs`.

## Changes

### 1. `loom_path_in_main_checkout` (`_containment.sh`)

```text
# loom_path_in_main_checkout <abs-physical-path> - return 0 when the path is
# the main repository's root or lies under it AND loom_path_class prints
# `outside` for it; return 1 otherwise.
```

- The repository root is derived exactly as `loom_path_class` derives it (the physical form of
  the worktree path with `/.worktrees/<id>` removed).
- Return 1, whatever the path, when that root is `/`, is the physical `$HOME`, or is an
  ancestor of the physical `$HOME`: a repository that holds the home directory would otherwise
  turn every tool path into a block.
- `loom_path_class` itself is unchanged: its `outside` class keeps meaning "anything else, the
  main checkout's own files included", and its other callers keep working.

### 2. The rule (`worktree-isolation.sh`)

Add `check_main_checkout_tokens`, called from `validate_bash_command_token_path` after the
cross-worktree check, with `BLOCK_RULE=main-checkout` and a block printer modelled on
`print_cross_worktree_block`. For each token of `LOOM_TOKENS`:

- A word token (per `loom_token_is_word`) that starts with `/` or `~/`: normalise it with
  `loom_normalize_word` (base `/`; a word it refuses is skipped) and block when
  `loom_path_in_main_checkout` holds.
- A word token of the form `<name>=<value>` or `--<flag>=<value>` whose value starts with `/`
  or `~/`: judge the value the same way.
- A token that is not a word (quoted text with whitespace) counts only when it STARTS with an
  absolute path; judge its first whitespace-delimited field. A path that appears later in
  quoted prose is not a path argument.
- `cd <absolute path>` needs no special case: its target is a word token and is judged above.

The first stderr line of the block is `[worktree-isolation/main-checkout]` followed by the
existing banner style. The message says what was reached, that the worktree holds the same
files at the stage's own revision, and that loom's state under `.loom/work/` stays readable.

The hook's early exits are unchanged: a session that is not in a stage worktree (a merge
resolver in the main checkout, an operator's own session) is not judged.

## Tests (write them; the orchestrator runs them)

`loom-hooks/tests/worktree-isolation-main-checkout.sh`, registered in `run-all.sh` beside the
three `worktree-isolation-*` entries. Fixture: a temp `REPO` with `REPO/.worktrees/stage`,
`REPO/src/main.rs`, `REPO/.loom/work/signals/s.md`, and `HOME` pointed at a second temp
directory.

- Block (exit 2, first stderr line starts `[worktree-isolation/main-checkout]`):
  `cd $REPO/src && cat main.rs`; `cat $REPO/src/main.rs`; `rg -n foo $REPO/src`;
  `cargo test --manifest-path=$REPO/Cargo.toml`; `X=$REPO/src/main.rs cat "$X"`;
  `cat "$REPO/src/main.rs"`.
- Allow (exit 0): `cat $REPO/.loom/work/signals/s.md`; `cat $REPO/.worktrees/stage/src/x.rs`;
  `cat /etc/hostname`; `ls $HOME/.cargo/bin`; `echo "the brief mentions $REPO/src here"`;
  `git status`; a path under `LOOM_SCRATCH_DIR`.
- Rule off: the same block cases exit 0 in a fixture whose `HOME` is `REPO` itself.
- Unchanged: one traversal case and one cross-worktree case still block with their own tags.

`containment-cwd-resolve.sh`: cases for `loom_path_in_main_checkout` (repository file: 0; state
path: 1; worktree path: 1; system path: 1; repository-is-home: 1), and one asserting
`loom_path_class` still prints `outside` for a repository file.

Contracts to turn green: the two tests in `hook_main_checkout_contracts.rs`.

## Proof

`bash -n` on each shell file you changed. The new test script needs mode 0755: say so in your
report.

## Traps

- Bash 3.2: the token arrays are indexed arrays; guard an empty array before expanding it, as
  `check_cross_worktree_tokens` does.
- A rule that blocks every absolute path outside the worktree breaks every session: the signal
  file, the state directory, the toolchain and `/tmp` all live outside it.
- `$VAR` inside a token is not expanded by the hook (`loom_normalize_path` returns 1 on an
  unknown `$`); such a token is skipped. The `X=$REPO/...` case above is caught at
  the assignment, where the value is literal in the fixture's command string.
- The hook is embedded in the binary (`loom/src/fs/permissions/constants.rs`) and also runs for
  the codex lane. Add no new hook file: the helper goes into `_containment.sh`, which is
  already installed.
- Blocks are recorded by rule name. If any table in the hook scripts enumerates the rule names
  of this hook, add `main-checkout` to it and report where.
