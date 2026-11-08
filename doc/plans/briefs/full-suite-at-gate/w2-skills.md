# w2-skills: skill edits for "full suite only at the gate"

Owner of the edits: one `loom-software-engineer` (sonnet), files under `skills/` only, excluding `skills/loom-orchestration/`, `loom-plan-writer`, `loom-wiring-test`, `loom-before-after`, `loom-dead-code-check`.

Confirmed by reading: the four excluded skills use `cargo test ...` only as plan acceptance criteria and `loom plan verify` examples (`loom-wiring-test/SKILL.md:112,158,174,188`, `loom-dead-code-check/SKILL.md:134`, `loom-before-after/SKILL.md:54-185`, `loom-plan-writer/**`). Plan authors, not agent runs. Leave them.

## Standard wording

Define once; every edit below reuses one of these three shapes. The wording names no language and no duration.

- **S (standard sentence):** "In a loom stage, run the targeted form for the code you changed; the whole-suite command is the stage gate's."
- **Table label (P1):** append `, whole suite (the stage gate's)` or `(the stage gate's)` to the task name of a whole-suite row, and put the targeted form in the same row or point to the existing targeted row.
- **Checklist item (P2):** `<targeted command> green for the code you changed (<whole-suite command> is the stage gate's)`.
- **Gate block heading (P3):** keep the block; the heading gains `the stage gate runs the <runner> line; while working, run <targeted form>`.

Do not edit any frontmatter (`triggers:` lists carry `pytest`, `go test`, `ctest` etc. as match keywords; `loom/tests/integration/hooks_skill_trigger_evidence.rs` indexes the real `skills/loom-golang/SKILL.md` and reads those triggers).

## Are skills embedded in the binary?

No, with two exceptions that this brief does not touch. `include_str!` pulls in `skills/core-skills.txt` (`loom/src/skills/index_catalog.rs:26`) and, in test code only, `loom-orchestration/SKILL.md` and `loom-plan-writer/{SKILL.md,references/*}` (`loom/src/orchestrator/signals/tests_doctrine*.rs`, `tests_size.rs`, `tests_commit_timing.rs`). Every file edited below is read from disk at run time, so no rebuild is needed; `install.sh` (run by `dev-install.sh`) copies the skills into `~/.claude/skills` and `~/.claude/loom-skill-catalog`, so run `dev-install.sh` once to see the change in a live session.

Tests that read real skill files: `loom/src/language/tests.rs::test_every_variant_has_a_skill_directory` (file existence only), `loom/tests/integration/hooks_skill_trigger_evidence.rs` (loom-golang frontmatter triggers), `loom/tests/signal_replay/report.rs` (byte sizes recorded, shape check only). None pins body text of the skills edited here. There is no `skills::` test that pins skill text: `cargo test --lib skills::` covers the indexer and project detection, not these edits.

## Line counts

All replacements are one line for one line, so no file grows. `skills/loom-javascript/SKILL.md` is 399 lines and `skills/loom-c/SKILL.md` is 399; keep them single-line. These files already exceed 400 and stay at their current count: loom-python 637, loom-rust 559 (untouched), loom-typescript 849 (untouched), loom-swift 513, loom-golang 497, loom-ruby 459, loom-php 449, loom-dart 467, loom-cpp 429, loom-csharp 415, loom-kotlin 412. The only way to break the limit is to add a line; do not.

## Edits

Each site gives the path, the line, the current text, and the exact replacement. Where the current text is a substring of a longer line, it is marked `(substring)`; replace only that substring. Apply with the Edit tool, `old_string` exactly as quoted.

### Process skills

#### 1. skills/loom-testing/SKILL.md:54 (a)

Current (substring):

```text
a red test proves the test actually exercises the code; a test that never failed asserts nothing.
```

Replacement:

```text
a red test proves the test actually exercises the code; a test that never failed asserts nothing. Run that one test (file or name filter) for each red and green step; the whole suite is the stage gate's.
```

#### 2. skills/loom-refactoring/SKILL.md:47 (a)

Current (substring):

```text
Run them before you touch anything (establish green).
```

Replacement:

