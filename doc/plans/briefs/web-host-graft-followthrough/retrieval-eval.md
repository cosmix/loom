# Natural-language retrieval: measured followthrough

## Outcome and scope

Implement the report's recommendation to measure bounded retrieval quality with
held-out questions, then fix the demonstrated loss of topic evidence in the
existing lexical pipeline. This is a local, deterministic correctness project:
no embedding service, semantic enrichment, dependency addition, paid task run,
general search rewrite, or claims about end-to-end token savings.

The worker must deliver working retrieval and hermetic regression coverage.
Merely adding the failing question to the live evaluation file, increasing a
stopword ratio, or declaring an aggregate score acceptable does not complete it.

## What exists, and what is missing

- `context/rank/corpus/stopwords.rs::partition_terms` already computes corpus
  stopwords, protects backticks, and rescues up to three rare dropped terms.
  `over_stopworded` triggers for zero survivors or fewer distinct survivors than
  `min_knowledge_terms` on a sufficiently long question. It counts surviving
  strings, including zero-document-frequency terms, rather than whether those
  terms preserve useful candidate coverage. `rescue_rarest` also considers
  zero-frequency dropped terms. These are concrete diagnostic targets, not
  proof that changing either alone fixes the observed question.
- `rank/corpus.rs::assemble` is the single partition point for scan and cached
  corpora. `LexicalCorpus::score` feeds both knowledge and source ranking;
  `surviving_terms` additionally feeds source candidacy. Corpus frequencies must
  remain complete because the exact-match gate also consumes them.
- `rank.rs::score_chunk` admits a lexical candidate with any surviving match;
  `rank_source.rs::score_node` applies additional source candidacy checks.
  `hook/user_prompt_compose.rs::clears_item_floor` then needs an exact rung or
  enough distinct matches. Candidate presence, top-five rank, packed presence,
  and prompt eligibility are separate outcomes.
- `commands/knowledge/eval.rs::run_case` already calls the production retrieval
  entry point. Its case schema and metrics cover relevance, forbidden results,
  required IDs, abstention, rendered cost, true precision@5, and MRR. Do not
  rebuild these metrics or claim the current evaluation has only symbol cases:
  `loom/eval/retrieval-cases.yaml` already contains natural-language sandbox,
  phantom-merge, identity, conversational, and prose/symbol collision cases.
- The CLI evaluation deliberately reads the live index. Its result depends on
  current knowledge and cannot be the reproducible implementation gate.
  Existing synthetic rescue tests and temporary-project retrieval tests provide
  the appropriate hermetic construction patterns.

Read-only reproduction on 2026-09-13:

```text
loom knowledge context --query 'how is the source graph refreshed after a commit' --budget-tokens 1500 --explain
```

The pack dropped `source` and `graph`, kept `commit`, returned seven items, and
omitted the lifecycle answer. Its first result was the concern describing this
failure and quoting the exact question; unrelated retry and hook material also
appeared. The concern's historical assertion that evaluation is symbol-only is
outdated. Treat the query quotation as a contamination adversary, not a correct
answer. Do not use this live run as an acceptance criterion.

## Implementation sequence

1. Freeze a small fixture corpus and labels before changing the ranking policy.
   Use fresh temporary roots and in-memory graphs/cache directories; never read
   this checkout's knowledge, local overlay, or shared `.loom/work` at test time.
   Curate a minimum of twelve questions covering at least four independent
   concepts: graph refresh, sandbox settings, session identity, and delivery
   suppression. Include natural-language paraphrases with no paths, symbols,
   backticks, or verbatim answer headings. Reserve at least one paraphrase per
   concept as held-out evaluation rather than selecting policy on every case.
   Include a frozen extract of the actual lifecycle answer with its original
   heading and vocabulary; do not rewrite it to include query terms. The current
   lexical tokenizer does not stem `refreshed` to `refresh`. Preserve that real
   mismatch in the regression. Keep tuning and validation partitions distinct.
2. Include controlled corpus growth: add ordinary design prose using the same
   project vocabulary, but with different answers. Also inject a concern-like
   distractor that quotes each diagnostic question without answering it. Freeze
   labels based on answer content, not on whatever the current ranker returns.
   Require the substantive answer in the packed top five with and without these
   distractors. The quoted concern may be supplementary, but cannot satisfy the
   answer assertion. A question should fail when only its quotation is returned.
3. Instrument the fixture assertions at partition, candidate, rank, pack, and
   eligibility boundaries to identify the first failing boundary. Reuse the
   existing ranking APIs and diagnostics; do not add a permanent telemetry
   subsystem. Capture baseline failures before implementation.
4. Implement the smallest general rule supported by those failures in the
   existing corpus/rank machinery. First remove any zero-frequency terms from
   the evidence used to declare rescue unnecessary or to spend rescue capacity,
   while preserving explicit/backticked lookups. This addresses a demonstrable
   weakness of the current count-based rule but is insufficient by itself if
   unrelated in-vocabulary survivors still suppress the subject of the query.
