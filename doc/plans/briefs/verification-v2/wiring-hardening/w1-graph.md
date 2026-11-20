# wiring-hardening / W1 — in-memory worktree graph

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D11 (worktree graph bullet).
Knowledge: `architecture/source-graph.md` (honesty contract, extractor, limits);
`architecture/context-retrieval-state.md` (base/overlay layers). Code:
`context/graph_store/mod.rs` (`ResolvedGraph` L135-141 with pub `files: BTreeMap<String, FileEntry>`;
`GraphStore::new` L183, `load_base` L215, `resolved` L338-362),
`context/extract/mod.rs::extract_file` (L199), `context/resolve.rs::resolve_graph` (L213).
The test helper `context/coverage.rs:193` shows how a `ResolvedGraph` is built from entries.

W2 and W3 call your function in parallel. Its signature is pinned:

```rust
pub struct WorktreeGraph { pub graph: ResolvedGraph, pub degraded: Option<String>, pub changed: Vec<PathBuf> }
pub fn build_for_worktree(working_dir: &Path) -> Result<WorktreeGraph>;
pub fn build_worktree_graph(project_root: &Path, worktree: &Path, working_dir: &Path, base_revision: &str, changed: &[PathBuf]) -> Result<WorktreeGraph>;
```

`build_for_worktree` discovers everything itself, with read-only git, so callers pass only the
directory they already have:

- worktree: `git rev-parse --show-toplevel`;
- project root (where `.loom/cache` lives): the parent of
  `git rev-parse --path-format=absolute --git-common-dir`;
- base revision: the newest revision with a published base layer that is an ancestor of HEAD
  (list the base directory, `git merge-base --is-ancestor <rev> HEAD`);
- `changed`: `git diff --name-only <rev>` plus untracked files, minus
  `git::worktree::is_worktree_scaffold_path`, relative to the worktree.

It then calls `build_worktree_graph`. `WorktreeGraph.changed` returns that list, so callers such
as impact-selected tests reuse it.

## Files you own

`loom/src/context/worktree_graph.rs` (new), `loom/src/context/worktree_graph_tests.rs` (new),
`loom/src/context/mod.rs` (one `pub mod worktree_graph;` line).

## Tasks

1. Load the base layer for `base_revision` read-only (`GraphStore::load_base`, never
   `ensure_snapshot`, never `publish_base` or `save_overlay`). Build a `ResolvedGraph` from it.
2. For each changed path: read it from the worktree (bounded, no symlink following out of the
   worktree), `extract_file` with the default extractor registry (find how the refresh path
   builds the registry and reuse it), and replace or insert its `FileEntry`. Drop deleted
   files.
3. `resolve_graph` on the result.
4. No base layer (none published, or none an ancestor of HEAD): extract every `git ls-files`
   source file under `working_dir` (read-only git) and set `degraded` to a one-line reason.
5. No writes anywhere. The stage sandbox cannot write the cache.

## Named test (binding), in `worktree_graph_tests.rs` (declared with `#[cfg(test)] #[path]`)

- `worktree_graph_includes_uncommitted_new_file`: a temp git repo with a committed file and a
  base layer published for HEAD through the public store API. Then an UNCOMMITTED new file
  whose function calls a function in the committed file. The built graph contains the new
  file's node and a resolved `Calls` edge to the committed function, and the cache directory is
  byte-identical before and after.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib context::worktree_graph`

## Report

Files changed; the public signature as written; the proof result.
