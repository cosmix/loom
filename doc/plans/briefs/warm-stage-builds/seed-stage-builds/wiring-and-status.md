# Brief: wiring-and-status

Stage `seed-stage-builds` of `doc/plans/PLAN-warm-stage-builds.md`. Read the plan's "Decisions" section first. You connect the seeding functions to the stage start path, carry the host variables they read, and report the seed to the operator.

Another worker writes these functions while you work; call them by the signatures in the stage description's PUBLIC SURFACE block and do not open their files:

```rust
crate::stage_cache::seed::seed_worktree(repo_root: &Path, worktree: &Path) -> Vec<Seeded>
crate::stage_cache::seed::seed_host_caches(repo_root: &Path, cache_root: &Path,
    host_env: &BTreeMap<String, OsString>) -> Vec<Seeded>
crate::stage_cache::seed::seed_sources(repo_root: &Path,
    host_env: &BTreeMap<String, OsString>) -> Vec<String>
crate::stage_cache::seed::{Seeded { what: String, outcome: SeedOutcome }, SeedOutcome { Cloned, Skipped(String) }}
crate::fs::clone_tree::can_clone_in(dir: &Path) -> bool
```

Until that worker's files exist your code does not compile; write against the signatures and compile when the orchestrator says the tree is whole.

## Files you own

- `loom/src/orchestrator/core/build_seed.rs` (new), `loom/src/orchestrator/core/build_seed_tests.rs` (new)
- `loom/src/orchestrator/core/stage_executor.rs`, `loom/src/orchestrator/core/mod.rs` (the `mod build_seed;` line)
- `loom/src/stage_cache/environment.rs`, `loom/src/daemon/server/environment.rs`
- `loom/src/orchestrator/terminal/launch/build_cache.rs`, `loom/src/orchestrator/terminal/launch/mod.rs`
- `loom/src/commands/run/checks.rs`, `loom/src/commands/status.rs`

Read-only: `loom/src/stage_cache/seed_contract_tests.rs` is frozen.

## Step 1: host variables

`loom/src/stage_cache/environment.rs:29` holds `HOST_ENV_NAMES`, the nine names `host_env_from_process()` reads. Add `BUN_INSTALL_CACHE_DIR`, `npm_config_cache`, `UV_CACHE_DIR`, `GOCACHE` and `LOOM_BUILD_SEED`, and fix the array length. The daemon keeps only the variables in `HOST_ENV_ALLOWLIST` (`loom/src/daemon/server/environment.rs:8`, the Go and Gradle names are at lines 73-76): add the same five there. The existing test at `loom/src/daemon/server/environment.rs:269` lists the kept names; extend it, and extend the matching test in `loom/src/stage_cache/host_env_tests.rs` if it pins the list (that file is not yours: report the line if it needs a change).

`GOCACHE` here is the host's value, read to find the seed. It must not reach a stage's environment: `StageCache::environment` sets the stage's own `GOCACHE` under the cache root (`ROOT_DIRS`, same file, line 124) and that stays as it is. Check that adding the name to `HOST_ENV_NAMES` does not change any map `environment()` returns; if a test shows it does, stop and report.

## Step 2: seed a new worktree

`Orchestrator::start_worktree_stage` (`loom/src/orchestrator/core/stage_executor.rs:23-44`) calls `resolve_worktree`, then `pre_spawn_gates_passed`, which runs the before-stage checks and provisioning. The seed goes between them and runs only for a worktree this call created.

- Whether the worktree is new: `get_or_create_worktree` (`loom/src/git/worktree/operations.rs:250-281`) reuses a registered worktree and otherwise creates one, and does not say which. Decide it in `start_worktree_stage` before `resolve_worktree`: the worktree is new when `<repo_root>/.worktrees/<stage_id>/.git` does not exist at that moment. `operations.rs` is not yours and needs no change.
- In `build_seed.rs`, add a method on `Orchestrator` that takes the stage id and the worktree path, and:
  1. calls `seed_worktree(&self.config.repo_root, worktree)`;
  2. resolves the stage cache with `crate::verify::criteria::stage_cache_for(&self.config.work_dir, stage_id)` and `ensure()`s it, as `loom/src/orchestrator/core/provision_gate.rs:126` does, then calls `seed_host_caches` with `crate::stage_cache::host_env_from_process()`;
  3. prints one line per `Seeded` to stderr, where the daemon's other stage diagnostics go (`eprintln!` as at `loom/src/orchestrator/core/spawn_setup.rs:99`), naming the stage, `what`, and `cloned` or the skip reason.
