# Sandbox Protected hooks/ Directory

> Claude Code's sandbox write-protects the project-root `hooks/` directory as part of its bare-git-repo rule; shell writes there fail as "Operation not permitted".

## The Rule

Claude Code's Bash sandbox treats a fixed set of paths as belonging to a "bare git repository" group and write-protects them regardless of per-project or per-user permission configuration: top-level `HEAD`, `objects`, `refs`, `config`, and `hooks`. Documented at <https://code.claude.com/docs/en/sandboxing#protected-paths>. In an ordinary (non-bare) repository the rule still matches the project-root `hooks/` directory, even though here it is a plain source directory rather than git's own hook directory.

## Probed Behavior (2026-09-02)

| Path | Shell write |
| --- | --- |
| `hooks/` | denied |
| `hooks/tests/` | denied |
| `.git/hooks/` | denied |
| `agents/` | allowed |
| `skills/` | allowed |
| `doc/` | allowed |
| `loom/` | allowed |
| `loom/x/hooks/` (nested) | allowed |
| `hooks2/` | allowed |
| `.husky/` | allowed |
| `.githooks/` | allowed |

Only a directory literally named `hooks` at the point the rule matches is affected. A nested `hooks/` several path segments down, or a differently-named directory, is unaffected.

## No Scoped Override Exists

Neither an `allowWrite` rule nor an `Edit(...)` allow rule in `.claude/settings.json` lifts this protection — it is enforced ahead of the ordinary permission system, not as part of it. The only ways to lift it are `sandbox.filesystem.disabled` or listing the specific command under `excludedCommands`; this repository sets `allowUnsandboxedCommands: false`, so neither is in effect here.

## Consequences Seen 2026-09-02

- A `chmod +x hooks/tests/run-all.sh` acceptance criterion looked like an impossible requirement from inside a sandboxed session and was adjudicated away rather than recognized as a sandbox artifact.
- A merge resolver could not resolve two `hooks/*` merge conflicts from the shell at all.

## Workarounds

- Edit files under `hooks/*` with the Edit or Write tools, never with `sed`, `chmod`, redirection, or `cp` — those tools are not subject to this sandbox rule.
- Resolve `hooks/*` merge conflicts from an operator shell outside the sandboxed session.

## Recommended Fix

Rename the directory (for example to `loom-hooks/`) once the current plan completes. Two of its remaining stages still reference `hooks/` paths, so renaming mid-plan would break them.

- **Refinement (2026-09-12):** a new file created under `hooks/tests/` with the Write tool lands
  as `100644`; a plain shell `chmod +x` on it fails with the same sandbox denial as above, even
  though the Write itself succeeded. This is not actually blocking — `run-all.sh` invokes every
  test file through `bash`, and many committed `hooks/tests/*.sh` are already `100644` (checked
  via `git ls-files -s hooks/tests`). A prior claim that all sibling test files are `100755` was
  wrong; do not assume mode parity with siblings without checking `git ls-files -s`. The
  orchestrator can still set the mode outside the sandbox, or with `git add --chmod=+x`, but a new
  hook test that stays `100644` will run correctly regardless.
