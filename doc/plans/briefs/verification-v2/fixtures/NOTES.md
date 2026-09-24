# Fixture capture notes

Captured on 2026-09-24, host `linux/amd64`. All commands were run from the
scratch project directories under
`/tmp/claude-1000/-home-dkaponis-src-loom/99212933-6012-4c4f-a6a4-f4e7f5668350/scratchpad/runner-projects/<runner>/`
(scratch, not checked in). Each adapter directory here holds
`<scenario>.stdout` / `.stderr` / `.meta` (+ `.nocolor.*` where the runner
supports disabling color), and a `TESTS.md` describing the fixture's exact
source and test names.

## Runner versions

| Tool | Version |
| --- | --- |
| cargo | 1.97.1 (c980f4866 2026-06-30) |
| go | go1.25.5 linux/amd64 |
| python3 | 3.14.4 |
| pytest | 9.0.2 |
| node | v22.18.0 |
| bun | 1.3.14 |
| vitest | 5.0.1 (project-local, via bunx) |
| jest | 30.5.2 (project-local, via bunx) |
| mocha | 12.0.2 (project-local, via bunx) |
| cmake / ctest | 4.2.3 |
| dart | 3.5.1 stable |
| flutter | 3.24.1 stable (Dart 3.5.1, DevTools 2.37.2) |
| dotnet | 10.0.112 (xUnit template, SDK-bundled) |
| ruby | 3.3.8 |
| minitest (gem) | 5.20.0 (Ruby stdlib) |

## Adapters captured (14)

cargo-test, go-test, pytest, unittest, node-test, bun-test, vitest, jest,
mocha, ctest, dart-test, dotnet-test, minitest, flutter-test.

flutter-test: project creation (`flutter create --platforms=linux`) took
about 1.4s and the first `flutter test` run about 6.3s — well under the
5-minute skip threshold, so it was captured in full.

## Skipped runners

cargo-nextest, gradle, maven, rspec, php/phpunit/pest, swift, elixir/mix,
sbt — all confirmed not installed on this host per the brief; not probed.

## Command-form deviations and notable findings

- **ctest**: the `ctest` resolved first on `PATH` is a broken pip-installed
  shim at `~/.local/bin/ctest` (`ModuleNotFoundError: No module named
  'cmake'`). All ctest commands were run via the absolute path
  `/usr/bin/ctest` (the real CMake-suite binary, same 4.2.3 as `cmake`)
  instead of the bare `ctest` the brief's command form shows.
- **ctest `build-error`**: ctest itself never compiles — `add_test()` just
  runs an already-built executable. Unlike cargo/go/dotnet/dart, whose test
  command performs the build, capturing a real build failure required the
  paired pipeline `cmake --build build && ctest --test-dir build -R
  '^alpha_passes$' --output-on-failure`; that is the exact command recorded
  in `ctest/build-error.meta`. It fails at the `cmake --build` stage
  (compiler diagnostics on stderr, exit 2) and `&&` short-circuits, so
  ctest never runs.
- **pytest**: both filter forms from the brief were captured — the base
  `<scenario>.*` files use `pytest '<file>::<name>' -q`, and `<scenario>.k.*`
  files use `pytest <file> -q -k <name>`. They diverge on `no-match`: the
  `::` form exits 4 (usage error, nonexistent node id), the `-k` form exits
  5 (no tests collected).
- **No `.nocolor` variants** were captured for go-test, unittest, ctest, and
  minitest — none of the four has ANSI color output or a documented
  no-color flag to test against (confirmed by inspecting default output
  bytes / `--help`).
- For bun-test, node-test, vitest, jest, and mocha, the `.nocolor` variant
  bytes came out byte-identical (or near-identical, trailing-whitespace
  only) to the default run: all five already suppress color when stdout/
  stderr are not a TTY, which is always true under this capture method.
  `dart test` is the outlier — its default output DOES carry ANSI codes
  even on a redirected pipe (`--no-color` measurably shrinks it); `flutter
  test` (same underlying test runner, wrapped) does not.

## `no-match` exit codes (zero-exit-on-no-match is the risk case)

| Adapter | `no-match` exit | Note |
| --- | --- | --- |
| cargo-test | **0** | "running 0 tests" is success |
| go-test | **0** | `testing: warning: no tests to run` |
| pytest (`::` form) | 4 | usage error, nonexistent node id |
| pytest (`-k` form) | 5 | "no tests ran" |
| unittest | 1 | loader `AttributeError` treated as run error |
| node-test | **0** | 0 run / 0 fail is not a failure |
| bun-test | 1 | explicit `error: regex "..." matched 0 tests` |
| vitest | **0** | all tests reported "skipped" |
| jest | **0** | "Tests: 3 skipped, 3 total" |
| mocha | **0** | "0 passing" |
| ctest | **0** | `No tests were found!!!` on stderr |
| dart-test | 79 | `No tests match "..."` on stderr |
| flutter-test | 1 | same message as dart-test, different code |
| dotnet-test | **0** | `No test matches the given testcase filter ...` |
| minitest | **0** | "0 runs, 0 assertions, ... 0 failures" |

Nine of fourteen adapters (cargo-test, go-test, node-test, vitest, jest,
mocha, ctest, dotnet-test, minitest) exit **0** when the filter matches
nothing: a loom adapter that treats "exit 0" as "tests ran and passed"
will silently report a false pass whenever a test name is mistyped or
renamed. Only pytest (both filter forms), unittest, bun-test, dart-test,
and flutter-test surface a nonzero exit on their own.
