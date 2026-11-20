# Brief: probes-and-ci

Stage `seed-stage-builds` of `doc/plans/PLAN-warm-stage-builds.md`. Read the plan's "What was measured while planning" section first: your probes turn those one-off measurements into tests.

The existing toolchain probes prove that a cache persists across two commands of ONE stage (`bun_cache_persists_across_commands`, `cargo_dependencies_stay_fresh`). You add the probe that was missing: a SECOND tree, seeded, starts warm.

Another worker writes the seeding functions while you work. Call them by these signatures and do not open their files:

```rust
crate::stage_cache::seed::seed_worktree(repo_root: &Path, worktree: &Path) -> Vec<Seeded>
crate::stage_cache::seed::seed_host_caches(repo_root: &Path, cache_root: &Path,
    host_env: &BTreeMap<String, OsString>) -> Vec<Seeded>
crate::stage_cache::seed::{Seeded { what: String, outcome: SeedOutcome }, SeedOutcome { Cloned, Skipped(String) }}
crate::fs::clone_tree::can_clone_in(dir: &Path) -> bool
```

## Files you own

- `loom/src/stage_cache/toolchain_tests/mod.rs`, `rust.rs`, `js.rs`, `python.rs`, `go.rs`
- `.github/workflows/ci.yml`

Read-only: `loom/src/stage_cache/seed_contract_tests.rs` is frozen.

## How the existing probes work

`loom/src/stage_cache/toolchain_tests/mod.rs`: `Rig::start(test, tools)` (line 49) returns `None`, after printing a skip, when a tool is missing or a confined command cannot start; `Rig::write`, `Rig::run(script)` (line 100) runs a shell script confined in the rig's tree with the rig's stage cache; `Rig::root()` is the cache root; `seed(test, VARIABLE)` (line 140) reads a `LOOM_TEST_*` variable or skips; `output(result)` joins stdout and stderr. Skips go through `process::sandbox_probe::skip_unless`, which panics when `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`. Every probe is `#[serial_test::serial]`. `go.rs:29-44` is the shortest complete example.

## What to add to the rig

- A second tree: a way to get a sibling directory of the rig's tree on the same filesystem, written with the same files, in which a confined command runs with the same stage cache. Seeding needs both trees to be git checkouts whose ignore rules cover the build directory: `git init` each and write a `.gitignore`. Find how the tree's tests run git with ambient configuration shut out and use that.
- A clone guard: `fn clones(test: &str, dir: &Path) -> bool` that returns `can_clone_in(dir)` and, when it is false, prints a `SKIP` line naming the filesystem, or panics when `LOOM_TEST_REQUIRE_CLONE=1`. This is separate from the sandbox guard: the Linux CI runners sit on ext4, where a sandbox starts and a clone cannot.

## The probes

Each one seeds, then runs the tool confined in the second tree and asserts on what the tool reports.