- Wrap the whole method body in `busy::while_busy` with `BusyOp::Provisioning`, the way `provision_with` wraps each entry (`loom/src/orchestrator/core/provision_gate.rs:130`): a clone of a large tree takes several seconds and the daemon tick must stay fresh. Do not add a `BusyOp` variant; its wire name is read by the web client. `while_busy` must not nest (`loom/src/orchestrator/busy.rs:99-101`), so the seed call sits outside `pre_spawn_gates_passed`.
- Nothing here can fail the stage. An error resolving or ensuring the cache is printed and the stage starts cold.

## Step 3: tests (`build_seed_tests.rs`)

Use the fixtures the neighbouring orchestrator tests use (`loom/src/orchestrator/core/provision_gate_tests.rs` builds a stage repository; `crate::test_cache_env::CacheEnv::pin()` pins `HOME` so the stage cache lands in a temp directory).

- A main checkout with an ignored `target/marker` and a `Cargo.toml`: after the seed method runs for a fresh worktree, `<worktree>/target/marker` exists when `can_clone_in` is true for that directory, and does not when it is false.
- The decision "is this worktree new" is false for a worktree that already has its `.git` file, and the seed method is then not called: assert through the absence of `target` in a reused worktree whose main checkout has one.
- The daemon tick is stamped while the seed runs; `provisioning_stamps_the_daemon_tick` in `provision_gate_tests.rs` shows how that is observed.

Tests that set `HOME` are `#[serial_test::serial]`.

## Step 4: status lines

`sccache_status_line()` (`loom/src/orchestrator/terminal/launch/build_cache.rs`; `loom map --outline` gives its lines) is printed by `loom run` (`loom/src/commands/run/checks.rs`) and `loom doctor` (`loom/src/commands/status.rs`); `rg -n sccache_status_line loom/src/commands` finds both call sites.

- Add `pub(crate) fn build_seed_status_line(repo_root: &Path, work_dir: &Path) -> String` beside it, re-export it in `loom/src/orchestrator/terminal/launch/mod.rs` next to the `sccache_status_line` re-export, and print it at both call sites directly after the sccache line, with the same prefix each site uses. It starts with `build seed:` and then says one of:
  - `off (LOOM_BUILD_SEED=0)`;
  - `this filesystem cannot clone; every stage builds cold`, when `can_clone_in(work_dir)` is false;
  - `nothing to seed; build the main checkout once to warm new stages`, when `seed_sources` is empty;
  - the comma-separated `seed_sources` names otherwise, for example `build seed: loom/target, web/node_modules, bun, npm`.
  When `work_dir` does not exist (a repository that was never initialised) skip the clone probe and report the sources alone.
- Reword `sccache_status_line`. Its "not installed" text says each worktree compiles its dependencies from scratch and that installing sccache shares them. The tree does not do that: `RUSTC_WRAPPER` is never exported to a host job and reaches a session only where `sccache_usable_in` allows (same file). State what is true in both branches: found, it is exported to sessions that can reach its socket and used in a host job only where the project's own build names it as a compiler launcher; not found, nothing uses it. Keep the `sccache:` prefix. A test in the same file asserts on the line; update the assertion to the new text (the stage description records this as an expected integrity event).
- Unit tests for `build_seed_status_line`: each of the four outcomes, with a temp repository.

## Done when

From the repository root: `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::core::build_seed`, `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::terminal::launch::build_cache` and `cargo test --manifest-path loom/Cargo.toml --lib daemon::server::environment` pass. Files stay at most 400 lines (check `build_cache.rs` before adding to it) and functions at most 50. Report: files changed, each command with its result, anything unresolved or outside your files, any mistake worth recording.
