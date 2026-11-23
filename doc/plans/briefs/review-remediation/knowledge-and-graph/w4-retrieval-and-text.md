# knowledge-and-graph / w4-retrieval-and-text: eval capture, eval messages, rescue filter, skill matcher, frontmatter BOM, grammar pins

Tier: sonnet. Start after w2 has reported (the C6 fixture test needs w2's enumeration change on disk).
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/knowledge-graph.md`, sections C5, C6, CR-49, CR-50, CR-52, CR-15.

## Findings covered

- C5: `loom knowledge eval` runs `retrieve_for_stage` per case, so every case refreshes the catalog, loads the graph view and can see a different revision. It captures once instead.
- C6, two parts: the eval failure line prints scores with `{:.2}` (`0.40 < 0.40`); no test shows that a generated bundle's symbol no longer makes the farewell case emit.
- CR-49 (function-word filter only): `rescue_rarest` can put English function words back into a thin query.
- CR-50: the skill trigger matcher keeps punctuation on prompt words (`auth,` never equals `auth`) and matches multi-word triggers as substrings (`go mod` inside `django model`).
- CR-52 (byte-order mark only): a `SKILL.md` or knowledge file starting with U+FEFF fails frontmatter parsing.
- CR-15: the PHP extractor stamps grammar version `0.24.2` against the `=0.25.1` pin; nothing ties extractor versions to the manifest.
Left out on purpose: the `min_knowledge_terms` tuning for the correction case (needs a measured run against the live corpus with a rebuilt binary), CR-49's length filter (it would delete the rescue of rare 1-2 character identifiers), CR-52's manifest-list divergence and unbounded `SKILL.md` read.

## Files

Own (write):
- `loom/src/context/retrieve.rs`, `loom/src/context/retrieve/capture.rs` (new), `loom/src/context/retrieve/channels/tests.rs`
- `loom/src/commands/knowledge/eval.rs`, `loom/src/commands/knowledge/eval/report.rs`, `loom/src/commands/knowledge/eval/tests_refresh.rs`, `loom/src/commands/knowledge/eval/tests_abstention.rs`, `loom/src/commands/knowledge/eval/tests_precision_format.rs` (new)
- `loom/src/context/rank/corpus/stopwords.rs`
- `loom/src/skills/matcher.rs`
- `loom/src/parser/frontmatter.rs`
- `loom/src/context/extract/php.rs`, `loom/src/context/extract/dialect.rs`, `loom/src/context/extract/tests.rs`

Read only: `loom/src/context/retrieve/graph.rs` (`GraphLoad`, `apply_freshness`), `loom/src/context/retrieve/request.rs`, `loom/src/commands/knowledge/eval/cases.rs`, `loom/src/commands/knowledge/eval/tests_prompt_mode.rs` (copy its `case()` helper), `loom/src/context/refresh/snapshot.rs`.

Frozen (contract tests, never edit): `loom/src/parser/frontmatter_bom_contract_tests.rs`, `loom/src/skills/matcher_contract_tests.rs`.

## Changes

### 1. C5: capture once, retrieve per case

`retrieve.rs` is 359 lines: the new code goes in `loom/src/context/retrieve/capture.rs`, declared `mod capture;` in `retrieve.rs` with `pub(crate) use capture::{capture, retrieve_with, Captured};`.

`capture.rs` (a child of `retrieve`, so it reaches the private `resolve_catalog`, `ResolvedRoots`, `graph::GraphLoad`, `request`, `channels`, `windows`) holds:
- `pub(crate) struct Captured { roots: ResolvedRoots, config: RetrievalConfig, catalog: Catalog, state: StoreState, graph_load: GraphLoad }` with `pub(crate) fn structural_revision(&self) -> &str` (`state.structural.revision`) and `pub(crate) fn semantic_revision(&self) -> &str` (`state.semantic.revision`).
- `pub(crate) fn capture(query: &StageQuery) -> Result<Captured>`: today's first half of `retrieve_for_stage`: `resolve_roots_optional(&query.work_dir_hint)?`, `RetrievalConfig::load(&roots.main_project_root)`, `resolve_catalog(&roots.store, roots.knowledge_root.as_deref())?`, `graph::load_resolved_graph(&query.work_dir_hint, &roots.store, &state.semantic.revision, &query.overlay)`, then `graph_load.apply_freshness(&mut state)` once.
- `pub(crate) fn retrieve_with(captured: &Captured, query: &StageQuery, budget_tokens: usize) -> Result<ContextPack>`: today's second half: `request::check_require_ids`, `channels::rank_stage_channels(query, &captured.catalog, graph, &captured.config, captured.roots.store.root())`, `fuse`, `request::build_pack_request(query, budget_tokens, captured.state.clone(), ranked.dropped_terms, ranked.surviving_terms, captured.graph_load.degraded.clone())`, `pack`, `windows::attach_windows`. `graph` is `captured.graph_load.graph.as_ref()`.
`retrieve_for_stage(query, budget)` becomes `let captured = capture(query)?; retrieve_with(&captured, query, budget)`; its signature, doc and every other caller (`commands/knowledge/context.rs`, `commands/hook/user_prompt.rs`, `orchestrator/signals/retrieval.rs`, the worker brief) stay unchanged. Move now-unused imports out of `retrieve.rs`. Keep `take_evaluate_state_call_count` and the `EVALUATE_STATE_CALLS` increment in `evaluate_state` as they are.

`eval.rs`: extract the body of `eval()` between `refresh_source_graph` and `aggregate` into `fn run_cases(cases: &[EvalCase], budget_tokens: Option<usize>, work_dir_hint: &Path, config: &RetrievalConfig) -> Result<(Vec<CaseResult>, Corpus)>`. It calls `capture(&StageQuery::new(work_dir_hint, String::new()))` once (case queries differ only in text, required ids and budget; every case uses the `Local` overlay, the default scope and the default surface), with the context `eval retrieval setup failed`, and `run_case` takes `&Captured` and calls `retrieve_with(captured, &query, budget)`. `Corpus` is `pub(super) struct Corpus { pub structural: String, pub semantic: String }` in `report.rs`, filled from `structural_revision()` and `semantic_revision()`. `print_human` gains a `corpus: &Corpus` parameter and prints `Corpus: structural=<first 8> semantic=<first 8>` (use `crate::context::refresh::short_revision`) after the case lines; `print_json` gains the same parameter and two keys `"structural_revision"` and `"semantic_revision"` (full strings).

### 2. C6: unrounded scores in the failure line (`eval/report.rs`, `exit_reason`)

Format `hit_rate_at_5`, `pass_floor`, `precision_at_5` and `precision_floor` with `{}` instead of `{:.2}` (f32 `Display` is the shortest round-trip form: `0.3958333 < 0.4`). The labels stay `hit@5` and `precision@5` so the existing `.contains("hit@5")` assertions hold.

### 3. CR-49: no function word is rescued (`rank/corpus/stopwords.rs`, `rescue_rarest`)

Add `.filter(|term| !is_function_word(term))` before `.collect()` of `rarest` (`is_function_word(term: &str) -> bool` is in the same file). Do not add a length filter. Update the doc comment of `rescue_rarest` by one sentence.

### 4. CR-50: punctuation and whole-word phrases (`skills/matcher.rs`)

- `normalize_text` keeps its public name and signature. After lowercasing and mapping `_` and `-` to spaces, split on whitespace, and clean each word: strip leading characters from the edge set (comma, semicolon, colon, exclamation mark, question mark, parentheses, square brackets, braces, double quote, single quote, backtick) and trailing characters from that set plus the full stop; drop words that become empty; join with single spaces. Interior punctuation and a leading `.` stay, so the shipped triggers `c++`, `c#`, `.net`, `node.js`, `node:test`, `std::span`, `ci/cd`, `a/b test`, `package.json`, `openapi 3.1`, `asp.net core` still match themselves. Trigger normalisation (`skills/index.rs`, `add_skill`) goes through the same function, so both sides agree; the function is idempotent.
- Phrase match: for a trigger of two or more words, match when `words.windows(n)` contains a window equal to the trigger's words (compare as `&str`); drop the `normalized.contains(..)` substring test. Single-word triggers keep the exact word comparison. Scores stay 2.0 for a phrase and 1.0 for a word. This also means a phrase no longer matches inside a longer word (`refresh token` does not match `refresh tokens`); that is intended.
- `match_skills` has a ledger row (`function src/skills/matcher.rs match_skills 55` in `loom/maintainability-baseline.txt`, not yours). Move the per-trigger decision into a private `fn trigger_score(words: &[String], trigger_words: &[&str]) -> Option<f32>` so `match_skills` ends under 50 lines, and report its new length; the main agent removes the ledger row.

### 5. CR-52: byte-order mark (`parser/frontmatter.rs`, `split_frontmatter_raw`)

First statement: `let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);` and use that shadowed slice for everything after (the returned slices borrow it, lifetimes unchanged). One BOM only. `extract_frontmatter_raw` delegates and is covered; skills (`skills/index.rs`), knowledge, handoff watch and plans all go through this function.

### 6. CR-15: grammar version pins

`context/extract/php.rs` `identity()`: `grammar_version: "0.25.1"`. `context/extract/dialect.rs`: the php row's `grammar` becomes `"tree-sitter-php 0.25.1 (php)"`. Entries stamped `php:0.24.2` re-extract once; no migration code.

## Interfaces shared with other workers

None. w2's enumeration change (graph skips `linguist-generated` paths) must be on disk before the C6 fixture test passes.

## Tests

- C5, `eval/tests_refresh.rs` (`#[serial]`): `the_eval_captures_the_corpus_once`: a `TempDir` with `.loom/work` and no knowledge directory (so the capture takes the `evaluate_state` branch that `take_evaluate_state_call_count` counts); three `EvalCase` values built with a copy of `tests_prompt_mode.rs::case`; `take_evaluate_state_call_count()` to zero, `run_cases(..)`, assert the count is 1. Control in the same test: three `retrieve_for_stage` calls count 3. Second test `later_cases_score_the_captured_revision`: a project with `doc/loom/knowledge/conventions.md` (model: `tests_abstention.rs::project`), `capture`, overwrite the file with different text, `retrieve_with` again; the pack's `structural_freshness.revision` equals `captured.structural_revision()` and the pack text has no word from the new text. Both fail today (no `capture`).
- C6 format, `eval/tests_precision_format.rs` (new; add `#[cfg(test)] #[path = "eval/tests_precision_format.rs"] mod tests_precision_format;` to `eval.rs`): `exit_reason(&Aggregates { hit_rate_at_5: 1.0, precision_at_5: 0.3958333, ..Aggregates::default() }, 0.5, 0.4)` contains `0.3958333` and `precision_floor 0.4`. Fails today (`0.40 < 0.40`). `tests_eval.rs` is 397 lines: do not add there.
- C6 fixture, `eval/tests_abstention.rs`, test `a_generated_bundle_symbol_does_not_make_the_farewell_emit` (`#[serial]`): a git repo from `crate::test_git::TestRepo` (`commit_file` for each file) holding the two knowledge files `project()` writes, `web/dist/assets/flow.js` with a minified `function iS(e){return e}`, plus `.loom/work` created and `crate::git::worktree::capture_main_identity(root, &work)` as in `tests_source_graph/mod.rs`; build the snapshot with `super::ensure_local_snapshot(root)`. Arm A: no `.gitattributes`: `emits(root, "farewell-abstains")` is true (the bundle's symbol reaches the emit floor as an exact symbol; if the fixture does not reproduce it, vary the bundle symbols until it does and say so, or report). Arm B: a second repo with `.gitattributes` line `web/dist/** linguist-generated` committed before the snapshot: `emits` is false. Arm B fails today.
- CR-49, `retrieve/channels/tests.rs`, test `a_function_word_between_the_drop_and_rescue_ceilings_is_not_rescued`: 24 chunks (helper `chunk`), `which`, `would`, `should` and `latency` each in 6 chunks, `zebra` in one, no heading text carrying them; with default config the ubiquity floor is `max(24 * 0.10, 5) = 5` and the rescue ceiling `24 * 0.25 = 6`; query `zebra which would should latency`; `rank_channels(&[Channel::Knowledge], ..)` gives `surviving_terms == ["zebra", "latency"]` and `dropped_terms == ["which", "would", "should"]`. Verify those numbers against `ubiquity_floor` before trusting them; the test must be red on the old code (it rescues `latency`, `should`, `which`).
- CR-50, `matcher.rs` `mod tests`: `fix auth, now` matches trigger `auth`; `django model` does not match `go mod`; `port the c++ and node.js, code` matches triggers `c++` and `node.js`; `(refresh token)` matches `refresh token`; `refresh tokens` does not. The first two fail today.
- CR-52, `parser/frontmatter.rs` `mod tests`: `split_frontmatter_raw("\u{FEFF}---\nname: x\n---\nbody")` is `Ok(("name: x\n", "body"))`; a BOM that is not first is not stripped. Fails today.
- CR-15, `extract/tests.rs`: `grammar_versions_match_the_manifest_pins` over `DIALECTS` (crate name and version are the first two words of `grammar`; read the pin with `toml_edit` from `include_str!("../../../Cargo.toml")`, `dependencies.<crate>.version` without the leading `=`), and a `#[cfg(feature = "source-graph")]` test over `registry()` comparing `cache_identity().grammar_version` with the pin of the crate named in `dialect().grammar`. Both fail today for php.
- Frozen contracts to turn green: `byte-order-mark-frontmatter-parses`, `skill-triggers-match-whole-words`.

## Proof

From `loom/`: `cargo check --all-targets`; `cargo test --lib context::retrieve::`; `cargo test --lib context::rank::`; `cargo test --lib commands::knowledge::eval`; `cargo test --lib skills::`; `cargo test --lib parser::`; `cargo test --lib context::extract::`; `cargo fmt` on the files you wrote. Run `skills::` and `commands::knowledge::` to the end and report any existing test the matcher change breaks.

## Traps

- `take_evaluate_state_call_count` only moves when there is no knowledge root; with one, `resolve_catalog` goes through `refresh_snapshot`, whose pass counter is private to `refresh.rs` (w1's file). Hence the no-knowledge project in the C5 test.
- `apply_freshness` mutates the state: call it once in `capture`, never per case.
- Stripping every `.` or `+` from words would turn `c++` into `c`; only the listed edge characters go, and only at word edges.
- Do not edit `loom/maintainability-baseline.txt`; report the new `match_skills` length.
- `tests_eval.rs` and `index_tests.rs` are not yours except that their tests must keep passing.
- Report existing tests you changed (expected: none) and any matcher-related failure in `skills::`.