| File | Test | Setup | Assertion |
| --- | --- | --- | --- |
| `rust.rs` | `a_seeded_target_keeps_the_dependency_fresh_in_a_second_tree` | package `app` depending by path on a crate in a third directory outside both trees (no registry access); `cargo build --offline` confined in the first tree; write the second tree afterwards; `seed_worktree(first, second)` | `target` is `Cloned`; `cargo build --offline -v` in the second tree prints `Fresh` for the outside crate and `Compiling app`; running `target/debug/app`, which prints `env!("CARGO_MANIFEST_DIR")`, prints the second tree's path |
| `js.rs` | `a_seeded_node_modules_resolves_without_an_install` | first tree: `package.json` and a hand-written `node_modules/dep/index.js`; `seed_worktree` | `node -e "require('dep')"` confined in the second tree exits 0 |
| `js.rs` | `bun_installs_offline_from_the_cloned_host_cache` | `seed(test, "LOOM_TEST_BUN_SEED")` gives `<cache dir>=<package>@<version>`; `seed_host_caches` with `BUN_INSTALL_CACHE_DIR` naming that directory | `bun` is `Cloned`; `bun add --offline <package>@<version>` confined in a tree with a bare `package.json` exits 0; the seed directory's listing is unchanged afterwards (`listing`, as `go.rs:68-99` checks its seed) |
| `js.rs` | `npm_installs_offline_from_the_cloned_host_cache` | `LOOM_TEST_NPM_SEED` gives `<npm cache dir>=<package>@<version>`; `seed_host_caches` with `npm_config_cache` | `npm` is `Cloned`; `npm install --offline --no-audit --no-fund --ignore-scripts <package>@<version>` exits 0 |
| `python.rs` | `uv_installs_offline_from_the_cloned_host_cache` | `LOOM_TEST_UV_SEED` gives `<uv cache dir>=<package>==<version>`; `seed_host_caches` with `UV_CACHE_DIR` | `uv` is `Cloned`; `uv venv .venv && uv pip install --offline --python .venv/bin/python <package>==<version>` exits 0 |
| `go.rs` | `the_standard_library_is_warm_from_the_cloned_host_cache` | `LOOM_TEST_GO_SEED_BUILD_CACHE` names a `GOCACHE` in which the standard library was built; a module with one `main.go` importing `fmt` and `net/http`; `seed_host_caches` with `GOCACHE` naming it | `go-build` is `Cloned`; `go build -x ./...` confined prints at most 3 `/compile ` lines (planning saw 122 cold and 2 with the cloned cache; `compiles()` in `go.rs` counts them) |

In every probe the project fixture needs the marker that gives it the right kind for `seed_host_caches` (`package.json`, `go.mod`, and for Python the marker `loom/src/language/evidence.rs` lists), in a git checkout. Pass a `host_env` map you build yourself, with `HOME` set to a temp directory, so no probe reads the operator's real caches.

Where `clones(...)` is false a probe asserts the seed outcome is `Skipped` and that nothing was created, then returns: on such a filesystem that is the whole behaviour.

## CI (`.github/workflows/ci.yml`)

Both test jobs have a "Seed the toolchain caches" step (Linux at about line 193, macOS at about line 367) that builds seed projects under `$RUNNER_TEMP` and exports `LOOM_TEST_*` coordinates through `GITHUB_ENV` (lines 259-261 and 433-435), and a "Run the confined toolchain probes" step that runs `cargo test --lib -- stage_cache:: ...` with `LOOM_TEST_REQUIRE_SANDBOX_FREE: "1"`.

- In both seeding steps, create the four new seeds under `$RUNNER_TEMP` with the tools the job already installs: a bun cache holding one small package (`BUN_INSTALL_CACHE_DIR=<dir> bun add <package>@<version>` in a scratch project), an npm cache (`npm install --cache <dir> <package>@<version>`), a uv cache (`UV_CACHE_DIR=<dir> uv pip install` into a scratch venv), and a Go build cache (`GOCACHE=<dir> go build std`). Pin exact versions. Export the four variables in the form the probes parse.
- In the macOS job only, set `LOOM_TEST_REQUIRE_CLONE: "1"` on the probes step: the macOS runner is APFS, so a probe that cannot clone there is a failure. Do not set it on Linux.
- Quote any `run:` value ending in `::`; an unquoted one is a YAML mapping error. Keep each step's existing structure and only add lines.

## Local check

On macOS with `cargo`, `node`, `bun`, `npm`, `uv` and `go` installed, build the four seeds by hand in a temp directory, export the variables, and run `LOOM_TEST_REQUIRE_SANDBOX_FREE=1 LOOM_TEST_REQUIRE_CLONE=1 cargo test --manifest-path loom/Cargo.toml --lib stage_cache::toolchain_tests` from a normal shell (a stage session's own sandbox refuses the nested sandbox these probes start; if yours does, say so and give the orchestrator the exact command to run through `loom check`). Parse the workflow file with a YAML parser to confirm it still loads.

## Done when

The six probes pass locally with skips fatal, the existing probes still pass, the workflow parses, and `cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings` is clean for your files. Report: files changed, each command with its result, the exact package versions you pinned in CI, anything unresolved, any mistake worth recording.
