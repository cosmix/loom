# Loom verification continuation — 2026-10-16

**The five required Rust gates pass on the repaired final sources.** All 47 test targets completed: 8,509 passed, zero failed, seven ignored. Doctests separately passed three executed and three compile-only examples, with seven existing ignores. Formatting, Clippy and warnings-denied rustdoc passed. The earlier native-startup stalls below remain historical evidence; the later host audit and bounded runs are recorded in [the macOS test-host report](macos-test-host-2026-10-16.md).

This developer verification record continues the [bug sweep and repair handoff](astra-bug-sweep-2026-10-15.md). That report preserves the original 35 findings, repair inventory, historical check scope, and execution chronology. This continuation does not replace those historical results with the latest partial counts.

## Source and execution scope

- Initial source: `079ce9b5`, including repair commits `f94aa14a`, `483c2e93`, and `bc5adc8b`. The initial attempts made no source changes. The later host-diagnosis work on `fix/macos-test-host-load` isolated fixtures, reduced load, and corrected a stale-socket fixture before fresh final-source verification.
- Final source commits: `19441ba4`, `bff898bd`, `9c6067b5`, and `0b635718`; all used normal hooks. The host report describes their scope.
- Required Cargo commands run from `/Users/dkaponis/src/loom/loom`.
- Host: Apple Silicon, `aarch64-apple-darwin`; Homebrew Rust `1.99.0 (b940084d7 2026-09-28)` and Cargo `1.99.0 (5f94df478 2026-08-27)`.
- `tmux`, Bun, and `cargo-audit` were present in the recorded environment; `cargo-deny` was absent. No Loom stage environment variables were present.
- Initial logs under `/private/tmp/loom-verification-20261016` are no longer present after the host interruption; their recorded chronology is retained below. Current complete logs, actual exits, process cleanup, Cargo metadata, and 47-target accounting are retained under `loom/target/test-runs/`, with exact directories in the macOS report.
- The unrelated untracked `doc/RETRIEVAL-AB-METHOD.md` remains untouched. Security controls, signatures, and services were unchanged.

## Required gate ledger

| Required command from `loom/` | Final evidence | Status |
| --- | --- | --- |
| `cargo test --all-targets --no-fail-fast -- --test-threads=4` | Isolated invocation with `LOOM_E2E_REQUIRE_TMUX=1`, Cargo runner override and `--color never`: 47 targets, 8,509 passed, 0 failed, 7 ignored; exit 0; `/usr/bin/time -l` 252.62 s. | Passed |
| `cargo test --doc --workspace --all-features -- --test-threads=1` | Isolated rustdoc invocation, also `--no-fail-fast --color never`: 13 registered, 3 executed + 3 compile-only passed, 7 ignored; exit 0; 3.96 s. | Passed |
| `cargo fmt --all -- --check` | Exit 0, 1.78 s. | Passed |
| `cargo clippy --all-targets -- -D warnings` | Exit 0, 11.71 s. | Passed |
| `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps` | Exit 0, 4.84 s; environment override recorded in result JSON. | Passed |

The bounded launcher provides these Cargo scopes from the crate directory;
its exact argv, `/usr/bin/time -l` output and exits are retained. Both test runs
had no cleanup signals, sampling errors, or survivors, and removed their roots.
The first host-diagnosis run completed all targets but failed the stale-socket
fixture and obsolete maintainability ledger entry (8,507 passed, 2 failed,
7 ignored). Those failures prompted corrections and the fresh passing run above.

## Initial static checks and startup recovery

Before the host-diagnosis source changes, formatting passed in 2.04 s, Clippy
in 10.15 s, and warnings-denied rustdoc in 5.08 s. Those passes do not stand in
for the final checks above.

Those initial static checks demonstrated some native execution recovery after the first stalls and justified fresh test attempts. `retirement-check.json` confirms all recorded earlier test, doctest, formatter, and diagnostic PIDs are gone, including the three `ps` diagnostics.

## First all-target attempt

Started at `2026-10-16T13:45:36.545156Z`, with a 1,200-second overall watchdog and 300-second idle watchdog. The command entered the library harness and printed 7,809 passing outcomes, zero failed outcomes, and one existing ignored outcome. No library summary was emitted. Two tests remained without final outcomes:

