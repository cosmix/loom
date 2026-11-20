# language-skills — shared specification

Every new skill is a catalogued loom skill (not a core skill: `skills/core-skills.txt` does not
change). `loom/build.rs` embeds every `skills/*/SKILL.md` automatically; nothing else needs
registering except the catalog table row (W5).

## Shape

Model the new skills on `skills/loom-python/SKILL.md` (608 lines) and `skills/loom-rust/SKILL.md`:
open them and follow their frontmatter, heading style, density and code-block conventions.

Frontmatter (exact keys):

```yaml
---
name: loom-<lang>
description: <Language> language expertise for idiomatic, production-quality code.
triggers:
  - <lang>
  - <ecosystem words: build tool, test runner, package manager, major frameworks>
---
```

Required `##` sections, in this order (add language-specific sections between 2 and 3 as the
language needs, like loom-python's "Async / Await" or loom-rust's "Error Handling"):

1. `## Overview`
2. `## Tooling` (build tool, package manager, formatter, linter, the canonical lint/typecheck/test gate)
3. language sections (types, errors, concurrency, modules/packages, the major frameworks)
4. `## Testing`
5. `## Loom Test Runner Adapter` (exact heading; the stage acceptance counts it)
6. `## Anti-Patterns`
7. `## Expert Practices` (idioms, gotchas, performance, security at the language level)
8. `## Verification Checklists`

Length: 350–550 lines per skill. Every fenced code block names its language. Write in the plain,
direct register of the existing skills. The words and constructions listed in CLAUDE.md Rule 19
do not appear.

## `## Loom Test Runner Adapter` section

Take every fact from `doc/plans/briefs/verification-v2/DESIGN.md` D5 and D7. Contents, in order:

1. **Adapter** — the loom adapter name(s) in backticks (e.g. `` `gradle` ``, `` `maven` ``) and
   the D7 detection rule that picks each (`loom project detect` prints it per package).
2. **Single-test command** — the exact command D5 lists, as a `bash` block.
3. **The `test` field** — what a contract's `test` value is for this runner (the D5 "`test`
   value" table), with one concrete example.
4. **No-match behaviour** — whether the runner exits 0 when a filter matches nothing (the D5
   table, or "documented" for runners without a captured fixture), and that loom reads the
   runner's summary, so a contract test whose name does not match fails the freeze.
5. **Writing contract tests** — where test files live for this language (the D6 test-file
   globs), how to name a test so the adapter selects exactly one, and one worked contract in
   plan YAML:

   ```yaml
   contracts:
     - id: rejects-symlinked-spool
       file: <a test file path in this language>
       test: <the exact test value>
       scenario: <what the test sets up>
       rejects: <the plausible wrong implementation this test fails on>
   ```

6. **Build failures** — for compiled languages, that a contract test which does not compile yet
   counts as red at freeze time.