```text
Run the tests for the code you will change before you touch anything (establish green).
```

#### 3. skills/loom-refactoring/SKILL.md:48 (a)

Current (substring):

```text
re-run tests after each.
```

Replacement:

```text
re-run the touched module's tests after each.
```

#### 4. skills/loom-refactoring/SKILL.md:82 (a)

Current:

```text
1. Green baseline   → run the full suite; if red, stop (fix or characterize first)
```

Replacement:

```text
1. Green baseline   → run the touched module's tests; if red, stop (fix or characterize first)
```

#### 5. skills/loom-refactoring/SKILL.md:85 (a)

Current:

```text
4. Re-run tests     → green? commit. red? inspect the current small diff and rethink
```

Replacement:

```text
4. Re-run tests     → touched module only: green? commit. red? inspect the current small diff and rethink
```

#### 6. skills/loom-refactoring/SKILL.md:87 (a)

Current:

```text
6. Final gate       → full suite + lint + build green; diff is behavior-neutral
```

Replacement:

```text
6. Final gate       → full suite + lint + build green (in a loom stage the stage gate runs them); diff is behavior-neutral
```

#### 7. skills/loom-refactoring/SKILL.md:156 (a)

Current:

```text
- [ ] Full suite green BEFORE first change (or characterization tests written)
```

Replacement:

```text
- [ ] Touched module's tests green BEFORE first change (or characterization tests written)
```

#### 8. skills/loom-debugging/SKILL.md:97 (a)

Current (substring):

```text
Repro locally: run the full suite (not just the one test), randomize order, and force the CI thread count (`cargo test -- --test-threads=N`, `pytest -p xdist -n N`, `jest --runInBand`).
```

Replacement:

```text
Repro locally: run the failing test's whole file or module (not just the one test), randomize order, and force the CI thread count (`cargo test <module>:: -- --test-threads=N`, `pytest <file> -p xdist -n N`, `jest <file> --runInBand`). In a loom stage the whole-suite run is the stage gate's.
```

#### 9. skills/loom-debugging/SKILL.md:126 (b)

Current:

```text
cargo test -- --test-threads=1     # if this fixes it → shared-state/ordering isolation bug
```

Replacement:

```text
cargo test <module>:: -- --test-threads=1     # if this fixes it → shared-state/ordering isolation bug
```

#### 10. skills/loom-debugging/SKILL.md:128 (b)

Current:

```text
pytest -p no:randomly / -p randomly --randomly-seed=N   # pin/vary order to bisect ordering deps
```

Replacement:

```text
pytest <file> -p no:randomly / -p randomly --randomly-seed=N   # pin/vary order to bisect ordering deps
```

#### 11. skills/loom-debugging/SKILL.md:216 (b)

Current:

```text
cargo test -- --test-threads=1          # serialize to expose shared-state flakes
```

Replacement:

```text
cargo test <module>:: -- --test-threads=1          # serialize to expose shared-state flakes
```

#### 12. skills/loom-git-workflow/SKILL.md:103 (a)

Current:

```text
cargo test                         # ALWAYS re-test: a clean textual merge can be a semantic conflict
```

Replacement:

```text
cargo test <module>::              # ALWAYS re-test the resolved files' module: a clean textual merge can be a semantic conflict
```

#### 13. skills/loom-git-workflow/SKILL.md:108 (a)

Current (substring):

```text
Run tests after every resolution.
```

Replacement:

```text
Run the tests for the resolved files' module after every resolution; the whole suite is the stage gate's.
```

#### 14. skills/loom-code-migration/SKILL.md:196 (a)

Current (substring):

```text
Run formatter + full test suite after; a codemod that compiles can still be semantically wrong.
```

Replacement:

```text
Run the formatter and the tests for every touched module after (the whole suite is the stage gate's); a codemod that compiles can still be semantically wrong.
```

#### 15. skills/loom-test-strategy/SKILL.md:161 (b)

Current:

```text
npm test -- --shuffle           # order dependence?
```

Replacement:

```text
npm test -- <test-file> --shuffle           # order dependence?
```

#### 16. skills/loom-test-strategy/SKILL.md:162 (b)