- `verify::contracts::format_gate::tests::a_formatted_file_gives_no_problem`
- `verify::contracts::format_gate::tests::an_unformatted_file_gives_one_problem_naming_the_check`

Both printed the libtest notice that they had run for over 60 seconds. A sample of `cargo-fmt` PID `68716`, parented by the library test process, contained 1,750 samples at `_dyld_start+0`. Only that formatter was sampled. This establishes where that sampled process was waiting; it does not establish a common root cause for every stalled process or a product test failure.

The watchdog itself stalled while launching `ps`; a later Homebrew Python `ctypes` diagnostic also stalled. Process recovery used `/usr/bin/python3` and native `libproc`. Test and formatter parents and the supervising sessions were retired. Three `ps` diagnostics required KILL; their tool session eventually reported exit 143. The all-target supervising session also reported exit 143, while the actual Cargo exit code was not captured. `all-targets.result.json` records zero completed harnesses, two pending library tests, 46 unstarted targets, and no remaining tracked processes. This was external termination, not a successful Cargo result.

## First doctest attempt

Started at `2026-10-16T13:55:05.223895Z`, with a 300-second overall watchdog and 120-second idle watchdog. Output reached `Doc-tests loom`; no doctest harness, summary, or per-test outcome followed. Executed, compile-only, and ignored doctest counts are all unknown, rather than zero.

The idle watchdog did not complete cleanup. The wrapper was subsequently retired manually and reaped with exit 143; the actual Cargo exit code remains unavailable. Parent `93644`, Cargo `93645`, and child `93664` were confirmed gone. This attempt provides neither a doctest pass nor a doctest assertion failure.

## Recovered all-target attempt

Started at `2026-10-16T14:00:56.549132Z`, using the same exact Cargo command with `LOOM_E2E_REQUIRE_TMUX=1`, a 1,200-second overall watchdog, and a 180-second idle watchdog. The library completed in 129.76 seconds: **7,811 passed, 0 failed, 1 ignored, 0 filtered**. Both formatter tests that lacked first-attempt outcomes passed.

Cargo next launched the main test binary, PID `60457`, but its harness never entered. Its sample contained 1,775 samples at `_dyld_start+0`. No main test outcomes were emitted, and none of the 45 integration targets launched. The watchdog retired the process tree after 180 seconds without output: supervising exit **124**, actual Cargo termination **SIGTERM (`-15`)**, elapsed **313.03 seconds**. `all-targets-recovered.result.json` records complete cleanup and no survivors. `target-accounting.json` records the state of all 47 targets.

The successful library result establishes full library execution on the unchanged source. The later startup stall still prevents all-target certification. Sampling establishes the main process's observed loader state, without proving the host root cause.

## Recovered doctest attempt

Started at `2026-10-16T14:06:48.456218Z`, after the successful warnings-denied documentation build, with the exact required doctest command. The harness registered **13 tests** and reported these outcomes:

| Outcome | Doctest |
| --- | --- |
| Compile-only passed | `fs::session_files::find_session_for_stage`, `src/fs/session_files.rs:163` |
| Compile-only passed | `fs::session_files::find_sessions_for_stage`, `src/fs/session_files.rs:213` |
| Ignored | `git::cleanup`, `src/git/cleanup/mod.rs:17` |
| Ignored | `git::worktree::paths::find_repo_root_from_cwd`, `src/git/worktree/paths.rs:22` |
| Ignored | `git::worktree::paths::find_worktree_root_from_cwd`, `src/git/worktree/paths.rs:102` |

The log next announced `hooks::validators::bash::validate_bash_command`, `src/hooks/validators/bash.rs:115`, without a final outcome. Eight registered doctests lack outcomes, including that test; no runtime pass or assertion failure was observed. There is no complete doctest summary.

The 120-second idle watchdog terminated the attempt after **131.01 seconds**: supervising exit **124**, actual Cargo termination **SIGTERM (`-15`)**. Cleanup retired Cargo `66460`, rustdoc `66475`, and `rust_out` `66592`; `doctests-recovered.result.json` records complete cleanup with no survivors. The partial compile-only and ignored outcomes do not certify the remaining runtime doctests.

## Cargo target accounting

