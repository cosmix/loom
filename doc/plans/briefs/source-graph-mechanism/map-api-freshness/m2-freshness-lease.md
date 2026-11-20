# M2: freshness states, stale serving, and the reconcile lease

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 1, alongside M1 and M3.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 9 in
  full, and the `snapshot` object and stale-serving rule in section 10.

## Goal

`current`, `stale`, `never built` and `unavailable` become distinct, explicit states
on every surface. A failed build serves the newest older base labelled stale. A
never-built graph never triggers a detached rebuild from the prompt hook. The
background reconciler gets exponential backoff, a bounded pending queue of one, and
`--cancel`.

## Files you own (write)

- `loom/src/context/freshness.rs`, including its full `Freshness` literal
- `loom/src/context/refresh.rs`: the three full `Freshness` literals, the
  `semantic_freshness_against_head` producer (step 1), the `tests_states` declaration,
  and the existing `pub(crate) use source_graph::{...}` line, to which you add
  `excluded, EXCLUDED_ROOTS`. `mod source_graph;` is private, so M3 (who edits
  `refresh/source_graph.rs`) makes both items `pub(crate)` and imports them as
  `crate::context::refresh::{excluded, EXCLUDED_ROOTS}`; your build resolves that line
  only once M3 has finished
- `loom/src/context/refresh/snapshot.rs` (including its full `SnapshotOutcome`
  literals), `refresh/snapshot/describe.rs`, and a new `refresh/snapshot/state.rs` if
  `snapshot.rs` would pass 400 lines
- `loom/src/context/refresh/tests_snapshot.rs` (including its full `SnapshotOutcome`
  literals), `tests_freshness.rs`, and a new `refresh/tests_states.rs` declared from
  `refresh.rs`
- `loom/src/context/graph_store/fallback.rs`: add
  `pub fn fell_back(&self) -> bool` on `GraphStore`
- `loom/src/context/tests/store.rs`: its full `Freshness` literal
- `loom/src/context/tests/retrieve_source.rs`:
  `retrieve_for_stage_is_not_degraded_when_the_semantic_layer_was_never_built` (a
  never-built, empty graph is degraded)
- `loom/src/orchestrator/signals/format/brief.rs`: `freshness_word` only, and
  `brief_tests.rs` for the freshness-word assertions only
- `loom/src/commands/knowledge/context.rs`: the freshness lines only (anchor on
  `print_freshness_line` and `format_degraded`, never on line numbers)
- `loom/src/commands/knowledge/bootstrap/tests.rs`: the full `SnapshotOutcome` literal
  in `fn snapshot`
- `loom/src/context/retrieve/graph.rs`: `degraded_reason` only
- `loom/src/commands/hook/reconcile_graph.rs`, `reconcile_graph/lock.rs`, the new
  `reconcile_graph/tests_wants_rebuild.rs` (declared from `reconcile_graph.rs`), and
  their tests (`commands/hook/tests_reconcile_graph.rs` and the lock test module)
- `loom/src/cli/types_ops.rs`: the `ReconcileGraph` variant only (add
  `#[arg(long)] cancel: bool`)
- `loom/src/cli/dispatch*.rs`: the one `HookCommands::ReconcileGraph` arm only. Find
  it with `rg -n 'HookCommands::ReconcileGraph' loom/src/cli`: sibling plan worker C
  splits `dispatch.rs` into `dispatch_commands.rs`, so the arm may have moved

Read-only: everything else, including `loom/tests/map_api_contracts.rs` (frozen). It
pins `loom::commands::hook::reconcile_graph::wants_rebuild(&Freshness, Option<&str>) -> bool`,
and `snapshot.state == "current"` and `base_revision` in `loom map --json`.

You own every construction site your new fields break. `SnapshotOutcome` has no
`Default`: add the two new fields (`persisted`, `serving`) at each full literal. Add
`unavailable` to each full `Freshness` literal.

## Steps

