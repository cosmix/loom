# Validation: knowledge CLI, context retrieval, source graph, extractors, skills matcher

Tree: HEAD `62be2a3d2`. Installed `loom --version` prints `loom 1.3.2-dev.1037+bb442570c (bb442570c, 2026-11-18)`, which is OLDER than HEAD. Every CLI measurement below (`knowledge check`, `knowledge eval`, `map --census`, `map --outline`) ran on that binary. Static findings were read from the tree at HEAD. Working tree has uncommitted changes under `doc/loom/knowledge/` and many untracked `doc/plans/` files (these are indexed as prose, so they shift retrieval numbers slightly).

Cross-finding map (read first):

- `CR-14` web/dist inclusion is the direct cause of the farewell-prompt emission in `C6` (a minified `iS` function matches the word "is" as `exact-symbol`).
- `C2`, `CR-51` and `C5` all touch `fingerprint_file` / `catalog::process_file` / `refresh_snapshot`; do `C2` and `CR-51` as one change.
- `C3` and `CR-48` both start in `working_tree()` and the 399 MB view load; `C3` has a contained fix, `CR-48` does not.
- `C4` and `CR-14` both live in `context/refresh/source_graph/` enumeration.

---

## C1 — Concurrent knowledge writes can publish an obsolete index

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/fs/knowledge/index.rs:242-254` `write_index`: calls `generate_index(root)` (a lock-free directory scan with plain `fs::read_to_string`, lines 109-200) FIRST, then `locked_write(&path, &content)` (line 253), which takes the `root` directory flock only for the final atomic write.
- `loom/src/fs/knowledge/dir.rs:215, 261, 316` call `refresh_index_if_hierarchical()` (dir.rs:342) after the content lock is released. Topic files live in `<root>/<category>/`, so topic writers in different categories lock different directories and run `write_index` concurrently.
- Interleaving A-generate, B-write-topic, B-generate, B-publish, A-publish leaves A's older bytes on disk. `index_is_stale` (index.rs:235) and `check` then report a stale index until the next write or `loom knowledge sync`.
- Guard looked for: `locked_write` serializes only the rename, not generation. The comment in dir.rs:324-340 explains the no-nesting rule but does not close the window.
**Fix seam:** `loom/src/fs/knowledge/index.rs:242` `write_index`. Wrap generation, the equality check and the publish in one critical section on `root`: `crate::fs::locking::locked_dir_update(root, || { let content = generate_index(root)?; <read existing, return Ok(()) if equal>; atomic_write_locked(&path, &content) })` (`fs/locking.rs:164` `locked_dir_update<T,F>(dir:&Path,f:F)->Result<T>` and `fs/locking.rs:182` `atomic_write_locked(path:&Path,content:&str)->Result<()>`). `locked_dir_update(root)` takes the same flock `locked_write(root/INDEX.md)` takes today. No nesting: `generate_index`/`scan_topics` use plain `fs::read*` (no locks), and `refresh_index_if_hierarchical` is already called after the content lock is released. Callers affected, all unchanged in signature: `fs/knowledge/dir.rs:108,168,215,261,316`, `commands/knowledge/sync.rs:78,103`, `commands/knowledge/annotate.rs:56`. Topic writers still do not take the root lock for their own content write, so a topic edit landing mid-generation can still be missed by that one pass; the writer's own post-write `write_index` then republishes, which closes the lost-update case the review describes.
**Regression test:** extend `loom/src/fs/knowledge/index.rs` `mod tests` (uses `TempDir` and `KnowledgeDir`). Deterministic form: hold `locked_dir_update(root, ..)` on the main thread, spawn `write_index(root)` in a thread, sleep briefly so it has started, then inside the held lock create a new topic file, release. Assert the written `INDEX.md` lists the new topic. Today the thread has already generated before blocking on the lock, so the index lacks the topic; after the fix generation waits for the lock.
**Files a fix touches:** `loom/src/fs/knowledge/index.rs`.
**Size / tier:** S; judgment (concurrency, but a 10-line change).
**Overlap:** `C2` (same subsystem, different file), `CR-51`.

---

## C2 — Structural freshness can describe different bytes from the indexed catalog

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/context/refresh.rs:108-136` `evaluate_inner` calls `fingerprint_once` (line ~121) and computes `tree_revision` from it. `refresh_snapshot_inner` (refresh.rs:241-285) carries those `fingerprints` into `rebuild_and_persist` (line 291), which calls `tree_revision(&fingerprints)` (line ~306) and then separately `catalog::build(knowledge_root)` (line ~309). The two passes read files independently: `fs/knowledge/catalog.rs:95` `process_file` does its own `fs::read`; `context/fingerprint.rs:37-43` `fingerprint_file` does another.
- The persisted `structural.revision` is the pre-build fingerprint revision; the chunks come from the later read. An edit between the passes, reverted before the next evaluate, leaves new chunks under the old revision and the next `evaluate` sees a match.
- Guard looked for: none. `structural_snapshot::publish` (refresh/structural_snapshot.rs) makes the publication atomic but does not compare inputs.
**Fix seam:** Options:
  1. (Recommended) Derive the revision from the bytes actually indexed. Add `catalog::build_with_fingerprints(root) -> Result<(Catalog, Vec<FileFingerprint>)>` that hashes the `bytes` already read in `catalog.rs:95-98` `process_file` (reuse `fingerprint.rs:46` `fingerprint_bytes(path, &bytes)`; make it `pub(crate)`) and the prose bytes (`catalog/prose.rs` `for_each_file`/`chunks` read through `read_bounded_at`; return their hashes the same way `fingerprint.rs:100` `prose_fingerprints` does). `rebuild_and_persist` (refresh.rs:291) then ignores the pre-build list and uses the returned one. `evaluate_inner` keeps its cheap pre-pass only for the stale/current decision.
  2. Bracket: re-run `fingerprint_tree` after `catalog::build` and retry a bounded number of times when `tree_revision` differs. Smaller, but an edit-and-revert fully inside the build window (the exact case the review describes) still passes; so it only narrows the race.
  Recommend option 1; it also removes the third read of every file and gives `CR-51` one read helper to harden.
  Callers: `context/retrieve.rs:~205` `resolve_catalog`, `refresh()` users in `commands/knowledge/{sync,check}.rs`, tests in `context/refresh/tests_*.rs`.
