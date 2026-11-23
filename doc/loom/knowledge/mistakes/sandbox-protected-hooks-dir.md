# Directory Named `hooks/` Is Sandbox-Protected

> Sandbox-protected hooks/ dir, inert Write() rules, hook preflight denylist

## The Rule

Claude Code's Bash sandbox treats a fixed set of paths as belonging to a "bare git repository" group and write-protects them regardless of per-project or per-user permission configuration: top-level `HEAD`, `objects`, `refs`, `config`, and `hooks`. Documented at <https://code.claude.com/docs/en/sandboxing#protected-paths>. In an ordinary (non-bare) repository the rule still matches the project-root `hooks/` directory, even though here it is a plain source directory rather than git's own hook directory.

## Probed Behavior (2026-09-02)

| Path                     | Shell write |
| ------------------------ | ----------- |
| `hooks/`                 | denied      |
| `hooks/tests/`           | denied      |
| `.git/hooks/`            | denied      |
| `agents/`                | allowed     |
| `skills/`                | allowed     |
| `doc/`                   | allowed     |
| `loom/`                  | allowed     |
| `loom/x/hooks/` (nested) | allowed     |
| `hooks2/`                | allowed     |
| `.husky/`                | allowed     |
| `.githooks/`             | allowed     |

Only a directory literally named `hooks` at the point the rule matches is affected. A nested `hooks/` several path segments down, or a differently-named directory, is unaffected.

## No Scoped Override Exists

Neither an `allowWrite` rule nor an `Edit(...)` allow rule in `.claude/settings.json` lifts this protection — it is enforced ahead of the ordinary permission system, not as part of it. The only ways to lift it are `sandbox.filesystem.disabled` or listing the specific command under `excludedCommands`; this repository sets `allowUnsandboxedCommands: false`, so neither is in effect here. The workaround is to route around the shell rather than lift the rule: edit files under a protected `hooks/*` path with the Edit or Write tools, never `sed`, `chmod`, redirection, or `cp`, and resolve `hooks/*` merge conflicts from an operator shell outside the sandboxed session.

## Consequences Seen 2026-09-02

A `chmod +x` acceptance criterion targeting a test runner under a `hooks/`-named directory looked impossible from inside a sandboxed session and was adjudicated away rather than recognized as a sandbox artifact, and a merge resolver could not resolve two `hooks/*` merge conflicts from the shell at all. Both are this rule, not a broken tool — check for a directory literally named `hooks` before treating either symptom as a real defect.

## Test-File Modes (2026-09-12)

A new file created under the former `hooks/tests/` with the Write tool landed as `100644`, and a plain shell `chmod +x` on it failed with the sandbox denial described above, even though the Write itself succeeded. This was never blocking: `loom-hooks/tests/run-all.sh` runs every test file through `bash`, and many committed test files are `100644` (check with `git ls-files -s loom-hooks/tests`). A prior claim that all sibling test files are `100755` was wrong, so check `git ls-files -s` before assuming a new test must match its siblings. The mode can still be set outside the sandbox or with `git add --chmod=+x`, but a test left at `100644` runs correctly.

## Extending the Check 5 Program Denylist Was Proposed as a Fix

**What happened:** `loom run` refused a user-level `bunx -y ccstatusline@latest --hook` hook. The first response split package runners out of `SELF_DIRECTED` with a better message, scanned `statusLine`, then built a trust-once flow, and recommended `bun add -g` plus an absolute binary path as the safe alternative. The owner rejected it as insufficient and it was reverted.

**Why:** check 5 is a list of named programs to refuse, and the fix only extended that list. Hooks also run session-written code through subcommand runners (`pnpm dlx`, `bun x`, `dnx`, `jbang`, `nix run`), version-manager shims reading cwd files (e.g. `.tool-versions` `path:` versions, `rust-toolchain.toml` `path`, `bunfig.toml` preload), runtimes loading writable caches (deno, go build cache, bun auto-install), Linux hard links from bun/uv/pnpm caches into global installs, shebangs, and launchers check 5 never reads (MCP stdio servers, plugin hooks, managed settings, `apiKeyHelper`).

**Prevention:** treat an unsandboxed hook as running every file and cwd config its runtime loads; before extending check 5's tables, enumerate those load paths across ecosystems and both platforms. Never recommend a global install as safe without checking whether the manager hard-links it from a session-writable cache.

**Fix:** reverted; the structural option (run non-loom hooks inside the session sandbox) is open with the owner.

## Sandbox Write Rules Inert

