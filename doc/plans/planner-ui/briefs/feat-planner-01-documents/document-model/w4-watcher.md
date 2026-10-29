# W4: the polling watcher

Tier: opus (`loom-senior-software-engineer`). Read `doc/plans/briefs/feat-planner-01-documents/common.md`
first, then `doc/product/section_04.md` "The watcher". You start after W1 and W2 have returned; use
their `discover` skip rules and `sections::normalized_hash`.

## Files you own

`loom/src/planning/watch.rs`, `loom/src/planning/watch/tests.rs`.

## Behaviour

Signatures in common.md ("Watcher").

- **State.** A map `rel path -> (size, mtime_ns, inode, hash)` from the previous walk, a pending
  batch, and `OwnWrites` (an `Arc<Mutex<HashMap<PathBuf, String>>>` the server and plan 4's engine
  share).
- **Walk.** Call W1's `crate::planning::discover::walk_doc(root, WalkScope::IncludeSummaries)`
  (see w1-discovery.md section 4): the same no-follow walk and skip rules as `discover`, but it
  descends into `doc/loom/summaries/`, because outside edits to summary files are re-evaluated
  (section 04). Each `WalkEntry` carries `rel`, `size`, `mtime_ns`, `inode`, `via_symlink`. Hash
  only files that are new or whose `(size, mtime_ns, inode)` triple differs: read the BYTES with a
  bounded no-follow read (`fs::safe_read::read_bounded`, cap `MAX_DOC_BYTES`); a valid-UTF-8 `.md`
  file hashes with `sections::normalized_hash`, any other file with `context::source_graph::body_hash`
  of the raw bytes. A per-file failure (over the cap, a swapped symlink, unreadable) never fails the
  walk: the file gets the sentinel hash `unreadable:<size>:<mtime_ns>` and is reported as changed
  whenever its triple changes. Only a failure to list `doc/` itself is a walk error (pattern:
  plan 1 `Project::load` turns read failures into `read_error`).
- **Diff.** New path -> added; vanished -> removed; triple changed and hash changed -> changed; triple
  changed and hash equal -> nothing (a touch). After diffing one walk, pair every removed path with an
  added path of the same hash as a rename (first by sorted path when several share a hash) and move
  them to `renamed`.
- **Batching.** `poll` merges each walk's diff into the pending batch (an add then a remove of the
  same path cancels; a change after an add stays an add; a rename followed by an edit of the new path
  stays a rename plus a change). When a walk finds no difference and the pending batch is non-empty,
  return it and clear. The first `poll` returns `initial: true` with every file in `added` at once,
  without waiting for a quiet walk (start must catch up immediately, section 04 "Start").
- **Own writes.** Before reporting an added or changed path under `doc/loom/summaries/`, look its
  hash up in `OwnWrites`; on an equal hash remove the entry and drop the path from the batch; on a
  DIFFERENT hash remove the entry too (the registration is stale) and report the path. A path
  outside the store is never suppressed. Plan 4 registers before its rename and unregisters with
  `OwnWrites::unregister(&self, rel: &Path)` when the write fails (add that method).
- **Thread.** `spawn(self, interval, stop, on_batch)` runs `poll` every `interval` until `stop` is
  set, using `std::thread::Builder::new().name("loom-plan-watch".into())`. The callback receives
  batches only. A walk error does not stop the loop: the watcher keeps the last error message and
  exposes it through `pub fn last_error(&self) -> Option<String>` (and, for the spawned thread, a
  shared `Arc<Mutex<Option<String>>>` returned by `pub fn error_slot(&self)` before spawning).
  Batch lists are sorted.
- **Bounds.** Same depth and entry bounds as `discover`.

## Tests (yours, `watch/tests.rs`)

Drive `poll` directly (no sleeps beyond what mtime granularity needs; set mtimes explicitly with
`nix::sys::stat::utimensat` or rewrite content with a different length so size changes).
Cases: a PNG and a 9 MiB file present from the first poll (initial batch arrives; later edits
still batch); a stale registration dropped; initial batch; add/change/remove; rename across directories; rename plus edit in one quiet
window; a touch with identical content reports nothing; editor temporaries and dotfiles never
reported; an escaping symlink never reported; own-write suppression consumed once (the same hash
written again later is reported); a summary file changed by "someone else" reported; `spawn` stops
when `stop` is set (join within 2 s).

## Proof

`cargo test --lib planning::watch::`, `cargo clippy --all-targets -- -D warnings`. Report anything you
needed from W1's files that `walk_doc` did not provide.
