# knowledge-and-graph / w1-knowledge-store: index lock, byte-derived revision, bounded reads, index byte budget, dead methods

Tier: sonnet (the long worker of the stage; it owns five findings across two directories)
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/knowledge-graph.md`, sections C1, C2, CR-12 (only the INDEX.md size item), CR-51; `doc/plans/briefs/review-remediation/validation/delivery.md`, section CR-78 bullet 5b.

## Findings covered

- C1: `write_index` generates the index before it takes the directory lock, so two writers can publish an older index over a newer one.
- C2: the persisted structural revision comes from a fingerprint pass that is separate from the catalog build, so an edit-and-revert between the two leaves new chunks under an old revision.
- CR-51 (both halves): `strip_repeated_heading` misses CRLF; knowledge files are read with unbounded `fs::read` / `fs::read_to_string`.
- CR-12, index budget only: the generated `INDEX.md` (19,536 bytes for 226 entries) is over `MAX_INDEX_BYTES` (16,384). The budget rises to 24,576 bytes; the row format does not change.
- CR-78 bullet 5b: `KnowledgeDir::append`, `replace_section` and `read_index` have no production caller.

## Files

Own (write):
- `loom/src/fs/knowledge/index.rs`, `loom/src/fs/knowledge/tests/index.rs`, `loom/src/fs/knowledge/tests/mod.rs`, `loom/src/fs/knowledge/tests/read.rs` (new)
- `loom/src/fs/knowledge/read.rs` (new), `loom/src/fs/knowledge/mod.rs`
- `loom/src/fs/knowledge/dir.rs`, `loom/src/fs/knowledge/tests_dir.rs`, `loom/src/fs/knowledge/tests_dir_bugfixes.rs`
- `loom/src/fs/knowledge/catalog.rs`, `loom/src/fs/knowledge/catalog/fingerprints.rs` (new), `loom/src/fs/knowledge/catalog/prose.rs`, `loom/src/fs/knowledge/catalog/size.rs`, `loom/src/fs/knowledge/tests/catalog.rs` (the test near `index_of_exactly(at.path(), 16_384)` pins the budget)
- `loom/src/context/fingerprint.rs`, `loom/src/context/ingest.rs`, `loom/src/context/refresh.rs`
- `loom/src/context/refresh/tests_freshness.rs`, `loom/src/context/refresh/tests_source_graph/tombstones_freshness.rs` (one call site only, see 2g)
- `loom/src/commands/knowledge/mod.rs`, `loom/src/commands/knowledge/tests.rs`

Read only: `loom/src/fs/safe_read.rs` (`read_bounded`, `is_not_found`), `loom/src/fs/locking.rs` (`locked_dir_update`, `atomic_write_locked`), `loom/src/context/refresh/tests_states.rs`.

Frozen (contract tests, never edit): `loom/src/fs/knowledge/index_contract_tests.rs`, `loom/src/context/refresh/freshness_contract_tests.rs`.

## Changes

### 1. C1: generation, comparison and publish in one critical section (`fs/knowledge/index.rs`, `write_index`)

Today: `generate_index(root)` runs first (lock-free), then the equality check, then `locked_write`. Replace the body so that everything runs inside `crate::fs::locking::locked_dir_update(root, || { ... })`: `generate_index(root)?`, read the existing `INDEX.md` (keep the `NotFound` and other-error arms exactly as they are today), return `Ok(())` when the bytes are equal, otherwise `crate::fs::locking::atomic_write_locked(&path, &content)`. Wrap the whole call with `.with_context(|| format!("Failed to write {}", path.display()))`. `locked_dir_update(root, ..)` takes the same directory flock `locked_write(root/INDEX.md)` takes today, so no caller changes: `dir.rs` (`refresh_index_if_hierarchical`, `initialize`), `commands/knowledge/sync.rs` and `annotate.rs` already call it outside any lock. `generate_index` and `scan_topics` take no lock, so there is no nesting.

### 2. C2 and CR-51 reads: one read per file, and that read decides the revision

a. New `loom/src/fs/knowledge/read.rs`, declared `pub(crate) mod read;` in `fs/knowledge/mod.rs`:
`pub(crate) const MAX_KNOWLEDGE_FILE_BYTES: usize = 1024 * 1024;` and
`pub(crate) fn read_knowledge_file(root: &Path, relative: &Path) -> anyhow::Result<Vec<u8>>`, which calls `crate::fs::safe_read::read_bounded(root, relative, MAX_KNOWLEDGE_FILE_BYTES)` and adds `.with_context(|| format!("Failed to read knowledge file: {}", root.join(relative).display()))`. A file over the cap is an error, not a skipped file. Reason (record it as a `loom memory decision`): a skip needs a new `CatalogIssue` variant and edits to every exhaustive match over it (`catalog/order.rs`, `commands/knowledge/check_lines.rs`); 1 MiB is 100 times the tier-2 file limit, so the error names a file that is already wrong, and every retrieval caller degrades on a refresh error (`orchestrator/signals/retrieval.rs` logs and returns no brief). Revisit if a legitimate oversized knowledge file appears.

b. `context/fingerprint.rs`: `fingerprint_file` reads through `read_knowledge_file(knowledge_root, path)` (drop the `fs::read`). Make `fingerprint_bytes` `pub(crate)`. Add `pub(crate) fn prose_fingerprint(relative: &Path, bytes: &[u8]) -> FileFingerprint` that builds the `prose:`-prefixed path exactly as `prose_fingerprints` does today, and make `prose_fingerprints` call it. `fingerprint_tree` keeps its signature and its early return for a missing root; it remains the cheap pre-pass `evaluate_inner` uses to decide stale or current.

c. `fs/knowledge/catalog/prose.rs`: replace `ProseSources::chunks()` by `pub(crate) fn chunks_and_fingerprints(&self) -> (Vec<KnowledgeChunk>, Vec<FileFingerprint>)`, one `for_each_file` pass that, for each file, pushes `prose_fingerprint(&relative, &bytes)` for every readable file and then the chunks exactly as `chunks()` builds them now (lifecycle filter, chunk errors skipped). Delete `chunks()` (its only caller is `catalog::build`; confirm with `cargo check`).

d. New `fs/knowledge/catalog/fingerprints.rs`, declared `mod fingerprints;` in `catalog.rs` with `pub(crate) use fingerprints::build_with_fingerprints;`. It holds
`pub(crate) fn build_with_fingerprints(root: &Path) -> anyhow::Result<(Catalog, Vec<FileFingerprint>)>`:
 1. `let (mut chunks, issues, mut fingerprints) = super::build_curated(root, true)?;`
 2. `INDEX.md`: `fingerprint_tree` hashes every non-dot `.md` file including `INDEX.md`, and the catalog skips `INDEX.md`, so read it here with `read_knowledge_file(root, Path::new("INDEX.md"))` and push `fingerprint_bytes(PathBuf::from("INDEX.md"), &bytes)`; treat `crate::fs::safe_read::is_not_found(&error)` as absent, propagate anything else.
 3. Prose: `if let Some(sources) = prose::sources_for_knowledge_root(root) { let (prose_chunks, prose_prints) = sources.chunks_and_fingerprints(); chunks.extend(prose_chunks); fingerprints.extend(prose_prints); }`.
 4. If `!root.exists()` return an empty fingerprint list (the same early return `fingerprint_tree` has; without it a missing root with prose roots would never match its pre-pass revision).
 5. Return `(Catalog { revision: revision_for(&chunks), chunks, issues }, fingerprints)`.
`catalog.rs`: `pub fn build(root)` becomes `build_with_fingerprints(root).map(|(catalog, _)| catalog)`; `build_curated` returns `(chunks, issues, fingerprints)`; `structural_issues` ignores the third value; `process_file` reads through `read_knowledge_file(root, relative_path)` (replacing `fs::read`) and returns `(Vec<KnowledgeChunk>, FileFingerprint)` where the fingerprint is `fingerprint_bytes(PathBuf::from(display_path(relative_path)), &bytes)`. `catalog.rs` is 351 lines: it must end shorter than it starts. Remove imports that become unused (`std::fs` is still used by `markdown_files`).

e. `context/refresh.rs`: `rebuild_and_persist` loses its `fingerprints` parameter and calls `catalog::build_with_fingerprints(knowledge_root)?`; `current_revision = tree_revision(&fingerprints)` is computed from the returned list, and `IngestReport.files` is `fingerprints.len()`. Remove the `Evaluated::fingerprints` field and the `evaluated.fingerprints` argument `refresh_snapshot_inner` passes on, if nothing else reads them. Add, next to `AFTER_PUBLISH`, a test-only hook `static BEFORE_BUILD: std::cell::RefCell<Option<Box<dyn FnOnce()>>>` (same `thread_local!` shape) and run it (`take()` then call) at the top of `rebuild_and_persist`, before `build_with_fingerprints`. Net growth of `refresh.rs` (371 lines) must stay under 12 lines.

f. `context/ingest.rs`: `ingest` takes the catalog and the file count from one `build_with_fingerprints` call and drops its `fingerprint_tree` call and import.

g. `refresh/tests_source_graph/tombstones_freshness.rs`: the test `rebuild_and_persist_and_persist_semantic_freshness_do_not_revert_each_others_fields` passes a `fingerprints` list; remove that argument and the `fingerprint_tree` line that builds it. No assertion changes.

### 3. CR-51 reads in the index generator (`fs/knowledge/index.rs`)

`scan_topics` reads each topic through `read_knowledge_file(root, Path::new(category.dir_name()).join(file_name))` instead of `fs::read_to_string(&path)`; keep the `Failed to read topic file` context. `append_tier1_table` reads each tier-1 file through `read_knowledge_file(root, Path::new(file_type.filename()))`. Convert bytes with `String::from_utf8_lossy(&bytes).into_owned()` where a `String` is needed. `index_is_stale` keeps reading `INDEX.md` itself with `fs::read_to_string`.

### 4. CR-51 CRLF heading (`commands/knowledge/mod.rs`, `strip_repeated_heading`)

Accept a CRLF line end after the repeated heading: the condition becomes `after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n")`, and the returned text is `after.trim_start_matches(['\r', '\n']).trim_end().to_string()`. A heading that is only a prefix of a longer heading (`## Merge` against `## Merge Flow`) must still not match.

