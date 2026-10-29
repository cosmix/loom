# feat-planner-03-file-mode: shared context for every worker

Plan: `doc/plans/PLAN-feat-planner-03-file-mode.md`. It implements product section 06 (`doc/product/section_06.md`,
read the sections your brief names) on top of plan 1 (`loom::planning`) and plan 2
(`loom::plan_server`, `loom::web_server`, the planning web app under `web/src/plan/`). Read their
committed code before relying on a name; the plan prose "Interfaces this plan consumes" lists them.

## Rules that bind every worker

- Rust crate `loom/`; run cargo with `--manifest-path loom/Cargo.toml` from the worktree root.
  Web package `web/`; run its tools as `cd web && node_modules/.bin/<tool>` (tsc, oxlint, oxfmt,
  vitest, vite) after the provisioned install. Import `z` only from `@/api/zod`.
- 400 lines per file, 50 per function. The ledger `loom/maintainability-baseline.txt` is exact
  (growth and shrinkage both fail); only the `verify-split` worker changes it.
- No placeholders, stubs or empty bodies; no compatibility shims.
- Dependencies only through `bun add` (web) / `cargo add` (none expected in Rust).
- Every server open of `doc/` goes through `fs::safe_read` / plan 1's `planning::discover::read_doc`
  with no-follow opens; every write through the new `fs::safe_replace` (stage `file-server`).
  Never `std::fs` on a path a session can write.
- Paths in requests are relative to `doc/` and pass `planning::paths::check_relpath_syntax`
  first, then must be a document `planning::discover` lists (else 404). Server `git` calls take
  only such paths.
- Untrusted text (documents, names, paths) is never inserted as HTML in the browser: no
  `dangerouslySetInnerHTML`, no `innerHTML`.
- Never write a code comment, commit message or doc line that mentions an AI system.
- Record every mistake, decision and surprise with `loom memory note` / `loom memory decision` at
  once. Never Claude Code auto-memory. Never `git add`, `git commit`, `loom stage complete`.

## Editability (section 06 "What may be edited"; the server decides)

A path is editable when all hold: Markdown; under `doc/product/` or `doc/plans/`, or in the Other
group; `LockedSet::lock_reason(rel)` is `None` (no `IN_PROGRESS-`, `DONE-`, `RETIRED-`, not under
`doc/plans/archive/`, not `REVIEW-*`, not a brief of a locked plan); not under `doc/loom/`; at most
1 MiB. Lock notices, verbatim: running plan "This plan is running. `loom run` works from the
committed copy."; done plan or archive "This plan is done. Record changes in a new plan."; brief of
a locked plan: the same notice naming the plan; knowledge "The knowledge base changes through
`loom knowledge`. Ask in chat."

## Hash

The opened hash and every hash on the wire is plan 1's `planning::sections::normalized_hash(text)`
(`sha256:<hex>` after line-ending and trailing-whitespace normalisation), the same value as
`Project::content_hash`.
