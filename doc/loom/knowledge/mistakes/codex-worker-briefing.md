# Codex Worker Briefing Gotchas

> Codex brief pitfalls: braces, doc placeholders, path reuse, jq status

## A Bash Command Text Containing Both "loom" and Any "complete" Substring Gets Pinned, Even for Unrelated Commands

**What happened:** `loom-hooks/loom-control-complete.sh` (`PreToolUse:Bash`) blocked a `loom knowledge replace-section ...` call — nothing to do with completing a stage — because the heredoc content passed on stdin contained the word `Completed` (a `SessionExitReason` variant) and the command also invoked `loom`. The hook error was `LOOM_CONTROL_ERROR: completion must be one exact pinned command`.

**Why:** the hook matches on the RAW text of the whole Bash `tool_input.command` — any command matching `*loom*complete*` (case-insensitive) or a `<loom> stage <verb containing complete>` token is held to the exact pinned `loom stage complete <stage>` form, regardless of what the command actually does. A codex forwarding brief hit the same trap from the other side: a forwarder whose task text (passed through an unquoted heredoc) contained the word "complete" or the phrase "loom stage" was blocked before its job ever launched (`loom-hooks/loom-control-complete.sh:155`).

**Prevention:**

- When a Bash call needs to embed multi-line content that might contain the substring "complete" alongside the word "loom" (doc prose, knowledge bodies, code samples), write the content to a file first via the Write tool (which this hook does not gate) and pipe it in with `< file` instead of a heredoc in the same Bash command — the substring then never appears in the command text itself.
- Codex/forwarder briefs must avoid apostrophes (so the forwarder can single-quote the task as one shell argument) and must never contain the literal substring `complete` or the phrase `loom stage`, even in prose describing what the unit should NOT do.

## Rust Text Written by a Codex Unit Can Silently Break the Whole Crate Build

Three distinct compile-breaking patterns, all found only because the orchestrator compiled right after each unit (a codex unit's own check is compile-only or skipped — see [Owned Waits § Codex unit verification limit](../architecture/owned-waits.md)):

- **Un-escaped braces in `format!(concat!(...))`:** a regex quantifier like `[A-Za-z0-9._-]{1,64}` inside a `format!` string is parsed by rustc as a `{}` placeholder (`invalid format string ... expected }`). Briefs for doctrine/format! text must say to double every literal brace (`{{` `}}`).
- **Bare CLI placeholders in doc comments:** a doc comment containing `<agent-id>` or `<unit-id>` unescaped is an unclosed HTML tag under `rustdoc` with warnings denied — the crate's own gate (fmt/build/clippy/tests) can be green while the repo's pre-commit hook (which also runs rustdoc) still rejects the commit. Write CLI placeholders in doc comments inside backticks: `` `<agent-id>` ``.
- **Reuse means the exact import path, not a second `#[path]` inclusion:** briefing a unit to "reuse" an existing module by `#[path]`-including its file a second time loads that file as two separate modules, which `clippy -D warnings` rejects as `duplicate_mod`. A reuse brief must name the actual import path (`use crate::...`) and, if needed, say the owning file's item may be bumped to `pub(crate)`.

## Integration-Test Submodules Need `#[path]`, Not a Bare `mod x;`

`loom/tests/<target>.rs` is a cargo integration-test crate ROOT; a bare `mod support;` inside it resolves against `loom/tests/`, not `loom/tests/<target>/`, so rustc reports `E0583 file not found for module support` even though `loom/tests/<target>/support.rs` exists on disk. Submodules under a test target's own directory need `#[path = "<target>/<file>.rs"] mod support;` (or a `mod.rs`-style directory target). This applies to every `loom/tests/*.rs` integration target that grows helper submodules (fixtures, scenarios, support).

## Retiring an Evidence Format Requires Sweeping Test Fixtures, Not Just Production Readers

When a unit retires one evidence file/record shape (e.g. a per-agent `<agentId>.json` termination record replaced by a `lifecycle.jsonl` journal line), every test FIXTURE that wrote the old shape to force a scenario (not just production callers) must be converted — a fixture helper like `write_start_and_stop()` that still writes the retired file silently produces the OLD behavior (a stale-but-present record reads as fresh under a debounce window) while the test's assertion still passes, because the fixture's job was to construct that exact prior state. A compile-only check cannot catch this; only running the affected unit's own tests does. Sweep every test fixture that constructs the old format with `rg` before considering the retirement done.

## `2>/dev/null || true` on a jq Call Turns a Program Defect Into a Silent No-Match

A hook's jq filter with an unclosed `select(` is a jq syntax error (exit 3, "unexpected end of file"); piping it through `2>/dev/null || true` (common in hooks that treat "no match" as a normal, silent outcome) makes that syntax error indistinguishable from a legitimate empty result — every real event the filter should have matched silently skipped. **Prevention:** in hooks, check jq's exit status separately from an empty result; do not blanket-swallow jq's stderr/exit code when "no match" and "program broken" must be told apart.

## JSONL Fixtures Must End With a Trailing Newline

A reader that validates transcript/journal freshness by checking the last byte (to detect a torn, still-being-written line) rejects a JSONL fixture whose last row has no trailing `\n` as `Stale('transcript is growing or torn')` — even though the content is complete and correct. Any fixture-writing helper that joins rows with `\n` must also terminate the final row, not just separate them.

## `cargo fmt` Runs After a Codex Unit's Own Size Check, and Can Push a File or Function Over the Limit

A codex unit that checks its own file/function line counts BEFORE the orchestrator's `cargo fmt` pass can pass its own check and still land oversized: `cargo fmt` reflows hand-wrapped code (breaking long argument lists, chained calls, etc. onto more lines), so a file measured at 505 lines pre-fmt can land at 519 post-fmt — over a 514-line ceiling the unit never saw fail. **Prevention:** run `cargo fmt` BEFORE measuring any file or function against a size limit (in a brief or in the orchestrator's own post-unit check), and brief workers to keep at least a 10% margin under each limit, since fmt reliably grows hand-wrapped code, never shrinks it.

