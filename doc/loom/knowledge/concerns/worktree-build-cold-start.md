# Worktree Build Cold Start

> Cold builds

## Every Stage Worktree Builds From Cold, and the Contract Freeze Pays First

**Symptom.** A contract phase that writes three tests takes about 3.5 minutes. Measured on loom, stage
`web-server-extract` (plan "`loom plan` server foundation and access"):

| Step | UTC | Elapsed |
| --- | --- | --- |
| Contract writer session starts | 04:15:56 | |
| Writer calls freeze; host Freeze job `started_at` | 04:17:01 | about 65 s writing three tests |
| `<worktree>/loom/target/` created | 04:17:02 | |
| Job writes `contract-reports.json` | 04:19:22 | about 2 min 21 s |
| `freeze.json` written, implementer starts | 04:19:27 | 5 s |

All three contracts ended `build_failed` (exit 101), the expected red. The worktree `target/` held
17 GB in `debug/deps`, 3.1 GB in `debug/incremental` and 274 MB in `debug/build` afterwards; the main
checkout's `loom/target` is 98 GB. Evidence lives in
`.loom/work/verification/<stage>/<job>/record.json` (`started_at`, `finished_at`) and the mtimes of
`contract-reports.json` and the worktree `target/`.

**Cause.** A stage worktree starts with sources only. The freeze job
(`host_verify/freeze.rs::run_contracts`) is the first process that compiles in it, so it pays the cold
build of every dependency and the workspace crate. It then runs each contract as its own command, one
after another, plus the format gate (`format_outcome`). The cost is not specific to the contract phase:
whichever step compiles first in a new worktree pays it, in every ecosystem whose build state lives in
the worktree.

## Build sharing does not happen today

- Sandboxed sessions get no `RUSTC_WRAPPER`: `orchestrator/terminal/native/build_cache.rs::sccache_usable_in`
  is `!sandbox.enabled || network.allow_all_unix_sockets` (the gate added after the AF_UNIX denial broke
  every cargo command, see mistakes/sandbox-tooling-and-network.md).
- Confined host jobs (freeze, completion, criteria) start from `env_clear()`, and
  `process/environment.rs::STAGE_HOST_ENV_ALLOWLIST` forwards `SCCACHE_DIR` and `SCCACHE_CACHE_SIZE` but
  never `RUSTC_WRAPPER`, by design (comment at the allowlist). The running daemon's environment holds no
  `RUSTC_WRAPPER` either.
- Even with the variable set, a host job could not reach an sccache server on the host: Linux confinement
  is bubblewrap `--unshare-all` with `--share-net` only for `NetworkAccess::Provision`
  (`verify/criteria/confine_host.rs::platform_command`); the macOS Seatbelt profile allows
  `network-outbound` only for `Provision`.
- Result on this machine: `~/.cache/sccache` holds 4 KB, so nothing has ever been cached.
  mistakes/ci-toolchain-and-cargo.md, "Every Stage Worktree Compiled Its Dependencies From Scratch",
  names sccache as the fix; the gates above leave it inert in both places that compile.

**What a confined host job can reach** (`confine_host.rs::read_roots`, `writable_roots`): read-only
`~/.rustup`, `~/.bun/bin`, `~/.bun/install/cache`, `~/.cache/uv`, `~/Library/Caches/uv`,
`~/.local/share/uv/python`, and `$CARGO_HOME/{bin,registry,git}`; writable only the working tree, the
command scratch and the linked Cargo home. No root covers `GOCACHE`/`GOMODCACHE`, `~/.npm`, the pnpm
store, `~/.gradle`, `~/.m2` or a ccache directory. Unverified at runtime: what a Go, npm, pnpm, Gradle or
Maven build does inside the jail with those caches missing, and whether a uv or bun cache miss there
fails or silently refetches.

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

Most ecosystems already ship a cache that is safe across parallel worktrees; the gap is that loom's
sessions and host jobs cannot reach it. Rust and C/C++ are the ecosystems where a reachable cache still
leaves the workspace crates cold.

## Options considered

1. **Copy a warm build directory into each new worktree.** Rejected: on ext4 every copy writes the full
   size. `cp --reflink=auto` is free only on btrfs, xfs, bcachefs or ZFS 2.2+, so it cannot be the
   general mechanism.
2. **Move a finished predecessor's build directory into the next stage.** `rename` is instant and adds
   no disk. Rejected: it helps only a sequential chain, and parallel stages are common.
3. **Reach each tool's own shared cache from sessions and host jobs.** Open; the recommended first step.
   - A data-driven table per ecosystem names the cache directories and environment variables
     (`GOCACHE`, `GOMODCACHE`, `UV_CACHE_DIR`, `BUN_INSTALL_CACHE_DIR`, the pnpm store, `npm_config_cache`,
     `GRADLE_USER_HOME`, `~/.m2`, `SCCACHE_DIR`, `CCACHE_DIR`). Sessions and host jobs mount (bubblewrap)
     or allow (Seatbelt `file-read*`/`file-write*` subpath rules) those paths and forward the variables.
     `read_roots` already holds a hand-written subset (bun, uv, Cargo registry); the table would replace it.
   - Rust and C/C++: the host job starts its own sccache server inside the jail, so no socket crosses the
     confinement and `--unshare-all` stays. On macOS sccache talks to its server over localhost TCP, which
     the Seatbelt profile must allow (`network*` to localhost) for those jobs. In sessions it stays gated
     by `sccache_usable_in` unless a socket-free path exists; sccache's local read-only mode
     (`SCCACHE_LOCAL_RW_MODE=READ_ONLY`, availability in 0.13 unverified) would let sessions read without
     writing.
   - Coverage: dependencies in every ecosystem, on both platforms, for any number of parallel stages. It
     does not warm Cargo workspace crates, `node_modules` under npm, or framework build directories.
   - Open decision, cache write scope. Any cache that a stage's own code can write can be poisoned: a
     `build.rs`, a postinstall script or a test writes a bad entry under a valid key, and a later stage or
     the operator's own build links it (the same class as
     [state-confinement-gaps](state-confinement-gaps.md)). Choices: one global cache (warmest, widest blast
     radius), one cache per plan (parallel stages of a plan share it; damage stays inside the plan), or
     writes only from host jobs with sessions read-only (host jobs still run agent-written build scripts,
     so this narrows the vector without closing it).
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

- The freeze runs every contract through its own command (`run_contracts`). When contracts share one
  test target and the first run is `build_failed`, the remaining runs repeat the same failed compile.
  Grouping contracts by resolved target and reusing a `build_failed` result per group saves a few seconds
  per extra contract once the build is warm.
- Starting the freeze's compile while the writer still writes (option 6 scoped to the contract phase)
  hides most of the writer's time behind the cold build.

## Next steps when this is picked up

- Probe what a confined host job reaches today, per ecosystem: run a Go, npm, pnpm, uv, bun and Gradle
  build through `loom check <stage> --no-cache` on a fresh worktree and record which caches are hit, missed
  or unreachable.
- Measure the loom split between dependency and workspace-crate compile time in a cold worktree, to size
  what option 3 can save for Rust.
- Decide the cache write scope in option 3 before any cache becomes session-writable.
- Revisit condition: a stage whose first build in a fresh worktree exceeds about a minute in any
  ecosystem, or any change to `read_roots`, `STAGE_HOST_ENV_ALLOWLIST` or `sccache_usable_in`.