Current:

```text
npm test -- --maxWorkers=4      # parallel race?
```

Replacement:

```text
npm test -- <test-file> --maxWorkers=4      # parallel race?
```

### Language skills

#### 17. skills/loom-javascript/SKILL.md:52 (b), keep one line (file is 399 lines)

Current:

```text
| Test, whole suite | `bun test` / `bunx vitest run` / `node --test` |
```

Replacement:

```text
| Test, whole suite (the stage gate's) | `bun test` / `bunx vitest run` / `node --test`; the changed code: `bun test f.test.js -t 'name'` / `bunx vitest run f.test.js -t 'name'` / `node --test f.test.js` |
```

#### 18. skills/loom-python/SKILL.md:50 (b)

Current:

```text
| Run in project env | `uv run pytest` (auto-syncs first; no manual activate) |
```

Replacement:

```text
| Run in project env | `uv run pytest tests/test_x.py -k name` (auto-syncs first; no manual activate; bare `uv run pytest` is the stage gate's) |
```

#### 19. skills/loom-python/SKILL.md:625 (a)

Current:

```text
- [ ] `pytest` green; new behavior has tests (parametrized edge cases, `pytest.raises(match=)`)
```

Replacement:

```text
- [ ] `pytest <changed test files>` green (bare `pytest` is the stage gate's); new behavior has tests (parametrized edge cases, `pytest.raises(match=)`)
```

#### 20. skills/loom-golang/SKILL.md:478 (a)

Current:

```text
- [ ] `go build ./...` and `go test ./...` green
```

Replacement:

```text
- [ ] `go build ./...` clean and `go test ./<changed pkg>/...` green (`go test ./...` is the stage gate's)
```

#### 21. skills/loom-golang/SKILL.md:479 (a)

Current:

```text
- [ ] `go test -race ./...` on any code touching goroutines/shared state
```

Replacement:

```text
- [ ] `go test -race ./<changed pkg>/...` on any code touching goroutines/shared state
```

#### 22. skills/loom-golang/SKILL.md:449 (a)

Current (substring):

```text
run `GOEXPERIMENT=loopvar go test ./...` first.
```

Replacement:

```text
run `GOEXPERIMENT=loopvar go test ./<changed pkg>/...` first; the stage gate runs the whole module.
```

#### 23. skills/loom-java/SKILL.md:46 (b)

Current:

```text
| Unit tests | `./gradlew test` | `./mvnw -q test` |
```

Replacement:

```text
| Unit tests, whole suite (the stage gate's) | `./gradlew test` | `./mvnw -q test` |
```

#### 24. skills/loom-java/SKILL.md:379 (a)

Current:

```text
- [ ] `./gradlew check` or `./mvnw -q verify` passes, with `-Xlint:all -Werror` and the project's static analysis
```

Replacement:

```text
- [ ] `./gradlew check` or `./mvnw -q verify` passes, with `-Xlint:all -Werror` and the project's static analysis (the stage gate runs it; while working, run the touched tests and `testClasses`)
```

#### 25. skills/loom-kotlin/SKILL.md:44 (b)

Current:

```text
| Unit tests | `./gradlew test` |
```

Replacement:

```text
| Unit tests, whole suite (the stage gate's) | `./gradlew test` |
```

#### 26. skills/loom-kotlin/SKILL.md:398 (a)

Current:

```text
- [ ] `./gradlew check` passes with `allWarningsAsErrors`; detekt and ktlint (or ktfmt) clean
```

Replacement:

```text
- [ ] `./gradlew check` passes with `allWarningsAsErrors`; detekt and ktlint (or ktfmt) clean (the stage gate runs `check`; while working, run the touched tests and `detekt`)
```

#### 27. skills/loom-scala/SKILL.md:47 (b)

Current:

```text
| All tests | `sbt test` |
```

Replacement:

```text
| All tests (the stage gate's) | `sbt test` |
```

#### 28. skills/loom-scala/SKILL.md:360 (a)

Current:

```text
- [ ] `sbt scalafmtCheckAll "scalafixAll --check" test` passes, with warnings as errors (sbt-tpolecat or `-Werror`)
```