**Regression test:** `loom/src/context/refresh/tests_freshness.rs` or `tests_states.rs` (existing helpers: `AFTER_PUBLISH` hook in refresh.rs, `take_fingerprint_pass_count`). Add a test-only hook between fingerprint and build (like `AFTER_PUBLISH`) that edits a knowledge file; assert the persisted `structural.revision` equals `tree_revision` of the files whose text is in the returned catalog chunks. Then repeat with edit-and-revert to show option 2 would not catch it.
**Files a fix touches:** `loom/src/fs/knowledge/catalog.rs` (+ `catalog/prose.rs`), `loom/src/context/fingerprint.rs`, `loom/src/context/refresh.rs`, tests under `loom/src/context/refresh/`.
**Size / tier:** M; judgment.
**Overlap:** `CR-51` (same two read sites), `C5`.

---

## C4 — Removing a kept, ignored file from the Git index leaves old symbols visible

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/context/refresh/source_graph/enumerate.rs:46-70` (Overlay branch): `known` comes from `git ls-files -s` (index), `untracked` from `ls-files --others --exclude-standard` (excludes ignored), and `deleted` is `known.keys()` whose file is missing on disk (line 62-66). A path that was committed (so it is in the base), is `git rm --cached`, remains on disk and is ignored, lands in none of the three sets.
- `loom/src/context/refresh/source_graph/layer.rs:179-211` `deleted_paths`: adds `enumeration.deleted`, symlink/gitlink `not_source` paths that the base holds, and dirty paths missing on disk. The staged removal is a dirty path, but it exists on disk, so it is not added. `active_paths` (layer.rs:160-174) never includes it, so no entry (live or tombstone) is written and the base entry resolves through.
- Guard looked for: `persist_layer` only prunes tombstones; nothing adds one for a base path outside the active set. The reported CLI behavior (`original_symbol` surviving) follows from this reading; I did not repeat it.
**Fix seam:** `layer.rs:179` `deleted_paths`: in the Overlay case also tombstone every base entry that is not a tombstone and is in neither `enumeration.known` nor `enumeration.untracked` nor already deleted: `base.files.iter().filter(|(p,e)| e.coverage != FileCoverage::Deleted && !enumeration.known.contains_key(*p) && !enumeration.untracked.binary_search(p).is_ok())`. `enumeration.untracked` is sorted (enumerate.rs:60) so `binary_search` works. This subsumes the existing `not_source` clause (layer.rs:190-197), which can then be deleted together with `Enumeration::not_source` (enumerate.rs:21-24) and its `parse_known` return value; keep both only if you prefer the smaller diff. Excluded paths cannot appear in the base because `excluded()` filters base enumeration too. Consumers: `build_layer` (layer.rs:33), `persist_layer` prune (layer.rs:327-333, already drops tombstones the base lacks), `layer::assemble_layer`, and `files_deleted` counters.
**Regression test:** `loom/src/context/refresh/tests_source_graph/tombstones_freshness.rs`, next to `a_deleted_tracked_file_is_tombstoned_and_absent_from_the_resolved_graph` (line 8). Helpers in `tests_source_graph/mod.rs`: `init_repo`, `stores`, `head_sha`, `base_scope`, `overlay_scope`, `git_ok`, `reconcile_source_graph`. Steps: reconcile base; write `.gitignore` containing `src.rs`; `git_ok(root, &["rm","--cached","src.rs"])`; reconcile an overlay; assert `resolved.files` has no `src.rs` and the overlay has a `FileCoverage::Deleted` entry. Fails today (entry survives).
**Files a fix touches:** `loom/src/context/refresh/source_graph/layer.rs` (and `enumerate.rs` if `not_source` is removed), test file above.
**Size / tier:** S; mechanical.
**Overlap:** `CR-14` (same directory).

---

## C3 — A warm map can reread every dirty byte

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/context/refresh/snapshot.rs:86` `ensure_snapshot` calls `working_tree()` first; `context/refresh/source_graph/generation.rs:20-31` `working_tree` runs `git status --porcelain=v1 -z` then `generation()` (generation.rs:45-62).
- `generation()` calls `read_source(project_root, path)` (`context/source_graph/read.rs:91-104`) for every dirty path, which streams the whole file through `SourceRead::read` (read.rs:27-60): hashes all bytes, counts lines, and also copies up to `MAX_EXTRACTED_FILE_BYTES` (512 KiB, `source_graph/mod.rs:93`) into a `Vec` that `generation()` throws away. Untracked files are dirty (porcelain `??`), so a large untracked or generated file is reread on every query, even when the layer is reused.
- Same call happens on the prompt hook path: `context/retrieve/graph.rs:92` (`working_tree(project_root).ok()`), and `commands/map.rs` `load_graph` → `ensure_snapshot`.
- Guard looked for: no stat/mtime short circuit anywhere in `generation()`.
**Fix seam:** Contained fix (recommended, S-M): in `generation()` use a content-free identity for files above the extraction cap. Stat the file with `symlink_metadata`, and when `len > MAX_EXTRACTED_FILE_BYTES` fold `size`, `mtime` (ns), `ctime` (ns), `ino`, `dev` into the hash instead of reading (`MetadataExt` from `std::os::unix::fs`). Tail-only and same-size edits change `mtime`/`ctime`, so they are still detected; `ctime` cannot be reset by `touch`. Files at or below the cap keep the full-content hash (bounded at 512 KiB each), so the pinned hash test at generation.rs:94 is unaffected. For the small-file path also add a hash-only reader (no `bytes` retention) next to `SourceRead::read` to drop the wasted 512 KiB copy. The generation value changes only for oversized dirty files; existing overlays rebuild once, which is acceptable under the no-migration rule. A full persistent stat cache (path to size/mtime/hash across processes) is the alternative; it adds a new on-disk store and a racy-timestamp rule and belongs in its own plan only if the contained fix is judged insufficient. Add the review's instrumentation (bytes and ms spent in `generation`) to `SourceGraphCounters` only if wanted; not required for the fix.
**Regression test:** `loom/src/context/refresh/source_graph/generation.rs` `mod tests` (line 78; has `parsed_dirty`) for the identity; plus `context/source_graph/read.rs` tests (`sparse_oversized_source_has_bounded_retention_and_full_identity`, line ~120, shows the sparse-file helper). Assert: a sparse file of `MAX_EXTRACTED_FILE_BYTES * 32` produces a generation without reading (use a FIFO-free check: make the file unreadable with mode 0o000 but stat-able, the generation must still be stable and differ after `set_len` growth or a tail write), and a same-size tail write changes the generation.
**Files a fix touches:** `loom/src/context/refresh/source_graph/generation.rs`, `loom/src/context/source_graph/read.rs` (hash-only reader).
**Size / tier:** M; judgment (cache identity vs. detection guarantees).
**Overlap:** `CR-48` (prompt hook latency), `CR-14` (generated/bundled files are the likely large dirty files).

---