The captured Cargo metadata lists 48 target entries. `build-script-build` is a `custom-build` entry, leaving **47 test targets: one library, one binary, and 45 integration targets**. It is not a 48-test-target suite. Doctests are a separate required command.

The recovered pre-diagnosis attempt completed only the library; main and all 45 integrations remained unverified at that point. The final bounded invocation completed every target below, including all eight maintainability checks. Cargo metadata and raw harness summaries were matched by source path with no missing or duplicate target.

| Kind | Target | Passed | Failed | Ignored |
| --- | --- | ---: | ---: | ---: |
| lib | `loom` | 7814 | 0 | 2 |
| bin | `loom` | 5 | 0 | 0 |
| test | `adjudication_e2e` | 13 | 0 | 0 |
| test | `binding_resolver_contracts` | 5 | 0 | 0 |
| test | `codex_evidence` | 21 | 0 | 0 |
| test | `completion_flow` | 3 | 0 | 0 |
| test | `completion_gates_contracts` | 7 | 0 | 0 |
| test | `completion_replay` | 22 | 0 | 0 |
| test | `daemon_owned_commits_contracts` | 6 | 0 | 0 |
| test | `e2e` | 137 | 0 | 5 |
| test | `edge_quality_contracts` | 4 | 0 | 0 |
| test | `failure_resume` | 16 | 0 | 0 |
| test | `graph_contract_contracts` | 6 | 0 | 0 |
| test | `integration` | 249 | 0 | 0 |
| test | `language_packs_contracts` | 5 | 0 | 0 |
| test | `maintainability` | 8 | 0 | 0 |
| test | `map_api_contracts` | 8 | 0 | 0 |
| test | `map_cli` | 9 | 0 | 0 |
| test | `merge_off_checkout` | 3 | 0 | 0 |
| test | `merge_off_checkout_cli` | 2 | 0 | 0 |
| test | `phantom_merge` | 7 | 0 | 0 |
| test | `plan_amendment_integration` | 4 | 0 | 0 |
| test | `plan_environment_contracts` | 7 | 0 | 0 |
| test | `platform_portability_contracts` | 5 | 0 | 0 |
| test | `resolved_view_contracts` | 3 | 0 | 0 |
| test | `retrieval_delivery_contracts` | 6 | 0 | 0 |
| test | `session_auth_and_stalls_contracts` | 4 | 0 | 0 |
| test | `signing_env` | 1 | 0 | 0 |
| test | `stage_exits_contracts` | 2 | 0 | 0 |
| test | `stage_transitions` | 3 | 0 | 0 |
| test | `subagent_owned_wait` | 19 | 0 | 0 |
| test | `target_attestation_scope_tests` | 3 | 0 | 0 |
| test | `target_cli_contracts` | 3 | 0 | 0 |
| test | `target_cli_tests` | 4 | 0 | 0 |
| test | `target_guard_contracts` | 13 | 0 | 0 |
| test | `target_ref_hook_contracts` | 3 | 0 | 0 |
| test | `target_ref_hook_refusal_tests` | 5 | 0 | 0 |
| test | `target_ref_hook_tests` | 10 | 0 | 0 |
| test | `token_flow_bounds_contracts` | 3 | 0 | 0 |
| test | `token_flow_contracts` | 18 | 0 | 0 |
| test | `token_insights_contracts` | 8 | 0 | 0 |
| test | `token_optimization_comparison` | 4 | 0 | 0 |
| test | `token_optimization_contracts` | 8 | 0 | 0 |
| test | `token_optimization_knowledge` | 4 | 0 | 0 |
| test | `web_host` | 1 | 0 | 0 |
| test | `worker_evidence` | 10 | 0 | 0 |
| test | `worktree_remove_safety` | 8 | 0 | 0 |

## Existing ignored tests

The final all-target run observed all six existing ignores below, plus the user-authorized 3,000-file census stress case. The new default census test retains the real >2 MiB Git input boundary using 32 physical files. Existing ignored tests were not enabled or modified. The initial recovered doctest attempt observed only three ignores; the completed final doctest run observed all seven listed after this table.

