# Worktree Build Cold Start

> Cold

## Every Stage Worktree Builds From Cold, and the Contract Freeze Pays First

A newly created stage worktree contains sources without ignored build outputs. Its first compiler or dependency-install invocation pays the cold-build cost for outputs that remain worktree-local.

The contract freeze runs through `host_verify/freeze.rs::run_contracts` and `freeze_memo::run_memoized`: contracts with the same prepared build key reuse a captured `build_failed` report, while independently keyed contracts run separately. The format gate runs after contract evaluation. This removes repeated failed compiles within one freeze without warming the first build or sharing outputs across stage worktrees.

Revisit when a measured first build exceeds the project's acceptable stage-start budget. Preserve parallel stages, confinement, stage-keyed worktree identity and the operator's restriction against copying build trees or introducing a worktree pool.

## Build sharing: what the stage cache covers and what stays cold

Every stage has one build-cache root outside its worktree, shared by its session, its Linux workers and its confined host jobs, with the host's caches as read-only seeds and the operator's toolchains readable ([Stage Build Cache](../architecture/stage-build-cache.md)). It was built from option 3 below, with the write scope settled per stage. It is warm for dependencies and cold for everything a stage compiles itself:

- Go (`GOCACHE`, `GOMODCACHE` seeded from the host module cache), Maven (`repo.local.tail`), Gradle (`GRADLE_RO_DEP_CACHE`) and ccache resolve host-seeded dependencies. The Linux probe built and ran every toolchain probe except Maven's offline resolve (its seed lacked the plugins) and left the bubblewrap host-launcher tests and Go's `std_build_is_reused_by_the_second_command` unconfirmed; CI is the gate.
- Cargo's registry and git checkouts are the host's own, read in place (macOS) or bound at the host path (Linux), so dependencies stay warm; workspace crates in the dev profile always rebuild, because sccache does not cache incremental compilations and the target directory stays in the worktree.
- `RUSTC_WRAPPER` stays unset for host jobs (`process/environment.rs`), and a sandboxed session gets one only where `orchestrator/terminal/launch/build_cache.rs::sccache_usable_in` allows it. sccache serves host-job builds only where a project's own build names it (a CMake launcher), through the consumer's private socket.

What stays cold:

- Compiled objects on a stage's first build in each ecosystem: the stage's own crates, packages and C objects. The seed warms dependencies only.
- npm, pnpm, yarn, bun, deno, uv and pip downloads on a stage's first install: those tools have no seed in the table, so a fresh stage downloads or links from an empty cache. Installing needs a `provision` entry (network), and the downloads then stay in the stage's root for its later commands.
- Maven older than 3.9: it ignores `maven.repo.local.tail`, so the host repository is not a seed.
- Directories that live in the worktree: `node_modules`, `.venv`, framework build outputs (`.next`, vite, tsc incremental, turbo).
- macOS tools reached through an xcrun shim with a non-default SDK ([residual](sandbox-and-confinement-gaps.md#xcrun-shims-are-bypassed-not-cached)).

## Constraints set by the operator

- Parallel stages are common. A fix that assumes a sequential chain does not solve the problem.
- loom is used mostly on non-Rust projects. A fix must cover all ecosystems, not only Cargo.
- macOS and Linux are both supported. Linux confinement is bubblewrap; macOS is Seatbelt
  (`sandbox-exec`), with no overlayfs.
- No per-worktree copies of build directories. This machine's root is ext4 (no reflink), where a copy
  writes the full 17-20 GB per loom worktree.
- No worktree pool. loom keys worktrees by stage id (`.worktrees/<stage-id>/`, `loom/<stage-id>`,
  `LOOM_WORKTREE_PATH`, hooks, guards, Rule 10 of the doctrine), and leasing fixed slots breaks those.

## Where each ecosystem's cold cost lives

| Ecosystem | Cold cost in a new worktree | Tool-native shared cache |
| --- | --- | --- |
| JS/TS | dependency install; framework build caches (`.next`, vite, tsc incremental, turbo) | bun and pnpm link installs from a global store; npm keeps only a download cache, so installs stay slow |
| Python | venv creation and install | uv links from its global cache; a venv records absolute paths and cannot be moved |
| Go | compile | `GOCACHE` is content-addressed and safe for concurrent builds across directories |
| JVM | dependency resolution, compile | `~/.m2`, `~/.gradle/caches`, the Gradle build cache |
| Rust, C/C++ | compile | sccache (Rust, gcc, clang, MSVC, nvcc, HIP) or ccache; sccache does not cache incremental compilations, so Cargo workspace crates in the dev profile always rebuild |

Most ecosystems already ship a cache that is safe across parallel worktrees, and the stage cache gives
loom's sessions and host jobs one per stage. Rust and C/C++ are the ecosystems where a reachable cache
still leaves the workspace crates cold.

## Options considered

1. **Copy a warm build directory into each new worktree.** Rejected: on ext4 every copy writes the full
   size. `cp --reflink=auto` is free only on btrfs, xfs, bcachefs or ZFS 2.2+, so it cannot be the
   general mechanism.
2. **Move a finished predecessor's build directory into the next stage.** `rename` is instant and adds
   no disk. Rejected: it helps only a sequential chain, and parallel stages are common.
3. **Reach each tool's own shared cache from sessions and host jobs.** Built as the stage cache
   ([Stage Build Cache](../architecture/stage-build-cache.md)): one root per stage (the write scope is
   per stage, so a stage's own `build.rs`, postinstall script or test can poison only its own cache),
   the host's Go, ccache, Maven and Gradle caches as read-only seeds, toolchains under the operator's
   home readable, and one sccache server per consumer. Per-plan and global roots were rejected for the
   blast radius of a poisoned entry (the same class as [state-confinement-gaps](state-confinement-gaps.md)).
   It does not warm Cargo workspace crates, `node_modules` under npm, or framework build directories.
4. **Copy-on-write seed of the main checkout's build directories.** Dropped as the primary mechanism;
   kept as a possible addition for directories that option 3 leaves cold.
   - Linux: an overlay with the warm seed as the read-only lower layer and a per-worktree upper layer.
     bubblewrap 0.11.1 has `--overlay-src`, `--overlay`, `--ro-overlay` and `--tmp-overlay`;
     `fuse-overlayfs` is installed; kernels from 5.11 allow overlayfs in an unprivileged user namespace.
     macOS APFS: `clonefile(2)` (`cp -c`) clones a tree copy-on-write. CoW Linux filesystems: reflink.
   - Problems: three platform mechanisms to keep working. A bubblewrap overlay exists only inside its
     namespace, while the agent's Claude Code sandbox and loom's host jobs are separate namespaces, so it
     needs a host-side `fuse-overlayfs` mount per worktree with unmount on cleanup and crash recovery.
     Path-baked outputs do not survive a different worktree path: Cargo fingerprints workspace crates by
     path (they rebuild, dependencies stay warm) and Python venvs break, so the table must say which
     directories can be seeded. The seed goes stale as the main checkout moves; correctness rests on each
     tool's own invalidation and speed degrades until the seed is refreshed.
5. **A pool of reusable worktrees at fixed paths** (`.worktrees/slot-<n>`, `git clean -fd` keeping ignored
   build output). Rejected by the operator: it breaks the stage-id-keyed worktree model loom depends on.
6. **Pre-warm a stage's own worktree.** Open. At worktree creation, start a confined dependency build or
   install (`cargo test --no-run`, `bun install`, `uv sync`, `go build ./...`) in the background, while the
   contract writer writes or before the implementer's first build. Saves at most the time until the first
   real build (about 65 s in the measurement above); adds no disk beyond what the stage builds anyway;
   works with any number of parallel stages and on both platforms. Needs a per-ecosystem warm command,
   which overlaps with option 3's table, and a rule for what the first real build does while the warm-up
   still holds Cargo's build lock.
7. **Relocatable per-slot output directories outside the worktree** (`CARGO_TARGET_DIR`, Go's cache
   already global). Not pursued: only tools with a relocatable output directory benefit (`node_modules`
   and `.venv` cannot move), so it is not language-agnostic, and leased slots carry one stage's outputs
   into another.
8. **Build-system caches** (Bazel, Buck, Gradle build cache, Turborepo, Nx). A project that already uses
   one benefits once option 3 passes its cache through; loom cannot require one.
9. **Containers with cached image layers per lockfile.** Works on both platforms but runs through a Linux
   VM on macOS, with slower file I/O for every build. Worth weighing only alongside a container terminal
   backend.

## Contract-freeze specifics, independent of the above

`host_verify/freeze.rs::run_contracts` passes contracts through `freeze_memo::run_memoized`, keyed by `PreparedContract::build_key` (lines 204-240). Contracts sharing a failed build reuse the captured failure with a shared-from label; successful or independently keyed runs retain their own evaluation. `host_verify/tests_freeze_memo.rs` covers this owner. Cold compilation of the first target remains. Pre-warming during contract writing is an optional scheduling design, subject to concurrent source writes, build-lock ownership and cancellation.

## Next steps when this is picked up

- Measure the split between dependency and workspace-crate compile time in a cold worktree, to size what a seed of the main checkout's build directories (option 4) could still save for Rust.
- Decide whether npm, pnpm, yarn, bun, deno, uv and pip get host seeds; none has one today.
- Revisit condition: a stage whose first build in a fresh worktree exceeds about a minute in any ecosystem, or any change to `stage_cache::roots::TOOLCHAIN_ROOTS`, `HOST_ENV_NAMES`, `STAGE_HOST_ENV_ALLOWLIST` or `sccache_usable_in`.

## The Lib Test Crate's rustc Needs More Memory Than the macOS Runner Has

Building the `loom` lib test binary (`cargo test --lib --no-run`, the library plus about 14,400 in-crate tests in one rustc invocation) peaks at 10.4 GB resident with `debug = 2`, 9.2 GB with `line-tables-only` and 8.5 GB with `debug = 0`; `codegen-units = 16` changes nothing. Measured on a 16-core, 128 GB Mac with `CARGO_INCREMENTAL=0`, each build takes 63-76 s, so debuginfo does not change build time where memory is ample.

GitHub's `macos-latest` arm64 runner has 3 vCPUs and 7 GB RAM; `ubuntu-latest` has 4 vCPUs and 16 GB. In run 44879373671 the same build took 14m 01s on macOS (Flake check (macos)) against 3m 20s on Linux, and the `--all-targets` build took 19m 16s against 3m 07s: the macOS build swaps for most of its length.

How CI responds (`.github/workflows/`):

- `CARGO_PROFILE_DEV_DEBUG: "0"` in `.github/workflows/ci.yml` and `.github/workflows/nightly.yml` lowers the peak by about 2 GB. Panic messages keep file and line; backtraces lose line numbers.
- The macOS test suites (lib, integration, host-job lane) run in `.github/workflows/nightly.yml` (schedule, `workflow_dispatch`, pull requests), so no push waits on them. `.github/workflows/ci.yml` keeps the macOS lint, flake-check and hook jobs; Flake check (macos) still builds the lib test binary and is the longest push job.

Open options: a larger macOS runner (`macos-latest-xlarge`, billed per minute) removes the swap; moving the in-crate tests out of the lib crate, or splitting the crate, would bring a single rustc under 7 GB. Merging the 113 integration-test crates under `loom/tests/` into one would cut about 112 link steps on both runners.