## `Stage::new` Derives a Timestamped Id — a Fixture Must Set `stage.id` Explicitly

`Stage::new(name, ..)` (`loom/src/models/stage/methods.rs:95-102`) derives `id` as `stage-<name>-<timestamp>`, never the bare `name` passed in. A fixture that calls `Stage::new("notes", None)` and then looks up behavior by `"notes"` (e.g. `handle_stage_completed("notes")`) silently misses: the stage saves as `01-stage-notes-<ts>.md`, session lookups keyed by the derived id return `None`, and the code path under test never runs even though the test may still pass on an unrelated assertion. Set `stage.id` explicitly after `Stage::new` in any fixture that needs a known, stable id.

## `loom subagents watch --worker codex:<unit>` Can Report "unknown" With Exit 0

Running `loom subagents watch --worker codex:<unit>` right after spawning
`loom-codex-forwarder` agents printed `unknown: worker set does not resolve to one Claude
parent UUID`, yet the Bash exit code was 0. Read the watch output itself, never trust the exit
code alone; fall back to Agent completion notifications plus the `LOOM-CODEX-EVIDENCE` trailer
when the watch cannot resolve the worker set.

## `erasableSyntaxOnly` Rejects Codex-Written TypeScript Parameter Properties

A codex (`gpt-5.6-terra`) unit wrote TypeScript constructor parameter properties (`readonly
title: string` in the constructor signature) in two `web/` test files. `web/tsconfig` enables
`erasableSyntaxOnly`, so `tsc -b` fails `TS1294` while `vitest` still passes -- esbuild strips
the construct at test time, so the break is invisible until the gate runs the typechecker.
Web briefs for codex units must state "no parameter properties, enums or namespaces --
erasableSyntaxOnly", and the inter-wave gate must run typecheck, not tests alone.

## Codex Units in `web/` Do Not Run `oxfmt`

7 of 12 files failed `format:check` after all codex units in a `web/` wave returned; codex
units do not run `oxfmt` on their own output. Either the orchestrator runs `bun run --cwd web
format` before the final gate, or each unit's brief must explicitly tell it to run `oxfmt` on
its own files before reporting back.
