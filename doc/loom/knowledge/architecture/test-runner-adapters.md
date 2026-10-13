# Test Runner Adapters

> 23 adapters, profiles, quoting, detect

## Test-Runner Adapters, Language Profiles, and Project Detection

`loom/src/testrun/` holds 23 `TestRunnerAdapter`s (`adapters/*.rs`: cargo-test, cargo-nextest, go-test, pytest,
unittest, vitest, jest, mocha, bun-test, node-test, gradle, maven, sbt, dotnet-test, rspec, minitest, phpunit,
pest, swift-test, mix-test, ctest, dart-test, flutter-test), a `registry` (`recognize`, `resolve_adapter`), a
`RunSummary`/`RunOutcome` parse layer (`Passed`, `Failed`, `BuildFailed`, `NotSelected`, `Unparsed`), and
`languages.rs`, one `LanguageProfile` per language (test-file globs, a declaration regex, an assertion regex).
Consumers: the zero-test guard, contract freeze and completion, impact-selected tests and test integrity
(see `architecture/verification-v2-gates.md`, `architecture/contract-phase.md`).

**Fixtures.** Each adapter's parser is pinned by captured runner output under `testrun/fixtures/<adapter>/`.
Fourteen are captured from real runs. Nine are written from the runner's documented output format because the
runner was not installed on the recording host; each carries a `PROVENANCE.md`. They are listed in
`concerns/verification-v2-followups.md`.

**Recognition helpers** (`testrun/recognize.rs`) are `pub` in a `pub` module. A `pub(crate)` helper with no
caller raises `dead_code` under `clippy -D warnings`; a `pub` item reachable from `lib.rs` does not.

**Shell safety.** `testrun/command.rs` quotes every `{test}` and `{file}` placed into an adapter command:
`shell_quote` always, `shell_word` only when needed (so simple names stay byte-identical to the documented
command), `regex_literal` for runners whose filter is a regex. Formatting a test name into a command with
`format!` let an apostrophe break the command and a quote inject shell. `dotnet-test` emits
`--filter 'FullyQualifiedName=<name>'` in single quotes for the same reason. `glob` 0.3 has no brace
alternation, so JS test globs expand one pattern per extension.

**Parse rules that were easy to get wrong.** cargo-test: `executed = sum(running N tests) - sum(ignored)`;
`build_failed` only when no test binary started; a failing doctest is a test failure, not a build failure.
node-test reports a no-match file as a passing subtest with an inner `1..0` plan, so a `# tests`/`# pass`
footer reader classifies no-match as `Passed`. unittest reports both a missing name and a module that fails to
import as an `ERROR` from `_FailedTest`; only the `AttributeError` case is `NotSelected`, or a contract whose
module imports a not-yet-written name is rejected at freeze.

**Detection** (`skills/project/markers.rs`, `runners.rs`, `probe.rs`, `scan.rs`). Kinds added: java, kotlin,
scala, csharp, ruby, php, swift, elixir, cpp, dart, javascript. Rules that differ from a literal reading of
the design: `javascript` means `package.json` with no `typescript` kind (no `tsconfig.json`, no `typescript`
dependency), so the two never co-occur; gradle wins when `build.gradle*` OR `settings.gradle*` exists (a
multi-project root has only settings); flutter-test needs the substring `sdk: flutter` matched by line through
`probe::has_line`, not bare `flutter`; pytest evidence also accepts a `[tool.pytest]` table. Java versus kotlin
follows the build-script DSL (`build.gradle.kts` is kotlin), not the source language; the runner stays correct.
Probe reads go through `fs::safe_read::read_bounded` with an `lstat` regular-file pre-check, because
`safe_read` opens `O_RDONLY` without `O_NONBLOCK` and a FIFO named like a manifest would block the prompt hook.
`loom project detect [PATH] [--json]` (`commands/project.rs`, `cli/types_project.rs`, declared by `#[path]` in
`types.rs`) prints per package the kinds, the runner and the skills; `"runner":null` means unsupported.
`loom-hooks/skill-trigger.sh` maps a kind to `loom-<kind>` and finds no skill for `javascript`
(`recommend::resolve_skill` maps it to `loom-typescript`).

## Run-Root Markers

`TestRunnerAdapter::run_root_markers` names the files that make a directory the one the runner must run from: the runner reads its config, or finds its project, only in its cwd. A `*.ext` entry matches any file with that extension. Contracts use it (see architecture/contract-phase.md, "Where a Contract Runs"). Set: bun-test bunfig.toml; vitest `vitest.config.*`; jest `jest.config.*`; mocha `.mocharc.*`; dart/flutter pubspec.yaml; go `go.mod`; cargo-test and nextest `Cargo.toml`; swift `Package.swift`; mix `mix.exs`; rspec `.rspec`, `Gemfile`; minitest `Gemfile`, `Rakefile`; phpunit and pest `phpunit.xml*`; dotnet `*.sln`, `*.slnx`, `*.csproj`, `*.fsproj`, `*.vbproj`. None for node-test, pytest, unittest, gradle, maven, sbt and ctest: pytest locates its config from its arguments, and the build-tool runners select by name across a multi-module build, where a module dir is not reliably the right root. A plan sets contract `cwd` for those.

Excluded on purpose: `vite.config.*` (an app's build config usually has no `test` block, and would shadow a root `vitest.config` defining projects) and `composer.json` (a path-repository package has no `vendor/bin` of its own).

bun-test passes every relative file as `./<path>`: without the prefix bun reads the argument as a substring filter. Its parser classifies `did not match any test files` and `Test filter ... had no matches` as `NotSelected`, so a freeze refuses a run that found no file instead of accepting it as `exit_nonzero_unverified`. `testrun/command.rs::package_runner` looks for a bun lockfile in the package dir and each ancestor up to the first one holding `.git`, because a workspace's lockfile sits at the repository root.