### 5. CR-12, index byte budget (`fs/knowledge/catalog/size.rs`)

Change `MAX_INDEX_BYTES` from `16384` to `24576` and rewrite its doc comment to say what the number is for: the byte budget of the generated `INDEX.md`, which an agent reads at session start, sized for about 300 topic rows at today's row length. Do not change the row format: two readers outside the knowledge store parse the linked rows (`plan_server/files/knowledge_index.rs`, `loom-hooks/knowledge-orient.sh`), and a new row shape would make every committed index stale for the new binary.

Run `rg -n '16384|16,384|16 KiB' loom/src loom/tests skills loom-hooks CLAUDE.md.template` and update every hit that states the index budget and sits in a file you own (a test that builds an index just over the old budget, an error message, a doc comment). Report each hit in a file you do not own instead of editing it.

`index.rs` is exactly 400 lines and change 1 adds to it. Move the three tests `test_generate_index_lists_tier1_and_topics_with_marker`, `test_generate_index_separates_category_sections_with_blank_lines` and `test_generate_index_omits_topics_section_when_empty` into `fs/knowledge/tests/index.rs` (add `use crate::fs::knowledge::dir::KnowledgeDir;` there) with their bodies and names unchanged.

### 6. CR-78 5b: delete the dead methods (`fs/knowledge/dir.rs`)

