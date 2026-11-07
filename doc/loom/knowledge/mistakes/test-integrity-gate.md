# Test Integrity Gate

> Integrity gate false positives: formatter reflows read as changed asserts

## The Integrity Gate Read a Formatter Reflow as a Changed Assertion (2026-11-07)

**What happened:** integration-verify's merge resolver merged feat-planner-ui, whose commit 5b3ec4a3 left `web/src/plan/feature/spine-model.test.ts` failing `oxfmt --check`. The resolver formatted it to pass that criterion; oxfmt wrapped one `expect(...)` across five lines and added a trailing comma. Every acceptance criterion passed, then the test-integrity gate raised `TI-edit-web/src/plan/feature/spine-model.test.ts` and the stage was handed back to a fresh stage session, re-running the stage.

**Why:** `lost_assertions` (`loom/src/verify/integrity/edits.rs`) counted a removed assertion as kept only when an identical trimmed line was added back. A reflow never yields an identical line, so the formatter gate and the integrity gate could not both pass.

**Prevention:** a diff-based gate over source lines must compare modulo formatting, or it fights the repository's own formatter check. Compare whole hunks, so a reflow that also changes a token still counts.

**Fix:** a hunk whose removed and added text are equal after `normalize` (whitespace dropped outside string literals; a comma before `)`/`]`/`}` dropped except for Python, where `(1,)` is a tuple) contributes nothing. Tests in `loom/src/verify/integrity/tests.rs` cover Rust and TypeScript reflows and reflows hiding a value or string change.
