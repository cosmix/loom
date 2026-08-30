# Sandbox Write Rules Inert

> Sandbox Write() rules that are inert in loom generated settings versus the repository own committed settings.json.

**Status split on 2026-08-17: the generated-settings half is fixed; the repo-config half is
still open and needs an owner.**

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

That surviving-user-rule behaviour is deliberate, not a leftover (`settings.rs:399-411`):
pre-existing entries are carried forward verbatim, because stripping them would discard the
developer's own configuration. **Loom is strict about the rules it GENERATES and conservative
about the rules it INHERITS.** Do not "clean up" the test at `:1286` — it pins that policy.

## STILL OPEN — the repository's own committed `.claude/settings.json`

That file carries exactly three inert deny rules over the `.work` tree and **no `Edit(` rule
at all**: `Write(.work/**)` (line 12), `Write(../../.work/**)` (line 14) and an absolute
`Write(//home/.../loom/.work/**)` (line 23). Verified by
`rg 'Write\(|Edit\(' .claude/settings.json`.

Those three encode the CLAUDE.md rule that agents must never edit `.work` files directly.
Because the permission check reads only `Edit(path)`, they enforce nothing: the rule is
documented, warned about at session start, and **unenforced**.

**Why it was not fixed here.** `.claude/settings.json` sits outside the declared `allow_write`
scope of the plans that found it, so changing it would be an out-of-scope edit to the
developer's environment configuration.

**Recommended fix:** convert the three to `Edit(...)`, scoped carefully — and read the caution
below first. An `Edit(.work/**)` deny must not shadow the handoffs directory that stage
sessions legitimately write.

## Detection and caution

**Detection, generally:** grep for `Write(` in any settings file meant to gate file access.
`Edit` covers all file-editing tools, `Write` covers none of them. Note the asymmetry that
makes this class hard to see: the rule PARSES, so nothing fails.

**Caution when fixing:** deny beats allow, so a blanket `Edit(**)` deny cannot be paired with a
narrower `Edit(<dir>/**)` allow — the deny wins and blocks the directory the session needs.
Scope blanket denies to read-only modes only.