1. **`freshness.rs`.**
   - Add `GraphState` with `as_str()` (exact strings in design 9).
   - Add `Freshness.unavailable` as `#[serde(skip)]` (never persisted),
     `Freshness::state()` and `Freshness::unavailable(detail)`.
   - Producer: `refresh::semantic_freshness_against_head`. When
     `source_graph::head_revision(project_root)` returns `None`, it returns
     `Freshness::unavailable(detail)` (revision kept) instead of the stored value.
   - Unit tests for every state.
2. **`SnapshotOutcome`.**
   - Add `persisted: bool`, `serving: Option<String>` and `state()`.
   - Set `persisted` from `graph_store.fell_back()` after the build.
   - On a failed base build (`unavailable*` paths in `snapshot.rs`), load the newest
     older base (`graph_store.load_newest_base()`, which skips corrupt and
     stale-schema layers since stage `graph-contract`). When it exists, set
     `revision` to it and `serving: Some(rev)`, keeping `action: Unavailable` and the
     reason.
   - `describe()` appends the clauses in design 9.
   - Tests in `tests_states.rs`. Acceptance runs the first two with `--exact`, so use
     these names:
     - `failed_build_serves_newest_older_base_as_stale`: a build failure with an older
       base yields `revision` = that base, `serving == Some(..)` and
       `state() == Stale`. Force the failure by making the working-tree inspection
       fail after a first successful build, or through the existing test seam in
       `tests_snapshot.rs`; read how those tests force `Unavailable`;
     - `read_only_cache_reports_not_persisted`: a permission-denied write yields
       `persisted == false` and the `describe()` suffix (reuse the fallback test seam
       in `graph_store/fallback_tests.rs`);
     - without a base the state is `NeverBuilt`.
3. **Brief and CLI surfaces.**
   - `brief.rs::freshness_word` prints `state().as_str()`. Update the brief tests
     that pin `stale`/`current`: the brief test files are in
     `orchestrator/signals/format/brief_tests.rs`, which is also yours for that
     assertion only.
   - `commands/knowledge/context.rs` prints `Semantic: never built (<detail>)` and the
     like.
   - `retrieve/graph.rs::degraded_reason` keeps its signature. Its never-built reason
     fires only when `semantic_revision` is empty AND `graph.files.is_empty()`, with
     the fixed text `source graph never built; run loom map to build it`. An
     overlay-backed read with an empty semantic revision is not degraded.
   - Do not edit knowledge files: that is the distill stage's job.
4. **Hook** (`reconcile_graph.rs`).
   - Add `pub fn wants_rebuild(freshness: &Freshness, degraded: Option<&str>) -> bool`
     per design 9, and make `spawn_if_needed` call
     `wants_rebuild(&pack.semantic_freshness, pack.degraded.as_deref())`. Nothing else
     decides the spawn.
   - Truth table: `Stale` gives true; `Current` gives true exactly when `degraded` is
     `Some`; `NeverBuilt` and `Unavailable` give false whatever `degraded` holds.
   - Unit-test the full table:
     - `Freshness { revision: "abc".into(), stale: true, ..Default::default() }` gives
       true;
     - a current `Freshness` (revision `abc`) gives true with `Some("degraded")` and
       false with `None`;
     - `Freshness::never_built("x")` and `Freshness::unavailable("x")` give false with
       and without `degraded`.
