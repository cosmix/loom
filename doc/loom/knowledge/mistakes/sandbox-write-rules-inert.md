# Sandbox Write Rules Inert

>

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
