# Plan: Per-Stage Toolchain Build Caches for Confined Runs

Implements: none

## Overview

A stage's confined host jobs (contract freeze, `loom check`, `loom stage complete`, provisioning) start every command with empty, per-command build caches, cannot reach the host's caches, and cannot see toolchains installed in the operator's home. Probes on macOS (Seatbelt) and Linux (bubblewrap 0.9, Ubuntu 24.04 in a container) ran each ecosystem once unconfined and twice confined:

| Ecosystem | macOS today | Linux today | Cause in the runner |
| --- | --- | --- | --- |
| Rust (Cargo dependencies) | reuses (fixed: host Cargo home read in place) | reuses (`Fresh=256 Compiling=0`) | none left |
| Rust and C/C++ with sccache | fails: `sccache: error: Operation not permitted` | binary unreachable in `~/.local/bin`; once on `/usr/local/bin`, builds work with `Compile requests 0` | localhost TCP denied (Seatbelt); `~/.local/bin` not a read root; cache dir per command |
| C/C++ with ccache | not installed | cold every command: `Hits: 0 / 2` | `CCACHE_DIR` under the per-command `XDG_CACHE_HOME` |
| C/C++ plain cmake/make | in-tree build reuses; configure 10.3 s vs 0.65 s unconfined | reuses | xcrun cache DB denied, so every `/usr/bin` shim call starts `xcodebuild` |
| Every macOS tool behind an xcrun shim (`git`, `cc`, `clang`, `make`) | 5 × `git --version` 7.35 s; one `cc -c` 1.50 s; opendirectoryd at 50-100% CPU during host jobs | n/a | same: xcrun cannot read or write `$(getconf DARWIN_USER_TEMP_DIR)/xcrun_db`, which `TMPDIR` does not move |
| Go | fails: `GOMODCACHE` and `GOCACHE` unreadable | dependency builds fail: `mkdir /home/user/go: read-only file system`; std-only builds compile 89 packages every command | `~/go` not a root; `GOCACHE` per command |
| JVM, Maven | not installed | fails: `Could not create local repository at ~/.m2/repository` | `~/.m2` not a root |
| JVM, Gradle | not installed | fails: tool in `/opt` unreachable; once moved, `Could not initialize native services` | `/opt` and `~/.gradle` not roots |
| Python, uv | `.venv` reused; fresh `uv sync --offline` fails | `uv` in `~/.local/bin` unreachable; once moved, as macOS | `UV_CACHE_DIR` empty per command; bubblewrap 0.9 has no overlay seed |
| JS, bun | `node_modules` reused; fresh install fails | as macOS | `BUN_INSTALL_CACHE_DIR` empty per command |
| JS, npm | `npm ci --offline` fails with `EPERM` and deletes `node_modules` | fails: `ENOTCACHED` | `~/.npm` not a root |
| JS, tsc via `~/.bun/bin` | fails: symlink target unreadable | fails: `execvp ... No such file` | `~/.bun/install/global` not a root |

This plan gives each stage ONE persistent build-cache root outside its worktree, shared by the stage's session, its codex workers and its confined host jobs. Per-ecosystem environment variables point every tool's cache there. Host caches serve as read-only seeds where the tool supports layering, toolchains installed under the operator's home become readable, and CI runs real-tool probes for every ecosystem on Linux and macOS.

## Decisions (settled with the operator)

- **Write scope: per stage, host caches as read-only seeds.** A stage's code can poison only its own cache. No confined command and, after this plan, no session writes a host cache other than Cargo's and rustup's (unchanged: Cargo's registry path must stay the host's for fingerprint parity, see `doc/loom/knowledge/architecture/execution-containment.md`).
- **Linux verification during execution is operator-run.** The harness lives uncommitted at `.loom-linux-probe/` in the main checkout (excluded through `.git/info/exclude`). integration-verify blocks once and asks the operator to run it. Stage sessions get no Docker socket.
- **CI is the lasting Linux and macOS gate**: stage `stage-cache-sessions` installs every toolchain on both runners and runs the probes with skips made fatal.
- **Lanes:** Claude for the confinement boundary, the cache-root design and fixture-backed tests; codex for the cleanup hook and the CI workflow.
- **Prerequisite:** the Cargo-home fix (`CargoHome::Host`, uncommitted in `loom/src/verify/criteria/confine_scratch.rs`, `confine_seatbelt.rs` and their tests) is committed to `main` before `loom init`. This plan builds on it.

## Design (YAML is authoritative where prose and metadata differ)

