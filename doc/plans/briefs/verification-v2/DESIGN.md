# Verification v2 — authoritative design

Every stage of `doc/plans/PLAN-verification-v2.md` and every worker brief under
`doc/plans/briefs/verification-v2/` implements against this file. Where a brief and this file
differ, this file wins. Section ids (`D1` ...) are cited by the briefs.

Source: `doc/verification-report.md` and the operator's decisions recorded with the plan.

## D0. Engineering constraints (every code stage)

- **v1 plans keep today's behaviour exactly.** Nothing that changes whether a v1 stage passes or
  fails, what a v1 plan's `plan verify` reports as an error, or what a v1 stage's agent must do,
  may ship. New `plan verify` lints report as warnings on v1 and as errors on v2 unless a
  section says otherwise. Every v2 behaviour is gated on the plan version (D2).
- **Maintainability ledger.** `loom/maintainability-baseline.txt` is exact-match
  (`cargo test --manifest-path loom/Cargo.toml --test maintainability`). A ledgered file or
  function may never grow. Shrinking one requires lowering its entry to the new exact count, or
  removing the entry once the item is back under the limit. New code goes into new modules;
  a call site added inside a ledgered function is paid for by extracting lines out of that
  function in the same change. The stage's main agent owns the ledger file; workers report the
  exact new counts of every ledgered file or function they touched.
- **Size caps (CLAUDE.md Rule 17).** New files ≤ 400 lines, new functions ≤ 50 lines. Tests
  count. Split test modules into sibling files (`#[path = "..."] mod ...;`) rather than grow one.
- **No placeholders.** A module is created together with its first real content.
- **Named tests.** Each stage's acceptance selects the tests its prose names, by name, and
  asserts the exact number that passed. A missing, renamed or unselected test fails the stage.
  Test function names given in a brief are binding.
- **Doctrine surfaces.** Text pinned by `orchestrator/signals/tests_doctrine*.rs` (BLOCK-A/B/C/D)
  is not edited by any stage except `doctrine-v2`, and only by adding BLOCK-E.

## D1. Plan version

- `loom.version` accepts `1` and `2`. Any other value is an error:
  `Unsupported version: <n>. Supported versions: 1, 2.`
- A v1 plan that uses a v2-only field (D3) is rejected with one error per use:
  `` `<field>` requires `version: 2` `` (stage-scoped when the field is on a stage).
- Tests that used `version: 2` as the "unsupported" example move to `version: 3`:
  `tests/integration/plan_verify.rs` (`invalid_version_plan`, `test_invalid_version`),
  `plan/schema/tests/validation_tests.rs` (`test_validate_unsupported_version`,
  `test_validate_multiple_errors`), `plan/parser/validation.rs`
  (`test_validate_unsupported_version`), `plan/parser/mod.rs`
  (`test_parse_validation_fails_unsupported_version`).

## D2. Runtime plan version

- `Stage` (runtime, `models/stage/types.rs`) gains `plan_version: u32`, serde default `1`.
- `Stage::from_definition` takes the plan identity:

  ```rust
  pub struct PlanIdentity<'a> {
      pub id: &'a str,
      pub version: u32,
      pub ratchet_files: &'a [String],
  }
  impl Stage {
      pub fn from_definition(definition: &StageDefinition, plan: &PlanIdentity<'_>) -> Self;
  }
  ```

  `commands/init/plan_setup.rs::create_stage_from_definition` builds it from the parsed plan.
- Every v2 behaviour reads `stage.plan_version == 2`. Nothing reads the plan file at run time to
  learn the version.

## D3. v2 schema fields

