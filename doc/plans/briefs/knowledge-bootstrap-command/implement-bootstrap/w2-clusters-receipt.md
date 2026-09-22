# W2: Cluster partition, digests, fan-in, and bootstrap receipt

Plan: `doc/plans/PLAN-knowledge-bootstrap-command.md` (read its "Design decisions (settled)" table).
The crate is `loom/`; run cargo from `loom/`.

## Context

`loom knowledge bootstrap` (W3, wave 2) splits a repository into directory clusters. It briefs an
interactive Claude session on the clusters to explore, and on `--refresh` it explores only clusters
whose content digest changed since the last completed run. You build the pure logic, with no I/O
beyond the receipt file.

The main agent has already created `loom/src/commands/knowledge/bootstrap/mod.rs` with
`mod clusters; mod receipt;` and test-module declarations for `tests_clusters.rs` and
`tests_receipt.rs`. It also created your four files, each holding a single `//!` module doc
line so the crate compiles while you work; replace their content. Do not edit `mod.rs`; W3
owns it.

## Files you own (exclusive)

- `loom/src/commands/knowledge/bootstrap/clusters.rs`
- `loom/src/commands/knowledge/bootstrap/receipt.rs`
- `loom/src/commands/knowledge/bootstrap/tests_clusters.rs`
- `loom/src/commands/knowledge/bootstrap/tests_receipt.rs`

Read-only context: `loom/src/context/graph_store/mod.rs` (`FileEntry` at 53-61, `ResolvedGraph`
at ~135-165 with `files`, `nodes()`, `edges()`, `node(id)`),
`loom/src/context/source_graph/node.rs` (`SourceNode.path: PathBuf` at 134, `FileCoverage::status()`
at 108), `loom/src/context/source_graph/edge.rs` (`SourceEdge.from`/`.to` at 91/94). Confirm each
with `loom map --outline <file>` before relying on it.

`FileEntry` has NO path field. The path is the key of the `ResolvedGraph.files` BTreeMap
(`graph_store/mod.rs:108`, "keyed by project-relative, forward-slashed path"). The file
enumerator is `loom/src/context/refresh/source_graph/enumerate.rs`; it includes untracked
non-ignored files (:47-50), and markdown files get a `FileEntry` with a `content_hash`.

## `clusters.rs` API (exact, `pub(super)`)

```rust
/// One file's facts, extracted from the resolved graph.
pub(super) struct FileFacts {
    pub path: String,          // repo-relative, '/'-separated
    pub content_hash: String,  // FileEntry.content_hash
    pub symbols: usize,        // FileEntry.nodes minus the whole-file node (see below)
}

pub(super) struct Cluster {
    pub id: String,            // repo-relative dir, "." for root
    pub files: Vec<String>,    // sorted paths
    pub symbols: usize,
    pub digest: String,        // "sha256:<hex>"
    pub hot_files: Vec<(String, usize)>, // top 3 by fan-in, fan-in > 0 only, ties by path
}

pub(super) const MAX_CLUSTER_FILES: usize = 40;
pub(super) const MIN_CHILD_CLUSTER_FILES: usize = 8;
/// The knowledge tree and its receipt are outputs of bootstrap, never inputs.
pub(super) const KNOWLEDGE_PREFIX: &str = "doc/loom/knowledge/";

/// Facts for every graph file whose coverage status is not "deleted" and whose path
/// does not start with KNOWLEDGE_PREFIX, sorted by path. Iterates `graph.files` and
/// uses the map key as the path.
pub(super) fn file_facts(graph: &ResolvedGraph) -> Vec<FileFacts>;

/// Cross-file fan-in: for each edge whose two endpoints both resolve to a file path,
/// count it against the `to` path when the two paths differ. Unresolved endpoints
/// are skipped.
pub(super) fn fan_in(graph: &ResolvedGraph) -> BTreeMap<String, usize>;

/// Deterministic directory partition (rule below), clusters sorted by id.
pub(super) fn partition(files: &[FileFacts], fan_in: &BTreeMap<String, usize>) -> Vec<Cluster>;
```

