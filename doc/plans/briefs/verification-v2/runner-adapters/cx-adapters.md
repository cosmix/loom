# runner-adapters / codex units — one adapter file per unit (rows CX-A to CX-D)

For the orchestrator: spawn one `loom-codex-forwarder` per file below, in the foreground,
`--model gpt-6-sol --effort xhigh`, an explicit 600000 ms Bash timeout, at most 6 in flight.
Each prompt names ONE unit from the table and pastes that unit's block below verbatim, plus the
trait and helper signatures W1 reported. Tell every unit NOT to run git and NOT to touch any
path under `.loom/`. After each run, check `git status --short` yourself.

## Shared rules for every unit

- You own exactly one file, `loom/src/testrun/adapters/<module>.rs`, plus, for the nine
  documented-format runners, the directory `loom/src/testrun/fixtures/<adapter>/`.
- Start from `loom/src/testrun/adapters/cargo_test.rs` (the reference) and
  `loom/src/testrun/mod.rs` (the trait). Read them with `loom map --outline <file>` and open the
  reference in full. The design is `doc/plans/briefs/verification-v2/DESIGN.md` section D5; its
  table gives your adapter's name, language, single-test command, and what `{test}` means.
- Export `pub static ADAPTER: <Type>`. Implement every trait method. `select_command` returns
  `None` for the adapters D5 lists as unable to select.
- `parse` must turn the runner's own summary into counts. Nine runners exit 0 when a filter
  matches nothing (fixtures `NOTES.md`), so never infer "ran" from the exit code.
  `executed == Some(0)` is how `no-match` is recognised.
- Tests go in a `#[cfg(test)] mod tests` at the bottom of your file:
  - `single_test_command` output for one example;
  - `recognizes` positive and negative;
  - `is_full_run` for a filtered and an unfiltered form;
  - `parse` for every fixture scenario of your adapter via `crate::testrun::fixture_support::load`.
- File ≤ 400 lines, functions ≤ 50.
- You cannot compile your file: it is registered after you return. Your proof is
  `rustfmt --edition 2021 --check loom/src/testrun/adapters/<module>.rs` (syntax only). Say so
  in your report.
- Do not run git. Do not touch `.loom/`.

## Captured runners (fixtures already in `loom/src/testrun/fixtures/<adapter>/`, copied by W1)

Read your adapter's `TESTS.md` and every scenario's `.stdout`/`.stderr`/`.meta` before writing
`parse`. The fixture bytes are ground truth; if they disagree with anything you remember about
the runner, the fixtures win.

| Unit | Module | Adapter | Notes |
| --- | --- | --- | --- |
| CX-A1 | `go_test.rs` | `go-test` | count `--- PASS:` / `--- FAIL:` lines; `[no tests to run]` / `testing: warning: no tests to run` ⇒ executed 0; `build failed` / `[setup failed]` ⇒ build_failed |
| CX-A2 | `pytest.rs` | `pytest` | summary line `N passed, M failed ... in Xs`; `no tests ran`; exit 4 with `ERROR: not found` ⇒ executed 0; collection `ERROR` ⇒ build_failed; handle both the `::` and the `.k` fixtures |
| CX-A3 | `unittest.rs` | `unittest` | `Ran N tests`, `FAILED (failures=M)`; loader `AttributeError` before any run ⇒ executed 0 |
| CX-A4 | `vitest.rs` | `vitest` | `Tests  N passed \| M failed \| K skipped (T)`; all skipped ⇒ executed 0 |
| CX-A5 | `jest.rs` | `jest` | `Tests: ... passed, ... failed, ... skipped, T total` |
| CX-A6 | `mocha.rs` | `mocha` | `N passing`, `M failing`, `K pending` |
| CX-B1 | `bun_test.rs` | `bun-test` | `N pass`, `M fail`, `K skip` lines; `matched 0 tests` ⇒ executed 0 |
| CX-B2 | `node_test.rs` | `node-test` | TAP trailer `# tests N`, `# pass`, `# fail`, `# skipped` |
| CX-B3 | `ctest.rs` | `ctest` | `N% tests passed, M tests failed out of T`; `No tests were found!!!` ⇒ executed 0; the command is the `cmake --build build && ctest ...` pipeline of D5, and compiler errors from `cmake --build` ⇒ build_failed |
| CX-B4 | `dart_test.rs` | `dart-test` | `+N -M: ...` progress lines, final `All tests passed!` / `Some tests failed.`; `No tests match` ⇒ executed 0; strip ANSI (dart colours redirected output); compilation errors ⇒ build_failed |
| CX-B5 | `flutter_test.rs` | `flutter-test` | same format as dart-test; share nothing across files, re-implement |
| CX-B6 | `dotnet_test.rs` | `dotnet-test` | `Passed!  - Failed: M, Passed: N, Skipped: K, Total: T` / `Failed!  - ...`; `No test matches the given testcase filter` ⇒ executed 0; `error CS` / `Build FAILED` ⇒ build_failed |
| CX-C1 | `minitest.rs` | `minitest` | `N runs, A assertions, F failures, E errors, S skips`; executed = runs − skips |

