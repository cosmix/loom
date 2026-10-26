# W2: evaluation, engine, store lock, store writes, claims

Tier: opus (`loom-senior-software-engineer`). Read `doc/plans/briefs/plan-summary-store/common.md`
first. You start after W1 (format, store) has returned.

## Files you own

Under `loom/src/planning/summary/`: `evaluate.rs`, `evaluate/tests.rs`, `engine.rs`,
`engine/tests.rs`, `lock.rs`, `write.rs`, `claims.rs`, `claims/tests.rs`.

## `lock.rs`

`with_store_lock(paths, f)`: create the runtime directory chain if missing with
`crate::fs::safe_fs::safe_create_dir_all(loom_dir, "plan/<key>", 0o700)` (the runtime dir's parent is
`paths.dir.parent()`), open `paths.summaries_lock` with `OpenOptions::new().create(true).write(true)`
and `custom_flags(libc::O_NOFOLLOW)` and mode 0600, `fs2::FileExt::lock_exclusive`, run `f`, unlock
on every path (a guard type). Pattern: `quota/shared.rs::poll_shared` (`:129-160`), which opens a lock
file and calls `lock_exclusive` before its read-modify-write.

## `write.rs`

`pub(crate) fn write_summary(root, paths, id, text, own: Option<&OwnWrites>) -> anyhow::Result<()>`
and `pub(crate) fn delete_summary(root, paths, id, ...)`. Inside `with_store_lock`:
`safe_open_dirfd(root)`, `safe_create_dir_all(root, "doc/loom/summaries/<dirs>", 0o755)`, create
`doc/loom/summaries/<dir>/.<name>.tmp-<pid>` with `safe_create_new_in_workdir` (exclusive, no-follow;
every existing `.<name>.tmp-*` for this target, whatever its pid, is a temp of a crashed seal and is
removed first with `safe_remove_in_workdir`, listing the directory no-follow), then
`safe_rename_in_workdir` over the target. `safe_fs` refuses a symlink in any component, so a planted
link fails the write; surface that error unchanged. Register `(summary path, normalized_hash(text))`
with `own` BEFORE the rename (the watcher may walk between rename and register) and call
`own.unregister(path)` when the rename fails.
`write_summary_if(root, paths, id, expected_file_hash: Option<&str>, build: impl FnOnce() -> Result<String>, own)`:
the read-validate-write variant: under the lock, re-read the current summary file, compare its
`normalized_hash` with `expected_file_hash` (None = must be absent), and only then call `build` and
write; a mismatch returns `Ok(WriteOutcome::Changed)` without writing.

## `claims.rs`

Common.md "Locks, claims, drafts". Claim JSON `{ "summary", "session", "pid", "created_at" }`
(`session` null when `LOOM_PLAN_SESSION` is unset or empty; strip control characters and cap at 128
chars: it is untrusted text). `create_claim(paths, id) -> anyhow::Result<Result<Claim, Claim /* existing */>>`
with `safe_create_new_in_workdir` on a dirfd of `paths.dir` (`claims/<id with / -> __>.json`);
`EEXIST` -> read and return the existing claim. `list_claims`: no-follow listing
(`fs::safe_read::list_dir_no_follow`) and bounded reads (4 KiB); an unparsable claim still counts as a
claim on its file-name id with `session: None` (fail toward "claimed"). `release(paths, ids)`: when
`LOOM_PLAN_SESSION` is set, delete only claims whose session equals it (others -> error naming the
holder); when unset (a terminal), delete any. `clear_session_claims(paths, session)` deletes every
claim of that session (plan 10's server calls it). `delete_claim(paths, id)` for seal.

## `evaluate.rs`

`evaluate(project, claims) -> Evaluation` (common.md "Evaluation"), pure apart from no-follow reads
of the summary files (`discover::read_doc`):

1. For every expected id: read and `format::parse` the file; absent, unparsable or conflict-marked
   -> `Missing` with `error`.
2. Document summary: the source is the id's current path (`store::source_of`). A sentence is current
   when every `Source` link's span (`DocSections::find`) exists and hashes as `cites` records for that
   key (cites keys are matched by anchor, with the recorded path rewritten to the current source path
   when the summary's `of` names the same id's earlier path). A link whose `cites` entry is missing
   counts as stale.
3. Overview: a `Sentence` link is current when the member summary is not missing, holds that sentence
   id, and that sentence is current in the member's own evaluation (evaluate documents first). A
   `Source` link as in step 2 against the vision or the metaplan it names.
4. Summary state: `Missing` from step 1; else `Rebuilding` when a claim names the id; else `Stale`
   when any sentence is stale; else `Incomplete` when the document's `file` hash differs from the
   source's `file_hash`, or the overview's `members` map differs from the members computed now (same
   key set, same ordered-ids hashes); else `Current`.
5. Summary files present in the store for ids not expected are listed in `Evaluation` as
   `orphans: Vec<String>` (add the field; the engine deletes them).

## `engine.rs`

`SummaryEngine::on_batch(project, batch)`:

1. Rewrites: every document summary whose `of` differs from its source's current path and whose
   recorded `file` equals the source's current `file_hash` is rewritten: `of`, every `cites` key path,
   every body link target (recomputed with `store::link_from_summary`), through `write_summary_if`
   with the hash of the file it read (a seal that landed in between wins; the next batch retries)
   and `Some(&self.own)`. List the ids in `rewritten`. `batch.renamed` is a hint only; the hash rule
   decides (a server that was down missed the rename).
2. Deletions: every orphan from `evaluate` is deleted (`delete_summary`), listed in `deleted`.
3. `evaluation = evaluate(project, &list_claims(&self.paths)?)` after steps 1-2.
4. `invalidated`: ids whose state is Stale, Incomplete or Missing now and was not one of those three
   in the previous `on_batch` result (the first call compares with an empty map, so every
   out-of-date id is invalidated). Keep the previous states in `self`.

## Tests (yours)

Also: a stale temp with another pid removed before a write; registration before the rename and
unregister on a failed write; `write_summary_if` skipping when the file changed under it.


`evaluate/tests.rs`: section 04's sentence-state table (both columns) and summary-state table row
by row; an overview citing a member sentence that was reworded; a renamed plan with equal hash is
Current before any rewrite. `engine/tests.rs`: rewrite of a plan renamed into `archive/` (still named
by a metaplan), deletion of an archived plan that no metaplan names, `invalidated` across three
batches (only newly stale ids). `claims/tests.rs`: exclusive create, session-scoped release,
unparsable claim, `clear_session_claims`, a symlinked `claims/` directory refused.

## Proof

`cargo test --lib planning::summary::`, `cargo test --test planning_summary_contracts`,
`cargo clippy --all-targets -- -D warnings`.
