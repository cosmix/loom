# verification-evidence / w4-adapters: adapter filters and parses, and the staging cleanup

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/verification.md`, sections CR-47 and CR-24.

This unit runs no `git` command, touches no path under `.loom/`, and runs no tests. It compiles with `cargo check --all-targets` from `loom/`. It cannot see other workers' edits; none of them touch these files.

## Findings covered

- CR-47 (the kept sub-claims): `dotnet test -t` is read as a full run; a VSTest filter value is not escaped; `mvn -q` hides the Surefire aggregate of a passing single test; minitest ignores Bundler; a swift `error:` line before the first suite overrides parsed counts.
- CR-24: the staging cleanup unlinks entries while `readdir` walks the same directory and treats a `readdir` error as the end.

Left out of this unit (no change): Swift Testing (`@Test`) counts, and the gradle skipped-only lower bound.

## Files

Own (write):
- `loom/src/testrun/adapters/dotnet_test.rs`
- `loom/src/testrun/adapters/maven.rs`
- `loom/src/testrun/adapters/minitest.rs`
- `loom/src/testrun/adapters/swift_test.rs`
- `loom/src/verify/contracts/store_generation.rs`
- `loom/src/verify/contracts/store_generation_gc.rs` (delete)
- `loom/src/verify/contracts/store_tests.rs`

Read only: `loom/src/testrun/command.rs` (`marker_choice`, `shell_quote`, `HOSTILE_NAME`, `assert_one_word`), `loom/src/testrun/adapters/rspec.rs` (`runner`, the `marker_choice` pattern to copy), `loom/src/testrun/recognize.rs` (`has_flag`), `loom/src/fs/safe_fs/write/tree.rs` (`safe_remove_tree_in_workdir`), `loom/src/fs/safe_fs.rs` (`safe_open_dirfd`), `loom/src/verify/contracts/store.rs` (`refreeze_set` calling `generation::cleanup`).

Frozen: none for this unit.

## Changes

### 1. dotnet `-t` (`dotnet_test.rs`, `DotnetTest::is_full_run`)

Add `&& !has_flag(&args, "-t")` beside the `--list-tests` check (`-t` is its short form). Update the doc comment to name both.

### 2. VSTest filter escaping (`dotnet_test.rs`, `DotnetTest::single_test_command`)

Add a private `fn vstest_escape(value: &str) -> String` that puts a `\` before each of `\ ( ) & | = ! ~` (backslash first, so it is not doubled twice) and leaves every other character as is. Build the filter as `shell_quote(&format!("FullyQualifiedName={}", vstest_escape(test)))`. Doc comment on `single_test_command`: the filter is single-quoted so `sh` expands nothing, and VSTest operator characters are backslash-escaped; for an xUnit theory `FullyQualifiedName` holds no arguments, so a contract's `test` names the method.

### 3. maven `-q` (`maven.rs`, `Maven::single_test_command`)

Return `format!("mvn test -Dtest={}", shell_quote(test))`. `-q` suppresses the `[INFO] Tests run:` aggregate of a passing run, which `parse` needs.

### 4. minitest under Bundler (`minitest.rs`)

Add `fn ruby(package_dir: &Path) -> &'static str { marker_choice(package_dir, &["Gemfile"], "bundle exec ruby", "ruby") }` (as `rspec.rs` `runner` does; import `marker_choice` from `crate::testrun::command`). `single_test_command` uses `format!("{} -Itest {file} -n {name}", ruby(package_dir))`; `select_command` renames `_package_dir` to `package_dir` and uses `format!("{} -Itest {}", ruby(package_dir), shell_word(file))` for each run. `recognizes` and `is_full_run` do not change.

### 5. swift pre-suite `error:` (`swift_test.rs`, `SwiftTest::parse`)

Set `summary.build_failed = build_failed && summary.executed.is_none();` so parsed counts win, as `dotnet_test.rs` `parse` does. Update the doc comment: an `error:` before any suite starts is a build failure only when no `Executed N tests` total parsed (stdout and stderr are concatenated, not interleaved, so the order across streams proves nothing).

### 6. CR-24 staging cleanup (`store_generation.rs`, `cleanup`; delete `store_generation_gc.rs`)

