# file-server: codex units

Lane: `loom-codex-forwarder`, `--model gpt-6.1-terra --effort high`, foreground only, at most six
at once over disjoint files, each spawn with an explicit Bash timeout of 600000 ms. A unit is one
file, or one file plus its test (fixture JSON counts as that file's test data), at most three
steps, inside the wrapper's 540 s deadline; a timed-out unit is re-split, never re-forwarded as is.

## Rules every unit follows (codex reads them from this file; the prompt is only the fixed line)

- Read `doc/plans/briefs/plan-file-mode/common.md` and the spec section named below; write only
  the unit's files. Do not run git; touch no hidden loom state or work path. At most
  `cargo check --manifest-path loom/Cargo.toml` (or the web typecheck); the orchestrator runs tests.
- `loom map` answers from the base layer and cannot see what other units or the Claude workers of
  this stage wrote. Shapes from them are pinned in the spec sections: the wire types
  (`wire-files.md` "Types"), `TextStore`, `TurnId`, `PinnedText`, `TurnChange` (files-core.md
  "texts.rs"), `RouteSpec`, `ApiRequest`, `ApiResponse` (plan 2's committed `plan_server/routes.rs`,
  `request.rs`).
- Rust: leave 10% under the limits (360 lines per file, 45 per function); double literal braces
  inside `format!`. Web: no TypeScript parameter properties; `z` only from `@/api/zod`; the
  orchestrator runs `oxfmt` after each web unit.

## Orchestrator duties

**Module seeding (every stage):** whenever the orchestrator adds a `mod` line, it creates the
module's file at the same moment holding only a `//!` purpose line, so the crate always compiles
and every unit's proof selects its tests as soon as the unit writes them. The unit or worker that
owns the file replaces the seed; the seed is not ownership.


Before wave 0: widen `mod lexical` in `loom/src/context/mod.rs` and `mod corpus` in
`loom/src/context/rank.rs` to `pub(crate) mod` (visibility only); declare `pub mod files;
pub mod search;` in `loom/src/plan_server/mod.rs` and `pub mod files;` in
`loom/src/plan_server/wire/mod.rs`; write `loom/src/plan_server/files/mod.rs` declaring `pub mod`
`tree`, `knowledge_index`, `doc`, `plan_view`, `diff`, `save`, `texts`, `validate` and
`#[cfg(test)] mod tests;`, `loom/src/plan_server/files/tests/mod.rs` declaring `mod
knowledge_index; mod tree; mod plan_view; mod doc; mod diff; mod save; mod texts; mod validate;
mod routes;`, and `loom/src/plan_server/search/mod.rs` declaring `pub mod index; pub mod query;`
and `#[cfg(test)] mod tests; #[cfg(test)] mod tests_query;`; seed every file those lines name with
a `//!` line. `files/mod.rs` then passes to the Claude `files-core` worker (it adds `routes()` and
`install`) and `search/mod.rs` to F10 (it adds `routes()` and `install`); neither removes a module
line. After F1: add the three `PlanMessage` variants to
`loom/src/plan_server/wire/message.rs` and to the union in `web/src/plan/api/schema.ts` (three
lines each, names in wire-files.md). After every unit: `git status --short` and its proof command.

## Units

| Unit | Wave | Owns | Reads | Entry points | Spec | Proof (orchestrator) |
| ---- | ---- | ---- | ----- | ------------ | ---- | -------------------- |
| F1 | 0 | `loom/src/plan_server/wire/files.rs` (inline parity tests), `web/src/plan/api/fixtures/files-*.json` | plan 2 `wire/server.rs` (inline test pattern) | every type in wire-files.md "Types" | wire-files.md "Types", "Fixtures and tests" | `cargo test --manifest-path loom/Cargo.toml --lib plan_server::wire::files` |
| F2 | 0b | `web/src/plan/api/schema-files.ts`, `web/src/plan/api/schema-files.test.ts` | F1's fixtures and `wire/files.rs`, plan 2 `web/src/plan/api/schema.ts` | the zod schemas and const arrays | wire-files.md "Fixtures and tests" | `cd web && node_modules/.bin/vitest run src/plan/api` |
| F3 | 0 | `loom/src/plan_server/files/knowledge_index.rs`, `loom/src/plan_server/files/tests/knowledge_index.rs` | `doc/loom/knowledge/INDEX.md` (format) | `parse_index(text) -> Vec<IndexTopic { category, slug, path, blurb }>` | files-core.md "tree.rs, knowledge_index.rs" | `--lib plan_server::files::tests::knowledge_index` |
| F4 | 1 | `loom/src/plan_server/files/tree.rs`, `loom/src/plan_server/files/tests/tree.rs` | plan 1 `planning::project::Project`, `planning::briefs`, F3; `files/mod.rs` is seeded with every module line, and the Claude `files-core` worker writes `files/texts.rs` first in wave 1 (`loom map` cannot see it; read the file): `TextStore::{current, begin_turn, end_turn, turn_pin, turn_end}`, `PinnedText { text, hash }` (files-core.md "texts.rs") | `build_tree(project: &Project, validation: &HashMap<PathBuf, ValidationView>) -> FileTree` | files-core.md "tree.rs, knowledge_index.rs" | `--lib plan_server::files::tests::tree` |
| F5 | 1 | `loom/src/plan_server/files/plan_view.rs`, `loom/src/plan_server/files/tests/plan_view.rs` | `plan/parser/extraction.rs` (`extract_yaml_metadata`), `crate::yaml::from_str`, plan 1 `planning::{implements, pressure, sections}`; `files/mod.rs` is seeded with every module line, and the Claude `files-core` worker writes `files/texts.rs` first in wave 1 (`loom map` cannot see it; read the file): `TextStore::{current, begin_turn, end_turn, turn_pin, turn_end}`, `PinnedText { text, hash }` (files-core.md "texts.rs") | `plan_view(root: &Path, rel: &Path, text: &str, sections: &DocSections, validation: Option<ValidationView>) -> PlanView` | files-core.md "doc.rs, plan_view.rs" | `--lib plan_server::files::tests::plan_view` |
| F8 | 0 | `loom/src/plan_server/search/query.rs`, `loom/src/plan_server/search/tests_query.rs` | `loom/src/context/lexical.rs` (`tokenize`) | `parse_query(q) -> Query { Literal(String) or Terms(Vec<String>) }`, `snippet(body, terms, max_chars) -> String`, `literal_count(text, needle) -> usize` | diff-search.md "search/" (literal mode, snippet) | `--lib plan_server::search::tests_query` |
| F9 | 1 | `loom/src/plan_server/search/index.rs`, `loom/src/plan_server/search/tests.rs` | `fs/knowledge/chunker.rs` (`chunk_sections`), `context/lexical.rs` (`field_tokens`), `context/rank/corpus.rs` (`prepare_lexical`, `LexicalCorpus::score`), F8's functions (pinned above); `files/mod.rs` is seeded with every module line, and the Claude `files-core` worker writes `files/texts.rs` first in wave 1 (`loom map` cannot see it; read the file): `TextStore::{current, begin_turn, end_turn, turn_pin, turn_end}`, `PinnedText { text, hash }` (files-core.md "texts.rs") | `SearchIndex::{build, apply, query}`, `Scope` | diff-search.md "search/" | `--lib plan_server::search::tests` |
| F10 | 2 | `loom/src/plan_server/search/mod.rs` | F9, plan 2 `routes.rs` | `routes()` (`GET /api/search`), `install(ctx)` | diff-search.md "search/" (`search/mod.rs`) | `--lib plan_server::search` |

Waves: 0 runs F1, F3, F8 beside the Claude `verify-split` worker (background), then F2; 1 runs
F4, F5 and F9 beside the Claude `files-core` and `doc-routes` workers (background);
2 runs F10. F6 and F7 are not used: the document and diff routes belong to the Claude
`doc-routes` worker (`doc-routes.md`).
