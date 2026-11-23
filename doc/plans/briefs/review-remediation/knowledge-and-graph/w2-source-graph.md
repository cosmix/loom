# knowledge-and-graph / w2-source-graph: graph enumeration, overlay tombstones, dirty-file identity, overlay save

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/knowledge-graph.md`, sections C3, C4, CR-14 (parts 1 and 2 only), CR-73.

## Findings covered

- C4: a file removed from the Git index but kept on disk and ignored stays visible in an overlay, because no enumeration set contains it and no tombstone is written.
- CR-14 parts 1 and 2: graph enumeration ignores `linguist-generated` and `linguist-vendored`, so `web/dist/**` bundles and `loom/tests/fixtures/**` enter the graph (the bundle's minified `iS` is what makes the farewell prompt emit in the retrieval eval). Part 3 (tolerant TSX extraction) is out of this plan.
- C3 (the contained fix): every dirty file is read in full on every `working_tree()` call, including files far above the extraction cap, only to compute the generation hash.
- CR-73: `GraphStore::save_overlay` re-parses the whole base layer to prune tombstones that `persist_layer` already pruned.

## Files

Own (write):
- `loom/src/context/refresh/source_graph/enumerate.rs`, `loom/src/context/refresh/source_graph/layer.rs`, `loom/src/context/refresh/source_graph/generation.rs`
- `loom/src/context/source_graph/read.rs`
- `loom/src/context/census/classify.rs`, `loom/src/context/census/mod.rs`
- `loom/src/context/graph_store/mod.rs`, `loom/src/context/graph_store/tests.rs`
- `loom/src/context/refresh/tests_source_graph/mod.rs`, `loom/src/context/refresh/tests_source_graph/enumeration.rs`, `loom/src/context/refresh/tests_source_graph/overlay_tombstones.rs` (new)
- `.gitattributes` (new, repository root)

Read only: `loom/src/context/refresh/source_graph.rs` (`reconcile_scope`, `excluded`), `loom/src/context/refresh/tests_source_graph/tombstones_freshness.rs` (owned by w1; copy its helpers' usage, never edit it), `loom/src/context/eval_edges/build.rs` (confirms the labelled-edge evaluation has its own directory walk and never calls `enumerate`).

Frozen (contract tests, never edit): `loom/src/context/refresh/source_graph_contract_tests.rs`.

## Changes

### 1. C4: tombstone every base entry the overlay no longer carries (`source_graph/layer.rs`, `deleted_paths`)

Replace the `not_source` block (the `deleted.extend(enumeration.not_source...)` statement and its comment) by one rule: when `base` is `Some`, add to `deleted` every base entry whose coverage is not `FileCoverage::Deleted`, whose path is not a key of `enumeration.known`, and whose path is not in `enumeration.untracked` (a sorted `Vec<String>`: use `binary_search`). The other two sources (`enumeration.deleted`, dirty paths missing on disk) stay. This covers a kept ignored file whose index entry was removed, and subsumes the symlink/gitlink case, so delete `Enumeration::not_source` and the second return value of `parse_known` in `enumerate.rs`, and the doc comments that describe them. Base scope still returns an empty set at the top of the function.

### 2. CR-14: honour git attributes in enumeration (`source_graph/enumerate.rs`, `census/classify.rs`, `census/mod.rs`, `.gitattributes`)

- `census/classify.rs`: change `Attributes` and `attributes` from `pub(super)` to `pub(crate)`; add `impl Attributes { pub(crate) fn is_vendored_or_generated(&self, path: &str) -> bool }` returning `self.vendored.contains(path) || self.generated.contains(path)`. Keep `attributes_within` as it is. In `census/mod.rs` add `pub(crate) use classify::{attributes, Attributes};` (the module stays private).
- `enumerate.rs`: after the file lists are known (Base: `known`; Overlay: `known` and the filtered `untracked`), make one `crate::context::census::attributes(project_root, &candidates)?` call with `candidates` the known keys plus the untracked paths as `&str`, then `retain` both collections on `!attributes.is_vendored_or_generated(path)`. Do this before `deleted` is derived so an attribute-filtered path is never counted as deleted. Only the two `linguist-*` attributes count: do not use the census's other heuristics (vendor directory names, generated-file header markers), which would hide Go `vendor/` trees and read file heads. An error from `attributes` propagates: `reconcile_scope` already turns an `enumerate` error into a degraded outcome ("failed to enumerate source files"), and a silent fallback to "no filtering" would let a transient failure write a base layer that differs from the next run's, under the same revision key.
- `EXCLUDED_ROOTS` and `excluded()` are untouched (first-segment semantics are deliberate).
- New root file `.gitattributes`, exactly two lines:

```text
web/dist/** linguist-generated
loom/tests/fixtures/** linguist-vendored
```

The labelled-edge evaluation keeps reading its fixtures: `eval_edges/build.rs` walks the directory it is given with its own `walk`; only `excluded()` applies there. Do not touch it.

### 3. C3: no full read of an oversized dirty file (`source_graph/read.rs`, `source_graph/generation.rs`)

- `read.rs`: factor the open-and-check half of `read_source` into a private `fn open_regular(root: &Path, relative: &Path) -> Result<File>` (same `safe_open_dirfd`, `open_safely(.., O_RDONLY | O_NONBLOCK, 0)`, regular-file check and error text). Add `fn read_retaining(reader, limit: usize) -> Result<SourceRead>` holding today's loop with `retained = count.min(limit - bytes.len())`; `SourceRead::read(reader)` calls it with `MAX_EXTRACTED_FILE_BYTES`. Add
`pub(crate) fn source_identity(root: &Path, relative: &Path) -> Result<String>`: open with `open_regular`, take `file.metadata()?`; when `len() > MAX_EXTRACTED_FILE_BYTES as u64` return `format!("stat:{len}:{mtime}.{mtime_nsec}:{ctime}.{ctime_nsec}:{dev}:{ino}")` using `std::os::unix::fs::MetadataExt` without reading a byte; otherwise return `read_retaining(file, 0)?.hash` (the same `sha256:<hex>` string `read_source(..).hash` gives, with no retained copy).
- `generation.rs`, `generation()`: call `source_identity(project_root, Path::new(path))` where it calls `read_source(..)` and uses `.hash`; the error arms (`deleted` when the path is gone, the `unreadable` marker otherwise) stay as they are; drop the `read_source` import.
- Deliberate limit, say it in a comment: for a file above the cap the identity is `size`, `mtime`, `ctime`, `ino`, `dev`, so two writes of equal size inside one filesystem timestamp tick can share an identity. The graph holds only a size-and-hash `Oversized` entry for such a file; revisit if oversized files get real symbols. Existing overlays rebuild once for oversized dirty files; no migration. The pinned hash in `generation_hash_preserves_status_spaces_renames_and_deleted_paths` stays valid because small files keep the `sha256:` content hash.

### 4. CR-73: `save_overlay` writes what it is given (`graph_store/mod.rs`)

Delete the `persisted` clone, the `self.load_base(&layer.revision)?` call and the `retain` from `save_overlay`; it becomes `self.write_or_fall_back(&self.overlay_path(plan, stage), layer).map(|_| ())`. The prune stays in `persist_layer` (`layer.rs`), which already has the base. Remove imports that become unused (`FileCoverage` if nothing else in the file uses it). Update the doc comment. Delete the test `a_tombstone_for_a_path_the_base_lacks_is_dropped_at_persist` in `graph_store/tests.rs` (it pins the old store-level prune) and replace it by the persist-level test below; `a_tombstone_in_the_overlay_hides_the_base_entry` stays.

## Interfaces shared with other workers

None. w4's farewell fixture test (`commands/knowledge/eval/tests_abstention.rs`) passes only once this worker's enumeration change is on disk; it runs after you.

## Tests

- C4: `tests_source_graph/overlay_tombstones.rs` (new; add `mod overlay_tombstones;` to `tests_source_graph/mod.rs`), test `a_kept_ignored_file_removed_from_the_index_is_tombstoned`. Helpers from `mod.rs`: `init_repo`, `stores`, `head_sha`, `base_scope`, `overlay_scope`, `git_ok`. Steps: reconcile the base; write `.gitignore` containing `src.rs`; `git_ok(root, &["rm", "--cached", "src.rs"])`; reconcile `overlay_scope("ignored")`; assert `graph_store.resolved(&revision, Some(("plan-contract", "ignored")))` has no `src.rs` and the saved overlay holds a `FileCoverage::Deleted` entry for it. Fails today (the base entry resolves through).
- CR-73: same file. `persist_layer_drops_a_tombstone_the_base_lacks`: build a base layer with one file, call `persist_layer(&graph_store, &overlay_scope("prune"), &layer, None, Some(&base), BaseWrite::Publish)` with a layer holding a tombstone for `never-existed.rs`, assert the saved overlay lacks it. `save_overlay_stores_the_layer_it_is_given`: `publish_base` an empty base for a revision, `save_overlay` a layer with that tombstone, `load_overlay` still holds it. The second fails today (the store pruned it).
- CR-14: `tests_source_graph/enumeration.rs`, test `a_path_marked_linguist_generated_never_enters_the_graph`: commit `gen/bundle.js` and a `.gitattributes` line `gen/** linguist-generated` next to `src.rs`; base reconcile; `graph_store.load_base(&revision)` has `src.rs` and no `gen/bundle.js`. Then write an untracked `gen/extra.js`, reconcile an overlay, assert the resolved graph has neither. A third assertion with `vendored/x.rs` and `linguist-vendored`. Do not edit the baseline function `an_unreadable_file_survives_as_a_reported_lexical_only_entry` in that file.
- C3: `source_graph/read.rs` `mod tests`, test `identity_above_the_cap_is_stat_based_and_below_it_is_the_content_hash`: a sparse file of `MAX_EXTRACTED_FILE_BYTES + 1` bytes yields an identity starting `stat:`, equal on two calls; after a 50 ms sleep and a one-byte write at the end (same size) it differs; a small file's identity equals `read_source(..).hash`. Fails today (no `source_identity`).
- Frozen contracts to turn green: `ignored-file-removed-from-the-index-is-tombstoned`, `generated-attribute-path-absent-from-graph`.
- Existing tests that must stay green unchanged: everything under `cargo test --lib context::refresh::` and `context::graph_store::`, `context::census::`, `context::source_graph::`.

## Proof

From `loom/`: `cargo check --all-targets`; `cargo test --lib context::refresh::`; `cargo test --lib context::graph_store::`; `cargo test --lib context::census::`; `cargo test --lib context::source_graph::`; `cargo test --test edge_quality_contracts` (reads `tests/fixtures/source/labeled` directly); `cargo fmt` on the files you wrote.

## Traps

- Filtering `untracked` only in the Overlay branch is not enough: `known` is filtered in both scopes or the base keeps the bundle.
- `git check-attr` reads the working tree's `.gitattributes`; the committed one is what the repository ships. In the CR-14 test the file must exist on disk (commit it) before the base reconcile.
- `binary_search` on `untracked` needs the vector sorted: it is sorted at the end of the Overlay branch today; filtering with `retain` keeps it sorted.
- Do not drop the `FileCoverage::Deleted` guard in the C4 rule: a layer used as `base` can in principle hold tombstones.
- The mode-000 trick from the validation report does not work (tests may run as root, and the file must be opened to be statted by descriptor); assert on the identity string instead.
- `graph_store/tests.rs` is 385 lines: only delete there, add nothing.
- Report the deleted `graph_store` test by name.