**Stage cache root.** `StageCache::resolve(repo_root, stage_id, home, xdg_cache_home)` returns `<base>/loom/stage-cache/<24 lowercase hex of sha256(canonical repo root, NUL, stage id)>`. `<base>` is `$XDG_CACHE_HOME` when it is set and absolute, else `~/Library/Caches` on macOS and `~/.cache` on Linux. A fixed-length name keeps `<root>/<socket>` under SUN_LEN. `ensure()` creates it 0700, owned by this user, and refuses a root or a created parent that is a symlink or owned by another user, so a directory planted in advance is never adopted. `remove_stage_cache()` deletes it without following links.

**Per-ecosystem environment** (`StageCache::environment(consumer, host_env)`; `consumer` is `Session`, `Worker` or `HostJob`):

| Tool | Variables set (paths under the root) | Read-only host seed |
| --- | --- | --- |
| Go | `GOCACHE=go-build`, `GOMODCACHE=go-mod`, `GOPROXY=file://<host GOMODCACHE>/cache/download,<tail>`; the tail is the host's `GOPROXY` or `https://proxy.golang.org,direct` for `Session`/`Worker`, and `off` for `HostJob`. `HostJob` also appends `-count=1` to the host's `GOFLAGS` | host `GOMODCACHE/cache/download` (from `GOMODCACHE`, else `GOPATH`'s first entry, else `~/go`, + `pkg/mod`) |
| ccache | `CCACHE_DIR=ccache`, `CCACHE_BASEDIR=<worktree root>`; with a seed also `CCACHE_REMOTE_STORAGE` and `CCACHE_SECONDARY_STORAGE` = `file:<seed>\|read-only` | host `CCACHE_DIR`, else `~/Library/Caches/ccache` (macOS) or `$XDG_CACHE_HOME/ccache` / `~/.cache/ccache` (Linux) |
| sccache | `SCCACHE_DIR=sccache`, `SCCACHE_SERVER_UDS=<root>/s.sock` (`Session`), `w.sock` (`Worker`), `h.sock` (`HostJob`); omitted when the path would not fit SUN_LEN (104 bytes on macOS, 108 on Linux) | none |
| Maven | `MAVEN_OPTS` = host `MAVEN_OPTS` + `-Dmaven.repo.local=<root>/m2` + `-Dmaven.repo.local.tail=<seed>` (Maven 3.9+; older Maven ignores the tail) | `~/.m2/repository` |
| Gradle | `GRADLE_USER_HOME=gradle`, `GRADLE_RO_DEP_CACHE=<seed>` | host `GRADLE_USER_HOME` or `~/.gradle`, its `caches` dir when `caches/modules-2` exists |
| npm, pnpm, yarn | `npm_config_cache=npm`, `npm_config_store_dir=pnpm-store`, `YARN_CACHE_FOLDER=yarn` | none |
| bun, deno | `BUN_INSTALL_CACHE_DIR=bun`, `DENO_DIR=deno` | none |
| uv, pip | `UV_CACHE_DIR=uv`, `PIP_CACHE_DIR=pip` | none |
| anything else | `XDG_CACHE_HOME=xdg` | none |
| macOS developer tools | `PATH` = `<developer dir>/Toolchains/XcodeDefault.xctoolchain/usr/bin` (when present) + `<developer dir>/usr/bin` + the host `PATH`; `SDKROOT` = the macOS SDK path; `DEVELOPER_DIR` = the developer dir. Set for every consumer alike, so session and host-job builds see identical build-script inputs (cc-rs reruns on `SDKROOT`) | none |

Why `-count=1` for host jobs: a session can now write `GOCACHE`, and Go caches test results there, so without it a planted entry would let a criterion's `go test` print `ok ... (cached)` without running. Verified: `GOFLAGS=-count=1` makes `go test` rerun, and `go build` and `go vet` accept it.

Why sockets differ: the sccache server runs the compilers its clients ask for. A host-job client reaching the session's server would run commands outside the confinement, so each consumer starts its own server, and they share only `SCCACHE_DIR`.

**macOS developer tools.** `DeveloperTools::detect()` runs `xcode-select -p` and `xcrun --sdk macosx --show-sdk-path` once per process, unconfined, where xcrun's cache works, and caches the answer. `None` off macOS or when either fails. The real binaries then run directly instead of through the `/usr/bin` shims. Measured with xcrun's cache denied: 5 × `git --version` went from 7.35 s to 0.066 s, and one `cc -c` from 1.50 s to 0.05 s with `SDKROOT` set. The prefix must reach the original command before `confine_host::wrap` resolves its program, so a criterion's bare `git` resolves to the real binary.

**Read roots added to every confined command (read-only, existing paths only).**
- Seeds above.
- Toolchain install roots under HOME: `.local/bin`, `.local/pipx`, `go/bin`, `sdk`, `.bun/install/global`, `.nvm/versions`, `.volta`, `.local/share/fnm`, `.sdkman/candidates`, `.asdf/installs`, `.local/share/mise/installs`, `.pyenv/versions`, `.jdks`, `.deno/bin`.
- `/opt` and `/nix/store`.
- Every `PATH` entry below HOME, at least one level down. Never HOME itself, and never an entry that is, contains, or lies inside a credential directory from `models/stage/sandbox_config.rs::default_deny_read` (`.ssh`, `.aws`, `.gnupg`, `.config/gcloud`, ...).

**Writable roots added.** The stage cache root, plus Seatbelt permission to bind and connect a Unix socket at `h.sock` only. Under Seatbelt, `~/.m2/settings.xml` and `~/.m2/settings-security.xml` are hidden by name, metadata included, the way `confine_ancestors.rs` hides Cargo files: Maven fails on a settings file it can see but not read.

**Order.** In `confine_host.rs::apply_environment` the stage environment is applied after `CargoScratch::apply_environment`, so the per-command scratch values do not win. Without a stage cache (plan dry-run, `spawn_confined`, tests) behaviour is unchanged, including Linux's uv/bun overlays. With one, the overlays are not mounted.

## Non-goals

- **Granting the xcrun cache DB.** xcrun keeps `xcrun_db` under `DARWIN_USER_TEMP_DIR`, which `TMPDIR` does not move (`man xcrun`: only `--no-cache`/`xcrun_nocache`, which still rewrites the entry). A writable DB would let a confined command plant tool paths that the operator's own builds follow, so it stays denied. The developer-tools environment above avoids the shims instead. A tool reached only through xcrun with a non-default SDK (`xcrun --sdk iphoneos ...`) still pays the uncached cost.
- **`RUSTC_WRAPPER` for host jobs** stays dropped (`process/environment.rs` comment): sccache in host jobs applies only where the project itself names it (a CMake launcher).
- **Sharing caches across stages or with the operator's builds**: excluded by the write-scope decision.

## Execution Diagram

```mermaid
graph LR
    stage-cache-host --> stage-cache-sessions
    stage-cache-sessions --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

No knowledge-bootstrap: the tier-1 files and the confinement topics (`architecture/execution-containment.md`, `concerns/worktree-build-cold-start.md`) already describe this area.

### 1. `stage-cache-host`: the cache root and the confined runner

Adds `loom/src/stage_cache/` (root, per-ecosystem environment, seed and toolchain read roots) and threads it through every confined command a stage's host jobs run, on both launchers, with real-tool tests per ecosystem that skip when a tool is absent.

Necessity: Q4. The boundary change, its security review and seven real-tool test modules approach the single-session budget on their own. Stage 2's session, worker, cleanup and CI work would push it past.

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| --- | --- | --- | --- | --- | --- |
| core | foundation: `stage_cache` module | loom-senior-software-engineer | src/stage_cache/mod.rs, src/stage_cache/environment.rs, src/stage_cache/roots.rs, src/stage_cache/developer.rs, src/lib.rs | pinned interface in brief | doc/plans/briefs/toolchain-build-caches/stage-cache-host/core.md |
| confinement | thread the cache through the runner and both launchers | loom-senior-software-engineer | src/verify/criteria/config.rs, src/verify/criteria/runner.rs, src/verify/criteria/confine.rs, src/verify/criteria/executor.rs, src/verify/criteria/mod.rs, src/verify/criteria/confine_host.rs, src/verify/criteria/confine_seatbelt.rs, src/verify/criteria/confine_scratch.rs, src/host_verify/child.rs, src/orchestrator/provision.rs, src/orchestrator/core/provision_gate.rs, src/verify/contracts/format_gate.rs, src/verify/contracts/completion.rs, src/verify/completion/resolved.rs, src/commands/stage/acceptance_runner.rs | pinned interface in brief | doc/plans/briefs/toolchain-build-caches/stage-cache-host/confinement.md |
| toolchain-tests | real-tool probes per ecosystem | loom-software-engineer | src/stage_cache/toolchain_tests/** | after core and confinement | doc/plans/briefs/toolchain-build-caches/stage-cache-host/toolchain-tests.md |

`core` and `confinement` start together against the interface pinned in both briefs. `toolchain-tests` starts once both return; just before spawning it, the stage's main agent adds `#[cfg(test)] mod toolchain_tests;` to `src/stage_cache/mod.rs` (one line, no compile in between).

Risk checklist: (2) filesystem paths and symlinks, covered by `stage-root-is-stable-and-outside-the-tree`, `host-caches-stay-read-only`, `path-toolchains-are-readable` and `credential-dirs-on-path-stay-hidden`. (4) configuration propagation, covered by `stage-env-wins-over-per-command-scratch`, `host-seeds-are-named-in-the-environment`, `host-job-go-test-never-reuses-cached-results` and `developer-tools-bypass-the-xcrun-shims`. (5) lifecycle, covered by `stage-cache-persists-across-confined-commands`. Security of the shared server, covered by `host-job-sccache-socket-is-not-the-sessions`. (6) reachability, covered by `reachable`.

### 2. `stage-cache-sessions`: sessions, workers, cleanup and CI

Points the stage session's wrapper and codex worker boundary at the same root, grants the session write access to it and withdraws the host-cache write grants it replaces, removes the root when the stage is cleaned up, and makes CI install every toolchain and run the probes with skips fatal.

Necessity: Q4 (see stage 1), and Q3: sessions lose their host-cache write grants only after stage 1 proves the host side on its own.

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| --- | --- | --- | --- | --- | --- |
| session | session wrapper, sandbox grant, codex worker boundary | loom-software-engineer | loom/src/stage_cache/session.rs, loom/src/stage_cache/mod.rs, loom/src/orchestrator/terminal/native/wrapper.rs, loom/src/orchestrator/terminal/native/wrapper/host_env.rs, loom/src/orchestrator/terminal/native/launch.rs, loom/src/orchestrator/terminal/native/launch/host.rs, loom/src/sandbox/package_caches.rs, loom/src/native_worker/boundary/linux.rs, loom/src/native_worker/boundary/environment.rs | brief | doc/plans/briefs/toolchain-build-caches/stage-cache-sessions/session.md |
| cleanup | remove the root with the stage | codex sol | loom/src/stage_cache/cleanup.rs, loom/src/git/cleanup/batch.rs, loom/src/commands/clean/worktrees.rs, loom/src/commands/init/cleanup.rs | brief | doc/plans/briefs/toolchain-build-caches/stage-cache-sessions/cleanup.md |
| ci | toolchains and probes in CI | codex terra | .github/workflows/ci.yml | brief | doc/plans/briefs/toolchain-build-caches/stage-cache-sessions/ci.md |

`session` alone edits `loom/src/stage_cache/mod.rs`: it adds `pub mod session;` and `pub mod cleanup;`, so the `cleanup` worker's file compiles once both return.

Risk checklist: (4) configuration propagation, covered by `session-wrapper-exports-the-stage-cache`. (2) paths and symlinks, covered by `stage-cache-removal-never-follows-a-symlink`. Security, covered by `sessions-no-longer-write-host-caches`. (6) reachability, covered by `reachable`.

### 3. Integration Verification

Full suite, clippy and fmt (zero tolerance); parallel reviewers (security, architecture, test coverage); every finding fixed or disputed. Then the Linux check: after `loom check integration-verify` passes, the main agent files once

    loom stage block integration-verify "Run .loom-linux-probe/run.sh .worktrees/integration-verify from the main checkout (Docker running); it writes linux-probe.log into the worktree. Retry the stage when done."

On retry it reads `linux-probe.log` from the worktree root (never commits it), fixes any failure, and records the result in loom memory.

### 4. Knowledge Distillation

Curates memories. Rewrites `architecture/execution-containment.md` (the stage cache, toolchain roots, overlays only without a stage cache), closes option 3 in `concerns/worktree-build-cold-start.md` with what was built and what stays cold, records the xcrun shim bypass and its residual (a non-default SDK still goes through uncached xcrun), and updates `sandbox/package_caches.rs`'s knowledge counterpart.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files: [loom/maintainability-baseline.txt, doc/loom/knowledge/check-baseline.txt]
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
    network:
      allowed_domains: ["crates.io", "index.crates.io", "static.crates.io", "registry.npmjs.org"]
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: stage-cache-host
      name: "Stage cache root in the confined runner"
      summary: "Gives each stage one persistent build-cache root and points every confined host-job command's toolchain caches at it, with host caches as read-only seeds and home-installed toolchains readable."
      stage_type: standard
      skills: ["loom-rust"]
      implementers: ["claude"]
      subagent_timeout_secs: 900
      working_dir: "loom"
      dependencies: []
      description: |
        Read doc/plans/PLAN-toolchain-build-caches.md sections Overview, Decisions and
        Design first (YAML is authoritative where they differ), then
        doc/loom/knowledge/architecture/execution-containment.md section
        "Read This First: What \"Containment\" Means In Loom".
        Use parallel subagents and skills to maximize performance.

        PUBLIC SURFACE (contracts are written from this before any code):
          crate::stage_cache::StageCache (Clone, Debug, PartialEq):
            fn resolve(repo_root: &Path, stage_id: &str, home: &Path,
                       xdg_cache_home: Option<&Path>) -> anyhow::Result<StageCache>
              pure; no filesystem writes; errors on an empty stage id or one with '/', NUL or "..".
            fn root(&self) -> &Path
            fn ensure(&self) -> anyhow::Result<()>     0700, owned, refuses a symlinked root or parent it creates
            fn environment(&self, consumer: Consumer, worktree_root: &Path,
                           host_env: &BTreeMap<String, OsString>,
                           developer: Option<&DeveloperTools>) -> BTreeMap<String, OsString>
          crate::stage_cache::Consumer { Session, Worker, HostJob }
          crate::stage_cache::DeveloperTools { pub developer_dir: PathBuf, pub sdk_root: PathBuf }
            fn detect() -> Option<DeveloperTools>   macOS only; cached; unconfined subprocesses
          StageCache::environment takes a fourth argument, developer: Option<&DeveloperTools>;
            with Some, PATH, SDKROOT and DEVELOPER_DIR are set as in the Design table.
          crate::stage_cache::seed_read_roots(host_env: &BTreeMap<String, OsString>) -> Vec<PathBuf>
          crate::stage_cache::toolchain_read_roots(host_env: &BTreeMap<String, OsString>) -> Vec<PathBuf>
            host_env supplies HOME, PATH, GOMODCACHE, GOPATH, CCACHE_DIR, GRADLE_USER_HOME,
            XDG_CACHE_HOME, GOPROXY, GOFLAGS, MAVEN_OPTS; only existing directories are returned.
          crate::verify::criteria::run_spec_in_stage(spec: &CommandSpec, tree: Option<&Path>,
              working_dir: Option<&Path>, timeout: Duration, confinement: CommandConfinement,
              cache: &StageCache) -> anyhow::Result<CriterionResult>
            run_spec_in_optional_tree with the stage cache applied to the confined command.
          CriteriaConfig::with_stage_cache(self, cache: Option<StageCache>) -> Self
          Variable names and root-relative paths: the Design table of the plan.

        WORKERS: see the stage table in the plan prose; spawn core and confinement in ONE
        message. After both return, add `#[cfg(test)] mod toolchain_tests;` to
        src/stage_cache/mod.rs (before any compile), then spawn toolchain-tests. Each prompt: "Your brief:
        <path>. Read it in full before anything else."
        Codex is not used in this stage: every piece is security-sensitive boundary code or
        fixture-backed test convergence.

        Run cargo fmt before the final review round. Record every mistake, decision and
        surprise with loom memory immediately (subagents report theirs to you); never
        loom knowledge (implementation stage); never Claude Code auto-memory.
      files:
        - "src/stage_cache/**"
        - "src/lib.rs"
        - "src/verify/criteria/**"
        - "src/host_verify/child.rs"
        - "src/orchestrator/provision.rs"
        - "src/orchestrator/core/provision_gate.rs"
        - "src/verify/contracts/format_gate.rs"
        - "src/verify/contracts/completion.rs"
        - "src/verify/completion/resolved.rs"
        - "src/commands/stage/acceptance_runner.rs"
        - "maintainability-baseline.txt"
      artifacts:
        - "src/stage_cache/mod.rs"
        - "src/stage_cache/environment.rs"
        - "src/stage_cache/roots.rs"
        - "src/stage_cache/developer.rs"
      acceptance:
        - command: "cargo test --lib stage_cache::"
          timeout_secs: 900
        - "cargo test --lib verify::criteria::"
        - "cargo test --lib host_verify::"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
      before_stage:
        - command: 'rg -qF "CargoHome::Host" src/verify/criteria/confine_scratch.rs'
          description: "the host Cargo home fix this plan builds on is committed"
      after_stage:
        - command: 'rg -qF "with_stage_cache(" src/host_verify/child.rs'
          description: "host-job acceptance passes the stage cache to the criteria runner"
        - command: 'rg -qF "apply_stage_cache(" src/verify/criteria/confine_host.rs'
          description: "the confined environment applies the stage cache"
      reachable:
        - symbol: apply_stage_cache
          from: run_job_bound
          description: "a host job's confined criteria get the stage cache environment and roots"
        - symbol: toolchain_read_roots
          from: run_job_bound
          description: "home-installed toolchains are readable in host-job commands"
      contracts:
        - id: stage-root-is-stable-and-outside-the-tree
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            resolves StageCache for (TempDir repo, "alpha", TempDir home, None) twice and for
            "beta" once; also with xdg_cache_home Some(<abs dir>)
          rejects: >-
            a root keyed by a per-command temp directory, placed under the repository, or
            named after the raw stage id (unbounded length)
        - id: stage-env-wins-over-per-command-scratch
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            run_spec_in_stage runs a confined sh -c printing GOCACHE, XDG_CACHE_HOME,
            UV_CACHE_DIR, BUN_INSTALL_CACHE_DIR, npm_config_cache, CCACHE_DIR,
            GRADLE_USER_HOME and SCCACHE_DIR in a TempDir tree with a StageCache under a
            TempDir home
          rejects: >-
            stage variables applied before CargoScratch::apply_environment, so the
            per-command scratch values for XDG_CACHE_HOME, UV_CACHE_DIR and
            BUN_INSTALL_CACHE_DIR win
        - id: stage-cache-persists-across-confined-commands
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            one confined command writes $GOCACHE/probe through run_spec_in_stage, a second
            confined command with the same StageCache reads it back
          rejects: >-
            a root that is not granted writable inside the boundary, or one recreated per
            command
        - id: host-caches-stay-read-only
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            a TempDir HOME holds sentinels in go/pkg/mod/cache/download, .m2/repository and
            .gradle/caches/modules-2; a confined command reads each sentinel and then tries
            to write beside it
          rejects: >-
            seeds granted writable, or seeds not granted at all so the reads fail
        - id: host-seeds-are-named-in-the-environment
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            StageCache::environment(HostJob, ...) and (Session, ...) with a host_env whose
            HOME holds the three seed directories and GOPROXY=https://proxy.example
          rejects: >-
            a GOPROXY that drops the file:// seed, keeps the network proxy for HostJob
            (must end in ,off) or drops the host proxy for Session; a MAVEN_OPTS without
            -Dmaven.repo.local.tail=<seed>; a missing GRADLE_RO_DEP_CACHE
        - id: host-job-go-test-never-reuses-cached-results
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            StageCache::environment for HostJob and for Session with host GOFLAGS=-mod=mod
          rejects: >-
            a HostJob GOFLAGS without -count=1, one that replaces the host's -mod=mod, or a
            Session GOFLAGS that gains -count=1
        - id: path-toolchains-are-readable
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            a TempDir HOME with .local/bin/faketool symlinked to
            .bun/install/global/node_modules/x/bin/faketool (a script printing TOOL_OK);
            PATH lists HOME/.local/bin; run_spec_in_stage runs faketool confined
          rejects: >-
            toolchain read roots that ignore PATH entries under HOME or the install roots
            symlinks there point into
        - id: credential-dirs-on-path-stay-hidden
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            PATH lists HOME/.ssh and HOME/.aws/bin; toolchain_read_roots, and a confined
            cat of HOME/.ssh/key through run_spec_in_stage
          rejects: >-
            read roots granting every PATH entry under HOME, credential directories included
        - id: developer-tools-bypass-the-xcrun-shims
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            environment() for HostJob and Session with Some(DeveloperTools) naming a TempDir
            developer dir that holds Toolchains/XcodeDefault.xctoolchain/usr/bin and usr/bin,
            and host PATH=/usr/bin:/bin
          rejects: >-
            a PATH that keeps /usr/bin ahead of the developer dirs, SDKROOT or DEVELOPER_DIR
            set for one consumer but not the other, or a PATH that drops the host entries
        - id: host-job-sccache-socket-is-not-the-sessions
          file: src/stage_cache/contract_tests.rs
          scenario: >-
            environment() for HostJob, Session and Worker; then a StageCache under a home
            path long enough that <root>/h.sock exceeds 104 bytes
          rejects: >-
            one SCCACHE_SERVER_UDS shared by consumers, a socket outside the root, or a
            socket path emitted past SUN_LEN

    - id: stage-cache-sessions
      name: "Stage cache in sessions, workers, cleanup and CI"
      summary: "Points the stage session and codex workers at the stage's cache root, withdraws the host-cache write grants it replaces, removes the root with the stage, and makes CI run every toolchain probe on Linux and macOS."
      stage_type: standard
      skills: ["loom-rust", "loom-ci-cd"]
      implementers: ["claude", "codex"]
      subagent_timeout_secs: 900
      working_dir: "."
      dependencies: ["stage-cache-host"]
      description: |
        Read doc/plans/PLAN-toolchain-build-caches.md sections Decisions and Design, and the
        merged loom/src/stage_cache/ module from stage-cache-host.
        Use parallel subagents and skills to maximize performance.

        PUBLIC SURFACE (contracts are written from this before any code):
          crate::stage_cache::session::wrapper_exports(cache: &StageCache, worktree_root: &Path,
              host_env: &BTreeMap<String, OsString>) -> String
            the shell-escaped `NAME=value \` continuation lines (wrapper.rs CONTINUATION form) for
            Consumer::Session; create_session_wrapper_script embeds it before {claude_cmd}.
          crate::stage_cache::session::apply_session_grant(cache: &StageCache,
              config: &mut crate::sandbox::MergedSandboxConfig)
            pushes the root's absolute path onto filesystem.allow_write.
          crate::stage_cache::cleanup::remove_stage_cache(cache: &StageCache) -> anyhow::Result<bool>
            true when removed, false when absent; never follows a symlink at the root or below it.
          crate::sandbox::PACKAGE_MANAGER_CACHE_WRITE_PATHS keeps only the cargo and rustup entries.

        WORKERS: see the stage table in the plan prose; spawn session, cleanup and ci in ONE
        message. Start the cleanup and ci workers as codex units (cleanup on gpt-6.2-sol, ci on
        gpt-6.1-terra); session is a Claude loom-software-engineer. Each prompt: "Your brief:
        <path>. Read it in full before anything else."

        Run cargo fmt before the final review round. Record every mistake, decision and
        surprise with loom memory immediately; never loom knowledge; never Claude Code
        auto-memory.
      files:
        - "loom/src/stage_cache/**"
        - "loom/src/orchestrator/terminal/native/wrapper.rs"
        - "loom/src/orchestrator/terminal/native/wrapper/host_env.rs"
        - "loom/src/orchestrator/terminal/native/launch.rs"
        - "loom/src/orchestrator/terminal/native/launch/host.rs"
        - "loom/src/sandbox/package_caches.rs"
        - "loom/src/native_worker/boundary/linux.rs"
        - "loom/src/native_worker/boundary/environment.rs"
        - "loom/src/git/cleanup/batch.rs"
        - "loom/src/commands/clean/worktrees.rs"
        - "loom/src/commands/init/cleanup.rs"
        - ".github/workflows/ci.yml"
        - "loom/maintainability-baseline.txt"
      artifacts:
        - "loom/src/stage_cache/session.rs"
        - "loom/src/stage_cache/cleanup.rs"
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --lib stage_cache::"
          timeout_secs: 900
        - "cargo test --manifest-path loom/Cargo.toml --lib sandbox::"
        - "cargo test --manifest-path loom/Cargo.toml --lib orchestrator::terminal::native::"
        - "cargo test --manifest-path loom/Cargo.toml --lib git::cleanup::"
        - "cargo test --manifest-path loom/Cargo.toml --lib native_worker::"
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --all -- --check"
        - 'rg -qF "stage_cache::toolchain_tests::" .github/workflows/ci.yml'
      before_stage:
        - command: "test -f loom/src/stage_cache/environment.rs"
          description: "stage-cache-host's module is merged"
      after_stage:
        - command: 'rg -qF "remove_stage_cache(" loom/src/git/cleanup/batch.rs'
          description: "merge cleanup removes the stage cache root"
        - command: 'rg -qF "apply_session_grant(" loom/src/orchestrator/terminal/native/launch.rs'
          description: "the session sandbox is granted the stage cache root"
      reachable:
        - symbol: wrapper_exports
          from: prepare_session_launch
          description: "every stage session's wrapper exports the stage cache environment"
        - symbol: apply_session_grant
          from: prepare_session_launch
          description: "the session sandbox may write the stage cache root"
        - symbol: remove_stage_cache
          from: cleanup_after_merge
          description: "merge cleanup removes the stage's cache root"
      contracts:
        - id: session-wrapper-exports-the-stage-cache
          file: loom/src/stage_cache/session_contract_tests.rs
          scenario: >-
            wrapper_exports for a StageCache under a TempDir home, a worktree root with a space
            in its name and host GOPROXY=https://proxy.example
          rejects: >-
            exports that are not shell-escaped (the space splits the value), that omit
            GOCACHE, UV_CACHE_DIR, npm_config_cache or SCCACHE_SERVER_UDS, or that carry the
            HostJob values (GOPROXY ending in ,off, GOFLAGS with -count=1, h.sock)
        - id: sessions-no-longer-write-host-caches
          file: loom/src/stage_cache/session_contract_tests.rs
          scenario: >-
            builds a MergedSandboxConfig with crate::sandbox::merge_config for a standard
            stage, applies apply_session_grant, renders crate::sandbox::generate_settings_json,
            and reads sandbox.filesystem.allowWrite
          rejects: >-
            settings that omit the stage cache root, or that still grant ~/go/pkg,
            ~/.cache/go-build, ~/.npm, ~/.cache/uv or ~/.bun/install/cache
        - id: stage-cache-removal-never-follows-a-symlink
          file: loom/src/stage_cache/session_contract_tests.rs
          scenario: >-
            a StageCache root holding a symlink to a TempDir outside it (with a kept file),
            then a root that is itself a symlink; remove_stage_cache on each
          rejects: >-
            a removal that deletes the link target's contents, or one that follows a
            symlinked root

    - id: integration-verify
      name: "Integration Verification"
      summary: "Runs the full suite and reviews, proves the stage cache is wired into host jobs and sessions, and gets the Linux toolchain probe run from the operator."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-ci-cd"]
      implementers: ["claude"]
      working_dir: "."
      dependencies: ["stage-cache-sessions"]
      description: |
        Final verification after all stages. Verify FUNCTIONAL INTEGRATION, not just tests
        passing. NEVER Claude Code auto-memory.
        CONTEXT: read the plan's Overview, Decisions, Design and Non-goals sections by line
        range (doc/plans/), loom memory show --all, doc/loom/knowledge/INDEX.md and only the
        relevant sections.
        BUILD & TEST (zero tolerance): run loom check integration-verify once.
        CODE REVIEW: spawn parallel loom-code-reviewer subagents for security (the new read
        and write roots, the Unix-socket rule, credential hiding), architecture, and test
        coverage; fix ALL findings with an engineer agent. Every finding is fixed or disputed,
        never deferred. Weigh every pending reviewer suggestion.
        FUNCTIONAL: in this worktree, prove on macOS that a confined go test run twice
        through the host path compiles nothing the second time, using the toolchain tests'
        Go probe (cargo test --manifest-path loom/Cargo.toml --lib
        stage_cache::toolchain_tests::go:: -- --nocapture), and that a
        session wrapper written by the launch path contains GOCACHE under the stage root.
        LINUX: once loom check passes, file exactly once:
          loom stage block integration-verify "Run .loom-linux-probe/run.sh .worktrees/integration-verify from the main checkout (Docker running); it writes linux-probe.log into the worktree. Retry the stage when done."
        On retry, read linux-probe.log at the worktree root (never stage or commit it), fix
        any failure it shows and re-run loom check, and record the outcome with loom memory.
        Record discoveries for knowledge-distill, including stale knowledge:
        loom memory note "stale-knowledge: ...".
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --all -- --check"
      files: ["loom/src/**", ".github/workflows/ci.yml", "loom/maintainability-baseline.txt"]

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records the stage cache design, the probe results and the remaining cold spots in the knowledge base."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs. NEVER Claude
        Code auto-memory. SINGLE-AGENT: do NOT spawn subagents.
        START with loom memory pending --group; read the plan's Overview, Decisions, Design
        and Non-goals by line range.
        CORRECTIONS FIRST: apply every stale-knowledge: memory with loom knowledge
        replace-section. Then rewrite in place (replace-section, current truth only):
        architecture/execution-containment.md, the section on the Cargo home and private
        scratch (stage cache root, environment table, seeds, toolchain roots, overlays only
        without a stage cache); concerns/worktree-build-cold-start.md, the sections
        "Build sharing does not happen today" and option 3 (what this plan built, what stays
        cold: compiled objects on a stage's first build, npm/uv/bun downloads on a stage's
        first install, Maven older than 3.9). Record the xcrun shim bypass in
        execution-containment.md, and its residual (Non-goals) in
        concerns/sandbox-and-confinement-gaps.md, correcting
        mistakes/sandbox-tooling-and-network.md where it treats the xcrun_db warning as
        harmless. Record mistakes as mistakes entries.
        TIER ROUTING: about 40 lines or fewer inline in tier-1; larger via
        loom knowledge update <category>/<slug> with a 2-4 line tier-1 summary and link.
        Update README/CONTRIBUTING only where they describe confined criteria or caches; if
        nothing user-facing changed, record WHY in memory.
        SUGGESTIONS: record every unimplemented reviewer suggestion, then resolve it.
        RECEIPTS: loom memory resolve every entry; finish with loom memory pending --strict.
        LAST, if this stage removed structural issues:
        loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
      files: ["doc/loom/knowledge/**", "README.md", "CONTRIBUTING.md"]
```

<!-- END loom METADATA -->
