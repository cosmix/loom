# Plan: Warm stage builds by seeding each new stage from the operator's checkout and caches

Implements: none

## Overview

A new stage worktree holds sources and nothing a build produced, and its package-manager caches start empty. `PLAN-toolchain-build-caches` gave every stage one cache root with the host's Go, ccache, Maven and Gradle caches as read-only seeds. Its probes prove that a cache persists across the commands of one stage (`bun_cache_persists_across_commands`, `cargo_dependencies_stay_fresh`); none proves that a second stage starts warm, and for Cargo, JS and Python it does not.

This plan makes the first build of a new stage warm by cloning, copy-on-write, two kinds of directories when the stage's worktree is created:

1. the build directories the main checkout already holds (`target`, `node_modules`) into the same place in the worktree;
2. the operator's package caches that have no seed today (bun, npm, uv, the Go build cache) into the stage's cache root.

A clone costs no disk until the stage writes, takes seconds, and is private to the stage. A status line in `loom run` and `loom doctor` says what will be seeded, so the operator no longer has to infer it.

## What was measured while planning

Run on macOS (APFS) against this repository and the operator's caches. Paths are placeholders.

| Claim the design rests on | Observation |
| --- | --- |
| One `clonefile` call on a directory is fast enough to run at stage start | `target` (113 GB): 10.3 s. `node_modules` (302 MB, 20,179 files): 0.27 s. bun cache (4.9 GB): 2.75 s. npm `_cacache` (5 GB): 0.8 s. uv cache (18 GB): 6.75 s |
| A per-file clone walk is not | `cp -cR` of the bun cache: 88.6 s; of `node_modules`: 6.7 s |
| A cloned Cargo `target` at another path keeps registry dependencies | `cargo build -v`: `Fresh cfg-if`, `Dirty`/`Compiling` for both workspace crates; the binary prints the second checkout's `CARGO_MANIFEST_DIR` |
| Restoring source modification times is unsound | With the sources' original modification times Cargo reports all three crates `Fresh`, and the binary prints the FIRST checkout's `CARGO_MANIFEST_DIR` |
| A cloned `node_modules` works at another path | a vitest file of `web/` passes with no install |
| A cloned bun cache serves an offline install | `bun install --frozen-lockfile --offline`: 644 packages in 2.9 s |
| A cloned npm cache serves an offline install | `npm install --offline --cache <clone> isexe@2.0.0`: exit 0 |
| A cloned uv cache serves an offline install; a venv is not relocatable | `uv pip install --offline rich`: ok. The venv's scripts start with `#!<venv path>/bin/python` |
| A cloned Go build cache keeps the standard library | 122 compile steps cold; 2 with the cloned cache at another project path and another `GOMODCACHE` path (the module-cache dependency and `main` recompile); 1 when only the project path differs |
| `clonefile` works under the host job's kind of sandbox | inside `(deny default)` with `file-write*` on the tree: rc 0 |

## Decisions

1. **Clone, never copy.** A filesystem that cannot clone gets nothing: no fallback copy, no partial tree. This keeps the operator's rule against per-worktree copies of build directories.
2. **The seed is the operator's own state.** Sources are the main checkout's ignored build directories and the operator's host caches, the trust the existing read-only seeds already have. A clone is a snapshot: a stage's writes never reach the source, and no stage's output reaches another stage.
3. **Modification times are never restored.** A new checkout's fresh modification times are what make Cargo rebuild the workspace crates, and that rebuild is what keeps path-baked values (`env!("CARGO_MANIFEST_DIR")`) correct.
4. **An allowlist per ecosystem.** Only directories known to be relocatable are cloned: `target` for a Rust package, `node_modules` for a package with `package.json`. A venv is not cloned; `uv sync` rebuilds it from the seeded cache.
5. **Once, before anything touches the tree.** Seeding runs when the worktree is created, before the before-stage gate, provisioning and any session, so no other actor is in the worktree while paths are checked and cloned. A reused worktree is not seeded again.
6. **Advisory.** A seed that cannot be made is logged and the stage starts cold. `LOOM_BUILD_SEED=0` in the daemon's environment turns seeding off.

