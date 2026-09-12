# Sandbox Protected hooks/ Directory

> Resolved on 2026-09-13 by moving repository hook sources to `loom-hooks/`; the original sandbox rule and historical probes are retained below.

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

- A `chmod +x` acceptance criterion targeting the test runner under the old source root looked impossible from inside a sandboxed session and was adjudicated away rather than recognized as a sandbox artifact. The runner now lives at `loom-hooks/tests/run-all.sh`.
- A merge resolver could not resolve two `hooks/*` merge conflicts from the shell at all.

## Historical Workarounds

- Edit files under `hooks/*` with the Edit or Write tools, never with `sed`, `chmod`, redirection, or `cp` — those tools are not subject to this sandbox rule.
- Resolve `hooks/*` merge conflicts from an operator shell outside the sandboxed session.

## Completed Rename (2026-09-13)

Completed on 2026-09-13: the repository source directory is now `loom-hooks/`. Source embeds, script/test references, doctrine and active plans use that name. Installed paths remain `~/.claude/hooks/loom/` and `~/.codex/hooks/loom/`; the Rust module remains `loom/src/hooks/`. The 2026-09-02 probes and failures above describe the former directory, not the renamed source root. The rename verification includes a sandboxed write/delete probe and the Rust/hook gates.

## Test-File Modes (2026-09-12)

A new file created under the former `hooks/tests/` with the Write tool landed as `100644`, and a plain shell `chmod +x` on it failed with the sandbox denial described above, even though the Write itself succeeded. This was never blocking: `loom-hooks/tests/run-all.sh` runs every test file through `bash`, and many committed test files are `100644` (check with `git ls-files -s loom-hooks/tests`). A prior claim that all sibling test files are `100755` was wrong, so check `git ls-files -s` before assuming a new test must match its siblings. The mode can still be set outside the sandbox or with `git add --chmod=+x`, but a test left at `100644` runs correctly.
