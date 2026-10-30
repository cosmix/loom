# file-server / doc-routes: the document and diff routes

Tier: opus, Claude lane (two routes that serve repository content from request paths). Wave 1,
beside the Claude `files-core` worker and the codex units; read `doc/plans/briefs/feat-planner-03-file-mode/common.md`
first.

## Files you own

`loom/src/plan_server/files/doc.rs`, `loom/src/plan_server/files/tests/doc.rs`,
`loom/src/plan_server/files/diff.rs`, `loom/src/plan_server/files/tests/diff.rs`. The orchestrator
has seeded each with a `//!` line and declared them in `files/mod.rs` and `files/tests/mod.rs`;
`files-core` registers your handlers in `files/mod.rs`.

## Spec

- `doc.rs`: the "doc.rs, plan_view.rs" section of `files-core.md` (the document part), with the
  pinned functions `doc_route(req: &ApiRequest) -> ApiResponse` and
  `doc_view(ctx: &PlanContext, rel: &Path) -> Result<DocView, ApiResponse>`; it calls codex unit
  F5's `plan_view::plan_view(root, rel, text, sections, validation) -> PlanView` and
  `TextStore::current` (files-core).
- `diff.rs`: the "`files/diff.rs`" section of `diff-search.md` (`head_diff`, `turn_diff`).
- Both routes refuse before touching the filesystem or git: `planning::paths::check_relpath_syntax`
  first (400), then membership in `ctx.project().tree` (404), then for a document reached through
  a symlink only plan 1's resolved, no-follow read (`planning::discover::read_doc`). A path that
  fails any check never reaches `run_git`. The frozen contract `doc-and-diff-routes-refuse-bad-paths`
  (`loom/tests/file_mode_contracts.rs`) pins this; read it before writing either route.

## Tests

`files/tests/doc.rs`: lock notices, `too_large` truncation, repo-relative response paths, 404 for
an undiscovered path. `files/tests/diff.rs`: as `diff-search.md` "Tests" lists for diffs.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::files::tests::doc` and
`--lib plan_server::files::tests::diff` pass.
