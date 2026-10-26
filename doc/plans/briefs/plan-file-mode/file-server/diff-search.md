# file-server / diff-search: spec for the Claude `doc-routes` worker (diffs) and codex units F8-F10

The Claude `doc-routes` worker owns `files/diff.rs` and `files/tests/diff.rs`; codex F8
`search/query.rs` and `search/tests_query.rs`; F9 `search/index.rs` and `search/tests.rs`; F10
`search/mod.rs`. The orchestrator makes the visibility edits in `loom/src/context/mod.rs`
(`mod lexical` -> `pub(crate) mod lexical`) and `loom/src/context/rank.rs` (`mod corpus` ->
`pub(crate) mod corpus`) before wave 0; the Claude `files-core` worker declares `diff` in
`files/mod.rs` and registers the diff handlers. Read `doc/plans/briefs/plan-file-mode/common.md`
first. Spec: `doc/product/section_06.md` "Diffs", "Search"; `doc/product/section_12.md` "Paths the
server reads that sessions can write" (git paths).

## `files/diff.rs`

- Section 12 asks server `git` calls to pass paths after `--`. The only git read here is
  `git show HEAD:<path>`, where the path is part of a revision expression and `--` does not apply;
  the argument always starts with `HEAD:`, so it can never be read as an option, and the path has
  passed the syntax checks and discovery. Record this in a doc comment.
- `pub fn head_diff(req: &ApiRequest<'_>) -> ApiResponse` for `GET /api/diff/head?path=`: path passes
  `check_relpath_syntax` and is in `project.tree` (else 400/404); `before` =
  `crate::git::runner::run_git(&["show", &format!("HEAD:{rel}")], root)` (`git/runner.rs:136`; a
  non-zero exit, as for an untracked file, means `before: None`; a spawn error is 500), `after` =
  `TextStore::current`. The `HEAD:<path>` form takes no `--`; the path has passed the syntax checks
  and discovery, so it cannot start with `-` or carry `..`. Cap `before` at 1 MiB (larger: 413).
- `pub fn turn_diff(req)` for `GET /api/diff/turn?turn=&path=`: `TextStore::turn_pin` /
  `turn_end`; an unknown turn or a path the turn did not change is 404.
- Both answer `DiffTexts` with a repo-relative `path`. Role `Viewer`.

## `search/`

```text
pub struct SearchIndex { .. }        // in ctx.extensions()
impl SearchIndex {
    pub fn build(project: &Project, texts: &TextStore) -> SearchIndex;
    pub fn apply(&self, project: &Project, texts: &TextStore, batch: &Batch);
    pub fn query(&self, q: &str, scope: Scope) -> SearchResults;   // wire::files::SearchResults
}
pub enum Scope { Product, Plans, Knowledge, All }
```

- Chunking: `crate::fs::knowledge::chunker::chunk_sections(path, content, &Frontmatter::default())`
  (`fs/knowledge/chunker.rs:84`, `pub(crate)`; `Frontmatter` derives `Default`) over every document
  file mode shows: product, plans (briefs, done plans and `doc/plans/archive/` included: do NOT use
  `catalog::prose::prose_lifecycle`, which drops archive and `done-` plans at `prose.rs:~343`),
  knowledge, other. Never `doc/loom/summaries/`. Texts come from `TextStore` (no disk reads, no cache
  file anywhere).
- Ranking: terms per chunk from `context::lexical::field_tokens(&chunk)` (`lexical.rs:135`), query terms
  from `context::lexical::tokenize(q)` (`:39`), scored with
  `context::rank::corpus::prepare_lexical(&terms, documents, q, &RetrievalConfig::default())` (`corpus.rs:224`)
  and `LexicalCorpus::score(corpus_size, index)` (`:191`); rank by score alone, no token budget, no
  prose demotion. Recompute the corpus per query over the scope's chunks (doc trees are small); keep
  each chunk's token list cached in the index.
- Literal mode: a query wrapped in double quotes matches the inner text case-sensitively as a
  substring of chunk bodies and headings (`IN_PROGRESS-`, `--strict` survive), ranked by match count.
- Query over 256 chars -> 400 (route); empty -> empty results. Cap 50 hits, `truncated: true` beyond.
- Each hit: path repo-relative (`doc/...`, plan 2's wire path rule), document title, heading path (the chunk's heading and its parents
  from `planning::sections`), anchor (plan 1's GitHub anchor of that heading), snippet (about 160
  chars around the first match, whole chars only, `…` at cut ends), matched terms.
- `search/mod.rs`: `routes()` -> `GET /api/search?q=&scope=` (role Viewer); `install(ctx)` builds the
  index after `files::install` (it needs the `TextStore` from extensions) and registers an
  `on_batch` hook calling `apply`.

## Tests

`search/tests.rs`: scope filter; a done plan and an archived file are found; `"IN_PROGRESS-"`
quoted finds the literal and unquoted does not require it; 60 matching docs give 50 hits and
`truncated`; a summary-store file is never a hit; snippet cuts on char boundaries for a multi-byte
body; after a batch that edits a document the next query sees the new text (`apply`); building
and querying writes nothing under the repository (`git status --porcelain` stays empty); a query of
257 chars is 400. `files/tests/diff.rs`: head diff of a committed then modified file, an untracked file
(`before: null`), a turn diff from pins.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::search::` and
`--lib plan_server::files::tests::diff` pass; clippy clean.
