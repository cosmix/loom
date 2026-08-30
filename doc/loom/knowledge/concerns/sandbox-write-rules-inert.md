# Sandbox Write Rules Inert

> Sandbox Write() rules that are inert in loom's generated stage settings and in the
> `.claude/settings.json` loom writes for a project.

**Status split on 2026-08-17, both halves RESOLVED 2026-08-31.** Kept because the underlying
fact still governs every settings file loom writes.

The underlying fact is unchanged and is the thing to remember: Claude Code's file permission
check consults **only** `Edit(path)` rules. A `Write(path)` rule parses, prints a startup
warning, and is then ignored — so a `Write(**)` deny permits every write it was written to
block. The warning scrolls past during session startup:

```text
Permission deny rule (.claude/settings.local.json): Write(**) is not matched by file
permission checks — only Edit(path) rules are. Use Edit(**) instead.
```

## FIXED — loom's generated stage settings

`sandbox/settings.rs` now emits `Edit(...)` throughout: `:227` for the per-path allow rule,
`:287` for deny, `:302` and `:181` for the handoffs directory. An explicit `IMPORTANT` comment
at `:240-244` records why, naming this concern. Verified by
`rg '"Write\(|format!\("Write' loom/src/sandbox/settings.rs` — the only remaining `Write(`
occurrences are at `:1234` and `:1286`, both in the carry-forward test asserting a
USER-authored `Write(~/.bashrc)` **survives**.

Inherited entries were once carried forward verbatim, on the principle that loom is strict about
the rules it GENERATES and conservative about the rules it INHERITS. That principle survives, but
verbatim carry-forward does not: a `Write(...)` deny it preserved enforced nothing and re-printed
its warning at every session start, forever. `fs/permissions/write_rules.rs::migrate_inert_write_denies`
now rewrites an inherited `Write(<p>)` to `Edit(<p>)` — the same intent, in the enforceable
spelling — and drops it only where an enforced rule would be harmful (blanket `**`/`*`,
`../`-relative, or the knowledge dir). The tests that used to pin verbatim survival now pin the
migration.

## FIXED — a project's `.claude/settings.json` (2026-08-31)

**Correction to the earlier text here:** those rules were `allow` entries, not deny entries, and
the file is not committed config — it is loom's own output. `git ls-files .claude` returns
nothing; `fs/permissions/constants.rs` writes `Read(.work/**)` + `Write(.work/**)` into every
project on `loom init`, `git/worktree/settings.rs` added the resolved-absolute
`Write(/<abs>/.work/**)`, and `fs/permissions/sync.rs` promoted the worktree-relative
`Write(../../.work/**)` back into the main file.

All three are gone. `LOOM_PERMISSIONS` now grants `Edit(.work/handoffs/**)` — the one directory
file tools legitimately write, matching the narrow allow generated stage settings already use.
A broad `Edit(.work/**)` is NOT the fix: the main file is copied into every worktree, so it would
re-expose `.work/admin.token` and `.work/user.token` (S-1). `ensure_loom_permissions_to` prunes
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