Delete `KnowledgeDir::append`, `KnowledgeDir::replace_section` and `KnowledgeDir::read_index`. Port every test call: `tests_dir.rs` (16 sites) and `tests_dir_bugfixes.rs` (5 sites): `.append(KnowledgeFile::X, c)` becomes `.append_target(&KnowledgeTarget::Tier1(KnowledgeFile::X), c)`; `.replace_section(KnowledgeFile::X, h, c)` becomes `.replace_section_target(&KnowledgeTarget::Tier1(KnowledgeFile::X), h, c)`; `knowledge.read_index()` becomes `std::fs::read_to_string(knowledge.index_path())`. Two index assertions change with the compact rows: `tests_dir.rs` (`index.contains("architecture/merge-flow.md")`) becomes `index.contains("| merge-flow |")`; check `tests_dir_bugfixes.rs` near line 113 the same way. Also update `tests_sync.rs` `topic_row` to `format!("| grows | A topic that will grow. | {lines} |")` and drop the now unused `GROWN_TOPIC` link use only if the constant is otherwise unused (it still builds the path), and `tests_replace_section_levels.rs:82` to `index.contains("| admin1-overlay |")`. Remove `dir.rs` imports that become unused.

## Interfaces shared with other workers

None. The contract files pin these names, so keep them exactly: `crate::fs::knowledge::index::write_index(root: &Path) -> Result<()>`, `generate_index`, `crate::fs::knowledge::catalog::size::MAX_INDEX_BYTES` (now `24576`), and in `context/refresh.rs` the private test hook `BEFORE_BUILD` and the unchanged `refresh`, `evaluate`, `refresh_snapshot` signatures.