## What stays cold

- **A project's own compiled code.** Cargo workspace crates, Go packages of the main module and of the module cache, and JVM compilation rebuild in every new worktree, by decision 3 and because Go keys them by directory. For this repository the single `loom` crate is the long part of every build and this plan does not shorten it; that needs the crate split into several, which is separate work.
- **Filesystems without copy-on-write** (ext4, the Linux CI runners): nothing is seeded and the status line says so.
- **A main checkout that was never built**: there is nothing to clone. The status line lists what exists.
- **pnpm, yarn, deno, pip and the Gradle build cache**: no row. pnpm, yarn, deno and a JDK were not available to ground a row, and pip asks the index on every install.
- **Base checkouts** made for `loom check --at-base`.

## Preconditions

- The running plan has completed, and a loom whose host job lets a command signal its own processes is installed. Until then a full-suite run in a host job hangs on the tests that kill a stand-in process, and this plan's integration-verify cannot pass. The first stage's `before_stage` check looks for the committed profile rule.
- `loom plan verify --strict --dry-run doc/plans/PLAN-warm-stage-builds.md` has not been run: a plan is executing in this repository. Run it before `loom init`.

## Execution Diagram

```mermaid
graph LR
    seed-stage-builds --> integration-verify
    integration-verify --> knowledge-distill
```

No knowledge-bootstrap: `architecture/stage-build-cache.md` and `concerns/worktree-build-cold-start.md` describe this area and were read against the tree while planning.

## Stages

### 1. `seed-stage-builds`

The one implementation stage. Its three workers write disjoint files and start together; the two functions the others call are pinned in the stage description.

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| --- | --- | --- | --- | --- | --- |
| clone-and-seed | clone primitive, seed tables, path safety | loom-senior-software-engineer | loom/src/fs/clone_tree.rs, loom/src/fs/clone_tree_tests.rs, loom/src/fs/mod.rs, loom/src/stage_cache/seed.rs, loom/src/stage_cache/seed_tables.rs, loom/src/stage_cache/seed_tests.rs, loom/src/stage_cache/mod.rs, loom/src/stage_cache/roots.rs | brief | doc/plans/briefs/warm-stage-builds/seed-stage-builds/clone-and-seed.md |
| wiring-and-status | orchestrator call, host variables, status lines | loom-software-engineer | loom/src/orchestrator/core/build_seed.rs, loom/src/orchestrator/core/build_seed_tests.rs, loom/src/orchestrator/core/stage_executor.rs, loom/src/orchestrator/core/mod.rs, loom/src/stage_cache/environment.rs, loom/src/daemon/server/environment.rs, loom/src/orchestrator/terminal/launch/build_cache.rs, loom/src/orchestrator/terminal/launch/mod.rs, loom/src/commands/run/checks.rs, loom/src/commands/status.rs | brief | doc/plans/briefs/warm-stage-builds/seed-stage-builds/wiring-and-status.md |
| probes-and-ci | per-ecosystem second-stage probes, CI seeds | loom-software-engineer | loom/src/stage_cache/toolchain_tests/mod.rs, loom/src/stage_cache/toolchain_tests/rust.rs, loom/src/stage_cache/toolchain_tests/js.rs, loom/src/stage_cache/toolchain_tests/python.rs, loom/src/stage_cache/toolchain_tests/go.rs, .github/workflows/ci.yml | brief | doc/plans/briefs/warm-stage-builds/seed-stage-builds/probes-and-ci.md |

Risk checklist:

- (1) Untrusted input: a host variable may name any directory. Covered by `a-credential-directory-is-never-cloned`.
- (2) Paths and symlinks: the daemon clones outside any sandbox. Covered by `clone-refuses-a-symlinked-source`, `a-symlinked-package-directory-is-not-seeded` and `an-unclonable-filesystem-gets-no-copy`.
- (4) Configuration propagation: covered by `a-non-default-cache-directory-is-the-one-cloned`.
- (5) Lifecycle: a retried start meets an existing directory. Covered by `an-existing-destination-is-left-untouched`.
- (6) Reachability: `reachable` from `start_worktree_stage`, and `wiring` on the two status-line call sites.
- (7) External data: Cargo's freshness rules. Covered by `a-seeded-cargo-target-never-runs-the-first-checkouts-binary`, whose fixture is a real Cargo build.
- (3) Process I/O volume: the macOS clone is one system call and the Linux walk refuses at depth 64; no contract.