## C5 — Retrieval evaluation pays per-case refresh cost and can mix revisions

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/commands/knowledge/eval.rs:51-55,130-143` `run_case` calls `retrieve_for_stage(&query, budget)` once per case. `context/retrieve.rs:296-333` per call: `resolve_roots_optional`, `RetrievalConfig::load`, `resolve_catalog` (→ `refresh_snapshot` → `evaluate_inner` → `fingerprint_once`, refresh.rs:155-160), then `graph::load_resolved_graph` (`retrieve/graph.rs:80-110`: `working_tree()` plus the resolved-view load).
- Measured with the installed binary: `loom knowledge eval` over 23 cases took 42 s wall (about 1.8 s per case), single `loom knowledge context` query 4.2 s. The 399 MB view parse per case dominates; the fingerprint scan is the smaller part, so the review understates the cost.
- `eval.rs:85-110` `refresh_source_graph` refreshes the snapshot once up front, but nothing carries the loaded catalog or graph between cases.
**Fix seam:** Split `retrieve_for_stage` (retrieve.rs:296) into `capture(query_scope)` returning a struct holding `roots`, `config`, `catalog`, `state`, `graph_load`, and `retrieve_with(&captured, &query, budget)` doing `check_require_ids`, `rank_stage_channels`, `fuse`, `pack`, `attach_windows` (retrieve.rs:313-332). `retrieve_for_stage` becomes capture-then-retrieve (all other callers unchanged: `commands/knowledge/context.rs`, `commands/hook/user_prompt.rs:149`, `orchestrator/signals/retrieval.rs`, worker brief). `state` is mutated per query by `graph_load.apply_freshness(&mut state)`; clone it per case. `eval` captures once (all cases share `overlay: Local`, `scope: all`, default surface) and prints the captured structural and semantic revisions in `print_human`/`print_json` (`eval/report.rs`). Cases differ only in `text`, `required_ids` and budget, so one capture is valid for all.
**Regression test:** `loom/src/commands/knowledge/eval/tests_refresh.rs` (has `repo()` builder via `crate::test_git::TestRepo`). Use `take_evaluate_state_call_count` (`context/retrieve.rs:341`, already `pub(crate)` under `cfg(test)`) around a multi-case run and assert 1 instead of N; add a case that edits a knowledge file after capture and assert later cases still score the captured revision.
**Files a fix touches:** `loom/src/context/retrieve.rs`, `loom/src/commands/knowledge/eval.rs`, `loom/src/commands/knowledge/eval/report.rs`, test file above.
**Size / tier:** M; needs-exploration.
**Overlap:** `C2` (same refresh path), `C6` (same command), `CR-48` (view load cost).

---

## C6 — The checked-in retrieval benchmark fails on a clean checkout

**Verdict:** CONFIRMED (reproduced with the older installed binary on the current working tree)
**Evidence:**
- Command: `loom knowledge eval --json` (cases default `loom/eval/retrieval-cases.yaml`, 23 cases). Binary `bb442570c`, not HEAD. Result: exit 1, `hit_rate_hits 12/12` (hit@5 1.0), `precision_at_5 0.3958333` (floor `0.4000000059604645`), `abstention_correct 0/2` (`abstention_failures 2`), `mean_mrr 0.854`, `relevant_token_fraction 0.295`, `forbid_violations 0`. Stderr: `FAIL - precision@5 0.40 < precision_floor 0.40; 2 abstention case(s) emitted`. The reviewer's clean-tree row (12/12, 0.395833, 0/2) matches; their working-tree row (10/12, 0.4375) did not reproduce here because the working tree differs.
- Why the abstention cases emit (`loom knowledge context --json` on each query):
  - `thanks, that is all for now, nothing else to do here`: top item `web/dist/assets/flow.js#function:iS` with reason `exact-symbol` (the minified function `iS` matches the dropped-term word "is"). An exact rung clears the emit floor on its own (`commands/hook/user_prompt_compose.rs:129-143`). Same item answers the query `what is it`. This is the `CR-14` pollution.
  - `no, use the repository version of those files, not the installed copies`: lexical items `loom/src/git/signing.rs#constant:NOT_INSTALLED` and `mistakes/web-dashboard.md#markdown-lint-must-use-the-repository-configuration#0`, each with `matched_term_count 2`, which meets `min_knowledge_terms = 2` (`context/config.rs:162`).
- `eval/metrics.rs:52` calls the production `delivered()` (user_prompt_compose.rs:97) with an empty delivery set, as the review says. The query built at `eval.rs:130-143` is `StageQuery::new` (surface `StageBrief`, `Local` overlay). `Surface` only gates source-window attachment (`retrieve/windows.rs:36`, applied after ranking), so aligning the surface changes rendered tokens, not emission or ranking. The review's "align with the hook surface" advice therefore will not fix the failure by itself.
- The unit test `eval/tests_abstention.rs` passes because it runs over a 2-file synthetic knowledge tree (`project()`, lines 33-42), not this corpus; nothing in CI runs the real eval or `knowledge check` (no match in `.github/`). Cases were last re-judged 2026-10-01 (git log of `retrieval-cases.yaml`); the corpus has grown since.
- `eval/report.rs:~125-140` formats both numbers with `{:.2}`, hiding the miss (`0.40 < 0.40`).
**Fix seam:**
  1. `eval/report.rs` `exit_reason` (lines ~125-140): print four or more decimals for hit@5 and precision@5 (S).
  2. Remove the `web/dist` pollution (see `CR-14`); this should clear the farewell emission. Verify by re-running eval.
  3. The correction case needs a decision: either raise `min_knowledge_terms` (default 2, `context/config.rs:162`) to 3 and let the eval show whether any positive case loses its emission, or make a lexical-only source constant (kind `constant`/non-prose) unable to supply matched terms toward the emit floor in `user_prompt_compose.rs::clears_item_floor` (line ~139). Recommend measuring option "min_knowledge_terms 3" first because it is a one-line config default and the eval is the arbiter. The precision gap (0.3958 vs 0.40) is 0.004; after the two abstention cases stop emitting, re-measure before touching the floor, and re-judge or add `relevant:` ids rather than lowering `precision_floor`.
  4. For hermetic CI: the eval reads the live corpus, so a CI gate needs a pinned corpus (the synthetic project in `tests_abstention.rs` is the existing pattern) or a clean-checkout CI job that builds the graph first; the second is slower (a graph build) and ties the result to checkout content. State the choice in the plan.
