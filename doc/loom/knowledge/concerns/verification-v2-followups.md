# Verification V2 Followups

> Nine doc-derived adapters, parser gaps, v2 known gaps

## Nine Adapters Have Fixtures From Documented Formats, Not Captured Runs

`cargo-nextest`, `gradle`, `maven`, `sbt`, `rspec`, `phpunit`, `pest`, `swift-test` and `mix-test` have fixtures under
`loom/src/testrun/fixtures/<adapter>/` written from the runner's documented output format, because the runner was not
installed on the recording host. Each directory carries a `PROVENANCE.md`; the other 14 adapters were captured from real
runs. Their parsers are unproven against real output until someone records a live run and replaces the fixture.

Open parser and command gaps found while writing the language skills, checked against the tree on 2026-09-25:

- **swift-test** reads only XCTest's `Executed N tests`. Swift Testing (`@Test`) prints `Test run with N tests`, so a
  filter selecting only Swift Testing tests parses `executed 0` and reads as `NotSelected` at freeze and completion.
- **maven** runs `mvn -q test -Dtest=...`; quiet mode suppresses Surefire's `Tests run:` line on a passing run, so a
  passing contract likely parses `Unparsed` (exit 0 passes with a warning) instead of `Passed`.
- **pest** `--filter <name>`: the mechanism by which a multi-word description reaches PHPUnit's name filter was not
  found in Pest's source. Run one through the real runner when Pest is available.
- **ctest** runs `cmake --build build && ctest --test-dir build ...` relative to the package dir. `scan.rs` makes every
  directory with a `CMakeLists.txt` a package, so a contract under a `tests/CMakeLists.txt` runs where no `build/`
  exists and completion can never pass. Nothing configures `build/`. Fix by walking up to the configured CMake root.
- **minitest** runs `ruby -Itest <file>` without `bundle exec`, unlike rspec with a `Gemfile`.
- **JVM**: `mvn` is taken from `PATH` even when `./mvnw` exists; every Gradle subproject with its own `build.gradle*`
  is a separate package and falls back to `gradle` on `PATH`; Android (`test` aggregate) and Kotlin Multiplatform
  (`jvmTest`) modules have no plain `test` task for `--tests`.
- **csharp** detection needs a `*.csproj` or `*.sln`; a directory holding only `.slnx` is not detected (project
  directories beneath it still are).
- **dart** `--plain-name` is a substring match, so a description contained in another test's name selects both.
- The test-file globs for swift, dart, csharp, cpp, java, kotlin, scala, ruby, php and elixir were not pinned by the
  design; `testrun/languages.rs` is the authority and the language skills describe conventions only.

## Verification v2 Known Gaps

- **Contract outcomes are self-attested.** The daemon re-derives changed paths from git but does not re-run agent-written
  contract tests (untrusted code outside the sandbox). A raw-socket client could report `failed` for an
  already-passing contract; completion re-runs every frozen contract and requires a pass. Security review accepted it.
- **Tracking-key collision.** `loom-contract-<id>` (Contract session of stage `<id>`) equals the Stage key of a stage named
  `contract-<id>`; `orphan_candidates.rs:21-34` matches pid-file stems by prefix, so after a daemon restart the scanner
  can adopt one stage's process into the other. Same class as the `merge-`, `knowledge-` and `adjudication-` prefixes;
  `validation.rs` `RESERVED_NAMES` reserves none of them.
- **Escalated contract stage.** `loom stage retry` refuses `NeedsHumanReview`; the stage reaches retry only through
  `human-review --reject` (`Blocked`), and `human-review --approve` requeues without resetting the contract budget.
- **Continuation writer.** After a ceiling handoff the replacement contract writer gets a fresh contract signal without
  its predecessor's handoff (`generate_contract_signal` takes no handoff file).
- **Stale wiring pin from another stage.** Integration-verify has no self-service channel for it: `dispute-criteria` takes an
  acceptance index and the daemon admits a dispute only for the caller's own stage (`daemon/server/self_service.rs:86-94`).
  The repair is an operator `loom stage amend --field wiring`. Consider a dispute kind for aggregated wiring gaps, or
  have the aggregated check name the amend command.
- **Target-branch resolution differs.** `commands/hook/review_harvest.rs:119` resolves from the stage worktree, completion
  and both `loom stage review` commands from the CWD repo root. They agree while the CWD is inside the repo; share
  `commands/stage/review_status.rs::stage_worktree_and_target`.
- **Journal splitting.** `fs/memory/parser.rs:30` treats any line starting `###` as an entry header and `validate_content`
  checks length only, so a note or suggestion whose text starts with `###` splits the journal. Review harvest flattens
  suggestions to one line but does not escape a leading `###`.
- **Impact selection misses macro-only use** (`context/extract/rust.rs:19`) and skips every test in a contract file.
- **Reachable by bare name** passes when any symbol of that name is reached from any `from` node; common names (`run`,
  `handle`) can false-pass. `Partial` coverage counts as checked in definition-site exclusion.
- **Integrity blind spot.** A file marked skip-worktree or assume-unchanged hides its edits from the `git diff` the
  fingerprint and the count totals trust. `verify::integrity::current_events` has no production caller (kept for the
  pinned dispute-kinds API).
- **Duplicated helpers** to unify: `verify/review/store.rs::canonical_work_dir` and the private one in
  `verify/contracts/store.rs`; `daemon/server/contracts.rs::locate` and `adjudication/apply_contract.rs::stage_working_dir`
  (~20 lines); the journal walk in `signals/v2_section_review.rs` and `commands/memory/handlers/pending.rs::staged_entries`;
  `CwdGuard` copied into four test files; `verify/utils.rs` `.size_limit(1 << 20)` versus `wiring.rs` `PATTERN_SIZE_LIMIT`.
- **Slow hook tests** (`loom-hooks/tests/run-all.sh` is 162 s after the PATH-fixture fix): `prefer-modern-tools-missing-rg-fd.sh`
  78 s, `poll-guard-subagent-waits.sh` 63 s, and four others near 40 s.