Claude Code's file permission check consults **only** `Edit(path)` rules. A `Write(path)` rule
parses, prints a startup warning, and is then ignored — so a `Write(**)` deny permits every write
it was written to block. The warning scrolls past during session startup:

```text
Permission deny rule (.claude/settings.local.json): Write(**) is not matched by file
permission checks — only Edit(path) rules are. Use Edit(**) instead.
```

## Generated Stage Settings Emit `Edit(...)`

`sandbox/settings.rs::generate_settings_json` emits `Edit(...)` throughout: the per-path allow rule, the
deny rules and the handoffs directory. An explicit `IMPORTANT` comment in that function records why,
naming this rule. Verified by
`rg '"Write\(|format!\("Write' loom/src/sandbox/settings.rs`: the only `Write(` literals left in the emitter
are the prefix filter in `carry_forward_denies`; the tests (`sandbox/settings/tests.rs`,
`sandbox/settings/tests_read_denies.rs`) assert that no inert `Write(...)` deny survives regeneration.

Inherited entries were once carried forward verbatim, on the principle that loom is strict about
the rules it GENERATES and conservative about the rules it INHERITS. That principle survives, but
verbatim carry-forward does not: a `Write(...)` deny it preserved enforced nothing and re-printed
its warning at every session start, forever. `fs/permissions/write_rules.rs::migrate_inert_write_denies`
now rewrites an inherited `Write(<p>)` to `Edit(<p>)` — the same intent, in the enforceable
spelling — and drops it only where an enforced rule would be harmful (blanket `**`/`*`,
`../`-relative, or the knowledge dir). The tests that used to pin verbatim survival now pin the
migration.

## A Project's `.claude/settings.json`

Three legacy `Write(...)` rules were `allow` entries, not deny entries, reaching every generated
project file; the file is not committed config, it is loom's own output (`git ls-files .claude`
returns nothing). `fs/permissions/constants.rs` wrote `Read(.work/**)` + `Write(.work/**)` into every
project on `loom init`, `git/worktree/settings.rs` added the resolved-absolute
`Write(/<abs>/.work/**)`, and `fs/permissions/sync.rs` promoted the worktree-relative
`Write(../../.work/**)` back into the main file.

All three are gone. `LOOM_PERMISSIONS` now grants `Edit(.loom/work/handoffs/**)` — the one directory
file tools legitimately write, matching the narrow allow generated stage settings already use.
A broad `Edit(.loom/work/**)` is NOT the fix: the main file is copied into every worktree, so it would
re-expose `.loom/work/admin.token` and `.loom/work/user.token` (S-1). `ensure_loom_permissions_to` prunes
the three legacy spellings from files older versions wrote, and `ensure_loom_hooks_local` runs
`settings.local.json`'s deny list through the migration above, so `loom init` heals a polluted
repo instead of waiting for the next stage spawn.

## Detection and caution

**Detection, generally:** grep for `Write(` in any settings file meant to gate file access.
`Edit` covers all file-editing tools, `Write` covers none of them. Note the asymmetry that
makes this class hard to see: the rule PARSES, so nothing fails.

**Caution when fixing:** deny beats allow, so a blanket `Edit(**)` deny cannot be paired with a
narrower `Edit(<dir>/**)` allow — the deny wins and blocks the directory the session needs.
Scope blanket denies to read-only modes only.

## Denying Claude Code's TMPDIR fails every sandboxed Bash call

**What happened:** commit 07a62d50 added `/tmp/claude-<uid>` to every capsule's `denyWrite` and `Edit(//tmp/claude-<uid>/**)` to its permission denies, to stop sandboxed commands rewriting background-task output files. Claude Code's Bash wrapper records the shell's cwd at `/tmp/claude-<uid>/cwd-<id>`, so every sandboxed command printed its output and then exited 1 with `read-only file system: /tmp/claude-1000/cwd-1475`. The completion relay hook skips failed Bash calls, so no stage session could finish, and integration-verify blocked.

**Why:** the deny was proven only by capsule-content tests, which pinned the new entry instead of running a command under it. The comment reasoned about task output files and missed the other writers in the same directory.

**Prevention:** a new deny on a directory the harness owns needs one real run first: `claude -p --settings <capsule>` running a Bash command, checking exit status 0. A test that asserts a capsule's literal contents proves the entry exists, and says nothing about whether a session still works.

**Fix:** the deny is gone from `sandbox/control_surfaces/session_denies.rs` and `claude_code_temporary_directory_stays_writable` guards it. Linux skips `denyWrite` globs, so `*/*/tasks` is no alternative. The rewrite risk is open: [State Confinement Gaps](../concerns/state-confinement-gaps.md).