5. **Lease** (`lock.rs`).
   - The lock line becomes `"<epoch> <pid> <failures> <pending>"`. A line that does
     not parse as four fields is treated as no lock; add no two-field compatibility
     parsing (`loom/CLAUDE.md` forbids migration routines).
   - Every lock write goes through `fs::locking::locked_write` (a directory lock plus
     an atomic rename), never through the remove, `create_new` and `write_all`
     sequence of `claim_lock`: between those steps a reader sees no file or an empty
     one, `decide` returns `Spawn`, and a second reconcile runs beside the live holder.
   - Every write is read-modify-write under that one lock: it carries `failures` and
     `pending` forward. `mark_pending` keeps the holder's epoch and pid.
   - `decide`:
     - honours `debounce_secs * 2^min(failures, 5)`, capped at 21600;
     - a skip against a live holder records `pending = 1` through a new
       `mark_pending`.
   - `reconcile()` returns the `SnapshotOutcome`; a run failed when
     `outcome.state() != GraphState::Current` (serving a stale base means the build
     failed):
     - the holder keeps its pid through every pass. After a pass, under the lock, it
       reads the line. A line naming another pid (a takeover) is left as is, and the
       holder exits. With `pending == 1` it writes its own pid, a fresh epoch,
       `pending 0` and the pass's failure count, then runs one more pass. With
       `pending == 0` it writes `pid 0` and the final failures: 0 after a success,
       the carried count plus one after a failure (design 9);
     - the hook still exits 0; failure accounting is internal.
   - `--cancel`: `pub fn reconcile_graph(cancel: bool)`. When `cancel`, read the
     lock; signal the holder pid only when its NUL-split argv (from
     `/proc/<pid>/cmdline` on Linux, else `ps -p <pid> -o command=`) contains both
     `hook` and `reconcile-graph`, and the process started no later than the lock's
     epoch. Send `SIGTERM` with `nix::sys::signal::kill`. `nix` 0.31 with the `signal`
     feature is already a dependency; never add one. Any other pid (a reused pid
     naming the daemon or `loom stage complete`) is left alone. Either way, write
     `pid 0` with `failures` unchanged and exit 0.
   - Wire `cancel` through `cli/types_ops.rs` and the dispatch arm.
   - Tests. `tests_reconcile_graph.rs` is 394 lines, so new tests go in
     `reconcile_graph/tests_wants_rebuild.rs` (declared from `reconcile_graph.rs`):
     - backoff doubles and caps;
     - `two_failed_runs_count_two_failures`: two failed runs through `try_spawn` and
       `try_reconcile` end with `failures == 2`;
     - `never_built_degraded_pack_spawns_nothing`: `LOOM_WORK_DIR` set, a never-built
       degraded pack, and `SUPPRESSED_SPAWNS` unchanged after `spawn_if_needed`;
     - `pending_is_never_lost_across_threads`: two threads, one marking pending while
       the other finishes; `pending` survives and no reader sees an empty or partial
       lock;
     - `cancel_skips_a_decoy_process`: record a spawned `sleep 30` child's pid as the
       holder, run the cancel path, assert the child is still alive, then kill and
       reap it (no process may outlive the test);
     - pending is recorded on a skip and consumed once;
     - `--cancel` with no holder is a no-op;
     - a lock line that does not parse as four fields is no lock.

     In `tests_reconcile_graph.rs`, give `degraded_pack()` a semantic revision `"abc"`:
     without it the spawn tests at the refused and allowed cases pass vacuously or
     fail. Acceptance runs the four named tests (`never_built_degraded_pack_spawns_nothing`,
     `two_failed_runs_count_two_failures`, `pending_is_never_lost_across_threads`,
     `cancel_skips_a_decoy_process`) with `--exact`.

## Traps

- `detached-spawn-in-tests`: no process a test spawns may outlive the test. Reap every
  child. `SPAWN_ENABLED` is already false under `cfg(test)`; keep it that way.
- `brief.rs` output is pinned by doctrine and brief tests. Change only the freshness
  word, and grep `rg -n '"stale"|"current"' loom/src/orchestrator/signals` for other
  pins.
- `cli/dispatch.rs` is also edited by sibling plans (`PLAN-model-router-hooks.md`,
  `PLAN-web-host-graft-followthrough.md`, whose worker C splits it into
  `dispatch_commands.rs`). Touch the one `HookCommands::ReconcileGraph` arm only,
  wherever `rg -n 'HookCommands::ReconcileGraph' loom/src/cli` finds it.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::refresh:: 2>&1 | tail -15`

## Report

Report the files changed, the final API, and every pinned string you updated.