## Tests

- C1: `fs/knowledge/tests/index.rs`, test `write_index_generates_inside_the_directory_lock`. In a `TempDir` root: `write_index(root)` once; then inside `locked_dir_update(&root, || ...)` spawn a thread that calls `write_index`, wait on an `mpsc` channel until the thread started, sleep 300 ms, create `architecture/late.md` (`# Late`, `> Added while the writer waited.`), and return the `JoinHandle` out of the closure; join after the closure returns (joining inside deadlocks). Assert the written `INDEX.md` contains `| late |`. Fails today: the thread generated before it blocked.
- C2: `context/refresh/tests_freshness.rs`, test `rebuilt_revision_matches_the_pre_pass_revision_with_index_and_prose`: build a project (use the existing helper there) with `doc/loom/knowledge/architecture.md`, a generated `INDEX.md` and a prose file `doc/design.md`; `refresh`; assert `outcome.structural.revision == tree_revision(&fingerprint_tree(&root).unwrap())` and a second `evaluate` is not stale. Second test `an_edit_and_revert_during_the_build_is_not_hidden`: the scenario the frozen contract uses (set `BEFORE_BUILD` to overwrite a knowledge file, refresh, restore the old bytes, assert `evaluate(..).structural.stale`); it must be red on the old code (state that you saw it red or why you could not).
- CR-51 reads: `fs/knowledge/tests/read.rs` (declare in `tests/mod.rs`): a file of `MAX_KNOWLEDGE_FILE_BYTES + 1` bytes makes `read_knowledge_file` fail and the error text names the file; a symlink leaf is refused; `catalog::build` over a tree holding the oversized file returns `Err` naming it; a normal file reads. Model on `fs/knowledge/catalog/tests_prose_safety.rs`.
- CR-51 CRLF: `commands/knowledge/tests.rs` near the existing `\n` heading test (line ~255): `strip_repeated_heading("## Merge Flow\r\n\r\nBody\r\n", "Merge Flow")` equals `"Body"`; `strip_repeated_heading("## Merge Flow Extra\r\nBody", "Merge Flow")` keeps its first line. Do not edit the baseline function `test_knowledge_update_in_worktree_writes_to_worktree`.
- CR-12: in `fs/knowledge/tests/catalog.rs`, beside the existing test of the index size check (update its `16_384` to the constant), `an_index_of_twenty_thousand_bytes_is_within_budget` (a generated or hand-built index of 20,000 bytes raises no oversized-index issue; it raises one today) and `an_index_over_the_new_budget_is_reported` (24,577 bytes still raises it).
- Frozen contracts to turn green: `index-generation-waits-for-the-directory-lock`, `structural-revision-describes-the-indexed-bytes`.

## Proof

From `loom/`: `cargo check --all-targets`; `cargo test --lib fs::knowledge::`; `cargo test --lib context::refresh::`; `cargo test --lib context::tests::ingest`; `cargo test --lib commands::knowledge::`; `cargo fmt` on the files you wrote.

## Traps

- `INDEX.md` and prose files must be in the byte-derived fingerprint list or `evaluate` reports stale right after every refresh (a rebuild loop).
- A missing knowledge root: `fingerprint_tree` returns an empty list before prose; `build_with_fingerprints` must too.
- `refresh_fingerprints_the_tree_once_per_call` in `tests_freshness.rs` counts pre-pass calls and must still read 1: do not call `fingerprint_once` from the rebuild.
- Do not join the writer thread inside `locked_dir_update` in the C1 test.
- `index.rs`, `catalog.rs` and `refresh.rs` sit near 400 lines; new code goes in the new files named above.
- Report every existing test you changed: the `tests_dir*.rs` ports, the moved index tests, the index budget test, the `tombstones_freshness.rs` call site.