## Documented-format runners (no fixtures exist; you write them)

Also write `loom/src/testrun/fixtures/<adapter>/` with `one-pass`, `one-fail`, `no-match`,
`suite` (3 tests, 1 failing), and for compiled runners `build-error`: each `<scenario>.stdout`,
`<scenario>.stderr` and `<scenario>.meta` (`command:`, `exit:`, `cwd:` lines) in the runner's
documented output format. Add a `PROVENANCE.md` stating: "Written from the runner's documented
output format; the runner was not installed on the plan author's host. Replace with captured
output when available." Take the exit codes from the runner's documented behaviour; where it is
uncertain, say so in `PROVENANCE.md`.

| Unit | Module | Adapter | Summary to parse |
| --- | --- | --- | --- |
| CX-C2 | `cargo_nextest.rs` | `cargo-nextest` | `Summary [ ... ] N tests run: P passed, F failed, S skipped`; `no tests to run` (nextest exits 4 by default) |
| CX-C3 | `gradle.rs` | `gradle` | `N tests completed, M failed` (only printed on failure), `BUILD SUCCESSFUL`/`BUILD FAILED`; `No tests found for given includes` ⇒ executed 0; `Compilation failed` ⇒ build_failed; `> Task :test NO-SOURCE` ⇒ executed 0. A passing run prints no count, and Gradle fails a `--tests` filter that matches nothing, so a successful `> Task :test` records `executed: Some(1)` as a lower bound; document that in a comment and a test |
| CX-C4 | `maven.rs` | `maven` | `Tests run: T, Failures: F, Errors: E, Skipped: S`; `No tests matching pattern` / `No tests were executed` ⇒ executed 0; `COMPILATION ERROR` ⇒ build_failed |
| CX-C5 | `sbt.rs` | `sbt` | `Passed: Total T, Failed F, Errors E, Passed P`; `No tests to run` / `No tests were executed` ⇒ executed 0; `Compilation failed` ⇒ build_failed |
| CX-C6 | `rspec.rs` | `rspec` | `N examples, M failures(, K pending)`; `0 examples, 0 failures` ⇒ executed 0 |
| CX-D1 | `phpunit.rs` | `phpunit` | `OK (N tests, A assertions)` / `Tests: N, Assertions: A, Failures: F`; `No tests executed!` ⇒ executed 0 |
| CX-D2 | `pest.rs` | `pest` | `Tests:  F failed, P passed (A assertions)`; `No tests found` ⇒ executed 0 |
| CX-D3 | `swift_test.rs` | `swift-test` | `Executed N tests, with F failures`; `Executed 0 tests` ⇒ executed 0; `error:` from the compiler ⇒ build_failed |
| CX-D4 | `mix_test.rs` | `mix-test` | `N tests, F failures(, E excluded)`; `0 tests, 0 failures` or all excluded ⇒ executed 0; `== Compilation error` ⇒ build_failed |

## CX-D5 — `loom/src/testrun/languages.rs` (not an adapter)

DESIGN D6: `pub struct LanguageProfile { pub name: &'static str, pub test_file_globs: &'static [&'static str], pub test_declaration: &'static str, pub assertion: &'static str }`,
`pub fn all() -> &'static [LanguageProfile]`, `pub fn by_name(name) -> Option<&'static LanguageProfile>`,
`pub fn for_path(path: &str) -> Option<&'static LanguageProfile>` (glob via the `glob` crate's
`Pattern`), `pub fn skill_for(profile: &str) -> String` (D6 mapping). Profiles: `rust`, `go`,
`python`, `javascript`, `java`, `kotlin`, `scala`, `csharp`, `ruby`, `php`, `swift`, `elixir`,
`cpp`, `dart`, each with test-file globs (for example Rust: `**/tests/**/*.rs`, `**/tests.rs`,
`**/*_tests.rs`, `**/tests_*.rs`, `**/*_test.rs`; Go: `**/*_test.go`; Python: `**/test_*.py`,
`**/*_test.py`, `**/tests/**/*.py`; JS: `**/*.test.{js,jsx,ts,tsx,mjs,cjs}`,
`**/*.spec.{...}`, `**/__tests__/**`), a declaration regex (Rust `#\[(tokio::)?test\]`, Go
`^func Test`, Python `^\s*def test_`, JS `\b(it|test)\s*\(`, Java/Kotlin `@Test`, and so on) and
an assertion regex (Rust `\bassert(_eq|_ne)?!\(`, Python `\bassert\b|self\.assert`, JS
`\bexpect\(|\bassert\.`, Go `\bt\.(Error|Fatal|Fail)`, Java `\bassert[A-Z]\w*\(`, and so on).
The regexes are line-based and must compile under the `regex` crate. Named test (binding):
`every_language_profile_counts_samples`: for every profile, a positive sample line matches both
regexes as intended and a negative sample does not. Proof: `rustfmt --edition 2021 --check`
on the file (the main agent declares `pub mod languages;` in `testrun/mod.rs` when registering).