Replace the body of `pub(super) fn cleanup(dir: &Path) -> Result<()>` with: open `safe_open_dirfd(dir)?` and return `safe_remove_tree_in_workdir(dirfd.as_raw_fd(), Path::new("staging"))` (import it from `crate::fs::safe_fs`). It lists a directory in full before removing anything, never follows a symlink, and treats a missing `staging` as done. Delete `store_generation_gc.rs` and its `#[path = "store_generation_gc.rs"] mod gc;` declaration. `staging/` itself is now removed; `stage` recreates it with `safe_create_dir_all_in_workdir` (line 95-96). Add a comment: at most 4096 entries per cleanup (`safe_remove_tree_in_workdir`'s limit); a staged generation holds one stage's frozen files; revisit if freeze sets pass a few thousand files.

## Interfaces shared with other workers

None.

## Tests to write (the orchestrator runs them)

- `dotnet_test.rs` `mod tests`:
  - in `full_run_requires_no_test_filter_or_assembly`, add `assert!(!ADAPTER.is_full_run(&argv(&["dotnet", "test", "-t"])))`;
  - new `filter_characters_are_escaped`: `single_test_command("UnitTest1.cs", "Ns.C.M(x: 1)", Path::new("."))` contains `M\(x: 1\)`, and for `a=b|c&d!e~f\g` it contains `a\=b\|c\&d\!e\~f\\g`;
  - `hostile_test_name_stays_one_filter_word`: expected word becomes `format!("FullyQualifiedName={}", vstest_escape(HOSTILE_NAME))` (the name holds `(` and `)`).
- `maven.rs` `mod tests`: `named_maven_method_uses_surefire_filter` expects `mvn test -Dtest='AlphaTest#passes'`.
- `minitest.rs` `mod tests`: new `a_bundler_project_runs_through_bundle_exec`: a `tempfile::tempdir()` holding an empty `Gemfile`; `single_test_command("test/model_test.rb", "test_alpha_passes", dir.path())` is `bundle exec ruby -Itest test/model_test.rb -n '/^test_alpha_passes$/'`, and `select_command` over `test/a_test.rb` and `test/b_test.rb` is `bundle exec ruby -Itest test/a_test.rb && bundle exec ruby -Itest test/b_test.rb`. The existing tests pass `Path::new(".")` and keep their expectations.
- `swift_test.rs` `mod tests`: new `parsed_counts_win_over_an_earlier_error_line`: a `RunOutput` whose stdout is `error: a diagnostic printed before the run\nTest Suite 'All tests' started at 2024-01-01 00:00:00.000\nTest Case '-[M.T testA]' failed (0.001 seconds).\nTest Suite 'All tests' failed at 2024-01-01 00:00:00.010.\n\t Executed 1 test, with 1 failure (0 unexpected) in 0.001 (0.002) seconds\n`, exit `Some(1)`; `classify(&ADAPTER.parse(&out), Some(1)) == RunOutcome::Failed`. The fixture count assertion (`names.len() == 5`) does not change: add no fixture file.
- `store_tests.rs`: in `interrupted_generations_preserve_the_committed_snapshot` (line 84) replace the `read_dir(..).count() == 0` assertion with `assert!(!dir.join("staging").exists())`. Add `staging_cleanup_removes_a_large_nested_tree_and_keeps_link_targets`: under `<tmp>/stage/staging`, three nested directories holding 70 files each plus a symlink to a directory outside holding `keep`; `generation::cleanup(&dir)` succeeds, `staging` is gone, `keep` is intact. `staging_cleanup_does_not_follow_symlinks` stays as it is.

## Proof

```bash
cargo check --all-targets
```

The orchestrator then runs `cargo test --lib testrun::adapters` and `cargo test --lib verify::contracts::store`.

## Traps

- Escape the backslash first, or the escapes added for other characters get escaped again.
- `vstest_escape` happens before `shell_quote`; the shell quoting stays the outer layer.
- Do not change gradle or add Swift Testing parsing in this unit.
- `minitest` `select_command` keeps returning `None` for no targets.
- Remove every import the deleted `store_generation_gc.rs` left unused.