### 2. `integration-verify`

Runs the full suite, lint and format, reviews the change, and proves the seed is reachable: the built binary's `doctor` prints the build-seed line.

### 3. `knowledge-distill`

Rewrites `architecture/stage-build-cache.md` and `concerns/worktree-build-cold-start.md` to the new truth and curates the stage memories.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
        - "~/.config/gcloud/**"
        - "~/.gnupg/**"
      deny_write:
        - "../../**"
      allow_write: []
    network:
      allowed_domains:
        - crates.io
        - index.crates.io
        - static.crates.io
        - registry.npmjs.org
      allow_local_binding: false
      allow_unix_sockets: []
      allow_all_unix_sockets: false
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  provision:
    - working_dir: web
      command: >-
        test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile
        --ignore-scripts --backend=copyfile --config=/dev/null
  stages:
    - id: seed-stage-builds
      name: "Seed a new stage's build directories and package caches by copy-on-write clone"
      summary: >-
        Clones the main checkout's build directories and the operator's bun, npm, uv and
        Go build caches into each new stage, and reports what is seeded in loom run and
        loom doctor.
      stage_type: standard
      skills: ["loom-rust", "loom-ci-cd"]
      implementers: ["claude"]
      subagent_timeout_secs: 900
      description: |
        Read doc/plans/PLAN-warm-stage-builds.md sections Overview, What was measured,
        Decisions and What stays cold first (YAML is authoritative where they differ).
        Use parallel subagents and skills to maximize performance.

        PUBLIC SURFACE (contracts are written from this before any code):
          crate::fs::clone_tree::CloneOutcome { Cloned, Unsupported }
            derives Debug, Clone, Copy, PartialEq, Eq.
          crate::fs::clone_tree::clone_tree(source: &Path, destination: &Path)
              -> std::io::Result<CloneOutcome>
            Clones the directory tree at `source` to `destination` copy-on-write.
            Err(ErrorKind::InvalidInput) when `source`, read without following a
            symlink, is not a directory: a symlink to a directory is refused.
            Err(ErrorKind::AlreadyExists) when anything exists at `destination`.
            Ok(Unsupported) when the filesystem cannot clone; nothing exists at
            `destination` afterwards and no file content was copied.
            Ok(Cloned): every relative path of the source exists at the destination
            with the same content, permission bits and modification time; a symlink
            inside the tree is reproduced as a symlink with the same target text and
            is never followed.
          crate::fs::clone_tree::can_clone_in(dir: &Path) -> bool
            True when clone_tree of a one-file directory created inside `dir`
            returns Cloned. Removes what it created.
          crate::stage_cache::seed::SeedOutcome { Cloned, Skipped(String) }
          crate::stage_cache::seed::Seeded { pub what: String, pub outcome: SeedOutcome }
            Both derive Debug, Clone, PartialEq, Eq. `what` is, for a worktree
            directory, its path relative to the repository root with `/`
            separators ("loom/target", or "target" for a package at the root),
            and for a host cache the name of its directory in the stage cache
            root ("bun", "npm", "uv", "go-build").
          crate::stage_cache::seed::seed_worktree(repo_root: &Path, worktree: &Path)
              -> Vec<Seeded>
            `repo_root` is the main checkout, `worktree` a git checkout of the same
            tree at another path (the function reads no worktree registration).
            For every package ProjectProfile::discover(repo_root) lists: kind `rust`
            seeds `target`; kind `javascript` or `typescript` seeds `node_modules`
            when the package directory holds `package.json`. A directory is cloned
            from <repo_root>/<package>/<dir> to <worktree>/<package>/<dir> only when
            the source exists, every path component from `repo_root` to the source
            and from `worktree` to the destination's parent is a real directory
            (never a symlink), `git check-ignore` run in `worktree` reports the
            destination ignored, and nothing exists at the destination. Each
            candidate whose source exists yields one Seeded; a refusal or an
            unclonable filesystem is Skipped with the reason. Never changes a
            modification time in `worktree` outside the cloned directory.
          crate::stage_cache::seed::seed_host_caches(repo_root: &Path,
              cache_root: &Path, host_env: &BTreeMap<String, OsString>) -> Vec<Seeded>
            `cache_root` is an existing stage cache root. For the kinds
            ProjectProfile::discover(repo_root) finds: `javascript` or `typescript`
            seeds "bun" (source: host_env BUN_INSTALL_CACHE_DIR, else
            <HOME>/.bun/install/cache) and "npm" (source: <npm cache>/_cacache where
            <npm cache> is host_env npm_config_cache, else <HOME>/.npm; destination
            <cache_root>/npm/_cacache); `python` seeds "uv" (UV_CACHE_DIR, else
            <XDG_CACHE_HOME>/uv, else <HOME>/.cache/uv); `golang` seeds "go-build"
            (GOCACHE, else on macOS <HOME>/Library/Caches/go-build, else
            <XDG_CACHE_HOME>/go-build, else <HOME>/.cache/go-build). HOME and
            XDG_CACHE_HOME are read from host_env, never from the process. A source
            is used only when it is an existing directory that is neither inside nor
            contains a credential path of stage_cache/roots.rs (<HOME>/.ssh is one).
            A destination that exists is left alone.
          crate::stage_cache::seed::seed_sources(repo_root: &Path,
              host_env: &BTreeMap<String, OsString>) -> Vec<String>
            The `what` names of every source that exists and passes the checks
            above that need no worktree, in table order. Clones nothing.
          Both seed functions return an empty Vec, and clone nothing, when host_env
          (seed_host_caches) or the process environment (seed_worktree) holds
          LOOM_BUILD_SEED=0.

        WORKERS: see the stage table in the plan prose; territories are DISJOINT and
        workers never spawn subagents. Spawn all three BY AGENT TYPE in ONE message,
        each with: "Your brief: <path>. Read it in full before anything else."
        wiring-and-status and probes-and-ci call the functions above by these
        signatures and start beside clone-and-seed; compile the tree once when all
        three have returned.

        INTEGRITY: wiring-and-status rewords the sccache status line in
        loom/src/orchestrator/terminal/launch/build_cache.rs, whose test asserts on
        the line; an assertion changed there raises TI-edit and is expected, because
        the old line claimed that installing sccache shares dependency builds between
        stage worktrees, which the tree does not do.

        A check that fails only in the host job is reproduced by you with
        loom check seed-stage-builds --criterion <n>; hand its output to the worker.
        Run cargo fmt before the final review round.
        MEMORY: record mistakes, decisions and surprises with loom memory immediately
        (subagents report theirs to you); never loom knowledge (implementation
        stage); never Claude Code auto-memory.
      dependencies: []
      working_dir: "."
      files:
        - "loom/src/fs/**"
        - "loom/src/stage_cache/**"
        - "loom/src/orchestrator/core/**"
        - "loom/src/orchestrator/terminal/launch/build_cache.rs"
        - "loom/src/orchestrator/terminal/launch/mod.rs"
        - "loom/src/commands/run/checks.rs"
        - "loom/src/commands/status.rs"
        - "loom/src/daemon/server/environment.rs"
        - ".github/workflows/ci.yml"
        - "loom/maintainability-baseline.txt"
      before_stage:
        - command: "rg -qF 'allow signal (target same-sandbox)' loom/src/verify/criteria/confine_seatbelt.rs"
          description: "the host job lets a command signal its own processes"
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --lib fs::"
          timeout_secs: 900
        - command: "cargo test --manifest-path loom/Cargo.toml --lib stage_cache::"
          timeout_secs: 1800
        - command: "cargo test --manifest-path loom/Cargo.toml --lib orchestrator::core::build_seed"
          timeout_secs: 900
        - command: "cargo test --manifest-path loom/Cargo.toml --lib orchestrator::terminal::launch::build_cache"
          timeout_secs: 900
        - command: "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
          timeout_secs: 900
        - "cargo fmt --manifest-path loom/Cargo.toml --all -- --check"
      artifacts:
        - "loom/src/fs/clone_tree.rs"
        - "loom/src/stage_cache/seed.rs"
        - "loom/src/orchestrator/core/build_seed.rs"
      reachable:
        - symbol: seed_worktree
          from: start_worktree_stage
          description: "a new stage worktree is seeded on the stage start path"
        - symbol: seed_host_caches
          from: start_worktree_stage
          description: "a new stage's cache root is seeded on the stage start path"
      wiring:
        - source: "loom/src/commands/status.rs"
          pattern: "build_seed_status_line("
          literal: true
          description: "loom doctor prints the build-seed line"
        - source: "loom/src/commands/run/checks.rs"
          pattern: "build_seed_status_line("
          literal: true
          description: "loom run prints the build-seed line at startup"
      after_stage:
        - command: "rg -qF 'LOOM_TEST_REQUIRE_CLONE' .github/workflows/ci.yml"
          description: "the macOS CI job makes an unclonable probe fatal"
      contracts:
        - id: clone-refuses-a-symlinked-source
          file: loom/src/stage_cache/seed_contract_tests.rs
          scenario: "creates a directory with one file in a TempDir and a symlink to it beside it, then calls clone_tree with the symlink as source and a fresh sibling path as destination"
          rejects: "a clone_tree that follows the symlink and clones the directory it points to"
        - id: an-unclonable-filesystem-gets-no-copy
          file: loom/src/stage_cache/seed_contract_tests.rs
          scenario: "calls clone_tree on a TempDir tree of two files, a subdirectory and an inner symlink to a file outside the tree; when the result is Unsupported asserts nothing exists at the destination, and when it is Cloned asserts each file's content, permission bits and modification time equal the source's and the inner symlink is a symlink with the same target"
          rejects: "a clone_tree that copies file bytes or leaves a partly written destination when the filesystem cannot clone, or that follows the inner symlink and writes a copy of its target"
        - id: an-existing-destination-is-left-untouched
          file: loom/src/stage_cache/seed_contract_tests.rs
          scenario: "builds a git repository with Cargo.toml, a .gitignore naming target and target/from-main, and a second git checkout of the same files whose target already holds only target/kept; calls seed_worktree"
          rejects: "a seed that merges into or replaces a build directory the worktree already has, so target/from-main appears or target/kept is gone"
        - id: a-symlinked-package-directory-is-not-seeded
          file: loom/src/stage_cache/seed_contract_tests.rs
          scenario: "the main repository has pkg/Cargo.toml and an ignored pkg/target with one file; in the second checkout pkg is a symlink to an empty directory outside it; calls seed_worktree"
          rejects: "a seed that resolves the package path through the symlink and clones a build directory outside the worktree"
        - id: a-non-default-cache-directory-is-the-one-cloned
          file: loom/src/stage_cache/seed_contract_tests.rs
          scenario: "a git repository holding the marker file that gives a package the python kind; host_env has HOME with .cache/uv/default-marker and UV_CACHE_DIR naming another directory that holds chosen-marker; calls seed_host_caches with an empty cache root on the same filesystem and, when can_clone_in(cache_root) is true, reads cache_root/uv"
          rejects: "a seed that ignores UV_CACHE_DIR and clones the default location, so default-marker appears and chosen-marker does not"
        - id: a-credential-directory-is-never-cloned
          file: loom/src/stage_cache/seed_contract_tests.rs
          scenario: "a git repository with package.json; host_env has HOME with .ssh/id_test and BUN_INSTALL_CACHE_DIR naming HOME/.ssh; calls seed_host_caches with an empty cache root"
          rejects: "a seed that clones whatever directory the variable names, so cache_root/bun/id_test exists"
        - id: a-seeded-cargo-target-never-runs-the-first-checkouts-binary
          file: loom/src/stage_cache/seed_contract_tests.rs
          scenario: "a git repository with a Cargo package app whose main prints env!(\"CARGO_MANIFEST_DIR\") and which depends by path on a crate in a third directory outside both checkouts; runs cargo build --offline there, writes the same files into a second git checkout after that build has finished, calls seed_worktree and, when target was Cloned, runs cargo build --offline -v in the second checkout and then its target/debug/app"
          rejects: "a seed that restores or copies source modification times so Cargo reports app Fresh, and the binary printing the first checkout's path runs in the second; the outside dependency must stay Fresh and app must be compiled"
    - id: integration-verify
      name: "Integration Verification"
      summary: >-
        Runs the full suite and the review, and confirms the built loom reports the
        build seed in doctor.
      stage_type: integration-verify
      description: |
        Final verification after all stages. Verify FUNCTIONAL INTEGRATION, not just
        tests passing. NEVER Claude Code auto-memory.
        CONTEXT: read the plan's Overview, Decisions and What stays cold sections by
        line range (doc/plans/), loom memory show --all, doc/loom/knowledge/INDEX.md
        and only the relevant sections.
        BUILD & TEST (zero tolerance): run loom check integration-verify once. A
        test that needs a capability the host job lacks gets the matching
        sandbox_probe guard, never a --skip.
        CODE REVIEW: spawn parallel loom-code-reviewer subagents for security (the
        daemon clones outside any sandbox: symlinked sources, symlinked package
        paths, credential directories, the destination staying inside the worktree
        or the cache root), architecture, and test coverage; fix ALL findings with
        an engineer agent. Every finding is fixed or disputed, never deferred.
        Weigh every pending reviewer suggestion.
        FUNCTIONAL: build the binary in this worktree and confirm that
        loom/target/debug/loom doctor prints a line starting "build seed:" and an
        sccache line that no longer says installing sccache shares dependency builds
        between worktrees. On a filesystem that can clone, create a scratch git
        repository with an ignored target directory and a second checkout of it
        under the worktree scratch directory, and confirm through a short test or
        the seed functions that the directory is cloned and a second call skips it.
        Record discoveries for knowledge-distill, including stale knowledge:
        loom memory note "stale-knowledge: ...".
      dependencies: ["seed-stage-builds"]
      working_dir: "."
      files:
        - "loom/src/**"
        - "loom/tests/**"
        - ".github/workflows/ci.yml"
        - "loom/maintainability-baseline.txt"
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - command: "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
          timeout_secs: 900
        - "cargo fmt --manifest-path loom/Cargo.toml --all -- --check"
    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: >-
        Rewrites the stage build cache and cold-start knowledge to what the tree now
        does and records what this plan learned.
      stage_type: knowledge-distill
      description: |
        Curate all stage memories into permanent knowledge; update user docs.
        NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents.
        START with loom memory pending --group; read the plan's Overview, Decisions
        and What stays cold sections by line range, and the knowledge sections it
        touches.
        CORRECTIONS FIRST: apply every stale-knowledge memory in place with
        loom knowledge replace-section, never with loom knowledge update.
        REWRITE to the new truth, with replace-section: architecture/stage-build-cache.md
        (the environment table's seed column for bun, npm, uv and Go, and a section
        on worktree build-directory seeds: sources, the checks, when it runs, the
        status line, LOOM_BUILD_SEED) and concerns/worktree-build-cold-start.md
        (what is warm now, what stays cold and why, the measured clone times, the
        unsound modification-time restore with its evidence). Current truth only:
        history goes to the mistakes files.
        Then curate mistakes, patterns, decisions and conventions. TIER ROUTING:
        about 40 lines or fewer inline in the tier-1 file; larger through
        loom knowledge update <category>/<slug> with a 2-4 line tier-1 summary and
        link.
        Update README for the new status line and LOOM_BUILD_SEED (relevant
        sections only).
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or
        the topic it belongs to, then resolve it.
        RECEIPTS: every memory taken into knowledge gets loom memory resolve right
        after the write that used it; finish with loom memory pending --strict.
        LAST, if this stage removed structural issues:
        loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      dependencies: ["integration-verify"]
      working_dir: "."
      files: ["doc/loom/knowledge/**", "README.md", "CONTRIBUTING.md"]
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
```

<!-- END loom METADATA -->