Replacement:

```text
- [ ] `sbt scalafmtCheckAll "scalafixAll --check" Test/compile` passes, with warnings as errors (sbt-tpolecat or `-Werror`), and `sbt 'testOnly <changed suites>'` is green (the `test` task is the stage gate's)
```

#### 29. skills/loom-csharp/SKILL.md:45 (b)

Current:

```text
| Build / test | `dotnet build`, `dotnet test` (restores and builds first) |
```

Replacement:

```text
| Build / test | `dotnet build`, `dotnet test --filter "FullyQualifiedName~<ChangedClass>"` for the changed code (bare `dotnet test` is the stage gate's; it restores and builds first) |
```

#### 30. skills/loom-csharp/SKILL.md:71 (b)

Current:

```text
The gate (warnings are errors through `Directory.Build.props`):
```

Replacement:

```text
The gate (warnings are errors through `Directory.Build.props`; the stage gate runs the `dotnet test` line, while working run `dotnet test --no-build --filter "FullyQualifiedName~<ChangedClass>"`):
```

#### 31. skills/loom-csharp/SKILL.md:402 (a)

Current (substring):

```text
- [ ] `dotnet test` green; new behaviour has tests;
```

Replacement:

```text
- [ ] `dotnet test --filter "FullyQualifiedName~<ChangedClass>"` green (bare `dotnet test` is the stage gate's); new behaviour has tests;
```

#### 32. skills/loom-cpp/SKILL.md:46 (b)

Current:

```text
| Test | `ctest --test-dir build --output-on-failure -j 8` (`--test-dir` needs CMake 3.20+) |
```

Replacement:

```text
| Test, whole suite (the stage gate's) | `ctest --test-dir build --output-on-failure -j 8` (`--test-dir` needs CMake 3.20+); the changed code: `ctest --test-dir build -R '<name>' --output-on-failure` |
```

#### 33. skills/loom-cpp/SKILL.md:83 (b)

Current:

```text
The gate:
```

