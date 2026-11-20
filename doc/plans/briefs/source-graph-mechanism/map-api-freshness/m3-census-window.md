# M3: portfolio census and source windows

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alongside M1 and M2. M4 wires `--census` and `--window` in wave 2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` sections 5.1,
  5.3, 10.1 and 11 in full.

## Goal

Two library modules with exact APIs:

- `context::census`, which classifies every tracked file of one or more checkouts and
  reports coverage by files and by bytes, with exclusions shown beside the
  denominators;
- `context::window`, which returns the exact source of a node or a call site after
  verifying the file still matches the snapshot.

## Files you own (write)

- new `loom/src/context/census/mod.rs`, `census/classify.rs`,
  `census/subprojects.rs`, `census/report.rs` (the `Display` and `Serialize` shapes),
  `census/tests.rs`
- new `loom/src/context/window.rs`, `loom/src/context/window_tests.rs`
- `loom/src/context/mod.rs`: add `pub mod census;` and `pub mod window;` only
- `loom/src/context/refresh/source_graph.rs`: change `EXCLUDED_ROOTS` and `excluded`
  from `pub(super)` to `pub(crate)` only (one line each). `mod source_graph;` is
  private in `refresh.rs`, so M2 (who owns `refresh.rs` in this wave) adds
  `excluded, EXCLUDED_ROOTS` to the existing `pub(crate) use source_graph::{...}` line
  there; you import them as `crate::context::refresh::{excluded, EXCLUDED_ROOTS}`.

Read-only: `loom/src/git/runner.rs` (`run_git`, `NO_HOOKS_ARGS`; see step 1 for which
call uses what), the extractor registry, and `loom/tests/map_api_contracts.rs`
(frozen; its census contract pins
`totals.{eligible,vendored,generated,unsupported}.files`, and its scale contract uses
paths of about 800 bytes each).

## API (exact)

```rust
pub struct CensusOptions { pub roots: Vec<PathBuf> }
pub struct CensusReport { pub roots: Vec<RootCensus>, pub totals: ClassTotals }   // Serialize + Display
pub struct ClassTotals { pub eligible: Count, pub excluded: Count, pub vendored: Count, pub generated: Count, pub unsupported: Count }
pub struct Count { pub files: u64, pub bytes: u64 }
pub fn run(options: &CensusOptions, current: Option<(&Path, &ResolvedGraph)>) -> Result<CensusReport>;
```

- `current` is the project root and resolved graph of the checkout `loom map` runs in.
  A root equal to it uses that graph for coverage; any other root is extracted in
  memory (design 10.1).
- JSON serialization adds `"schema": "loom-census/1"` at the top.

`window.rs`: exactly design 11.

## Steps

1. **Classify.** `classify.rs` classifies each path from `git ls-files -s -z` in the
   order of design 10.1.
   - Enumerate with `git ls-files -s -z` through `git::runner::run_git` (untrimmed
     bytes), not `run_git_checked`, and read each entry's mode. A symlink (`120000`) or
     a gitlink (`160000`) is `unsupported` and never followed; the graph builder
     already skips them (`refresh/source_graph/enumerate.rs`).
   - Take a file's size from `symlink_metadata` and read at most 1024 bytes for the
     generated-marker scan. Apply the graph builder's Oversized cap before any
     in-memory extraction of another root.
   - Query `git check-attr --stdin -z linguist-vendored linguist-generated` once per
     root with every path on stdin, never once per file. It is the one exception to
     `run_git_checked`, which cannot feed stdin. Spawn git with the runner's
     `NO_HOOKS_ARGS`, piped stdin and stdout, and write stdin from a separate thread
     while the caller reads stdout. `verify/criteria/cache_ignore.rs` feeds stdin but
     discards stdout: do not copy it, because a full pipe deadlocks.
2. **Subprojects.** `subprojects.rs` assigns each file to the nearest ancestor
   directory containing a manifest from design 10.1, with the root as fallback.
3. **Coverage per dialect.** From the graph (current root), or from in-memory
   extraction through `extract_file(&registry(), ...)`: symbol-level files and bytes,
   parse errors, and extractor status via `extractor_for`. Other roots report
   `files_parsed`, `bytes_read` and `elapsed_ms`.
4. **Report.** `Display` prints:
   - one block per root;
   - one line per subproject and dialect;
   - a totals line naming eligible files and bytes and the excluded, vendored,
     generated and unsupported counts beside them;
   - the top 10 unsupported extensions by bytes.
5. **Window.** `window.rs` implements design 11 exactly. Site ids `path@start-end` are
   byte offsets (`start_byte`, `end_byte`). Line slicing expands the byte range to
   whole lines. A span beyond EOF is clamped, and lossy UTF-8 is used.
6. **Tests.**
   - Census (`census/tests.rs`), on a temp git repo:
     - classification of each class, including `.gitattributes` `linguist-vendored`;
     - generated-marker detection;
     - subproject grouping with two manifests;
     - bytes totals;
     - `census_never_follows_a_symlink`: a tracked `x.rs -> /dev/zero` is counted
       `unsupported` and the census returns (acceptance runs it by name);
     - a second root extracted in memory writes nothing under that root's `.loom/`.
   - Window (`window_tests.rs`):
     - a node id returns exactly its declaration lines;
     - a site id `path@start-end` returns its line;
     - a changed file returns `ChangedSinceSnapshot`;
     - an unknown id returns `UnknownId`;
     - a span splitting a multi-byte character does not panic;
     - a site id naming a path the graph does not contain (`../outside.txt@0-5`, or
       an absolute path) returns `UnknownId` and never opens that path. `read_window`
       resolves ids only through the graph's `files` map, then joins the map key onto
       `project_root`; it never joins user input onto a path.
   - Census scale (`census/tests.rs`): use the census contract's long paths: a temp
     repo with 3000 committed one-line `.rs` files whose repo-relative paths are about
     800 bytes each (13 nested directories of 60 characters, then `f<n>.rs`). The paths
     total about 2.4 MB, above Linux `ARG_MAX` (2 MiB) and far above the 64 KiB pipe
     buffer for both `check-attr`'s stdin and its output. It reports
     `eligible.files == 3000`, which proves `check-attr` is fed through stdin and read
     without deadlock.

## Traps

- Paths from `git ls-files -s -z` follow a `<mode> <object> <stage>` prefix and a tab,
  are NUL-separated, and may contain spaces or non-ASCII characters. Never split the
  path on whitespace.
- A tracked symlink can point at `/dev/zero` or a huge file: never open it or read its
  target.
- Large repositories: stream `check-attr` input and output through the writer thread.
  Never build one argv with every path (argv limits), and never write all of stdin
  before reading stdout.
- A census of another root must never create `.loom/` there. Assert it in a test.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::census:: 2>&1 | tail -15`

## Report

Report the files changed, the final API, and sample `Display` output from a test repo.