| Test | Source declaration and reason | Current observation |
| --- | --- | --- |
| `orchestrator::core::tests::test_running_session_count` | `loom/src/orchestrator/core/mod.rs:299`; requires a terminal emulator | `ignored` |
| `test_orchestrator_with_manual_mode` | `loom/tests/e2e/daemon_config/manual_mode.rs:24`; integration test requiring a Git repository | `ignored` |
| `test_orchestrator_respects_max_parallel_sessions` | `loom/tests/e2e/daemon_config/parallel_sessions.rs:54`; integration test, run with `--ignored` | `ignored` |
| `test_orchestrator_creation_with_config` | `loom/tests/e2e/daemon_config/tests.rs:18`; requires a terminal emulator | `ignored` |
| `test_orchestrator_creates_successfully` | `loom/tests/e2e/daemon_config/tests.rs:87`; requires a terminal emulator | `ignored` |
| `test_daemon_respects_auto_merge` | `loom/tests/e2e/daemon_config/tests.rs:153`; integration test, run with `--ignored` | `ignored` |

| Added stress ignore | Source declaration and reason | Final observation |
| --- | --- | --- |
| `context::census::tests::census_handles_thousands_of_long_paths` | `loom/src/context/census/tests.rs:331`; explicit 3,000-file stress fixture | `ignored` |

Final doctest ignores: `git::cleanup`, both `git::worktree::paths` root-finding
examples, `process::is_process_alive`, `skills`, `validation::clap_id_validator`,
and `verify::context::CriteriaContext::expand`. Compile-only passes are both
`fs::session_files` stage lookup examples and `orchestrator::skip::skip_stage`.
Runtime passes are the Bash validator, file-path validator, and ID validator.

## CI and additional coverage

The five requested gates do not cover every CI job. The repository's [CI workflow](../.github/workflows/ci.yml) additionally declares the following checks. The table distinguishes the completed native Rust gates from additional CI coverage.

| Coverage | Repository command or configuration | Continuation status |
| --- | --- | --- |
| Linux/macOS test environments | Both test jobs set `LOOM_E2E_REQUIRE_TMUX=1`; Linux and macOS have separate all-target runs | Final local macOS invocation passed with that variable; Linux unexecuted |
| Repeated timing-sensitive tests | `./scripts/flake-check.sh`; 20 repetitions each of `quota::`, `process::`, `verdict_apply_tests::`, and `stalled_judge_tests::` under load | Not rerun |
| Dashboard smoke | `./scripts/smoke-web-dashboard.sh loom/target/debug/loom` after building the binary | Not rerun |
| Terminal smoke | `./scripts/smoke-web-terminal.sh loom/target/debug/loom` after building the binary | Not rerun |
| Shell syntax | `./scripts/check-hook-syntax.sh` | Not rerun |
| Native hook suite | `bash loom-hooks/tests/run-all.sh`; macOS job requires Bash 3.2 | Not rerun |
| BSD tool compatibility | `LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh` in the Linux job | Not rerun |
| Partial-staging regression | `./scripts/test-pre-commit-partial-staging.sh` | Not rerun |
| Dependency security audit | `rustsec/audit-check`, working directory `loom` | Not rerun |
| Dependency policy | `EmbarkStudios/cargo-deny-action`, `command: check`, manifest `loom/Cargo.toml` | Not rerun; local `cargo-deny` absent |
| Dashboard checks/build | `bun run check`, `bun run build`, working directory `web` | Not rerun; CI job is path-filtered |

Historical results remain: dashboard checks **646 tests across 59 files**, production build and both smoke scripts passed; installer **9 tests**; hooks **102/102**; shell syntax **159/159**; partial-staging **6/6**. These are earlier repair evidence, not new execution in this continuation. The CI workflow also uses path filters for its Rust and shell jobs.

## Evidence boundaries

All five requested native macOS Rust gates are complete. Seven all-target and
seven doctest ignores are explicitly accounted for; they are not passes. The
original failure repairs, resolver Completed assertion, successful Go execution
and intended semantic rejection, real descendant retirement, confinement
controls, executable-byte mutation, and zero-test semantics passed independent
source/evidence review. No repeated flake stress run was performed during host
recovery. Additional platform/component checks retain the scope above.

The host's WindowServer/TCC root cause remains unproven. The audit found no
Input Monitoring API caller, removed incidental AppleScript calls from unit
tests, and reduced unnecessary fixture work. Resource metrics are sampled lower
bounds, not a controlled before/after causal experiment. The optional indexing
exclusion script was provided and syntax-checked, never executed.