5. Where fixtures demonstrate that remaining gap, use bounded lexical topic
   coverage to recover candidates: evaluate corroborated adjacent query terms
   against actual corpus/title evidence rather than globally readmitting common
   words. Keep a fixed bounded rescue/candidate budget and deterministic ties;
   preserve corpus-wide document frequencies and source exact-match gating.
   A phrase/coverage signal must remain lexical evidence and cannot award
   `ExactSymbol` to ordinary prose. Sol chooses and records the precise rule
   from the frozen failures; do not hardcode `source graph`, lifecycle IDs,
   project paths, question strings, or a list of privileged knowledge topics.
   Design checkpoint: before implementing this fallback, return the proposed
   predicate, tie-breaking and resource bound to the orchestrator for review.
   Use token-adjacent query pairs corroborated by heading/path/topic metadata
   already held with knowledge chunks; cached postings carry frequencies, not
   phrase positions. Do not infer phrase evidence from those postings or load
   source bodies to manufacture it. The orchestrator settles the precise rule
   within this territory before edits continue; this requires no user approval.
   Freeze policy/configuration on the tuning partition before running held-out
   cases. If later tuning is necessary, report them as validation cases rather
   than claim independent held-out performance. Never change labels to pass.
6. Check whether candidate recovery actually survives ordering, fusion, packing,
   and prompt admission. Adjust only the measured lexical boundary necessary
   to meet the fixture contract. Retain existing packing and emit-floor rules;
   do not lower the floor to conceal a ranking failure. If a proposed rule
   breaks collision/abstention cases, correct the policy before shipping it.
7. Add the real lifecycle question and at least one independently worded
   paraphrase to the optional live evaluation cases, using IDs established from
   the authoritative answer section. Explain that labels may identify a known
   answer which the old ranker misses; the old file comment saying expected IDs
   must already have been returned is inappropriate for a recall regression.

## Ownership and files

One Sol worker owns algorithm selection, implementation and regression tests in
this bounded territory. It does not spawn Terra/Luna workers or assign overlapping
files; other plan workers handle routine implementation and simple tests elsewhere.
All implementation occurs during plan execution, not while authoring this brief.

Expected production edits:

- `loom/src/context/rank/corpus/stopwords.rs`: partition/rescue evidence.
- `loom/src/context/rank/corpus.rs`: shared scan/index corpus plumbing, only if
  coverage evidence needs document access beyond current frequency inputs.
- `loom/src/context/rank.rs`: knowledge lexical candidacy/order only if measured
  necessary. This file is already close to 400 lines; factor a focused child
  module instead of enlarging it past the limit.
- New `loom/src/context/rank/corpus/topic_coverage.rs` for bounded coverage if
  measured failures require it; no unlisted helper-file territory.
- `loom/eval/retrieval-cases.yaml`: optional live regression labels and corrected
  label policy comment; existing cases remain.

Expected tests:

- New `loom/src/context/tests/retrieval_natural_language.rs`: hermetic driver,
  candidate-versus-pack assertions and cold/warm comparisons.
- New `loom/src/context/tests/retrieval_natural_language_fixtures.rs`: frozen
  corpus, independent question/answer labels, growth/contamination variants.
- `loom/src/context/tests/mod.rs`: register those modules.
- `loom/src/context/tests/rank_stopwords_rescue.rs`: focused zero-frequency and
  rescued-evidence regressions; place overflow in the owned new natural-language
  module rather than creating an unassigned file.

Read-only consumers to inspect before edits:
`context/rank_source.rs::score_nodes/score_node`,
`context/rank_source/candidacy.rs`, `context/lexical/evidence.rs`,
`commands/hook/user_prompt_compose.rs::clears_item_floor`,
`commands/knowledge/eval/{cases,metrics,report}.rs`.
Any need to change their public contracts requires explaining the measured gap
and updating the assigned file scope before beginning that expansion.

## Verification contract

- Each labelled positive has its actual answer in the packed top five at the
  normal 1500-token prompt budget and clears prompt delivery eligibility.
  Do not inject expected answer IDs through `StageQuery.required_ids`: that
  forces inclusion and makes recall success tautological. Assert expected IDs
  independently after natural retrieval. Required-item tests remain separate.
- Every held-out and corpus-growth positive passes individually; an aggregate
  score cannot excuse one concept failing. Run the contamination variant both
  with and without the quoted-question distractor.
- Negative fixtures retain abstention for conversational/stopword-only queries
  and reject prose words promoted to exact source-symbol matches. Preserve
  short precise lookup, backtick, repetition and above-ceiling protections.
- Scanned, cache-miss and cache-hit paths agree on candidate IDs, ordering,
  scores, term diagnostics and packed answer membership. Assert a nonempty
  expected answer before comparing results, so three empty runs cannot pass.
- Budget and omission diagnostics remain truthful. Assertions use existing
  rendered-cost accounting rather than equating candidate estimates with the
  final rendered wrapper cost.
- Orchestrator verification runs the new hermetic test module and existing
  stopword/source-candidacy/index-parity/prompt-admission regression families.
  Workers do not run the live-index evaluation as their acceptance gate.
- Publish fixture baseline/final outcomes, including failures, and the rationale
  for the chosen lexical rule. Synthetic cases establish regression resistance,
  not statistically demonstrated task quality, token savings, or latency p95.

## Report mapping

This implements a bounded portion of the report's Evaluation plan, Task set,
Retrieval and knowledge quality, and initial correctness gates. Deterministic
ranking, source graph retrieval, and evaluation metrics already shipped; the
new work is reproducible natural-language answer recall under realistic corpus
growth and query-quotation contamination. Cross-project agent task trials,
semantic summaries, embeddings, and paid savings comparisons remain deferred.
