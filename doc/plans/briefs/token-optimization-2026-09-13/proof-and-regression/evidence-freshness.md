# Source-backed knowledge evidence

## Boundary

Terra/xhigh worker. No git commands, verification, subagents or direct knowledge edits. Own `loom/src/fs/knowledge/catalog/evidence.rs`, `catalog.rs`, `catalog/issue.rs`, `catalog/order.rs`, relevant catalog tests, `loom/src/commands/knowledge/check.rs`, `loom/src/cli/types_memory.rs`, and `loom/src/cli/dispatch.rs`. New helpers/tests stay under `fs/knowledge/catalog/` and `commands/knowledge/`. Coordinate after the context-admission stage, which previously owns central dispatch. No source-graph refresh or context-store opening in this read-only command.

## Existing contract

Frontmatter already provides `sources` and `verified`; preserve its format. `catalog.rs::collect_chunk_issues` calls `evidence::changed_since_verified`, which currently compares `verified..HEAD` only. Invalid verification refs or git errors silently produce no evidence issue. `CatalogIssue::EvidenceChanged` and `UnverifiableReference` are review-only; `knowledge/check.rs::check` excludes them from ordinary `--strict`. Do not describe source changes as proof that prose is wrong.

## Implementation

Compare declared source evidence against the actual working tree as well as HEAD. Use literal source pathspecs, validated revision arguments, NUL-delimited filenames and bounded process output/time. Account for staged, unstaged, deleted and renamed declared sources; a relevant untracked declared source is changed/unverified. Never use shell interpolation or allow a source path beginning with pathspec magic to select unrelated files. Reuse the repository's safe subprocess/environment helpers where applicable. Group work by verification revision/source set within one catalog build so this does not add one expensive subprocess to every delivered excerpt.

Represent unavailable evidence explicitly rather than returning a clean result on invalid/missing revision, missing repository, command failure or resource limit. Prefer a new narrowly defined `CatalogIssue::EvidenceUnavailable { file, source_path, reason }` with bounded reason codes. Update all exhaustive serializers/renderers/order/`is_review_only` matches and their tests; inspect `rg -n 'CatalogIssue::' loom/src` before editing. Keep it review-only for compatibility with normal catalog retrieval and existing `--strict` behavior. No new event pipeline or database is needed.

Add `--strict-evidence` to `KnowledgeCommands::Check` and dispatch it through `knowledge::check::check`; update all callers and tests. This opt-in flag fails on changed/unavailable declared evidence as well as structural issues, while normal `--strict` remains unchanged. Files with no declarations remain explicitly unassessed, not verified. JSON adds evidence counts/status without removing existing `issues`, `review`, `count` fields. A missing knowledge root under strict-evidence is failure, not an empty clean catalog. The command stays read-only and never opens `ContextStore`.

Do not enable this gate globally over a corpus that has not been annotated. The final distill owner corrects high-impact pages and adds real source/revision annotations through existing knowledge CLI. Existing knowledge retrieval must continue on review-only evidence issues, visibly carrying its current uncertainty policy; do not silently drop required knowledge or pretend a hash validates arbitrary prose.

## Discriminating tests

Use the existing temporary-repository fixtures, with root injection rather than the user's home. Cover unchanged verified source, committed change, staged-only change, unstaged-only change, declared untracked source, deleted/renamed source, filenames with spaces/newlines/pathspec syntax, invalid/missing revision, unavailable git, missing root, and bounded-execution failure. Assert default/strict compatibility and strict-evidence failures, including decoded JSON. Verify each actual CLI argument reaches the checker; library-only tests cannot prove dispatch.

Add new `loom/tests/token_optimization_knowledge.rs` (owned) for CLI behavior. Include a sentinel shared-state directory and assert exact before/after inventory to prove no context-store writes. CLI tests run commands as test subprocesses in fixtures; workers themselves do not run git. Do not write evidence annotations here or mark unknown source evidence verified.

## Orchestrator proof

Run knowledge catalog/check library tests and `cargo test --offline --locked --manifest-path loom/Cargo.toml --test token_optimization_knowledge`, plus stage build/clippy/fmt. Full integration gate follows; strict-evidence is exercised on controlled fixtures, not imposed on the unassessed full knowledge corpus.
