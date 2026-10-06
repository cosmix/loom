# Source Graph Build

> Layer builder, freshness, lease

Builder and freshness of the source graph: how a layer is enumerated, reused and persisted,
which states it reports, and the lease that keeps the prompt hook from stacking rebuilds.
Contract and lifecycle are in [Source Graph](source-graph.md).

## Building and Persisting

`refresh::source_graph::reconcile_source_graph(store, graph_store, project_root, scope)`
is the builder: inspect the working tree, enumerate files through git, reuse or
re-extract each one, and persist the resulting `GraphLayer` through `GraphStore`.
`ensure_snapshot` (see *Lifecycle* in [Source Graph](source-graph.md#lifecycle-who-builds-it-and-when)) is the policy layer most callers go through; it
drives the same builder.

- **Working tree** (`refresh/source_graph/generation.rs::working_tree`): `HEAD` plus
  every dirty path from `git status --porcelain=v1 -z --untracked-files=all` (a
  rename records both paths). Its `generation` is a sha256 over `HEAD` and one
  `<status> <path> <content-hash|deleted>` line per dirty path; a clean tree's
  generation is `clean_generation(HEAD)`. Comparing generations is how an overlay
  proves it is current without re-walking anything.
- **Enumeration** (`refresh/source_graph/enumerate.rs`), path → git blob id:
  - `SourceGraphScope::Base { revision }` lists **committed** content
    (`git ls-tree -r -z HEAD`), and `build_layer` reads a dirty path's bytes with
    `git show HEAD:<path>` rather than from disk. A base therefore always describes
    committed `HEAD` and can be published from a dirty checkout.
  - `SourceGraphScope::Overlay { plan, stage }` lists the index
    (`git ls-files -s -z`) plus **untracked** files
    (`git ls-files --others --exclude-standard -z`, existing and not excluded), and
    records index paths missing from disk as deleted.
- **Tombstones.** In an overlay, every deleted path — an index path missing from
  disk, or a dirty path absent from disk — becomes `FileEntry::tombstone()`: no
  nodes, empty hash, `FileCoverage::Deleted`. `persist_layer` keeps a tombstone only
  when the base has that path, and keeps any other entry only when it differs from
  the base's; `GraphStore::save_overlay` applies the same tombstone filter.
  `GraphStore::resolved` REMOVES a tombstoned path from the resolved view instead of
  shadowing it, so a file a stage deleted no longer shows up in `loom map` or the
  source channel.
- **`GraphLayer`**: `revision` (a base's commit; for an overlay, the `HEAD` it was
  cut from), `generation` (the tree generation for an overlay, empty for a base),
  `built_at`, `files` (path → `FileEntry`), and `blob_index` (path → the git blob id
  whose bytes produced that entry). An overlay write is skipped when revision,
  generation, files and `blob_index` all equal the previous overlay's;
  `GraphStore::publish_base` never overwrites a revision already published.
- **`SourceGraphOutcome { nodes, edges, freshness, counters }`** describes the layer
  as built by THIS call; `counters.bytes_serialized` is 0 when nothing was written.
  A working-tree or enumeration failure is **data, not a crash**: the outcome is
  degraded, with `Freshness::never_built(detail)` and zero counts, and the stored
  semantic layer is marked stale.
- **`SourceGraphCounters`**: `files_enumerated`, `files_hashed`, `files_parsed`,
  `files_reused`, `files_deleted`, `files_untracked`, `bytes_serialized`, and
  `enumerate_ms` / `hash_ms` / `parse_ms` / `persist_ms`.
  `SnapshotOutcome::describe` prints the parsed, reused and deleted counts on its
  advisory line.
- `EXCLUDED_ROOTS` = `.loom`, `.work`, `.worktrees`, `target`, `node_modules`, `.git`
  (`refresh/source_graph.rs`), matched against the FIRST path segment only, applied to
  enumerated, untracked and dirty paths alike. `.loom` and `.work` are two separate
  top-level entries; there is no compound `.loom/work` entry.
- `context` reaches `git` only through `git::runner::run_git_checked`, from
  `enumerate.rs`, `generation.rs` and `layer.rs` under `refresh/source_graph/`. That
  is a deliberate downward edge, not a layering violation.

## Freshness States

`GraphState { Current, Stale, NeverBuilt, Unavailable }` (`context/freshness.rs`, `as_str()` gives
`current`, `stale`, `never built`, `unavailable`) is the one vocabulary every surface prints.
`Freshness::state()` returns `Unavailable` when the non-persisted `unavailable` flag is set, else
`NeverBuilt` when `revision` is empty, else `Stale` when `stale`, else `Current`.
`refresh::semantic_freshness_against_head` produces `Unavailable` (keeping the stored revision)
when `HEAD` cannot be read. `SnapshotOutcome::state()` maps `Reused`/`Updated`/`Rebuilt` to
`Current`, a failed build that serves an older base to `Stale`, a failed build with no base to
`NeverBuilt`, and an uninspectable working tree (no git, not a work tree) to `Unavailable`. A failed
build keeps `SnapshotAction::Unavailable` with or without `serving`, so bootstrap's
`require_snapshot` still refuses it.

Surfaces: the Knowledge Brief header (`orchestrator/signals/format/brief.rs::freshness_word`) and
`loom knowledge context` print `state().as_str()`; `loom map` serves the older base with
`state: "stale"` after a failed build, and with no base prints `source graph never built: <reason>`
on stderr, emits `state: "never built"` and exits 0 with empty views. `retrieve/graph.rs::degraded_reason`
reports a never-built graph only when the semantic revision is empty AND `graph.files` is empty
(`source graph never built; run loom map to build it`); an overlay-backed read with an empty
semantic revision is not degraded.

## The Reconcile Lease

`loom hook reconcile-graph` (`commands/hook/reconcile_graph.rs`, `.../lock.rs`, `.../cancel.rs`) heals
the graph from the prompt hook. `wants_rebuild(freshness, degraded)` is the single spawn predicate
and `spawn_if_needed` calls nothing else:

| `freshness.state()` | `degraded` | rebuild |
| --- | --- | --- |
| `Stale` | any | yes |
| `Current` | `Some` | yes |
| `Current` | `None` | no |
| `NeverBuilt`, `Unavailable` | any | no |

A never-built graph is built only by explicit commands: `loom init`, `loom run`, `loom map` and
`loom knowledge sync`.

**The lease** is one line, `"<epoch> <pid> <failures> <pending>"`, in `reconcile.lock` under the
context cache; a line that does not parse as four fields is no lock. Every write is a
read-modify-write under ONE directory flock (`fs::locking::locked_dir_update` plus
`atomic_write_locked`, `lock.rs::update_lock`), never a remove / `create_new` / write sequence:
between those steps a reader sees no file, `decide` returns `Spawn`, and a second reconcile runs
beside the live holder. `locked_write` cannot be nested inside the read because it takes its own
flock and would deadlock.

- `decide` skips a live holder younger than `stale_lock_secs`, spawns over a dead or stale one, and
  throttles a finished run (`pid 0`) for `backoff_secs = debounce_secs * 2^min(failures, 5)`, capped
  at 21600 s. A success resets `failures` to 0; a run counts as failed when
  `outcome.state() != Current` (serving a stale base is a failure).
- A request skipped while a live holder runs sets `pending = 1` through `mark_pending`, which keeps
  the holder's epoch and pid. The holder keeps its pid across passes: after a pass it reads the line
  under the lock; another pid (a takeover) means it exits, `pending == 1` means it writes a fresh
  epoch, `pending 0` and the failure count and runs one more pass (a queue bounded at one), and
  `pending == 0` means it writes `pid 0` with the final failures.
- `--cancel` sends `SIGTERM` only to a live holder whose NUL-split argv (`/proc/<pid>/cmdline`, else
  `ps -p <pid> -o command=`) contains both `hook` and `reconcile-graph` and which started no later
  than the lease epoch; a reused pid naming the daemon or a stage-completion process is left alone.
  It then records the run finished (`pid 0`, `failures` unchanged). With no live holder it is a
  no-op. Failure accounting is internal: `loom hook reconcile-graph` exits 0 whatever the outcome.
- The residual gaps (a sub-millisecond PID-reuse window, `read_lock` following symlinks, an
  unbounded `ps`) are in [Source Graph Review Backlog](../concerns/source-graph-review-backlog.md).
