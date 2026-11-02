# Automatic Knowledge Source Graph Followups
>
> Gaps

>

## Open Items From the Automatic-Knowledge Plan

**Bounded refresh source retention.** `context/refresh/source_graph/layer.rs::read_bytes` uses `context/source_graph/read.rs::SourceRead` through `read_source` or `run_git_stream`. The reader hashes and counts every byte through a fixed 16 KiB buffer while retaining at most `MAX_EXTRACTED_FILE_BYTES`; oversized nodes keep full-file content identity and byte spans without whole-file allocation. Source windows have a separate read path and require a separate bound audit.

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

## Pressure Gaps

> loom pressure smoke and dry-run gaps

## `loom pressure` Known Gaps

### `loom pressure` real-invocation smokes are manual-only

The two end-to-end smokes — Claude `/pressure` actually editing the plan, and Codex `$pressure` writing the `codex-` sidecar — need network + agent auth and are NOT exercised by `loom stage complete`. They are manual release-validation. Automated coverage is dry-run + 10 unit tests (argv, step order, exit classification, path resolution).

### Dry-run argv is not shell-quoted

`pressure::render_dry_run_step` (`commands/pressure/mod.rs`) prints each `claude`/`codex` argv with
`.join(" ")` and no shell quoting, and its output is pinned by tests. A multi-line or spaced
argument runs together, so the printed line cannot be pasted into a shell.
`knowledge bootstrap --dry-run` already quotes through `shell_quote_argv`
(`commands/knowledge/bootstrap/mod.rs`); a shared quoting helper would fix the pressure call sites.

### `git rev-parse --show-toplevel` Duplication

Repo-root resolution is inlined in two places: the merge-side resolution at
`commands/stage/merge/preflight.rs` and the pressure-side one at
`commands/pressure/paths.rs::resolve_repo_root`. Both are below the "extract at 3+" threshold in
conventions.md Import Deduplication — not an active consolidation candidate, but the two copies
could still drift.