(Line 83 only; the file has other lines starting with "The gate"? None. Replace the whole line 83, which is the line directly before the ```bash block holding `clang-format --dry-run --Werror`.)

Replacement:

```text
The gate (the stage gate runs the `ctest` line; while working, add `-R '<name>'` to it):
```

#### 34. skills/loom-cpp/SKILL.md:416 (a)

Current:

```text
- [ ] `ctest --output-on-failure` green, and green again in an ASan+UBSan build with `-fno-sanitize-recover=all`
```

Replacement:

```text
- [ ] `ctest -R '<changed tests>' --output-on-failure` green, and green again in an ASan+UBSan build with `-fno-sanitize-recover=all` (bare `ctest` is the stage gate's)
```

#### 35. skills/loom-c/SKILL.md:37 (b), keep one line (file is 399 lines)

Current:

```text
| Test | `ctest --test-dir build --output-on-failure` |
```

Replacement:

```text
| Test, whole suite (the stage gate's) | `ctest --test-dir build --output-on-failure` |
```

The next row (`One test`) is the targeted form; no new row.

#### 36. skills/loom-c/SKILL.md:351 (a)

Current:

```text
- [ ] Tests pass under ASan and UBSan (`-fno-sanitize-recover=all`), and under `-O2`
```

Replacement:

```text
- [ ] The tests for the changed code pass under ASan and UBSan (`-fno-sanitize-recover=all`), and under `-O2`
```

#### 37. skills/loom-swift/SKILL.md:45 (b)

Current:

```text
| Test | `swift test` (`--parallel` for XCTest; Swift Testing runs tests in parallel by default) |
```

Replacement:

```text
| Test, whole suite (the stage gate's) | `swift test` (`--parallel` for XCTest; Swift Testing runs tests in parallel by default); the changed code: `swift test --filter '<Module.Class>'` |
```

#### 38. skills/loom-swift/SKILL.md:78 (b)

Current:

```text
The canonical gate:
```

Replacement:

```text
The canonical gate (the stage gate runs the `swift test` line; while working, use `swift test --filter '<Module.Class>'`):
```

#### 39. skills/loom-swift/SKILL.md:494 (a)

Current:

```text
- [ ] `swift test` green; new behaviour has tests, parameterized where inputs vary
```

Replacement:

```text
- [ ] `swift test --filter '<Module.Class>'` green (bare `swift test` is the stage gate's); new behaviour has tests, parameterized where inputs vary
```

#### 40. skills/loom-ruby/SKILL.md:45 (b)

Current:

```text
| Run inside the bundle | `bundle exec rspec`, or the binstubs `bin/rails`, `bin/rspec` |
```

Replacement:

```text
| Run inside the bundle | `bundle exec rspec spec/foo_spec.rb` (bare `bundle exec rspec` is the stage gate's), or the binstubs `bin/rails`, `bin/rspec` |
```

#### 41. skills/loom-ruby/SKILL.md:56 (b)

Current:

```text
**The gate:**
```

Replacement (line 57 only; the file has one such line):

```text
**The gate** (the stage gate runs the test line; while working, run `bundle exec rspec spec/foo_spec.rb` or `bin/rails test test/foo_test.rb`):
```

#### 42. skills/loom-ruby/SKILL.md:441 (a)

Current (substring):

```text
- [ ] `bundle exec rspec` / `bin/rails test` green;
```

Replacement:

```text
- [ ] `bundle exec rspec spec/foo_spec.rb` / `bin/rails test test/foo_test.rb` green for the changed code (the bare runs are the stage gate's);
```

#### 43. skills/loom-php/SKILL.md:66 (b)

Current:

```text
**The gate:**
```

Replacement (line 66 only; the file has one such line):

```text
**The gate** (the stage gate runs the `phpunit`/`pest` line; while working, pass the changed test file, e.g. `vendor/bin/phpunit tests/Unit/Spool/WriterTest.php`):
```

#### 44. skills/loom-php/SKILL.md:432 (a)

Current (substring):

```text
- [ ] `vendor/bin/phpunit` / `vendor/bin/pest` green;
```

Replacement:

```text
- [ ] `vendor/bin/phpunit <test file>` / `vendor/bin/pest <test file>` green for the changed code (the bare runs are the stage gate's);
```

#### 45. skills/loom-dart/SKILL.md:50 (b)

Current:

```text
| Test | `dart test` / `flutter test` |
```

Replacement:

```text
| Test, whole suite (the stage gate's) | `dart test` / `flutter test`; the changed code: `dart test test/x_test.dart` / `flutter test test/x_test.dart` |
```

#### 46. skills/loom-dart/SKILL.md:78 (b)

Current:

```text
The canonical gate (swap in `flutter analyze` and `flutter test` for Flutter packages):
```

Replacement:

```text
The canonical gate (swap in `flutter analyze` and `flutter test` for Flutter packages). The stage gate runs the `dart test` line; while working, pass the changed test file:
```

#### 47. skills/loom-dart/SKILL.md:450 (a)

Current:

```text
- [ ] `dart test` / `flutter test` green; new behaviour has tests
```

Replacement:

```text
- [ ] `dart test <file>` / `flutter test <file>` green for the changed code (the bare runs are the stage gate's); new behaviour has tests
```

#### 48. skills/loom-elixir/SKILL.md:52 (b)

Current:

```text
**The gate:**
```

Replacement (line 52 only; the file has one such line):

```text
**The gate** (the stage gate runs the `mix test` line; while working, run `mix test path/to/file_test.exs --warnings-as-errors`):
```

#### 49. skills/loom-elixir/SKILL.md:358 (a)

Current:

```text
- [ ] `mix compile --warnings-as-errors` and `mix test --warnings-as-errors` pass
```

Replacement:

```text
- [ ] `mix compile --warnings-as-errors` and `mix test path/to/file_test.exs --warnings-as-errors` pass for the changed code (bare `mix test` is the stage gate's)
```

## Left unchanged

Reason class in brackets: CI = describes CI, not an agent run; API = library or fixture code; KW = frontmatter match keyword; CONTRACT = loom contract-harness description of one targeted command or runner behaviour; GATE = row or block already named the gate; FP = regex false positive.

- `loom-rust/SKILL.md`: 6 hits (383, 393-399, 412) [CONTRACT, doc note about doctests]; no whole-suite site exists, so no edit. The plan's mention of a rust run-command table does not match the file.
- `loom-typescript/SKILL.md`: 246, 273, 306-318, 336, 349 [CONTRACT or tool prose]; no whole-suite command or checklist line.
- `loom-golang/SKILL.md`: 28 [KW], 276 and 358 [CI / race-detector advice], 298-316 [CONTRACT].
- `loom-java/SKILL.md`: 44 "Full gate" row [GATE], 47 "One test" row, 252, 270-272, 288 [CONTRACT or prose].
- `loom-kotlin/SKILL.md`: 42 "Full gate" row [GATE], 45 "One test" row, 78 (fact about `UP-TO-DATE`), 281, 297-301 [CONTRACT or prose].
- `loom-scala/SKILL.md`: 46 "Full gate" row [GATE], 48-50 targeted rows, 290 [CONTRACT].
- `loom-csharp/SKILL.md`: 293, 305-333 [CONTRACT].
- `loom-cpp/SKILL.md`: 8 [KW], 39, 47 (`ctest -N` lists tests), 54-55, 70, 76, 89 (the gate block body), 201 [CI sanitizer advice], 241-340 [CONTRACT or API], 426.
- `loom-c/SKILL.md`: 38 "One test" row, 66, 301 [CI], 308-399 [CONTRACT].
- `loom-swift/SKILL.md`: 11-12 [KW], 83 (gate block body), 276-290, 344-373 [API / CONTRACT, 346 is the targeted form], 513 lines unchanged count.
- `loom-ruby/SKILL.md`: 15 [KW], 61 (gate block body), 238-355 [API / CONTRACT].
- `loom-php/SKILL.md`: 9 [KW], 56, 72 (gate block body), 228-354 [API / CONTRACT].
- `loom-dart/SKILL.md`: 10-11 [KW], 83 (gate block body), 324-348 [API / CONTRACT].
- `loom-elixir/SKILL.md`: 58 (gate block body), 245-284 [API / CONTRACT; 284 states the full suite runs in integration-verify, still true].
- `loom-python/SKILL.md`: 11 [KW], 48, 158, 170, 245-303 [API / CONTRACT, 283-284 are the targeted forms].
- `loom-javascript/SKILL.md`: 21-25 [KW], 45, 322-381 [API / CONTRACT; 325-327 are the targeted forms the new row 17 points at].
- `loom-testing/SKILL.md`: 26, 32-37 [KW], 104-135 [API fixture code], 234.
- `loom-ci-cd/SKILL.md`: 112, 174, 253, 258, 317, 390 [CI; 317 says run the full suite on the merge/trunk gate].
- `loom-test-strategy/SKILL.md`: 108, 116, 120, 123 [CI config], 126, 142, 160 (targeted `TestName` loop).
- `loom-concurrency/SKILL.md`: 89, 294 [CI race-detector advice].
- `loom-react/SKILL.md`: 368, 389, 395, 1396 [config / package.json scripts].
- `loom-documentation/SKILL.md`: 100-102, 235; `loom-technical-writing/SKILL.md:82` [doc-example advice, not a suite run].
- `loom-prompt-engineering/SKILL.md:143` [example acceptance line, targeted `--test retry`].
- `loom-skills/SKILL.md`: 60-111 [catalog blurbs]; `loom-accessibility/SKILL.md:268-269` [API]; `loom-e2e-testing/SKILL.md:108`, `loom-kubernetes/SKILL.md:385`, `loom-data-visualization/SKILL.md:79` [FP].

Out of scope but conflicting, report to the main agent: `skills/loom-plan-writer/references/parallelization.md:52-53` says "Coordinator A verify (optional, ONE scoped check, skip if unsure)". The operator decision allows any number of targeted runs, so that wording is stale. `loom-plan-writer` was excluded from this brief; whoever owns it should reword it to "the tests for your module".

## Ordered steps

1. Edit sites 1-16 (process skills), then 17-49 (language skills) in file order. Use Edit with the quoted `old_string`; for sites marked "line N only" include the following line (the opening code fence) in `old_string` to make it unique, then restore it in `new_string`.
2. Confirm no frontmatter changed: `git diff --stat -- skills` shows only body lines; `git diff -U0 -- skills | rg '^[+-]  - '` prints nothing.
3. Confirm line counts did not move: `wc -l skills/loom-javascript/SKILL.md skills/loom-c/SKILL.md` prints 399 and 399.
4. Run the acceptance checks below.

## Acceptance

Check A: none of the old whole-suite phrasings remain (expect no output):

```bash
rg -n -F \
  -e '| Test, whole suite |' \
  -e 'run the full suite; if red' \
  -e '→ full suite + lint + build green; diff' \
  -e 'Full suite green BEFORE' \
  -e 'Run them before you touch anything' \
  -e 're-run tests after each' \
  -e 'run the full suite (not just the one test)' \
  -e 'Run formatter + full test suite after' \
  -e 'Run tests after every resolution' \
  -e '`go build ./...` and `go test ./...` green' \
  -e '`go test -race ./...` on any code' \
  -e '| Unit tests | `./gradlew test`' \
  -e '| All tests | `sbt test` |' \
  -e '| Build / test | `dotnet build`, `dotnet test` (restores' \
  -e '| Test | `ctest' \
  -e '| Test | `swift test`' \
  -e '| Test | `dart test`' \
  -e '- [ ] `pytest` green' \
  -e '- [ ] `swift test` green' \
  -e '- [ ] `dart test` / `flutter test` green' \
  -e '- [ ] `vendor/bin/phpunit` / `vendor/bin/pest` green' \
  -e '- [ ] `bundle exec rspec` / `bin/rails test` green' \
  -e '- [ ] `dotnet test` green' \
  -e '- [ ] `ctest --output-on-failure` green' \
  -e '`mix test --warnings-as-errors` pass' \
  -e 'cargo test                         # ALWAYS' \
  -e 'cargo test -- --test-threads=1' \
  -e 'npm test -- --shuffle' \
  -e 'npm test -- --maxWorkers' \
  -e 'GOEXPERIMENT=loopvar go test ./...' \
  skills --glob '*.md' --glob '!skills/loom-orchestration/**' --glob '!skills/loom-plan-writer/**'
```

Check B: every line this brief changed that still names a runner also carries the new wording. Run the sweep over the whole edited set and read the remaining hits against the "Left unchanged" list; every hit outside that list is a missed site:

```bash
rg -n -i 'whole suite|full suite|entire suite|run (all|the) tests|re-?run (the )?tests|until .*(pass|green)|cargo test|go test|pytest|vitest|jest|bun test|npm test|dotnet test|rspec|mix test|phpunit|ctest|swift test|dart test|flutter test|gradle test|sbt test' \
  skills --glob '*.md' --glob '!skills/loom-orchestration/**' --glob '!skills/loom-plan-writer/**' \
  --glob '!skills/loom-wiring-test/**' --glob '!skills/loom-before-after/**' --glob '!skills/loom-dead-code-check/**' \
  | rg -v "stage gate|<module>|<file>|<test-file>|<changed|touched module"
```

The `rg -v` drops the lines this brief rewrote (each carries "stage gate" or a targeted placeholder). What remains must be only the lines listed under "Left unchanged".

Check C: no test pins these files, so the existing suites need no run for correctness. If skills are re-indexed (frontmatter untouched, so the index does not change), the closest pinned target is:

```bash
cargo test --manifest-path loom/Cargo.toml --lib skills::
cargo test --manifest-path loom/Cargo.toml --lib language::tests::test_every_variant_has_a_skill_directory
```

Both must stay green. The first is the only `skills::` target; it does not read skill bodies. The orchestration and plan-writer text-pinned tests (`tests_doctrine*.rs`, `tests_size.rs`, `tests_commit_timing.rs`) belong to the other brief.

Check D: line counts unchanged:

```bash
wc -l skills/loom-javascript/SKILL.md skills/loom-c/SKILL.md
```

Expected: 399 and 399.
