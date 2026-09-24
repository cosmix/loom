# Test-runner fixtures

Recorded output of real test runs, one directory per adapter, read at test time by
`testrun/fixture_support.rs`. `every_fixture_classifies_as_expected` in `testrun/tests.rs` parses
every scenario of every registered adapter and checks the resulting `RunOutcome`.

The 14 captured directories are byte-for-byte copies of
`doc/plans/briefs/verification-v2/fixtures/<adapter>/`, recorded on 2026-09-24 on a `linux/amd64`
host. Never edit their bytes: a parser that disagrees with a fixture is the parser's bug. The 9
runners that were not installed on that host (`cargo-nextest`, `gradle`, `maven`, `sbt`, `rspec`,
`phpunit`, `pest`, `swift-test`, `mix-test`) have fixtures written from the runner's documented
output format; each of those directories carries a `PROVENANCE.md` saying so.

## Layout

Each scenario is three files: `<scenario>.stdout`, `<scenario>.stderr` and `<scenario>.meta`
(`command:`, `exit:` and `cwd:` lines). `TESTS.md` in each directory names the fixture project's
tests and the exact filter command form.

| Scenario | Run | Expected outcome |
| --- | --- | --- |
| `one-pass` | a filter selecting one passing test | `Passed` |
| `one-fail` | a filter selecting one failing test | `Failed` |
| `no-match` | a filter selecting nothing | `NotSelected` |
| `suite` | the whole suite: 3 tests, 1 failing | executed 3, failed 1 |
| `build-error` | compiled runners only, a syntax error | `BuildFailed` |

Variant files `<scenario>.<variant>.*` carry their base scenario's expectation:
`.nocolor` (the runner's no-colour flag, where it has one) and `.k` (pytest's `-k <name>` filter;
the base pytest files use `'<file>::<name>'`). No `.nocolor` variant exists for go-test,
unittest, ctest and minitest, which print no ANSI colour. bun-test, node-test, vitest, jest and
mocha already suppress colour on a non-TTY, so their `.nocolor` bytes match the default run;
`dart test` is the one runner that prints ANSI codes to a pipe by default.

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

## `no-match` exit codes

A runner that exits 0 when its filter matches nothing reports a false pass for a mistyped or
renamed test unless the parser sees that zero tests ran.

| Adapter | `no-match` exit | Runner's signal |
| --- | --- | --- |
| cargo-test | **0** | `running 0 tests` |
| go-test | **0** | `testing: warning: no tests to run` |
| pytest (`::` form) | 4 | usage error, nonexistent node id |
| pytest (`-k` form) | 5 | `no tests ran` |
| unittest | 1 | loader `AttributeError`, reported as a run error |
| node-test | **0** | 0 run, 0 failed |
| bun-test | 1 | `error: regex "..." matched 0 tests` |
| vitest | **0** | every test reported `skipped` |
| jest | **0** | `Tests: 3 skipped, 3 total` |
| mocha | **0** | `0 passing` |
| ctest | **0** | `No tests were found!!!` on stderr |
| dart-test | 79 | `No tests match "..."` on stderr |
| flutter-test | 1 | same message as dart-test |
| dotnet-test | **0** | `No test matches the given testcase filter ...` |
| minitest | **0** | `0 runs, 0 assertions, ... 0 failures` |

## ctest `build-error`

ctest never compiles: `add_test()` runs an executable that is already built. The `build-error`
scenario therefore records the pipeline `cmake --build build && ctest --test-dir build -R
'^alpha_passes$' --output-on-failure`, the same form as ctest's single-test command. It fails in
`cmake --build` (compiler diagnostics on stderr, exit 2) and `&&` stops before ctest runs, so the
parser must recognise compiler output with no ctest output at all.

The capture ran `/usr/bin/ctest` by absolute path because the first `ctest` on that host's
`PATH` was a broken pip shim; the recorded bytes are the real CMake binary's.
