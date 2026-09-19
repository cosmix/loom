# W1 — retrieval precision and brief abstention

Tier: opus (`loom-senior-software-engineer`). Read `../common.md` first.

## Goal

Knowledge retrieval stops discarding the query's own domain words, the per-prompt brief abstains
when it has nothing relevant, and the eval can fail on precision. Evidence: report section 4.3.
Ten live queries lost `stage`, `merge`, `acceptance`, `worktree`, `context`, `codex`, `plan`,
`session` to corpus stopwording; `loom knowledge eval` reports hit@5 0.89, p@5 0.19, abstention
0 of 2; 60% of injected briefs carry a low-relevance line.

There is no embedding lane in this subsystem (`loom/src/context/mod.rs:6`). The header word
`Semantic` reports source-graph freshness only. Do not add one.

## Files you own (write)

- `loom/src/context/rank/corpus.rs`, `context/rank/corpus/stopwords.rs`, `context/rank.rs`,
  `context/lexical_index.rs`, and `context/tests/` files for them
- `loom/src/commands/hook/user_prompt_compose.rs`, `user_prompt.rs`, `user_prompt_attachments.rs`
- `loom/src/commands/knowledge/eval.rs`, `commands/knowledge/eval/`, `commands/knowledge/tests_eval.rs`
  and `loom/eval/retrieval-cases.yaml`

Do not edit `loom/src/fs/knowledge/**` (another stage owns it) or `context/schema*.rs`.

## Where things are

- `partition_terms` — `stopwords.rs:59-97`; sole caller `corpus.rs::assemble` (316-340). A term
  is dropped unless backticked or `df <= ubiquity_floor` (`stopwords.rs:233-235`). Rescue floor:
  `RESCUE_LIMIT=3`, `RESCUE_QUERY_MIN_TERMS=4`. At the call site only query terms, document
  frequencies, corpus size, raw query and config are in scope.
- Fusion: `fuse.rs` tier-2 scores are reciprocal-rank values near 1/60 and are not comparable
  with tier-1 scores (`fuse.rs:41-53`). Leave fusion alone.
- Prompt-hook floor: `clears_emit_floor` / `clears_item_floor`
  (`user_prompt_compose.rs:164-172`): an exact rung or `matched_term_count >=
  min_knowledge_terms` (default 2). Abstain reasons at `compose_with_reason` (55-90).
- `prose_demotion` — `rank.rs:97-103`, additive `knowledge_curated_prior` (default 5.0), clamp
  at zero is required (`rank.rs:77-87`).
- A file with no `##` heading becomes one chunk with `heading: ""`
  (`fs/knowledge/chunker.rs:162-167`); `prose.rs::ProseSources::chunks` (203-232) admits it.
- Eval: `eval/metrics.rs::score_case` (45-70), `eval/report.rs::exit_reason` (119-158) fails on
  any abstention failure; `precision_floor` defaults to 0.0 (`eval/cases.rs:59`). The two abstain
  cases are `conversational-correction-abstains` and `farewell-abstains`
  (`loom/eval/retrieval-cases.yaml:80-86`); both emit today.

## Steps

1. Protected terms. Build a `BTreeSet<String>` of lowercase tokens from knowledge chunk headings,
   annotate aliases, and source-graph symbol names, once per index build, and thread it into
   `assemble` and `partition_terms`. A query term in the set is never dropped as
   corpus-ubiquitous. Decide where the set is built from what `LexicalIndex` construction already
   has in hand; do not re-walk the tree per query. Keep the `--explain` dropped-terms line
   (`commands/knowledge/context.rs:219-221` is read-only for you; it prints what you return).
2. Stub chunks. In ranking candidacy, exclude a chunk whose heading is empty and whose body has
   fewer than 3 non-blank lines.
3. Abstention. Make both abstain cases pass without lowering recall on the existing positive
   cases: a prompt whose surviving terms are all protected-by-rescue only, or which matches no
   heading token, must not clear the emit floor. Remove the `Omitted: N weaker matches` line from
   the per-prompt brief (keep it in `loom knowledge context` output).
4. Precision floor. Run the eval tests, record p@5 before and after in your report, then set
   `precision_floor` in `retrieval-cases.yaml` to the after value multiplied by 0.9 and rounded
   down to two decimals. It must be greater than 0.19. If you cannot get above 0.19, stop and
   report what you measured; do not set a floor the suite fails.

## Traps

- `surviving` keeps repeated terms because BM25 sums them; the dropped list is display only.
- Eval budget constants in `eval/cases.rs:7,9` mirror `RetrievalConfig` defaults by hand.
- `would_emit` in the eval reuses the prompt-hook floor; stage and worker briefs are not
  floor-gated and stay that way in this plan.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib context::` — run once.
Add tests: a protected heading term survives a corpus where it is ubiquitous; a stub chunk is
not a candidate; both abstain cases abstain; one positive case still emits.