New types live in `plan/schema/types_v2.rs`, re-exported from `plan/schema/types.rs`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSpec {
    pub id: String,              // ^[a-z0-9][a-z0-9-]*$, unique within the stage
    pub file: String,            // test file, relative to working_dir, no `..`
    pub test: String,            // exact name the adapter selects (D5 table)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,  // adapter name; None = detected (D7)
    pub scenario: String,        // what the test sets up
    pub rejects: String,         // the plausible wrong implementation it must fail on
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReachableCheck {
    pub symbol: String,          // the new unit that must be reached
    pub from: String,            // the entry point it must be reached from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>, // 0.0..=1.0, default 0.0 (as `loom map --impact`)
    pub description: String,
}
```

| Field | Where | Serde | Meaning |
| --- | --- | --- | --- |
| `contracts: Vec<ContractSpec>` | `StageDefinition`, `Stage` | default, skip if empty | D8 |
| `harness: Vec<String>` | `StageDefinition`, `Stage` | default, skip if empty | globs (relative to working_dir) the contract session may also edit |
| `reachable: Vec<ReachableCheck>` | `StageDefinition`, `Stage` | default, skip if empty | D11 |
| `literal: bool` | `WiringCheck` | default false, skip if false | D11 |
| `ratchet_files: Vec<String>` | `LoomConfig`; copied onto every `Stage` | default, skip if empty | D13 |

`StageDefinition` and `LoomConfig` gain `Default` (derived or hand-written) so test fixtures
spread `..Default::default()`. `fs/stage_loading.rs::definition_from_stage` (the one production
construction) keeps listing every field explicitly.

Validation (v2 plans only; each rule an error):

- every `standard` stage has at least one contract; `knowledge`, `knowledge-distill` and
  `integration-verify` stages have none;
- contract `id` pattern and uniqueness; `file`, `test`, `scenario`, `rejects` non-empty; `file`
  and every `harness` entry relative with no `..` component;
- `reachable`: `symbol`, `from`, `description` non-empty; `min_confidence` within `0.0..=1.0`;
- `ratchet_files` entries relative with no `..`.

## D4. `plan verify` lints

One entry point, `plan/schema/validation/v2_lints/mod.rs`:

```rust
pub(crate) struct LintContext<'a> {
    pub metadata: &'a LoomMetadata,
    pub repo_root: Option<&'a Path>,
}
pub(crate) struct LintFinding {
    pub stage_id: Option<String>,
    pub message: String,
    pub error_in_v2: bool, // false = warning in both versions
}
pub(crate) fn run(ctx: &LintContext<'_>) -> Vec<LintFinding>;
```

Findings map to errors when `version == 2 && error_in_v2`, else to structural warnings (so
`--strict` fails on them). Commands scanned: `acceptance`, `setup`, `wiring_tests[].command`,
`before_stage[].command`, `after_stage[].command`, `dead_code_check.command`. Commands are lexed
with `plan/schema/validation/shell_lex.rs`, never split by hand.

| Lint | Rule | error_in_v2 |
| --- | --- | --- |
| Unknown `loom` subcommand | a simple command whose argv[0] is `loom` (or ends in `/loom`) names a subcommand path absent from `crate::cli::Cli::command()` (clap `CommandFactory`); a parent that requires a subcommand and gets none also counts | yes |
| Regex: wiring pattern does not compile | same `RegexBuilder` settings as `verify/goal_backward/wiring.rs` | yes |
| Regex: `[[` in a wiring pattern or a non-`-F` `rg`/`grep` pattern | a character class the author almost certainly meant literally | no |
| Regex: pattern read as a flag | the first positional pattern of `rg`/`grep` starts with `-` and no `-e`/`--regexp`/`--` precedes it | yes |
| Regex: `rg`/`grep` pattern without `-F` does not compile | Rust `regex` syntax | yes |
| Network without a domain | a network binary (the existing `Hazard::Network` set) while the stage's effective `network.allowed_domains` is empty | yes |
| Ungrantable resource | the existing `criterion_needs_ungrantable_resource` set (`tmux`, `docker`, `loom map`, `loom knowledge context`) | yes |
| Rustc wrapper | a `cargo` command while `orchestrator/terminal/native/build_cache.rs::find_sccache_path()` finds sccache and `LOOM_SCCACHE` is not `0`: message names the operator fix `LOOM_SCCACHE=0 loom run` | no |
| Knowledge check that cannot pass | a `knowledge`/`knowledge-distill` criterion `loom knowledge check --strict` without `--baseline` while the repository's knowledge tree has structural issues now (run the check in-process, read-only) | yes |
| Rust filter matches nothing (G5) | `cargo test` with `--lib <path>::` or a positional filter containing `::`, whose module path matches no node in the source graph's base layer for HEAD (loaded read-only with `GraphStore::load_base`, never `ensure_snapshot`) and no path in the stage's `files:`/`artifacts:` could create it (`a::b` ↔ `src/a/b.rs`, `src/a/b/mod.rs`, `src/a/b/**`). No base layer ⇒ one note, no finding | no |
| Contract runner unknown (stage `contract-phase`) | `runner` names no registered adapter | yes |
| Contract runner undetectable (stage `contract-phase`) | `runner` absent and D7 detection finds no adapter for the package owning `file`: message says completion falls back to exit code | no |
| IV lacks a full test command (G3, stage `contract-phase`) | a v2 `integration-verify` stage has no acceptance command that an adapter recognises as a full run (`is_full_run`) | yes |

`plan verify` stays free of side effects: no cache writes, no index writes, no builds.

## D5. Test-runner adapters

Module `loom/src/testrun/` (declared in `lib.rs`).

```rust
pub struct RunOutput<'a> { pub stdout: &'a str, pub stderr: &'a str, pub exit_code: Option<i32> }

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub executed: Option<u64>,   // tests that actually ran (passed + failed)
    pub passed: Option<u64>,
    pub failed: Option<u64>,
    pub skipped: Option<u64>,
    pub build_failed: bool,      // compilation / collection failed before tests ran
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome { Passed, Failed, BuildFailed, NotSelected, Unparsed }

pub struct TestTarget { pub file: String, pub name: Option<String> }

pub trait TestRunnerAdapter: Sync {
    fn name(&self) -> &'static str;
    fn language(&self) -> &'static str;                 // D6 profile name
    fn recognizes(&self, argv: &[String]) -> bool;      // one simple command
    fn is_full_run(&self, argv: &[String]) -> bool;     // recognised and no name/target filter
    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String;
    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String>;
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary;
}

pub fn classify(summary: &RunSummary, exit_code: Option<i32>) -> RunOutcome;
pub mod registry {
    pub fn all() -> &'static [&'static dyn TestRunnerAdapter];
    pub fn by_name(name: &str) -> Option<&'static dyn TestRunnerAdapter>;
    /// First adapter recognising any simple command of `command` (lexed with shell_lex),
    /// including package-script indirection (D5 below).
    pub fn recognize(command: &str, cwd: &Path) -> Option<&'static dyn TestRunnerAdapter>;
}
```

`classify`: `build_failed` ⇒ `BuildFailed`; `executed == Some(0)` ⇒ `NotSelected`;
`failed > 0` or (`executed ≥ 1` and exit ≠ 0) ⇒ `Failed`; `executed ≥ 1`, `failed == 0`, exit 0
⇒ `Passed`; nothing parsed ⇒ `Unparsed`. `Unparsed` is handled like an unsupported runner:
the exit code decides and a warning names the adapter.

Registration uses one macro invocation in `testrun/adapters/mod.rs`,
`adapters!(cargo_test, cargo_nextest, ...)`, which declares each module and lists each
`pub static ADAPTER` in `registry::all()`. Adding an adapter is one identifier in that list.

The 23 adapters (names are binding; the command column is `single_test_command`'s output with
`{file}`, `{test}` substituted; commands run with `package_dir` as cwd):

| Adapter | Language | Single-test command | No-match exit (fixture) |
| --- | --- | --- | --- |
| `cargo-test` | rust | `cargo test {test} -- --exact` | 0 |
| `cargo-nextest` | rust | `cargo nextest run -E 'test(={test})'` | documented |
| `go-test` | go | `go test ./{file_dir}/ -run '^{test}$' -v` | 0 |
| `pytest` | python | `uv run pytest '{file}::{test}' -q` when `uv.lock` is in package_dir, else `python3 -m pytest '{file}::{test}' -q` | 4 |
| `unittest` | python | `python3 -m unittest {test} -v` (`uv run python -m unittest ...` with `uv.lock`) | 1 |
| `vitest` | javascript | `bunx vitest run {file} -t '{test}'` (`npx` without a bun lockfile) | 0 |
| `jest` | javascript | `bunx jest {file} -t '{test}'` (`npx` without a bun lockfile) | 0 |
| `mocha` | javascript | `bunx mocha {file} --grep '{test}'` (`npx` without a bun lockfile) | 0 |
| `bun-test` | javascript | `bun test {file} -t '{test}'` | 1 |
| `node-test` | javascript | `node --test --test-name-pattern='^{test}$' {file}` | 0 |
| `gradle` | java | `./gradlew test --tests '{test}'` when `gradlew` exists, else `gradle test --tests '{test}'` | documented |
| `maven` | java | `mvn -q test -Dtest='{test}'` | documented |
| `sbt` | scala | `sbt 'testOnly {test}'` | documented |
| `dotnet-test` | csharp | `dotnet test --filter "FullyQualifiedName={test}"` | 0 |
| `rspec` | ruby | `bundle exec rspec {file} -e '{test}'` when `Gemfile` exists, else `rspec {file} -e '{test}'` | documented |
| `minitest` | ruby | `ruby -Itest {file} -n '/^{test}$/'` | 0 |
| `phpunit` | php | `vendor/bin/phpunit --filter '{test}' {file}` | documented |
| `pest` | php | `vendor/bin/pest --filter '{test}' {file}` | documented |
| `swift-test` | swift | `swift test --filter '{test}'` | documented |
| `mix-test` | elixir | `mix test {file} --only 'test:{test}'` | documented |
| `ctest` | cpp | `cmake --build build && ctest --test-dir build -R '^{test}$' --output-on-failure` | 0 |
| `dart-test` | dart | `dart test {file} --plain-name '{test}'` | 79 |
| `flutter-test` | dart | `flutter test {file} --plain-name '{test}'` | 1 |

What `{test}` (a contract's `test` field) is, per adapter:

| Adapter | `test` value |
| --- | --- |
| `cargo-test`, `cargo-nextest` | the full libtest path as `cargo test -- --list` prints it, e.g. `plan::schema::tests::v2_tests::rejects_x` |
| `go-test` | the test function name, e.g. `TestAlphaPasses` |
| `pytest` | the node id after `::`, e.g. `test_alpha_passes` or `TestSuite::test_alpha_passes` |
| `unittest` | dotted `module.Class.method` |
| `vitest`, `jest`, `bun-test`, `node-test` | the test's full name (describe titles included, space-joined) |
| `mocha` | the test's full title |
| `gradle` | `package.Class.method` |
| `maven` | `Class#method` |
| `sbt` | the fully qualified suite name |
| `dotnet-test` | `Namespace.Class.Method` |
| `rspec` | the example's full description |
| `minitest` | the method name, e.g. `test_alpha_passes` |
| `phpunit`, `pest` | the test method name or pest description |
| `swift-test` | `Module.Class/testMethod` |
| `mix-test` | the ExUnit test name including its `test` prefix |
| `ctest` | the `add_test` name |
| `dart-test`, `flutter-test` | the test description |

Fixtures:

- 14 runners were captured from real runs on the plan author's host
  (`doc/plans/briefs/verification-v2/fixtures/<adapter>/`, see its `NOTES.md` for versions,
  command deviations and the no-match table). Scenarios: `one-pass`, `one-fail`, `no-match`,
  `suite` (3 tests, 1 failing), and `build-error` for compiled runners. The adapters stage copies
  them to `loom/src/testrun/fixtures/<adapter>/` and never edits their bytes.
- The 9 runners not installed on that host (`cargo-nextest`, `gradle`, `maven`, `sbt`, `rspec`,
  `phpunit`, `pest`, `swift-test`, `mix-test`) get fixtures written from the runner's
  documented output format, each directory carrying a `PROVENANCE.md` that says so. Integration
  verify records these nine in loom memory; knowledge-distill lists them in `concerns`.
- A table-driven test classifies every fixture of every registered adapter and asserts the
  expected `RunOutcome` (`one-pass` ⇒ Passed, `one-fail` ⇒ Failed, `no-match` ⇒ NotSelected,
  `build-error` ⇒ BuildFailed, `suite` ⇒ executed 3, failed 1).

Recognition (`recognizes`, `is_full_run`) matches invocations with and without runner prefixes:
`bunx`/`npx`/`pnpm exec`/`yarn` for JS runners, `python`/`python3 -m`, `uv run`,
`bundle exec`, `cargo +<toolchain>`, `--manifest-path`, `env ...` and `cd <dir> &&` prefixes.
Package-script indirection: `npm test`, `npm run test`, `bun run test`, `pnpm test`,
`yarn test` resolve through `package.json` `scripts.test` in the command's cwd and are recognised
when that script is.

`select_command` (D14) returns `None` for adapters that cannot select by file or name
(`gradle`, `maven`, `sbt`, `dotnet-test`, `swift-test`, `mix-test`, `ctest`, `phpunit`, `pest`);
the rest build one command covering all targets (`cargo test -- <names...>`,
`go test ./d1/ ./d2/`, `pytest f1 f2 -q`, JS runners with file lists, `ruby -Itest` per file joined
by `&&`, `rspec f1 f2`, `dart test f1 f2`, `flutter test f1 f2`).

## D6. Language profiles

`loom/src/testrun/languages.rs`: one `LanguageProfile` per language — `name`,
`test_file_globs`, `test_declaration` regex, `assertion` regex. Languages: `rust`, `go`,
`python`, `javascript` (covers TypeScript), `java`, `kotlin`, `scala`, `csharp`, `ruby`,
`php`, `swift`, `elixir`, `cpp`, `dart`. The regexes count declarations and assertions line by
line; each profile has unit tests with a positive and a negative sample. `languages::for_path`
maps a file to its profile by extension and glob. `languages::skill_for(profile)` returns the
language skill: `go` ⇒ `loom-golang`, `javascript` ⇒ `loom-typescript`, every other profile ⇒
`loom-<profile>`. Each adapter's `language()` is a profile name (`gradle` and `maven` are `java`,
`sbt` is `scala`).

## D7. Detection and `loom project detect`

- `skills/project/markers.rs` gains kinds: `java` (`pom.xml`, `build.gradle`,
  `settings.gradle`), `kotlin` (`build.gradle.kts`, `settings.gradle.kts`), `scala` (`build.sbt`),
  `csharp` (any `*.csproj` or `*.sln` in the directory), `ruby` (`Gemfile`), `php`
  (`composer.json`), `swift` (`Package.swift`), `elixir` (`mix.exs`), `cpp` (`CMakeLists.txt`),
  `dart` (`pubspec.yaml`), `javascript` (`package.json` without `tsconfig.json`).
- New `skills/project/runners.rs::detect_runner(package_dir, kinds) -> Option<&'static str>`:

  | Kind | Runner |
  | --- | --- |
  | rust | `cargo-nextest` when `.config/nextest.toml` exists at the package or checkout root, else `cargo-test` |
  | golang | `go-test` |
  | python | `pytest` when `pytest.ini`, `conftest.py`, `[tool.pytest.ini_options]` in `pyproject.toml`, or `pytest` in requirements/pyproject dependencies; else `unittest` |
  | typescript, javascript | `vitest`, then `jest`, then `mocha` by presence in `dependencies`/`devDependencies`; else `bun-test` when `bun.lock`/`bun.lockb` exists or `scripts.test` starts with `bun test`; else `node-test` when `scripts.test` contains `node --test`; else none |
  | java, kotlin | `gradle` when a `build.gradle*` exists, else `maven` |
  | scala | `sbt` |
  | csharp | `dotnet-test` |
  | ruby | `rspec` when `.rspec` exists or `Gemfile` names `rspec`, else `minitest` |
  | php | `pest` when `composer.json` requires `pestphp/pest`, else `phpunit` |
  | swift | `swift-test` |
  | elixir | `mix-test` |
  | cpp | `ctest` |
  | dart | `flutter-test` when `pubspec.yaml` depends on `flutter`, else `dart-test` |

- Skill mapping: `loom-<kind>` for every kind, except `javascript` ⇒ `loom-typescript` and
  `golang` ⇒ `loom-golang` (already). New skills (stage `language-skills`): `loom-java`,
  `loom-kotlin`, `loom-scala`, `loom-csharp`, `loom-ruby`, `loom-php`, `loom-swift`,
  `loom-elixir`, `loom-cpp`, `loom-dart`.
- `ProjectProfile::package_details()` returns, per package, `path`, `kinds`, `runner`
  (`Option`), `skills`.
- `loom project detect [PATH] [--json]` (new top-level `Commands::Project`, subcommand `detect`)
  prints the checkout root, `truncated`, and one line per package:
  `<path>  kinds=<k1,k2>  runner=<adapter|unsupported>  skills=<s1,s2>`. `--json` prints one
  line of compact JSON (`serde_json::to_string`):
  `{"root":...,"truncated":...,"packages":[{"path":...,"kinds":[...],"runner":...,"skills":[...]}]}`,
  with `"runner":null` for an unsupported package.
  In this repository it reports `loom` ⇒ `cargo-test`/`loom-rust` and `web` ⇒ `vitest`.

## D8. Contract phase

- `SessionType::Contract`; `Session::new_contract(stage_id: &str)`; tracking key
  `loom-contract-<stage_id>`; model and effort resolve exactly as for `SessionType::Stage`.
  It joins `session_registry::SESSION_KINDS` (it is the stage's agent while it runs), the
  brokered kinds in `session_settings/contents.rs`, and gets a full row set in
  `relay/matrix.rs`: the same verdict as `Stage` for every request kind except `Dispute`,
  `Verdict` and completion, which it may not send, plus the new `FreezeContracts` kind, which
  only `Contract` may send.
- `start_stage`: for a v2 `standard` stage with contracts and no `freeze.json`, the session it
  writes ahead, assigns and spawns is a `Contract` session with the contract signal. Otherwise it
  spawns the `Stage` session as today. The spawn tail of `start_stage` moves into
  `orchestrator/core/stage_spawn.rs` so the contract-exit handler reuses it. No new
  `StageStatus`; no new transition edge: the stage stays `Executing` across both sessions.
- Contract signal (`orchestrator/signals/contract.rs`): the contracts table (id, file, test,
  adapter, scenario, rejects), `harness`, the stage description as context, the knowledge brief,
  the language skills, and the rules: write only contract and harness files; implement nothing;
  every contract test must fail now (a compile or collection failure counts); finish with
  `loom stage contracts freeze <stage-id>`; record memory as any stage does.
- `loom stage contracts freeze <stage-id>` (runs in the sandbox):
  1. changed paths versus the stage base (committed diff, tracked changes, untracked files,
     worktree scaffold excluded) must each be a contract `file` or match a `harness` glob;
  2. every contract `file` exists;
  3. each contract runs through its adapter's `single_test_command` with the criteria executor
     (confined, `working_dir`, 300 s): `Failed` or `BuildFailed` is required; `Passed` is an
     error ("passes before implementation, so it cannot tell right from wrong"); `NotSelected` is
     an error ("the runner did not select the test"); unsupported or `Unparsed` requires a
     non-zero exit and prints a warning;
  4. sends `Request::FreezeContracts { auth_token, stage_id, session_id, reports }` over the same
     socket → relay → spool channels `loom stage dispute-criteria` uses.
- Daemon handler: the caller must be the stage's current `Contract` session; it re-checks
  step 1 itself with read-only git, hashes every contract and harness-matched file, copies them
  to `.loom/work/contracts/<stage>/files/<relative path>`, and writes
  `.loom/work/contracts/<stage>/freeze.json`:

  ```json
  {
    "version": 1,
    "stage_id": "s",
    "session_id": "…",
    "frozen_at": "2026-09-24T12:00:00Z",
    "base": "<commit sha>",
    "files": [{ "path": "src/foo_contract_tests.rs", "sha256": "…" }],
    "contracts": [{ "id": "no-symlink-follow", "adapter": "cargo-test", "outcome": "failed", "exit_code": 101 }]
  }
  ```

- End of the contract phase. A Claude session does not exit when its work is done, so the
  trigger is the freeze record, not a process exit. Each tick, a stage that is `Executing`, whose
  session is a `Contract` session, and that has a `freeze.json` raises
  `MonitorEvent::ContractPhaseFinished`. The handler takes the contract agent down (kill and
  confirm death through `stage_takedown`'s take-down path), and defers to the next tick if it
  survives. It then spawns the `Stage` session through `stage_spawn`. A contract session whose
  process vanished without a freeze raises `ContractSessionEnded`: respawn the contract session,
  budget 3 per stage in `.loom/work/contracts/<stage>/attempts` (spent when handed out);
  exhausted ⇒ `NeedsHumanReview` with the reason. The contract signal tells the agent to stop
  after a successful freeze; loom ends the session.
- Contract sessions export `LOOM_SESSION_TYPE=contract`. The Stop hook `commit-guard.sh`
  gives them a contract-specific reminder ("finish with `loom stage contracts freeze <stage>`;
  do not commit; do not complete the stage") instead of the commit-and-complete advice.
- `loom stage contracts show <stage-id>` prints the freeze record and the frozen file paths;
  `loom stage contracts restore <stage-id> [--contract <id>]` copies frozen content back into the
  worktree. `sandbox/settings.rs::STATE_READ_DIRS` gains `contracts`.

## D9. Contract check at completion

`verify/contracts/completion.rs`, called from `commands/stage/complete_verification.rs::run`
for v2 `standard` stages with contracts: `freeze.json` exists; every frozen file's current
sha256 equals the frozen one; each contract runs through its adapter via the criteria runner
(`CommandSpec` plus fingerprint, so an identical earlier run is reused from the certified cache)
and classifies `Passed`. `NotSelected` fails ("contract test not selected"). Unsupported or
`Unparsed`: exit 0 passes with a warning. Every failure names the way out: restore
(`loom stage contracts restore`) or dispute (`loom stage dispute-contract`, D15).

## D10. Zero-test guard

For v2 stages only: after a criterion runs, `testrun::registry::recognize(command, cwd)`; when
an adapter recognises it and its parse gives `executed == Some(0)`, the criterion fails with
`selected zero tests (<adapter>)`. `verify/criteria/cache_contract.rs::CACHE_RECORD_VERSION`
becomes 3 and the certified verdict records `tests_executed`.

## D11. Wiring v2

- `verify_wiring` takes `plan_version`. v1 behaviour is unchanged. v2:
  - `source` containing any of `*`, `?`, `[` is a glob relative to `working_dir` (`glob` crate);
    no match ⇒ gap `no file matches source glob`; the check passes when any matched file
    matches the pattern;
  - `literal: true` ⇒ `regex::escape(pattern)`;
  - definition-site exclusion: a match on the line where a source-graph node whose name occurs
    in the matched text is defined does not count. When every match is excluded ⇒ gap
    `pattern matches only the definition of <name> in <file>; point it at a consumer`. Files
    without an extractor language are not excluded (a note says so).
- `reachable` (v2): `verify/goal_backward/reachable.rs`. Build the worktree graph once per
  verification; resolve `symbol` and `from` by exact name; `impact_with(graph, symbol_id, …)`
  with kinds `calls,references,implements,extends,contains,imports`, no depth limit,
  `limit 0`, `min_confidence` from the check; pass when a hit is the `from` node. Missing node ⇒
  gap `symbol not found`; no hit ⇒ gap `<symbol> is not reachable from <from>`
  (`GapType::Unreachable`). Symbols in a language without an extractor ⇒ warning, skipped.
- Worktree graph (`context/worktree_graph.rs`, `build_for_worktree(working_dir)`): it
  discovers everything with read-only git. The worktree is `--show-toplevel`; the project root
  is the parent of the absolute `--git-common-dir`; the base revision is the newest published
  base layer that is an ancestor of HEAD. It loads that base layer read-only, re-extracts every
  file changed versus it (plus untracked, minus worktree scaffold) with
  `extract::extract_file`, drops deleted files, and runs `resolve_graph`. No cache or overlay
  writes. No usable base layer ⇒ it extracts every `git ls-files` source file under
  `working_dir` and reports the degraded mode. Callers need no extra context, so no call site
  changes.
- IV re-verifies every completed v2 stage's `reachable` checks on the merged tree (stage
  `test-guards`).

## D12. Recorded review

- `agents/loom-code-reviewer.md` Output Format keeps its human sections and requires one final
  fenced block with info string `loom-review`:

  ```json
  {
    "findings": [
      { "severity": "critical|major|minor", "file": "src/a.rs", "line": 42,
        "claim": "…", "scenario": "input or state → wrong outcome", "rule": "cited rule or null" }
    ],
    "suggestions": [{ "file": "src/a.rs", "line": 10, "text": "…" }],
    "resolved": ["F-1-2"],
    "unresolved": ["F-1-3"]
  }
  ```

- A finding needs `file`, `line ≥ 1`, `claim`, and a non-empty `scenario` or `rule`; anything else
  in `findings` is recorded as a suggestion. Missing or unknown `severity` is recorded as
  `unspecified`. Every finding blocks completion regardless of severity.
- Harvest: `loom-hooks/subagent-stop.sh`, when `agent_type` is `loom-code-reviewer`, pipes
  `{stage_id, session_id, agent_id, transcript_path}` to the hidden delegate
  `loom hook review-harvest`. For a v2 stage it reads the worker's final assistant text
  (`commands/subagents/classify/entry.rs`), parses the last `loom-review` block, computes the
  change fingerprint, and writes `.loom/work/reviews/<stage>/round-<n>.json`; each suggestion
  becomes a `suggestion` memory entry in the stage journal. A missing or unparseable block
  records the round with `malformed: <reason>`. v1 stages: the delegate exits 0 without writing.

  ```json
  {
    "version": 1, "round": 2, "agent_id": "…", "harvested_at": "…",
    "fingerprint": "sha256:…", "files": { "src/a.rs": "sha256-hex", "src/b.rs": "deleted" },
    "malformed": null,
    "findings": [{ "id": "F-2-1", "severity": "major", "file": "src/a.rs", "line": 42,
                   "claim": "…", "scenario": "…", "rule": null }],
    "resolved": ["F-1-2"], "unresolved": ["F-1-3"],
    "suggestion_memory_ids": ["…"]
  }
  ```

- Change fingerprint: `sha256:` + hex of sha256 over `base:<base sha>\n` followed by one
  `<path>\t<sha256 hex | deleted>\n` line per path, sorted, for every path changed versus the
  base (`git diff --name-only <base>` plus `git ls-files --others --exclude-standard`), worktree
  scaffold excluded. Commits do not change it. Base: `git merge-base HEAD <base branch>` with the
  base branch `complete_verification` already passes to `run_unwired_check`.
- `loom stage review status <stage-id>` prints the rounds, every open finding (own and carried)
  with id, severity, `file:line` and claim, whether the latest round matches the current
  fingerprint, and the files whose hash changed since the latest round. The main agent passes
  that output to the next reviewer; a re-review covers those files plus the open findings.
- Gate (`verify/review/gate.rs`, v2 `standard` and `integration-verify`): the latest well-formed
  round's fingerprint equals the current one; every finding and every carried finding is closed,
  either listed in `resolved` by a later round or ruled `dismiss`/`defer` in
  `reviews/<stage>/rulings.json`. `uphold` does not close. Formats the gate reads:

  ```json
  { "version": 1, "rulings": [{ "finding": "F-1-2", "ruling": "dismiss", "target_stage": null, "dispute": 3 }] }
  ```

  ```json
  { "version": 1, "carried": [{ "id": "origin-stage/F-1-2", "origin_stage": "origin-stage",
    "finding": { "severity": "major", "file": "…", "line": 1, "claim": "…", "scenario": "…", "rule": null },
    "dispute": 3 }] }
  ```

- `sandbox/settings.rs::STATE_READ_DIRS` gains `reviews`.
- Memory: `MemoryEntryType::Suggestion` (`suggestion`, pending until it has a receipt, its own
  `--group` bucket `suggestions`, its own section in signal and handoff exports);
  `ReceiptOutcome::Implemented` (`implemented`, requires `--reason`).

## D13. Test integrity

`verify/integrity/` (v2 `standard` and `integration-verify`), computed at completion against
the same base as D12, for every language with a profile (D6); other languages are skipped with a
note:

| Event id | Raised when |
| --- | --- |
| `TI-decl-<lang>` | total test declarations in that language's test files (base files read with `git show <base>:<path>`) fell |
| `TI-assert-<lang>` | total assertions in those files fell |
| `TI-edit-<path>` | a test file that existed at base has assertion lines removed or changed whose exact text does not reappear among that file's added lines (moved lines do not count) |
| `TI-ratchet-<path>` | a `ratchet_files` entry's content differs from base |

`loom stage review integrity <stage-id>` prints the current events. The gate passes when every
current event is accepted in `reviews/<stage>/integrity.json` and is no worse than accepted
(count at least the accepted current count; file sha256 equal to the accepted one):

```json
{ "version": 1, "accepted": [
  { "event": "TI-assert-rust", "kind": "assert_total", "language": "rust", "base": 120, "accepted_current": 118, "dispute": 5 },
  { "event": "TI-edit-src/foo_tests.rs", "kind": "assertion_edit", "path": "src/foo_tests.rs", "accepted_sha256": "…", "dispute": 6 },
  { "event": "TI-ratchet-loom/maintainability-baseline.txt", "kind": "ratchet", "path": "loom/maintainability-baseline.txt", "accepted_sha256": "…", "dispute": 7 }
] }
```

## D14. Impact-selected tests

`verify/impact_tests.rs`, v2 `standard` stages, after the contract check: build the worktree
graph (D11), take every node in the files changed versus base, and collect the test nodes that
reach them through `impact_with` (kinds as D11, no depth limit). A test node is a node in a file
matching its language's `test_file_globs`. Group them by the adapter detected for their package
(D7), exclude the stage's own contracts, and run each adapter's `select_command` through the
criteria runner (cache reuse as D9, 300 s). A failing selected run fails completion and names
the tests. `select_command == None`, a timeout, or no test reached ⇒ a note, no failure: the full
suite still runs in integration-verify.

## D15. Disputes

- `DisputeRequest` gains `kind`:

  ```rust
  #[serde(tag = "kind", rename_all = "kebab-case")]
  pub enum DisputeKind {
      Criterion { criterion_index: usize },
      Findings { finding_ids: Vec<String>, evidence: Vec<FindingSnapshot> },
      Contract { contract_id: String },
      Integrity { event_ids: Vec<String>, evidence: Vec<IntegritySnapshot> },
  }
  ```

  `criterion_index` moves into `Criterion`; `loom stage dispute-criteria` is unchanged for
  users.
- CLI (new, one request each, several ids per request so one retire and respawn covers a whole
  review round): `loom stage dispute-findings <stage> --finding <id>... --reason <text>`,
  `loom stage dispute-contract <stage> --contract <id> --reason <text>`,
  `loom stage dispute-integrity <stage> --event <id>... --reason <text>`. Transport is the
  dispute-criteria transport, extracted into a shared helper.
- Daemon: `Request::FileDispute { auth_token, stage_id, session_id, kind, reason, evidence_commit }`.
  It checks every id exists (open finding or open carried finding; frozen contract; current
  integrity event), spends the per-kind budget (`Stage.finding_disputes`,
  `Stage.contract_disputes`, `Stage.integrity_disputes`, 3 each; exhausted ⇒
  `NeedsHumanReview`), writes `request.md`, and moves the stage to `NeedsAdjudication` as today.
- Judge prompts per kind (`orchestrator/adjudication/prompt/{findings,contract,integrity}.rs`):
  - findings: each finding (claim, scenario/rule, the file ±20 lines around `line`, its review
    round), the agent's reason, the stage diff for those files;
  - contract: the spec (`scenario`, `rejects`), the frozen content, the current content, their
    diff, the reason;
  - integrity: each event, the removed or changed lines (or the ratchet diff), the reason.

  Every prompt keeps the existing instructions, the 100,000-byte cap and the JSON contract for its
  kind.
- Verdicts: findings disputes use `{"verdict":"rulings","rulings":[{"finding":…,"ruling":"uphold|dismiss|defer","target_stage":…,"reasoning":…,"citations":[…]}]}`
  or `needs-more-evidence`; contract and integrity disputes use `accept`/`reject`/`needs-more-evidence`.
  `defer` is valid only when the disputing stage is not `integration-verify` and `target_stage`
  transitively depends on it and is not `Completed`; otherwise the verdict is coerced to
  `needs-more-evidence` naming why. **Integration-verify never defers.**
- Apply (new module beside `orchestrator/adjudication/apply.rs`):
  - rulings ⇒ append to `reviews/<stage>/rulings.json`; `defer` ⇒ append to
    `reviews/<target>/carried.json`; feedback lists upheld findings; requeue;
  - contract `accept` ⇒ re-freeze that contract's files at their current worktree content
    (hash, copy, rewrite `freeze.json`), apply an optional `plan_patch` to `contracts`
    (`AmendmentField::Contracts`, and `AmendField::Contracts` for `loom stage amend`); requeue;
  - contract `reject` ⇒ feedback telling the agent to run `loom stage contracts restore`; requeue;
  - integrity `accept` ⇒ append to `reviews/<stage>/integrity.json` with the current counts or
    hashes; `reject` ⇒ feedback; requeue;
  - criterion disputes behave exactly as today.

  Requeue goes through `requeue_or_hold_for_remaining_disputes`, so sibling disputes still hold
  the stage until every one has a verdict. Retiring the disputing agent is unchanged.

## D16. Signals and doctrine

- Stable prefixes are unchanged. v2 stages get a dynamic section
  (`orchestrator/signals/v2_section.rs`, appended in `generate.rs`):
  - standard: frozen contracts (ids, files; never edit them; `contracts show`/`restore`);
  - standard and IV: the review gate, the review order, `loom stage review status`, dispute
    commands, carried findings with ids, test-integrity (`loom stage review integrity`);
  - IV: every pending `suggestion` memory entry of the plan's stages with its id, and the rule:
    implement, defer or ignore each; resolve implemented ones with
    `loom memory resolve <id> --outcome implemented --reason <what changed>`; leave the rest;
  - knowledge-distill: record every unresolved suggestion in knowledge (`concerns` or the
    relevant topic), then resolve it `promoted`/`merged`/`discarded`.
- Review order (BLOCK-E). `review-harvest-gate` renders this exact text in the v2 review
  section; `doctrine-v2` copies it byte for byte into `skills/loom-orchestration/SKILL.md` and pins
  both copies:

  ```text
  **Review order (plan v2):** fix every finding, run the full gate, then run the final review round, then complete. Edit nothing after the final review round: any edit, formatting included, changes the change fingerprint and needs another round. A commit does not change it.
  ```

## D17. Token discipline

- Contract, impact and zero-test runs reuse the certified criteria cache (D9, D14).
- Re-reviews cover files changed since the previous round plus open findings (D12).
- Findings are disputed in one batch per review round (D15).
- The full suite runs once, in integration-verify. Standard stages run module-filtered tests.
