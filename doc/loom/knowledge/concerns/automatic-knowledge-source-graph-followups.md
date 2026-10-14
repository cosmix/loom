# Automatic Knowledge Source Graph Followups

> Knowledge followups

## Open Items From the Automatic-Knowledge Plan

**Whole-file read ahead of the size cap.** `read_bytes` (`context/refresh/source_graph/layer.rs`)
reads every tracked file with `read_bounded(.., usize::MAX)` before `extract_file` applies the
512 KiB `MAX_EXTRACTED_FILE_BYTES` cap. The cap bounds parsing but not allocation, so the daemon
spikes to the size of the largest tracked blob on every reconcile. `FileExtraction::file_level`
(`context/extract/mod.rs`) needs the bytes to build an oversized file node's span and its
whole-file `content_hash`. Bounding the read therefore means changing that node's span and hash
semantics, or threading a streamed line count through the extractor API. Peak is one file at a
time, and `EXCLUDED_ROOTS` skips `target/` and `node_modules/`, so the realistic worst case is a
transient spike.

**Three production-dead `KnowledgeDir` methods.** `append`, `read_index` and the
`KnowledgeFile`-keyed `replace_section` (`fs/knowledge/dir.rs`) have no non-test callers.
`read` is live (`commands/knowledge/bootstrap/mod.rs`), and so are `read_target` and
`replace_section_target` (`commands/knowledge/mod.rs`). All are `pub` on a `pub` type, so clippy
cannot flag them, and `tests_dir.rs` and `tests_dir_bugfixes.rs` exercise the dead three against
each other. Settle them in one change: delete the methods and their tests together, or wire them
to a real consumer. A dead-accessor list holds for one revision only; re-check it before acting.

**Plan-key normalisation on the writer side.** `delivery::plan_key_from` files a blank or absent
`plan_id` under `"default"`. `MergeLifecycle::plan_id` (`orchestrator/merge_lifecycle.rs`) reads
the raw config `plan_id` instead: an absent id skips overlay reconcile and discard, and a blank
one is used as is. Silent by construction; see `mistakes/writer-reader-address.md`.

**`LOOM_PERMISSIONS_WORKTREE` has no consumer.** The constant (`fs/permissions/constants.rs`)
lists worktree-shaped `Read(...)` rules and `Bash(loom *)`, but only its re-export in
`fs/permissions/mod.rs` and `constants_tests.rs` reference it; no settings writer uses it. Wire it
into the worktree settings or delete it with its tests.

## Stopwording Drops the Words a Natural-Language Source-Graph Question Is Asked In

**Observed:** `loom knowledge context --query "how is the source graph refreshed after a commit"`
returns low-confidence chunks unrelated to the question;
`architecture/source-graph.md#lifecycle-who-builds-it-and-when`, the section that answers it, is
not in the pack. Corpus-derived stopwording drops `source`, `graph` and `commit` as ubiquitous. The
same question phrased with a symbol (`reconcile_base`) ranks the right function first.

**Why it matters:** the knowledge-first doctrine tells every session to pull a question instead of
reading; a pull that misses on the project's own vocabulary sends the reader back to paging files.

**Where to look:** `context/rank/corpus/stopwords.rs` (the corpus-derived stopword threshold and
its rescue floor, described in
`architecture/context-retrieval-corpus.md#corpus-derived-query-stopwording-with-a-rescue-floor`),
and `loom/eval/retrieval-cases.yaml`. Its only natural-language case asks about hook
configuration, so `loom knowledge eval` does not measure this gap. A first step is adding a
lifecycle case so the gap is measured before the threshold is tuned.
