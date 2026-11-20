# E3: edge-quality evaluator, thresholds, CLI, and the operator protocol

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alone. E1 and E2 write the corpora in wave 2 and use your CLI as their
  check.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 14 in
  full, and section 10 (the `--eval-edges` row).

## Goal

`loom::context::eval_edges::evaluate_dir` scores a labelled corpus against the graph
loom builds for it, entirely in memory. Published thresholds are checked by a cargo
test and by `loom map --eval-edges`. The operator protocol for holdout projects and
the four-mode agent comparison is written down.

## Files you own (write)

- new `loom/src/context/eval_edges/mod.rs`, `labels.rs`, `metrics.rs`, `build.rs`
  (in-memory graph), `tests.rs`
- `loom/src/context/mod.rs`: the `pub mod eval_edges;` line only
- new `loom/eval/edge-quality-thresholds.yaml`
- `loom/src/commands/map.rs` and `loom/src/map/views/**`: the `--eval-edges <DIR>`
  flag, its dispatch, and its human/JSON rendering only. `MapArgs` gains `--eval-edges`
  through clap only
- `loom/src/commands/tests_map.rs`: the pinned `require_view` message in
  `map_without_a_view_flag_names_all_available_views` gains `--eval-edges` (a
  test-integrity event the stage disputes once)
- new `doc/source-graph-evaluation.md`

Read-only: the extractor registry, the resolver, `impact_with`, and
`loom/tests/edge_quality_contracts.rs` (frozen). The frozen file pins:

- `evaluate_dir(&Path) -> Result<EdgeQualityReport>` and the report field names;
- `load_thresholds(&Path) -> Result<Thresholds>` and
  `Thresholds::check(&self, &EdgeQualityReport) -> Vec<String>`, which returns one
  message per violation;
- that a corpus directory is found under `tests/fixtures/source/labeled/<dialect>`.

## Steps

1. **`labels.rs`.** Deserialize `labels.yaml` with `serde_yaml` (already a
   dependency), using `#[serde(deny_unknown_fields)]` on every struct. The format is
   design 14.1, plus an optional `syntax_error_files: [path]` list whose entries name a
   file stored with a `.txt` suffix: load it as the path without `.txt`.
2. **`build.rs`: in-memory graph.**
   - Walk `dir` recursively in sorted order, skipping `labels.yaml` and any path whose
     first segment is in `EXCLUDED_ROOTS` (reached as
     `crate::context::refresh::{excluded, EXCLUDED_ROOTS}`).
   - For each file with a dialect (after the `.txt` mapping), `extract_file` gives a
     `FileEntry::from_extraction`.
   - Build a `ResolvedGraph` keyed by forward-slashed paths relative to `dir`, then
     build the view with `crate::context::view::build_cold` (design 12: nothing outside
     `context/view` calls `resolve_graph`) and read the resolved graph from it.
   - Nothing is written anywhere.
3. **`metrics.rs`.** Compute every design 14.2 metric exactly:
   - a reference label matches an edge from that path whose `sites` contain a span on
     the labelled line and whose `symbol` equals the label's symbol or ends with it
     after a `::` or `.` separator (edge symbols keep the written qualifier:
     `util::parse`, `Widget::new`);
   - a missing edge counts as unbound;
   - a ratio whose denominator is 0 is a failure, never a pass;
   - impact uses `impact_with` with `max_depth = d`, the four semantic kinds and
     `follow_candidates: true`.

   `failures` lists one human line per miss, naming path, line, symbol, expected and
   actual.
4. **Thresholds.**
   - `loom/eval/edge-quality-thresholds.yaml` holds the exact values in design 14.2,
     with a header comment stating they were published before any holdout project was
     examined, and the date `2026-09-30`.
   - `load_thresholds` plus `Thresholds::check`. `check` also fails a corpus that lacks
     at least one target label, one external label, one ambiguous label, one impact
     label and one `syntax_error_files` entry.
5. **CLI.**
   - `loom map --eval-edges <DIR> [--json] [--thresholds <FILE>]`. `--json` and
     `--thresholds` are the only other flags. A directory containing `labels.yaml` is
     one corpus; otherwise every child directory with a `labels.yaml` is a corpus.
   - Print one line per dialect with every metric, then the failures (at most 20 per
     dialect, then `... N more`), then always the summary line
     `<n> corpora, <f> failing`.
   - Exit 2 when the directory holds no labelled corpus, 1 when any threshold fails,
     0 otherwise. The default thresholds are `loom/eval/edge-quality-thresholds.yaml`,
     compiled in with `include_str!`, so an installed binary needs no source tree;
     `--thresholds <FILE>` overrides them.
   - `--eval-edges` is a view flag for `require_view`.
   - When `--eval-edges` is the only view, `loom map` never opens the `ContextStore`
     and never calls `ensure_snapshot`: evaluation is pure and in memory, so it runs
     inside a stage sandbox and in any directory. Combined with other views, it
     evaluates in memory and the other views load the graph as usual.
6. **Tests** (`eval_edges/tests.rs`), using temp-dir corpora:
   - the label loader rejects an unknown key;
   - a perfect tiny corpus scores 1.0 across the board;
   - a wrong high-confidence bind counts in `false_high_confidence`;
   - an `ambiguous` label is correct only when the candidates match;
   - a `.txt` syntax-error file loads as its real extension;
   - a label symbol `parse` matches an edge written `util::parse` and `obj.parse`, but
     not `reparse`;
   - a corpus with empty declarations, references and impact lists (`src/lib.rs` =
     `fn a() {}`) has 0/0 ratios that fail, and `Thresholds::check` returns a non-empty
     list for it;
   - the exit-code mapping (2 for no corpus, 1 for a failing threshold, 0 otherwise)
     and the `<n> corpora, <f> failing` line, through the function `commands/map.rs`
     exposes for them. A binary test would spawn loom only through
     `helpers::loom_cmd()` (stage `map-api-freshness`'s CONTRACT SURFACE).
7. **Operator protocol** (`doc/source-graph-evaluation.md`). Write design 14.3 as a
   runnable procedure, under exactly these headings: `## Census`,
   `## Holdout labelling`, `## Agent-task comparison`, `## Metrics and stratification`
   and `## Release rule`. The stage's after check greps for `## Release rule`.
   - `## Census`: census commands (`loom map --census --root <checkout>...`);
   - `## Holdout labelling`: how to label a holdout project (copy the corpus format;
     label before running loom), and `loom map --eval-edges <holdout-dir>`;
   - `## Agent-task comparison`: the four-mode comparison, with the task-set YAML
     format, which you define here (task id, repo, revision, prompt, class, expected
     definitions/sites, allowable ambiguous answers), plus blinding, randomization and
     repeat count;
   - `## Metrics and stratification`: the metrics recorded, and stratification by
     language, task class, coverage tier and freshness;
   - `## Release rule`: thresholds are published before holdout results, and the
     release rule.

   Use fenced code blocks with languages. No history, no dates beyond the thresholds'
   publication date.

## Traps

- `serde_yaml` is 0.9: `deny_unknown_fields` works on structs, not on externally
  tagged enums. Model `expect` as a struct with three optional fields and validate
  that exactly one is set.
- The evaluator must never touch `.loom/`. A contract runs it inside the stage
  sandbox.
- Every file under 400 lines, and every function under 50.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::eval_edges:: 2>&1 | tail -15`

## Report

Report the files changed, the final report fields, and the metrics for any corpus E1
or E2 has already written.
