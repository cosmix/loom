# Brief: real-tool probes per ecosystem

Stage `stage-cache-host`, worker `toolchain-tests`, loom-software-engineer. Working directory `loom/`. Start after `core` and `confinement` returned.

## Goal

Lib tests under `src/stage_cache/toolchain_tests/` (module `stage_cache::toolchain_tests`; the stage's main agent has already added `#[cfg(test)] mod toolchain_tests;` to `src/stage_cache/mod.rs`, so create `toolchain_tests/mod.rs` before you compile). Each test runs a real tool twice through `crate::verify::criteria::run_spec_in_stage`, with ONE `StageCache` under a `TempDir` home, and asserts the tool's own evidence of reuse. CI (stage `stage-cache-sessions`) and the operator's Linux harness run them with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, which turns every skip into a failure.

## Files you own

`src/stage_cache/toolchain_tests/{mod.rs, go.rs, c_cpp.rs, rust.rs, jvm.rs, python.rs, js.rs}`. `mod.rs` holds the shared helpers: a fixture builder, a two-run helper and the skip helper. Run commands through `crate::verify::criteria::run_spec_in_stage`; detect the launcher with `crate::verify::criteria::host_launcher` (read its signature in `src/verify/criteria/confine.rs`). Do not edit any other file; report any helper you need that lives elsewhere.

## Skip rule

- A tool that is missing skips through `crate::process::sandbox_probe::skip_unless(found, name, "<tool> not installed")`. Follow the `optional_tool` pattern in `src/verify/criteria/confine_host_tool_tests.rs:151-163`.
- A missing launcher skips the same way.
- Seeds that need a network download are named by env vars. Skip the seed test when its variable is unset:
  - `LOOM_TEST_GO_SEED_MODULE` (`module@version`, present in the host `GOMODCACHE`);
  - `LOOM_TEST_MAVEN_SEED` and `LOOM_TEST_GRADLE_SEED` (`group:artifact:version`, present in `~/.m2` and the Gradle caches).

## Tests (one behaviour each; assert the quoted evidence)

| Module | Test | Assertion |
| --- | --- | --- |
| go | `std_build_is_reused_by_the_second_command` | second `go test -x ./...` prints no `compile` line for any package (count lines ending in `/compile`) |
| go | `test_results_are_never_reused_by_host_jobs` | the second `go test ./...` output does not contain `(cached)` |
| go | `host_module_seed_resolves_offline` | with the real host env, a module requiring `LOOM_TEST_GO_SEED_MODULE` builds confined, and the host `GOMODCACHE` is unchanged (compare a directory listing before and after) |
| c_cpp | `ccache_hits_on_a_fresh_build_dir` | the second fresh cmake build dir gives a `ccache -s` hit count greater than 0 (`CMAKE_C_COMPILER_LAUNCHER=ccache`) |
| c_cpp | `sccache_launcher_builds_confined` | a cmake build with `CMAKE_C_COMPILER_LAUNCHER=sccache` exits 0 twice; `sccache --show-stats` run confined reports a cache hit on run 2 |
| rust | `cargo_dependencies_stay_fresh` | a fixture crate with one registry dependency already in the host registry; the second confined `cargo build -v --offline` has zero `Compiling` lines for the dependency |
| jvm | `maven_resolves_from_the_host_seed_offline` | `mvn -o -B compile` exits 0 using `LOOM_TEST_MAVEN_SEED`; nothing is written under the host `~/.m2` |
| jvm | `gradle_resolves_from_the_read_only_cache_offline` | `gradle --offline --no-daemon compileJava` exits 0 using `LOOM_TEST_GRADLE_SEED` |
| python | `uv_cache_persists_across_commands` | run 1 runs `uv cache dir` and writes a sentinel there; run 2 sees it. Then an offline `uv pip install` of a wheel built in run 1 from a local fixture package succeeds in run 2 |
| js | `bun_cache_persists_across_commands` | `bun install` of a local `file:` dependency in run 1, then `bun install --frozen-lockfile` with `node_modules` deleted in run 2, exits 0 |
| js | `npm_cache_persists_across_commands` | `npm pack` a local fixture package, `npm install <tgz>` in run 1, `npm ci --offline` with `node_modules` deleted in run 2 exits 0 |

Keep fixtures offline except for the three seed variables. Build test packages locally: a local Go module, a local Python package, a local npm tarball. Each file stays under 400 lines; each function under 50.

## Done when

`cargo test --lib stage_cache::toolchain_tests::` passes on this macOS host with the tools that are installed (skips allowed here). Report each test's outcome (passed, or skipped with reason) and the evidence lines you assert on, quoted from a real run.
