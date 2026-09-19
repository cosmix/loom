# H3 — completion guard and file guard false positives, main-agent edit advisory

Tier: opus (`loom-senior-software-engineer`). Both hooks are security boundaries. Read
`../common.md` first.

## Goal

Two guards stop blocking legitimate work, without opening either boundary, and the file guard
gains the advisory that backs the delegation cost rule. Evidence: report section 4.6 (6a, 6e,
6f): `loom-control-complete.sh` 107 blocks with 8 flails and 15 verbatim retries, the flails all
on heredoc commands that complete nothing; `worktree-file-guard.sh` blocking the harness
scratchpad.

## Files you own (write)

- `loom-hooks/loom-control-complete.sh`
- `loom-hooks/worktree-file-guard.sh`
- `loom-hooks/tests/loom-control-complete*.sh`, `loom-hooks/tests/worktree-file-guard*.sh` and
  new test scripts. Do NOT edit `loom-hooks/tests/run-all.sh`: list the `run_test` lines your
  scripts need in your report; H6 adds them.

Read-only: `loom-hooks/_common.sh`, `doc/loom/knowledge/patterns/hook-content-stripping.md`
(lines 96-126 describe this exact failure), `mistakes/shell-command-matchers.md`.

## Part 1 — completion guard

`is_completion_command` (109-139) tokenises the raw command, heredoc bodies included; it is the
one hook here that never calls `strip_embedded_content`. A heredoc whose prose spells the
three-token completion shape, or whose apostrophe breaks tokenising and drops to the substring
fallback (119-123, `raw_has_completion_indicators`), is then rejected by
`completion_rejection_reason` (141-159) because separators are present.
Change: decide "is this a completion command" on the command with embedded content stripped
(heredoc bodies and quoted strings), using the shared helper. A real completion command hidden
inside `bash -c '...'` or a heredoc fed to a shell must still be caught: keep the raw-string
fallback for the case where the stripped command invokes a shell interpreter (`bash`, `sh`,
`zsh`, `eval`, `xargs`). The rejection rules for a genuine completion command do not change.
`loom-hooks/tests/loom-control-complete-knowledge.sh` exists with no `run_test` entry; report
the line for H6 and make the script cover `loom knowledge replace-section ... <<'EOF'` whose body contains the completion
words and an apostrophe.

## Part 2 — file guard scratchpad

`allow_background_output` (154-165) permits only `/tmp/claude-<uid>/.../tasks/*.output`. The
harness also assigns each session `/tmp/claude-<uid>/<project-slug>/<session>/scratchpad/`.
Permit read and write tools under that directory with the same ownership test the existing
function applies (owned by the current uid, no symlink components, no `..`). Nothing else under
`/tmp` changes.

## Part 3 — main-agent edit advisory (warn only)

The guard has no main-versus-subagent distinction today. Add one using the helper `_common.sh`
already provides for that (`rg -n 'loom_is_subagent' loom-hooks`); if none fits, use the
`agent_id` field of the hook input, which is absent for the main agent. For the main agent of a
stage session only (`LOOM_STAGE_ID` set), on Edit, Write, MultiEdit and NotebookEdit of a path
that is not under `doc/`, not `*.md`, and not a distill scratch file (`.kb_tmp_*`,
`.distill-body-*`): record the path in a session ledger (kind `edits`, the read ledger's
pattern). Warn when the new content of one call exceeds 20 lines, or when the path is the third
distinct one in the ledger. Message: the small-change test from common.md in one sentence, and
that anything larger is delegated. Never deny. Warn at most once per path.

## Traps

- `doc/loom/knowledge/mistakes/ambient-filesystem-trust.md` and
  `mistakes/untrusted-value-boundaries.md`: paths in hook input are untrusted; compare resolved,
  component-wise, never by prefix string.
- The guard blocks knowledge-directory writes unconditionally (300-308); keep that.
- An unwritable ledger must not turn the advisory into a failure.

## Proof

Your own test scripts, each run once (`bash loom-hooks/tests/<script>.sh`). Every new behaviour
has a case, including: a
genuine `loom stage complete` inside a heredoc passed to `bash` is still treated as a completion
command; a scratchpad path owned by another uid is still blocked; a subagent's edits never warn.
