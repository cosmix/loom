# Plan: Rename `hooks/` to `loom-hooks/`

**Date:** 2026-09-02
**Kind:** manual plan (no loom orchestration)
**Precondition:** `DONE-PLAN-release-versioning-config-and-loom-dir.md` has reached
`DONE-`. Two of its remaining stages, `doctrine-subagent-grouping` and `integration-verify`,
carry `hooks/` paths in their tasks and acceptance criteria; renaming before they run would
fail those criteria against a directory that no longer exists.

## Why

Claude Code's Bash sandbox write-protects five names at the working-directory root because
together they form the layout of a bare git repository: `HEAD`, `objects`, `refs`, and, when
they already exist, `config` and `hooks`. The rule is documented at
<https://code.claude.com/docs/en/sandboxing#protected-paths>, has no scoped override (an
`allowWrite` entry or `Edit` allow rule does not lift it), and this repository runs the sandbox
in strict mode (`allowUnsandboxedCommands: false`), so there is no interactive retry outside
the sandbox either. Our hook sources live at exactly that name.

Every shell write under `hooks/` from a loom session therefore fails with a bare
`Operation not permitted`. On 2026-09-02 that produced two misattributed failures in one run:

- a `chmod +x hooks/tests/run-all.sh` was reported as an impossible acceptance criterion and
  adjudicated away (the criterion was fine; the sandbox was the cause);
- a merge-resolution session could not resolve two `hooks/*` conflicts and gave up.

Probes from a sandboxed session: `hooks/`, `hooks/tests/`, and `.git/hooks/` are denied;
`agents/`, `skills/`, `doc/`, `loom/`, a nested `loom/x/hooks/`, `hooks2/`, `.husky/`, and
`.githooks/` are all writable. Only the literal root name collides.

## Target name

`loom-hooks/`. It says whose hooks they are, mirrors the `loom` segment of the install target
`~/.claude/hooks/loom/`, and collides with none of the five protected names.

## What does not change

- The install target `~/.claude/hooks/loom/` and every hook `command` in
  `.claude/settings.local.json`. Those point at Claude Code's own directory, which is unrelated
  to this rule.
- Hook script names and contents, apart from repo-relative self-references.
- The embedded-hook mechanism (`include_str!` in `loom/src/fs/permissions/`): only the paths
  inside the macros move.

## Blast radius (measured 2026-09-02)

| Area | Files | `hooks/` references |
| --- | --- | --- |
| `loom/src` | 41 | 156, of which 31 are `include_str!` embeds |
| `hooks/*.sh` self-references | | 62, many naming the install target, which stays |
| `loom/tests` | | 8 |
| `doc/loom/knowledge` | 27 | 128 |
| `CLAUDE.md`, `CLAUDE.md.template`, `agents/`, `skills/` | | 8 |
| `.gitignore`, `.markdownlintignore`, CI | | 0 |

## Steps

1. **Rename from an operator shell**, not from a sandboxed session: the rename itself is a
   write under `hooks/`.

   ```bash
   git mv hooks loom-hooks
   ```

2. **Source embeds.** Update every `include_str!("../../hooks/...")` (or equivalent) under
   `loom/src/fs/permissions/` and any other `hooks/` literal in `loom/src`. Find them with:

   ```bash
   rg -n 'include_str!\("[^"]*hooks/' loom/src
   rg -n '(^|[^./~])hooks/' loom/src loom/tests
   ```

   The second pattern deliberately skips `.claude/hooks/` and `~/.claude/hooks/`, which are
   install-target references and must stay.

3. **Scripts and tests.** In `loom-hooks/*.sh` and `loom-hooks/tests/*.sh`, change
   repo-relative references only (`hooks/tests/run-all.sh`, `hooks/_common.sh` when used as a
   repo path). `$(dirname "$0")` references need nothing. Run
   `bash loom-hooks/tests/run-all.sh`.

4. **Doctrine and prose.** Update `CLAUDE.md`, `CLAUDE.md.template`, `agents/*.md`, `skills/**`
   with the same pattern. For `doc/loom/knowledge/**`, replace the repo-directory references
   and leave install-target references alone, then run `loom knowledge sync` so `INDEX.md`
   and the source-reference check see the new paths.

5. **Plans.** Only active plans matter. Grep `doc/plans/PLAN-*.md` and
   `doc/plans/IN_PROGRESS-*.md` for `hooks/` and update criteria and file lists; `DONE-` plans
   are history and stay as written.

6. **Verify.**

   ```bash
   cd loom && cargo build && cargo clippy --all-targets -- -D warnings && cargo test
   bash loom-hooks/tests/run-all.sh
   loom repair                # hook install still resolves from the embedded copies
   loom knowledge check
   rg -n '(^|[^./~])hooks/' --glob '!doc/plans/DONE-*' --glob '!.git' .
   ```

   The last command must return nothing. Then, from a sandboxed session:

   ```bash
   touch loom-hooks/.probe && rm loom-hooks/.probe
   ```

   which must succeed where `touch hooks/.probe` failed before.

7. **Commit** as one `chore(hooks): rename hooks/ to loom-hooks/` commit; the rename and the
   reference updates are one concern and must land together.

## Rollback

`git mv loom-hooks hooks` and revert the single commit. Nothing outside the repository
changes, so no reinstall is needed.

## Acceptance

- No `hooks/` directory at the repository root.
- Build, clippy, the Rust suite, and the hook test suite pass.
- The straggler grep in step 6 is empty.
- A sandboxed session can create and delete a file under `loom-hooks/`.
- `doc/loom/knowledge/concerns/sandbox-protected-hooks-dir.md` records the rename date.