**Regression test:** `loom/src/commands/knowledge/eval/tests_abstention.rs` has `emits(root, case)` and `case_query(name)`; add a fixture file to `project()` containing a minified-style symbol `iS` in a source file and assert the farewell case still abstains (needs a source graph in the fixture: `crate::test_git::TestRepo` plus `ensure_snapshot`, as in `tests_refresh.rs`). Formatting test: `commands/knowledge/tests_eval.rs` `exit_reason_names_a_low_true_precision_score` (line 303), extend to assert the message contains the unrounded value.
**Files a fix touches:** `loom/src/commands/knowledge/eval/report.rs`, `loom/src/context/config.rs` or `loom/src/commands/hook/user_prompt_compose.rs`, `loom/eval/retrieval-cases.yaml` (only if re-judging), tests above, possibly `.github/workflows/ci.yml`.
**Size / tier:** M; judgment (retrieval tuning against a live corpus).
**Overlap:** `CR-14` (root cause of one failure), `CR-49` (stopword rescue, same ranking pipeline), `C5`.

---

## CR-07 — Global `reconcile.lock` shared across all worktrees and stages

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/commands/hook/reconcile_graph/lock.rs:65-67` `reconcile_lock_path(store)` returns `store.root().join("reconcile.lock")`. `ContextStore::open(&WorkDir)` resolves under the main project's `.loom/cache/context-v1` (confirmed on disk: one cache dir with graph files for several revisions), so every stage worktree shares the one lock.
- `reconcile_graph.rs:245-290` `try_spawn` claims it; when a live holder exists `try_claim` (reconcile_graph.rs:225-243) calls `mark_pending` (lock.rs:~230-237), setting one shared `pending` bit.
- `reconcile_graph.rs:140-166` `run_passes`: after the holder's pass, `end_pass` sees `pending` and runs another pass whose closure (`try_reconcile`, line 95-100) reconciles the HOLDER'S OWN `target`, not the requester's. Stage B's request is absorbed and never served.
- `lock.rs` failure counter and `decide` (backoff `BACKOFF_CAP_SECS = 21_600`, debounce `reconcile_debounce_secs` default 600) apply to the one file, so one worktree's failures throttle all. `cancel::cancel_holder(&reconcile_lock_path(&store), ..)` (reconcile_graph.rs:109) can signal whichever worktree's process holds it.
**Fix seam:** key the lease per checkout. Change `reconcile_lock_path(store: &ContextStore)` to take the project root and return `store.root().join(format!("reconcile-{}.lock", local_overlay_stage_name(project_root)))` (`context/local_overlay.rs:19`, `fn local_overlay_stage_name(&Path) -> String`, `map-<sha256 of canonical root>`, filename-safe). Three call sites: `reconcile_graph.rs:94` (`try_reconcile`, has `target.project_root`), `reconcile_graph.rs:109` (`try_cancel`, has `target`), `reconcile_graph.rs:259` (`try_spawn`, has `project_root`). A stage worktree is its own project root, so this is also the per-`(plan, stage)` key. A separate base-only lock (the review's second suggestion) is not needed: each worktree's pass runs its own `snapshot_policy()` and base publication is already idempotent (`BaseWrite::Publish`). Cache cleanup: lock files accumulate one per worktree; `loom clean` tooling should remove `reconcile-*.lock` (check `commands/clean`). Update `doc/loom/knowledge/architecture/source-graph-build.md:106` (describes the single `reconcile.lock`; record via `loom memory note stale-knowledge` during the stage). Tests' `LOCK_FILE` path literal usage: `tests_lock.rs`, `tests_rebind.rs`, `tests_read_lock.rs` build their own `reconcile.lock` paths in temp dirs and call `update_lock`/`try_claim` directly, so they remain valid.
**Regression test:** `loom/src/commands/hook/tests_reconcile_graph.rs` (end-to-end spawn/claim tests) or `reconcile_graph/tests_lock.rs`: two distinct project roots against one `ContextStore`: assert `reconcile_lock_path` differs, and a `try_claim` won on root A is `Won` (not `Queued`) on root B. Fails today (the second claim queues behind the first).
**Files a fix touches:** `loom/src/commands/hook/reconcile_graph/lock.rs`, `loom/src/commands/hook/reconcile_graph.rs`, `loom/src/commands/hook/reconcile_graph/cancel.rs` (if it builds the path), tests above, `doc/loom/knowledge/architecture/source-graph-build.md`.
**Size / tier:** M; needs-exploration.
**Overlap:** `CR-48` (hook latency), `C3`.

---

## CR-12 — Six active `loom knowledge check --strict` failures on HEAD

**Verdict:** CONFIRMED (exact list below; installed binary is older than HEAD, but the checks are file-based and the file sizes were re-measured)
**Evidence:** `loom knowledge check --strict` exits 1: `6 issue(s), 417 review note(s)`:
```text
! INDEX.md is 19536 bytes, over the 16384-byte budget - trim it
! mistakes/hooks-shell-portability.md: tier-2 file is 412 lines, over the 400-line limit
! mistakes/sandbox-hook-preflight.md: source reference "bunfig.toml" does not exist
! mistakes/sandbox-hook-preflight.md: source reference "rust-toolchain.toml" does not exist
! mistakes/testing-and-lint.md: tier-2 file is 408 lines, over the 400-line limit
! mistakes.md: tier-1 file is 274 lines - split the detail into a tier-2 topic file
```
Limits: `fs/knowledge/catalog/size.rs:14` tier-1 250 lines, `:18` tier-2 400 lines, `:21` index 16384 bytes. The 417 notes are non-failing (`note:` 327, `review:` 90).
**Fix seam (content only, no code):**
1. `doc/loom/knowledge/mistakes/sandbox-hook-preflight.md:9` (Why paragraph): the sentence listing version-manager shims names `rust-toolchain.toml` and `bunfig.toml` as live paths. Reword that sentence to carry an example marker, e.g. insert `e.g.` before the parenthesized list (`(e.g. `.tool-versions` ...`); `chunker/references.rs:62-77` `EXAMPLE_MARKERS` includes `e.g.`. Re-run check to confirm.
2. `mistakes/testing-and-lint.md` (408, needs at least 9 lines out): move the two maintainability-ledger sections `## Growing a Function That Carries a Maintainability Baseline Entry Breaks the Gate` (lines 100-119) and `## Ledger Growth Reached Push Time Again - Gates Moved Earlier` (lines 120-130), 31 lines, into the existing `mistakes/maintainability-baseline.md` (103 lines, becomes about 134). Leaves about 377. No new topic, no new INDEX row.
3. `mistakes/hooks-shell-portability.md` (412, needs at least 13 lines out): move the macOS and Bash 3.2 sections (lines 194-286: BSD wc padding, heredoc in `$( )`, first macOS hook run, `--version` probe recursion; about 93 lines) into a new topic `mistakes/hooks-macos-and-bash32.md` with a `# ` title and a `> ` blurb of at most 80 chars, then `loom knowledge annotate mistakes/hooks-macos-and-bash32 --blurb "..."`. Leaves about 319. This adds one INDEX row, so item 5 needs correspondingly more cuts.
4. `doc/loom/knowledge/mistakes.md` (274, needs at least 24 lines out): the three full-form sections are `## Brief-path extractor fed markdown into the run glob` (line 162, 10 lines), `## Treating this repository's conventions as loom's product behaviour` (line 200, 7 lines) and `## Plan Amendment Strips completed_commit From a Merged Stage` (line 224+, 7 lines). Move them into one existing or new tier-2 topic (e.g. `mistakes/plan-authoring.md` exists; the brief-path and amendment entries fit it) and replace all three with ONE bullet list under a single new heading (6 lines), net about -19; so also move one more small section (`## Sandbox Route Closure`, 8 lines, to `mistakes/planning-sandbox-delivery.md`) to reach 250 or fewer. Verify with `wc -l`.
5. `INDEX.md` (19536 B; needs at least 3.2 KB out, plus about 100 B per new topic). 226 rows average 82 B; 124 rows have blank blurbs, so shortening blurbs saves only about 700 B. Real savings need fewer rows: merge small topics within a category into themed ones. Candidates by line count (INDEX shows counts): `mistakes/{cargo-test-harness-layout (13), native-worker-tunnel-half-close (13), test-hermeticity (13), native-macos-delivery (25), seatbelt-loopback-grant (25)}` into one native/harness topic; `mistakes/{sandbox-hook-preflight (13), sandbox-protected-hooks-dir (37)}` into one; `concerns/` has 33 rows (3078 B), of which `state-confinement-gaps`, `codex-heartbeat-starvation`, `typed-config-values`, `macos-confined-networking`, `ci-macos-test-build-time`, `planner-kickoff-fixes-followups`, `stage-build-cache-gaps`, `plan-chat-followups` are 15-41 lines each and mergeable into 2-3 `concerns/` topics. Roughly 40 rows must disappear. Alternative the owner may prefer: raise `MAX_INDEX_BYTES` (a code constant at `size.rs:21`, 16 KiB is the stated session-start budget); that is a policy change, not a content fix, so flag it rather than doing it silently.
6. After every move: `loom knowledge sync` then `loom knowledge check --strict`; CI wiring for the check is a separate finding (`CR-69`).
**Regression test:** none new; the gate is `loom knowledge check --strict` exiting 0. Existing checker tests: `loom/src/fs/knowledge/catalog/tests*.rs`.
**Files a fix touches:** `doc/loom/knowledge/INDEX.md` (generated), `mistakes.md`, `mistakes/hooks-shell-portability.md`, `mistakes/testing-and-lint.md`, `mistakes/maintainability-baseline.md`, `mistakes/sandbox-hook-preflight.md`, new `mistakes/hooks-macos-and-bash32.md`, plus merged topics for item 5.
**Size / tier:** M (many doc moves); mechanical except item 5 (judgment on which topics merge).
**Overlap:** `CR-69` (CI gate), `C6`.

---

## CR-14 — 28 Vitest TSX files fail extraction; web/dist and fixtures enter the graph

**Verdict:** CONFIRMED, with three parts that need separate fixes.
**Evidence:**
- Part 1, `EXCLUDED_ROOTS` first segment: `loom/src/context/refresh/source_graph.rs:80-92`: `excluded(path)` compares only `path.split('/').next()` to `.loom .work .worktrees target node_modules .git`. So `web/dist/` and `loom/tests/fixtures/` pass. Also used by census (`census/mod.rs:104`), `eval_edges/build.rs:80`, `generation.rs:69`, `enumerate.rs:51,85`. Caveat: matching by first segment is deliberate for root-level directories; adding the string `dist` generically would hide legitimate source in other projects (loom drives many languages).
- Part 2, corpora: `loom map --census` (installed binary): `web javascript files 136 bytes 8200922 parse-errors 3` (the minified `web/dist/assets/*.js`; `git ls-files web/dist` = 186 tracked files, 9.6 MB, embedded into the binary by `loom/build/assets.rs`), plus `loom/tests/fixtures/source/**` (152 tracked files; per-language rows with 1-2 parse-errors each, plus `labeled/{go,rust}`). Census reports `generated 4 files`, because `census/classify.rs:92-211` honors `linguist-generated`/`linguist-vendored` via `git check-attr`; the repo has no `.gitattributes`, and the graph enumeration (`enumerate.rs`) never consults that classification at all. Size context: the cached base layer is 300 MB and the view 399 MB (`.loom/cache/context-v1/graph/`), but `web/dist` is only about 8 MB of source, so it is noise and a ranking hazard (it causes the `C6` farewell emission and the `what is it` hit), not the main storage driver; the 4075 Rust files (28 MB) are.
- Part 3, TSX misparse: `loom map --outline web/src/plan/chat/hearth.contracts.test.tsx` prints `coverage: parse-error - syntax error at line 17: >()`; line 17 is `...(await original<typeof import("./suggestion-cards")>()),`. `loom census` totals: `web tsx files 746 ... parse-errors 28`, `javascript parse-errors 3`. 29 TSX files under `web/src` contain `typeof import(`; the one `.ts` file with it (`web/src/plan/editor/load.ts`, type position only) parses fine, consistent with a TSX-only grammar problem on call type arguments. I did not confirm each of the 28 individually. `loom/src/context/extract/treesitter/mod.rs:170-180`: `root.has_error()` returns a file-level `FileCoverage::ParseError` with no symbols (deliberate, the comment says half a tree is worse than a gap).
- Grammar: `loom/Cargo.toml:40` pins `tree-sitter-typescript = "=0.23.2"`. crates.io lists 0.23.2 (2024-11-11) as the newest release (checked via the crates.io API), so no grammar bump exists. A fix inside this repository is a tolerant extraction, or avoiding the construct.
**Fix seam:**
- Part 1+2 (recommended): honor attributes in graph enumeration instead of hard-coding paths. In `enumerate.rs` (both Base and Overlay branches) drop paths that `census::classify::attributes(root, &paths)` (`census/classify.rs:150`, `fn attributes(root:&Path, paths:&[&str]) -> Result<Attributes>`) marks `generated` or `vendored`, and add a root `.gitattributes` with `web/dist/** linguist-generated` and `loom/tests/fixtures/** linguist-vendored`. One `git check-attr` call per enumeration, already used by census. Keep `EXCLUDED_ROOTS` first-segment semantics unchanged. Decision for the owner: whether the labeled fixtures must stay in the graph for `eval_edges` (it uses its own walk with `excluded`, so it is unaffected). Consumers affected: every `loom map`, retrieval and hook that reads the graph; census totals will shift (generated rows move to the "excluded/generated" class).
- Part 3: tolerant extraction in `treesitter/mod.rs` `extract` (lines 150-190): when `root.has_error()`, still run the compiled query but skip captures whose byte range intersects an `ERROR`/`MISSING` node (`collect.rs:355` `first_error` already walks them), and report a new partial coverage so consumers can tell. This requires a new `FileCoverage` variant (`source_graph/node.rs:73`) and consumer updates (`map/views/outline.rs`, `census/mod.rs`, `context/coverage`), so it is the larger change; it also reverses a documented design decision (`extract/mod.rs:27` table), so record the decision. A cheaper alternative for the 28 files: retry the parse for `tsx` with the plain `LANGUAGE_TYPESCRIPT` grammar when the TSX parse fails and the file has no JSX; not applicable here (these test files contain JSX), so not recommended.
**Regression test:** Part 1+2: `loom/src/context/refresh/tests_source_graph/enumeration.rs` (helpers `init_repo`, `plant_outside_symlink`, `git_ok`): commit `gen/bundle.js` plus a `.gitattributes` line, assert it is absent from `resolved.files` after a base reconcile. Part 3: `loom/src/context/extract/tsx/tests.rs` (line 148 asserts the `parse-error` status today for a broken file): add a TSX file with `await original<typeof import("./x")>()` and assert symbols from the rest of the file are returned.
**Files a fix touches:** `loom/src/context/refresh/source_graph/enumerate.rs`, `.gitattributes` (new), tests; Part 3 adds `loom/src/context/extract/treesitter/mod.rs`, `collect.rs`, `source_graph/node.rs`, `map/views/outline.rs`, `census/*`.
**Size / tier:** Part 1+2 M, judgment; Part 3 L, judgment. Plan them as separate stages.
**Overlap:** `C6` (farewell emission), `C4`, `CR-48` (graph size), `C3`.

---

## CR-15 — PHP extractor `grammar_version` hardcoded `0.24.2` against `=0.25.1`

**Verdict:** CONFIRMED
**Evidence:** `loom/Cargo.toml:50` `tree-sitter-php = "=0.25.1"` and `loom/Cargo.lock` resolves 0.25.1 (the registry has both 0.24.2 and 0.25.1 sources). `loom/src/context/extract/php.rs:162` stamps `grammar_version: "0.24.2"`; `context/extract/dialect.rs:162` says `"tree-sitter-php 0.24.2 (php)"`. All 11 other extractors match their Cargo pin (checked rust, typescript, tsx, javascript, python, go, java, c-sharp, ruby, c, cpp). The version feeds `ExtractorIdentity::to_parser_version` (`extract/mod.rs:101-111`) which `parser_version_matches` uses to decide cache reuse, so a stale `php:0.24.2+...` layer entry is reused after the grammar changed. Practical effect is low (the query digest and `extractor_version` also feed the string), but the stamp is wrong.
**Fix seam:** change both strings to `0.25.1` (`php.rs:162`, `dialect.rs:162`). Entries stamped `php:0.24.2` then fail `parser_version_matches` and re-extract once; no migration code. Add a contract test that every dialect's `grammar` and every extractor's `identity().grammar_version` equals the pinned version in `loom/Cargo.toml` (parse with `toml_edit`, already a dependency, or `include_str!("../../../Cargo.toml")`). Put it in `loom/src/context/extract/dialect.rs` tests or `extract/tests.rs` (line 70 has a hand-built identity; the registry is `crate::context::extract::registry()`). Cross-check: dialect `grammar` strings are `tree-sitter-<crate> <version>`; compare with the manifest entry `tree-sitter-<crate>`.
**Regression test:** the contract test above; it fails today for php.
**Files a fix touches:** `loom/src/context/extract/php.rs`, `loom/src/context/extract/dialect.rs`, one test file in `loom/src/context/extract/`.
**Size / tier:** S; mechanical.
**Overlap:** none.

---

## CR-48 — Prompt-hook retrieval parses a 399 MB JSON view near the 5 s hook ceiling

**Verdict:** CONFIRMED (cited lines are at the right functions)
**Evidence:** `loom-hooks/user-prompt-context.sh:80` runs `loom_run_bounded 5 loom hook user-prompt`; on timeout it `exit 0`s with nothing (lines 80-83), so context is silently dropped. `context/retrieve/graph.rs:125-135` `load_graph` → `graph_store.view_read(read)`; `context/view/store.rs:78-90` `load_view` → `read_view` parses the whole persisted view. On disk: `.loom/cache/context-v1/graph/view/62be2a3d...-ad8d1f962b6d.json` is 399,011,359 bytes and the base layer 300,009,662 bytes. Measured with the installed binary: one `loom knowledge context` query 4.2 s wall (2.96 s user), eval of 23 cases 42 s. The prompt path adds `working_tree()` (`C3`) on top, so the warm hook is within about 1 s of the ceiling on this checkout, and a cold or fallback path (`load_graph` falling to `resolve_read`, which parses the base and overlay) exceeds it.
**Fix seam:** This is a storage redesign and belongs in its own plan. The view is a serialized `ResolvedGraph` with all nodes, edges and per-file entries as one JSON document (`context/view/`, `graph_store/`), read in full for every query. The review's two options are both new formats: a compact retrieval index (symbol id, path, kind, name, rank features) separate from the edge-resolution payload, or a binary/SQLite store with indexed lookup. Neither can be done as a contained patch because `retrieve/channels` source ranking, `windows.rs`, `rank_source`, `loom map` views and `view/materialize` all consume the full `ResolvedGraph`. Contained mitigations that are NOT the fix: shrinking the corpus (`CR-14` removes about 8 MB of noise only), skipping the source channel in the hook when no view exists. Scope the plan: define the retrieval-only projection, version it with the view identity, load it for `Surface::Hook`/CLI retrieval, keep the full view for `loom map`.
**Regression test:** a latency budget test needs a large synthetic graph fixture; no helper exists. Closest: `loom/src/context/view/tests_store.rs` and `tests_store_cache.rs` for view identity, `loom/src/commands/hook/tests_user_prompt_e2e.rs` for the hook path. Assertion would be wall-time or bytes-read for a hook query over a 100k-node graph.
**Files a fix touches:** `loom/src/context/view/*`, `loom/src/context/graph_store/*`, `loom/src/context/retrieve/graph.rs`, `loom/src/context/retrieve/channels/*`, `loom/src/context/rank_source/*`.
**Size / tier:** L; judgment (own plan).
**Overlap:** `C3`, `C5`, `CR-07`, `CR-73`, `CR-14`.

---

## CR-49 — `rescue_rarest` re-admits English function words and short tokens

**Verdict:** NARROWED
**Evidence:** `loom/src/context/rank/corpus/stopwords.rs:232-252` `rescue_rarest` restores up to `RESCUE_LIMIT` (3) dropped terms whose document frequency is at or below `corpus_size * stop_rescue_max_ratio` (0.25), with no `is_function_word` or length filter. `over_stopworded` (lines 187-195) triggers on an empty surviving set. The drop floor is `stop_df_ratio` 0.10 (`config.rs:24`), so a term between 10% and 25% document frequency is dropped and then rescued, including a function word or a 1-2 character token. On this repository's corpus the effect does not reproduce: `loom knowledge context --query "what is it"` returned `dropped_terms ['what','is','it']` with nothing rescued (those words exceed 25% document frequency here). It depends on the corpus; a small knowledge tree or a fresh project is where it bites. The rescue is deliberate for rare short identifiers: the doc at `stopwords.rs:229-231` says the ceiling "excludes ubiquitous terms, including short tokens", and the doc at lines 187-210 states rescued terms never count toward the hook's emit floor, so the unsolicited hook stays silent either way; only `loom knowledge context` and stage briefs rank on rescued words. The review's cited contradiction (`stopwords.rs:130-132`) is the doc for `partition_terms`, not a contract on rescue.
**Fix seam:** `rescue_rarest` at `stopwords.rs:232`: add `.filter(|term| !is_function_word(term))` (`stopwords.rs:100`, `pub(super) fn is_function_word(term:&str)->bool`) before collecting `rarest`. Do NOT add the review's `term.len() >= min_query_token_len` filter: it would delete the rescue of rare 1-2 character identifiers (`df`, `rg`), which the length-based drop at line 148 intends to put back when rare. Single caller: `partition_terms` (line ~170). Both corpus representations call it through `corpus.rs` so cache hit and miss stay in agreement.
**Regression test:** `loom/src/context/retrieve/channels/tests.rs` (line 70 already tests the short-token `hi` behavior; model the new case on it) with a small corpus where a function word has document frequency between 10% and 25%; assert it is not rescued and a rare content word still is. Fails today.
**Files a fix touches:** `loom/src/context/rank/corpus/stopwords.rs`, `loom/src/context/retrieve/channels/tests.rs`.
**Size / tier:** S; judgment (ranking behavior).
**Overlap:** `C6`.

---

## CR-50 — Skill trigger matcher misses punctuated words and substring-matches phrases

**Verdict:** CONFIRMED
**Evidence:** `loom/src/skills/matcher.rs:7-13` `normalize_text` lowercases and maps only `_` and `-` to spaces, so the prompt word `auth,` is a different token from trigger `auth` (`words.iter().any(|w| w == &trigger_normalized)`, line 45). Lines 41-43: `is_phrase_match = trigger_words.len() > 1 && normalized.contains(&trigger_normalized)`, a raw substring test, so trigger `go mod` matches the text `django model`. Production wiring is real: `skills/recommend.rs:13-20` `for_files` → `index.match_skills(text, ..)` called from `orchestrator/signals/generate.rs:145`. Many shipped triggers contain punctuation that must survive: `c++`, `c#`, `.net`, `node.js`, `ci/cd`, `std::span`, `package.json`, `openapi 3.1`, `node:test` (checked across `skills/*/SKILL.md` and `codex/skills`). Trigger normalization at `skills/index.rs:125` goes through the same `normalize_text`, so any change applies on both sides.
**Fix seam:** change `normalize_text` and `split_into_words` in `loom/src/skills/matcher.rs`, not a blanket punctuation strip (that would turn `c++` and `c#` into `c`). Tokenize on whitespace, `_` and `-`, then trim from each token leading and trailing `, ; : ! ? ( ) [ ] { } " ' \`` and a trailing `.`; keep interior punctuation and the leading `.` of `.net`. For phrases, compare word sequences: `words.windows(trigger_words.len()).any(|w| w == trigger_words)`, which also fixes `go mod` vs `django model`. Keep `normalize_text`'s public name (it is also used by `index.rs:10,125`). Existing `test_normalize_text` expectations (lines 85-91) stay valid.
**Regression test:** extend `loom/src/skills/matcher.rs` `mod tests` (uses `match_skills` with hand-built `trigger_map`): `match_skills("fix auth, now", {"auth"})` must match; `match_skills("django model", {"go mod"})` must not; `c++` and `node.js` triggers must still match `"port the c++ and node.js, code"`. First two fail today.
**Files a fix touches:** `loom/src/skills/matcher.rs`; optionally `loom/src/skills/index_tests.rs`.
**Size / tier:** S; mechanical (with the trigger-punctuation constraint above).
**Overlap:** `CR-52` (same module family).

---

## CR-51 — `strip_repeated_heading` fails on CRLF; knowledge fingerprint/catalog lack `O_NOFOLLOW` and size caps

**Verdict:** NARROWED (CRLF part confirmed; the symlink claim is mostly guarded; the unbounded read is real)
**Evidence:**
- `loom/src/commands/knowledge/mod.rs:102-113`: after a match, `after.starts_with('\n')` is false for `"\r\n"`, so a CRLF-pasted body keeps its duplicate heading line. Confirmed by reading; test `tests.rs:255` covers only `\n`.
- `loom/src/context/fingerprint.rs:37-43` `fingerprint_file` and `loom/src/fs/knowledge/catalog.rs:95-98` `process_file` use plain `fs::read` on a path built from `root.join(relative)`: no bound on size, path-based so a swap between enumeration and read follows a symlink.
- Guard for the symlink half: both enumerators (`fingerprint.rs:124-130`, `catalog.rs:278-282`) use `DirEntry::file_type()` which does not follow links, and require `is_file()`, so a planted leaf symlink or symlinked directory under the knowledge root is skipped. What remains is a time-of-check race, an ancestor symlink above `knowledge_root` (not checked), and unbounded size (a multi-GB `.md` would be read whole, and hashed twice).
- The prose side already does this right: `fs/knowledge/catalog/prose.rs:198` `read_bounded_at(&self.project_fd, &relative, MAX_EXTRACTED_FILE_BYTES)`.
**Fix seam:**
- Heading: in `strip_repeated_heading` (mod.rs:102-113) accept `"\r\n"` as well: `after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n")` and trim leading `\r`/`\n`.
- Reads: add one helper in `fs/knowledge/` (for example `read_knowledge_file(root: &Path, relative: &Path) -> Result<Vec<u8>>`) built on `crate::fs::safe_read::read_bounded(root, relative, MAX_KNOWLEDGE_FILE_BYTES)` (`safe_read.rs:59`, `read_bounded(root:&Path, relative:&Path, max_bytes:usize)->Result<Vec<u8>>`), and use it from `fingerprint_file` and `process_file`. Pick the cap (1 MiB is far above the 400-line tier limit) and decide the over-limit behavior: error with a clear message (a refresh failure) versus skip with a catalog issue; recommend a `CatalogIssue::OversizedFile`-style issue plus skip, so one bad file cannot block all retrieval. Also `index.rs:138,189` use `fs::read_to_string` on topics; route them through the same helper.
- Do this together with `C2` (same two call sites; the single-read design in `C2` option 1 makes this one read).
**Regression test:** `loom/src/commands/knowledge/tests.rs` near line 255: `strip_repeated_heading("## Merge Flow\r\n\r\nBody\r\n", "Merge Flow")` must not contain the heading (fails today). Reads: `loom/src/fs/knowledge/catalog/tests_prose_safety.rs` (has the oversize and symlink patterns, imports `MAX_EXTRACTED_FILE_BYTES`) as the model for a knowledge-file cap test.
**Files a fix touches:** `loom/src/commands/knowledge/mod.rs`, `loom/src/context/fingerprint.rs`, `loom/src/fs/knowledge/catalog.rs`, `loom/src/fs/knowledge/index.rs`, tests.
**Size / tier:** M (heading part alone is S); needs-exploration (over-limit policy).
**Overlap:** `C2`, `C1` (`index.rs`).

---

## CR-52 — Competing manifest registries; BOM failure on SKILL.md; unbounded SKILL.md read

**Verdict:** NARROWED (BOM confirmed; registry duplication confirmed but not a defect on its own; read hardening is low value)
**Evidence:**
- BOM: `loom/src/parser/frontmatter.rs:80-85` `split_frontmatter_raw` requires `first_line.trim() == "---"`. `U+FEFF` is not Unicode White_Space, so `trim()` leaves it and a BOM-prefixed file fails with "No frontmatter delimiter found". This is the shared owner for skills (`skills/index.rs:104-107` `parse_frontmatter`), knowledge (`fs/knowledge/frontmatter.rs:186`), handoff watch (`orchestrator/monitor/handoff_watch.rs:229`) and plans, so one fix covers all.
- Registries: `skills/project/markers.rs:121-132` `package_boundary` lists 5 manifests; `verify/context.rs:135-142` `find_project_root` lists 6; `verify/execution_paths.rs:46` lists 4 (hint text only); `language/evidence.rs:56-90` `MARKERS` lists 15 ecosystems. The lists serve different questions (package boundary for type detection; project-root search for acceptance commands; error hint; language evidence) and the divergence is real, but I found no failing behavior tied to it. Unifying changes semantics of `find_project_root` for many users.
- SKILL.md read: `skills/index.rs:96` `fs::read_to_string(path)`, no bound; `skills/index.rs:59-64` uses `skill_dir.is_dir()` (follows symlinks). The roots are the user's or installed skill directories (`~/.claude/skills`, `catalog_dir`), not session-writable repo paths (`.claude/skills` is write-denied in the session sandbox), so the exposure is small.
**Fix seam:** BOM: in `split_frontmatter_raw` (frontmatter.rs:78) strip one leading `\u{FEFF}` at entry (`let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);`) and slice from that str; the returned slices borrow `content`, so lifetimes are unchanged. Registries: only if wanted, make `markers.rs::package_boundary` and `verify/context.rs::find_project_root` read a shared list exported from `language/evidence.rs`; recommend deferring (judgment on semantics). SKILL.md: replace `read_to_string` with `read_to_string_bounded(root, relative, MAX)` (`fs/safe_read.rs:161`) only if the skill roots are treated as untrusted.
**Regression test:** `loom/src/parser/frontmatter_contract_tests.rs`: `split_frontmatter_raw("\u{FEFF}---\nname: x\n---\nbody")` returns Ok with yaml `name: x\n` and body `body`. Fails today.
**Files a fix touches:** `loom/src/parser/frontmatter.rs`, `loom/src/parser/frontmatter_contract_tests.rs`.
**Size / tier:** S for the BOM fix; M and judgment for the registry unification.
**Overlap:** `CR-50`, `CR-51`.

---

## CR-73 — `save_overlay` redundantly re-parses the base layer

**Verdict:** CONFIRMED
**Evidence:** `loom/src/context/graph_store/mod.rs:221-232` `save_overlay` calls `self.load_base(&layer.revision)?` (line 223; `load_base` → `read_layer_or_memory` → `read_layer`, parsing the base JSON from disk when it is not in the memory fallback, `fallback.rs:105-110`) solely to drop tombstones for paths the base does not hold. `refresh/source_graph/layer.rs:327-333` `persist_layer` already does the identical prune against the `base: Option<&GraphLayer>` it holds (`FileCoverage::Deleted => base.is_some_and(|base| base.files.contains_key(path))`) immediately before calling `save_overlay` (line 342). With a 300 MB base layer on disk, each overlay save re-parses it. `save_overlay` is `pub`; production code reaches it only through `persist_layer`; the other callers are tests (`graph_store/tests.rs`, `view/tests_store*.rs`, `local_overlay.rs:104,107`, hook/signals test support).
**Fix seam:** make `save_overlay` a plain write: delete lines 222-229 (the `load_base` and the `retain`), write `layer` directly via `write_or_fall_back(&self.overlay_path(plan, stage), layer)`. The prune stays in `persist_layer`. Move the pruning assertion from `graph_store/tests.rs:110` `a_tombstone_for_a_path_the_base_lacks_is_dropped_at_persist` to a `persist_layer`-level test (see below); `graph_store/tests.rs:~90-108` (tombstone resolved against base) calls `save_overlay` with a base-held tombstone and is unaffected. Alternative if direct callers must keep the guarantee: add `save_overlay_pruned(plan, stage, layer, base: Option<&GraphLayer>)` and call it from `persist_layer`; recommend the plain-write option since no production caller needs the guarantee.
**Regression test:** `loom/src/context/refresh/tests_source_graph/tombstones_freshness.rs`: reconcile a base, delete nothing in the base but make `persist_layer` receive a tombstone for an absent path (as `a_tombstone_for_a_path_the_base_lacks...` does at the store level) and assert the saved overlay lacks it; plus a counting assertion that `load_base` is not called during `save_overlay` (use the store with no base file for the revision: today `save_overlay` still tries to read; assert via a base file made unreadable that the save succeeds).
**Files a fix touches:** `loom/src/context/graph_store/mod.rs`, `loom/src/context/graph_store/tests.rs`, one test in `loom/src/context/refresh/tests_source_graph/`.
**Size / tier:** S; mechanical.
**Overlap:** `CR-48` (graph size), `C4` (same overlay persist path, `layer.rs:327`).