Derive `Debug, Clone, PartialEq, Eq` on both structs. If an import path above differs in the tree,
use the real one and report it.

**Knowledge exclusion.** `file_facts` drops every path starting with `KNOWLEDGE_PREFIX`. The
source graph indexes untracked non-ignored files and overlays the dirty tree
(`refresh/snapshot.rs:129-144`), and `EXCLUDED_ROOTS` (`refresh/source_graph.rs:75-87`) does not
exclude `doc/loom/knowledge`. Without the filter, the session's tier-1 writes, `INDEX.md` and
the receipt itself change the digest of the cluster holding `doc/loom/knowledge`, and
`--refresh` is never current.

**Symbols.** Every file carries one whole-file node (`source_graph/node.rs:13`, "Whole-file node.
Always present"). Count the rest:
`entry.nodes.iter().filter(|n| n.kind != SourceNodeKind::File).count()`.

**Fan-in.** Never call `graph.node()` per edge: it is a linear scan
(`graph_store/mod.rs:153-155`, `self.nodes().find(..)`), and this repo's graph has 104,841 edges
over 21,591 nodes. Build `let mut owner: HashMap<&str, &str> = HashMap::new();` once from
`graph.files` (key = `entry.nodes[i].id`, value = the file's path), then resolve both endpoints
of every edge through it and count against the `to` path when the two paths differ. An endpoint
absent from the map is skipped. Precedent: `context/rank_source.rs:163`.

### Partition rule (settled; implement exactly)

For a directory `d` (start at root `.`), let `n(d)` be the number of files anywhere under `d`:

1. If `n(d) <= MAX_CLUSTER_FILES`, all files under `d` form ONE cluster with id `d`.
2. Otherwise, for each immediate child directory `c` in name order: if
   `n(c) < MIN_CHILD_CLUSTER_FILES`, add every file under `c` to the residual set; else emit
   `partition(c)`. Files directly in `d` join the residual too. A non-empty residual becomes a
   cluster with id `d`.

Operate on the '/'-separated string keys of `ResolvedGraph.files` (carried in
`FileFacts.path`). A root-level file's directory is `.`. Never use `std::path` components.

Consequences to test: the residual cluster can exceed 40 files; ids never collide, because a
directory is either one cluster or split; every input file lands in exactly one cluster.

`hot_files`: sort by fan-in descending, then path ascending; keep the top 3 with fan-in > 0.

Digest: reuse `crate::context::source_graph::body_hash(bytes)` (`source_graph/mod.rs:92`, returns
`sha256:<hex>`) over the concatenation of `"{path}\t{content_hash}\n"` for the cluster's files
in path order. Write no new Sha256 code and add no dependency.

## `receipt.rs` API (exact, `pub(super)`)

```rust
pub(super) const RECEIPT_FILENAME: &str = ".bootstrap-receipt.json";
pub(super) const RECEIPT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub version: u32,
    pub source_revision: String,   // ResolvedGraph.base_revision at run time
    pub model: String,
    pub effort: String,
    pub completed_at: String,      // RFC 3339 UTC
    pub clusters: Vec<ReceiptCluster>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReceiptCluster { pub id: String, pub digest: String, pub files: usize }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClusterStatus { New, Changed, Unchanged }

#[derive(Debug, PartialEq, Eq)]
pub(super) struct RefreshPlan {
    pub statuses: Vec<(String, ClusterStatus)>, // one per current cluster, by id
    pub removed: Vec<String>,                   // receipt ids absent now, sorted
}

impl Receipt {
    /// `knowledge_root.join(RECEIPT_FILENAME)`; Ok(None) when missing; Err naming the
    /// path when unreadable, oversized, unparsable, or version != RECEIPT_VERSION.
    pub(super) fn load(knowledge_root: &Path) -> Result<Option<Receipt>>;
    /// Pretty JSON + trailing newline, written under a lock (see note below).
    pub(super) fn save(&self, knowledge_root: &Path) -> Result<()>;
    pub(super) fn from_clusters(clusters: &[Cluster], source_revision: &str,
                                model: &str, effort: &str) -> Receipt;
}

/// With no receipt, every cluster is New and nothing is removed.
pub(super) fn refresh_plan(clusters: &[Cluster], receipt: Option<&Receipt>) -> RefreshPlan;
```

For `completed_at`, use `chrono::Utc::now().to_rfc3339()` (chrono 0.4 is already a dependency,
`loom/Cargo.toml:11`). For `save`, use `crate::fs::locking::locked_write(path, content)`
(`fs/locking.rs:264`). It flocks the parent directory and does the temp-file-plus-rename itself.
Do NOT use `atomic_write_locked`: its doc (`locking.rs:149-157`) says it is only safe under a
directory lock the caller already holds. Create the parent directory first if it is missing.
Do NOT import from `crate::quota`. The receipt is committed to git and a teammate can edit it, so treat it as
untrusted input: `deny_unknown_fields`, the version check, and no panics on bad data.

For `load`, return `Ok(None)` when `std::fs::symlink_metadata` on the receipt path fails with
`NotFound`. Otherwise read it with
`crate::fs::safe_read::read_to_string_bounded(knowledge_root, Path::new(RECEIPT_FILENAME), 1 << 20)`
(`fs/safe_read.rs:75`). It opens every path component without following symlinks and caps the
size, so a planted symlink or a huge file is an error, never a read outside the tree.

## Tests (module paths must contain `knowledge::bootstrap::`)

`tests_clusters.rs`, which builds `FileFacts` by hand (no graph needed):

- 10 files in 2 dirs gives one cluster with id `.`.
- The split case: root with 3 direct files, `src/` with 50 files spread as `src/a/` (30),
  `src/b/` (15) and `src/c/` (5), plus `src/` direct files (0). Check the exact ids and
  membership: `src/c` folds into the `src` residual, and root forms its own residual cluster.
- Every file lands in exactly one cluster (property over the split fixture).
- The digest is stable under input order shuffle, and changes when one `content_hash` changes or
  a file is added.
- `hot_files`: top 3 by fan-in, zero excluded, ties broken by path.
- For `fan_in` and `file_facts`, build a tiny `ResolvedGraph` with the test builders in
  `crate::context::resolve::fixtures` (`#[cfg(test)] pub(crate) mod` at `context/resolve.rs:240-241`):
  `graph_of` (fixtures.rs:101), `source_file` (:93), `file_node` (:15), `edge_at` (:124); use
  `FileEntry::tombstone()` (`graph_store/mod.rs:81`) for the deleted case. Cover: a cross-file
  edge counts, a same-file edge does not, an unresolved `to` is skipped, a `deleted` file is
  excluded from facts, and `symbols` does not count the whole-file node.
- Knowledge exclusion: a `doc/loom/knowledge/architecture.md` entry and a
  `doc/loom/knowledge/.bootstrap-receipt.json` entry are absent from `file_facts`.

`tests_receipt.rs`, using `tempfile::TempDir`:

- A save/load round trip is equal. A missing file gives `Ok(None)`. A receipt path that is a
  symlink gives `Err`.
- A corrupt JSON file, an unknown field, or `version: 2` each give `Err` whose message contains the path.
- `refresh_plan`: New/Changed/Unchanged/removed classification, and with no receipt everything is New.

## Requirements the main agent verifies

W3 has not written anything that calls your API yet, so the build will warn about dead code in
these modules. Add `#![cfg_attr(not(test), allow(dead_code))]` at the top of `clusters.rs` and
`receipt.rs` only as a TEMPORARY wave-1 measure, and say so in your report. W3 removes it.

`cargo build`, `cargo test --lib`, `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` are clean. Keep each file under 400 lines and each function under 50.

## Proof (run from `loom/`, ONE command)

```bash
cargo test --lib knowledge::bootstrap::
```

If a compile error is in a file you do not own, report it; do not edit it. The main agent runs
the full gate (build, clippy, fmt, tests) after each wave.

Report the final API with any deviation and its reason. Do not commit.
