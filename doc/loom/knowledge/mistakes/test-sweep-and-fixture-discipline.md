# Test Sweep And Fixture Discipline

> Sweep: startup, cache, fixtures

## A loader stall outlived a memory-only watchdog

**What happened:** The full runtime suite and an isolated commit check stalled in newly launched binaries. A host process sample showed the main test target at _dyld_start with no Rust frames and negligible memory after more than five minutes.

**Why:** A memory-headroom watchdog does not detect elapsed-time stalls; static gate success does not prove a new executable can start on the host.

**Prevention:** Distinguish loader startup from a Rust deadlock, sample a stalled process once, and use elapsed-time bounds as well as owned process-group cleanup.

**Fix:** Preserve logs and terminate/reap the stalled groups. Continue only focused repairs and affected checks; record any unresolved loader boundary separately from behavioral failures and do not bypass commit hooks.

## Use the crate manifest from the repository root

**What happened:** A formatter invocation from the repository root failed because Cargo.toml is under loom/.

**Why:** Repository and crate working directories were confused during the corrective batch.

**Prevention:** Run Cargo inside loom/ or pass --manifest-path loom/Cargo.toml explicitly.

**Fix:** The worker used the explicit manifest; the failed command made no source changes and was not a failed verification result.

## Diagnostic sampling must not bypass child cleanup

**What happened:** A bounded sampling command interrupted cleanup after timing out in a quota startup probe. A later verification supervisor launched `ps` on every monitoring iteration; `ps` itself stalled at startup, preventing the supervisor from applying its elapsed-time bound.

**Why:** A subprocess timeout cannot bound a launch that blocks before returning a child handle. Diagnostic subprocesses became part of the watchdog critical path.

**Prevention:** On hosts with executable-startup stalls, inspect process identities through OS APIs without launching fresh executables. Keep diagnostic failures outside the cleanup path and preserve explicit process ownership.

**Fix:** Retire owned diagnostic and verification processes using independently obtained identities, replace subprocess-based monitoring with native process queries, and preserve incomplete counts. Repository test deadlines and assertions remain unchanged.

## Stop redundant verification after independent host startup evidence

**What happened:** The corrective full-library run stalled in fresh executable fixtures with 300-second command limits after thousands of tests had completed; bounded independent Python probes also could not start trivial executable scripts.

**Why:** Repeating every broad-suite timeout would consume time without distinguishing code behavior from the host startup failure.

**Prevention:** Preserve the incomplete result and independent evidence, retire the run, then check concrete logic failures with explicit elapsed-time bounds. Never count unexecuted or interrupted tests as passing.

**Fix:** Stop and reap the stalled run, retain its exact tally and log, use focused corrective checks plus required static gates, and document unresolved coverage before committing the reviewed batch.

## Sweep follow-up fixture and navigation discipline

**What happened:** Quota fixtures and the Codex evidence guard helper depended on fresh shebang executables during a macOS executable-startup stall. Sampling the copied guard showed only `_dyld_start+0` before its body. A worker used Python replacement despite its apply_patch brief; another generated an incorrectly escaped Rust string; the coordinator tried nonexistent hook paths. Final verification paged 195 lines despite the graph locating the helper at line 194 and used head once where local guidance requested scoped log reads. Supplying Python as the exec shell fell back to sh and rejected Python syntax. A continuation also assumed tool-local commit metadata survived a user turn; reload the saved manifest from disk before staging. A later artifact diagnostic assumed os.listxattr existed on this Python build; inspect platform support and use the native read-only xattr utility when absent. During the recovered-host continuation, rg and the knowledge replacement command also stalled; apply_patch returned no result. Those owned attempts were stopped and their target sections confirmed unchanged before narrow fallback edits. The normal commit hook then stalled in /usr/bin/env before its body (795 loader samples); the aborted commit and hook were confirmed gone. The unchanged hook invoked through installed Bash reached cargo-fmt, which also stalled before code execution (791 loader samples); that attempt was retired. The coordinator inferred that authorization to commit also covered a command-scoped hook override. Automatic approval review rejected that action because the repository requires normal hooks and documented stalls do not authorize bypassing them. No follow-up commit was created. Require explicit operator approval for that override, or wait for the normal hooks to execute; do not route around the rejection.

**Why:** Fixture launch mechanics, layered string escaping and file-path/range assumptions obscured the intended checks.

**Prevention:** Pass interpreter commands through a supported shell rather than using an interpreter as the exec shell. Invoke installed interpreters where executable identity is irrelevant; retain actual executable-byte checks where required. Use apply_patch, inspect generated code, discover paths with rg --files and read graph/rg-located ranges with scoped rg/sed calls. If the patch launcher demonstrably stalls, retire it and use an asserted narrow replacement after confirming no partial edit.

**Fix:** Quota tests share production exchange through an explicit Command seam, preserving timeout/process assertions. Zero-test fixtures use shell builtins. The evidence guard helper explicitly invokes installed Bash with its copied script while preserving stdin, environment, arguments and authorization assertions. Reap superseded runs before exhaustive verification against the finished fixtures. The initial 37-case focused pass, including the original 13 outcomes, alone does not certify the full suite.

## Loaded executable hard links do not guarantee startup

**What happened:** The host confinement canary passed focused checks with a hard-linked test executable but failed again during the full runtime suite. An independent fresh test-binary launch produced no output and reached its 45-second watchdog.

**Why:** Sharing an inode does not eliminate the observed macOS executable-startup failure mode.

**Prevention:** Use an installed interpreter for syscall canaries and run the same operations unconfined as positive controls; retain direct executable mutation in cache tests that specifically prove executable identity.

**Fix:** The canary uses Bash builtins with the existing 15-second deadline, requiring real sibling read/write/network denial and successful unconfined controls.

## Executable-byte cache coverage needs no shebang fixture

**What happened:** The nested-executable cache test passed focused execution but timed out before its first output during the broad run. The standalone subagent-start ledger test passed its parent-identity assertion but missed its worker-command marker; sampling the temporary executable during the three-second call showed every sample at `_dyld_start+0` before its shell body.

**Why:** Fresh shebang executables exercised the observed host startup failure before the cache and argv-recording assertions could be checked.

**Prevention:** Retain real PATH resolution, executable mode, byte mutation and arguments, while using shell interpretation when native startup is not the property under test.

**Fix:** The cache executable omits its shebang and uses shell ENOEXEC fallback plus a source builtin, preserving first/second output, hit/miss and execution count. The standalone worker-command recording fixture uses the same shell fallback while preserving its PATH invocation, argument assertions and original production deadline.

## Host assessment delays can block unrelated executable startup

**What happened:** Final full tests stalled before Rust test startup and before a temporary shell fixture body. Read-only codesign verification and metadata display also blocked in `Security.framework` at `SecAssessmentTicketLookupXPC`, waiting for an XPC reply. The stalled Rust artifacts had no extended attributes.

**Why:** Samples place the observed waits before test code and in host executable assessment; they do not establish the underlying service failure. Execution-policy logs alone do not prove that an unsigned-file diagnostic caused a particular timeout.

**Prevention:** Keep verification wall-clock and memory bounded, reap diagnostics and superseded test groups, and distinguish host startup waits from executed test failures. Do not weaken test deadlines or change system security settings to obtain a pass. Recheck temporary artifacts and process ownership after an external host-state change. Discover the service label from its plist; `com.apple.syspolicyd` is not the launchd label, which is `com.apple.security.syspolicy`.

**Fix:** The 40-minute invocation completed the library (7,811 passed, 1 ignored) and main (5 passed), then timed out before the next harness. After host execution recovered and temporary state cleared, a fresh all-target invocation completed the library (7,809 passed, 2 failed, 1 ignored), then stalled before main and was reaped with exit 124 after 1,801.56 seconds. The formatter failure has a loader sample; the offline-Cargo timeout has no contemporaneous sample and remains unexplained. Independent integration and doctest gates were reaped before harness entry; knowledge commands and commit hooks also stalled. Later read-only diagnostics and direct env, cargo-fmt and loom launches completed promptly while the policy service retained its original PID; no security settings, signatures or services were changed. After integrity corrections, the final library run passed 7,811 tests with 0 failures and 1 existing ignored test, including both preceding failures. Formatting passed, while the remaining all-target harnesses and doctests still did not complete. The operator ended further diagnosis and directed committing the reviewed fixes. Normal commit hooks subsequently completed without overrides, including all eight maintainability checks and warnings-denied rustdoc when triggered. The underlying host cause remains unproven.

## All-target tests exclude documentation examples

**What happened:** The initial final-gate list used `cargo test --all-targets` and warnings-denied rustdoc generation but omitted the crate's Rust documentation examples.

**Why:** All-target selection excludes doctests; generating documentation does not compile or execute those examples as tests.

**Prevention:** Check doctest configuration and source examples when defining a full Rust verification gate. Include the separate documentation test command when examples are enabled.

**Fix:** A separate `cargo test --doc --workspace --all-features -- --test-threads=1` attempt reached `Doc-tests loom`, then rustdoc stalled before harness entry; a process sample showed `_dyld_start+0`. Its 300-second watchdog retired the owned process tree with exit 124 after 301.65 seconds. Executed, compile-only and ignored doctest counts remain unreported; this is not a passing documentation-test gate.

## Bound reads and discover hook paths before navigation

**What happened:** A verification continuation truncated combined long reads, assumed the hook location, and ran metadata outside the crate (exit 101). A knowledge replacement then passed a literal escaped apostrophe and temporary body; the CLI appended an unwanted section because the heading did not match.

**Why:** Output budgets, working directories and shell quoting were assumed; the replacement body was not prepared before mutation.

**Prevention:** Bound reads, discover paths, set each Cargo cwd, and send complete knowledge bodies and exact existing headings through structured subprocess arguments. Inspect replacement confirmation.

**Fix:** Re-read needed content and rerun metadata from `loom/`; remove the erroneous section and replace the actual gate guidance with a complete body. No test gate ran from the wrong directory. A relative Markdown link escaping the knowledge directory also triggered the knowledge checker despite resolving on disk; use the report as a repository-relative code path instead.

A later inventory repeated the nonexistent `.githooks/*` glob and used a pathless `git diff --numstat`. Discover hook files with repository-wide `rg --files`, without naming unverified directories (even a guessed `hooks` search root repeats the mistake), before opening them; use `git diff --stat` for aggregate summaries and explicit paths for further diff reads.

Rust test module names also need graph lookup: `orchestrator::core::tests` does not imply a `core/tests.rs` file. Use `loom map --find-all` for its symbol instead of guessing a search path.

## A Daemon Test Fixture Overran the macOS Socket Path Limit

**What happened:** `0539705c` added `tests/completion_replay/generated.rs`, the first completion-replay test to start a real daemon. The fixture's temp root used the prefix `loom-completion-replay-`; under the macOS runner's 49-byte `$TMPDIR` (`/var/folders/xx/<28 chars>/T/`) the daemon socket `<root>/.loom/work/orchestrator.sock` reached 107 bytes, over the 104-byte `sun_path` limit, and the daemon exited before ready (`src/daemon/socket.rs::socket_path_fits`). Linux passed locally because `/tmp` is short.

**Why:** the prefix was harmless until a test bound a socket under it, and Linux CI and local runs never approach the limit.

**Prevention:** any fixture whose tests start the daemon keeps its temp-root prefix short: macOS leaves under 25 bytes for the root name. A local Linux pass says nothing about socket path length.

**Fix:** the prefix is `lcr-` (`tests/completion_replay/support.rs`), 88 bytes on the same runner.

## Raw Tempdir Fixtures Recurred With the Install-Root Resolver (2026-10-19)

**What happened:** commit e747e9dc made the install-root resolver canonicalize (`canonical_or_lexical`, `assets/install/resolve.rs`). Three tests (`hooks::tests::generator_tests::find_hooks_dir_*`, `settings_tests::codex_forward_allow_entry_names_the_relocated_hooks_dir`) built their expectations from a raw `TempDir::new()` path, so macOS CI compared `/private/var/...` against `/var/...`. Linux passed.

**Why:** the canonical-temp-root rule in the 2026-10-13 entry above was not applied to new fixtures; `assets/install/tests_resolve.rs` already had a `canonical_temp()` helper the new tests did not use.

**Prevention:** any test that compares a path produced by the install-root resolver (or any canonicalizing production code) builds it from `temp.path().canonicalize()`. To reproduce on Linux, point `TMPDIR` at a symlink to a real directory under a SHORT path (for example `/tmp/lt`): a long path breaks the unix-socket length tests, and the sandbox confinement tests fail under a symlinked temp root on Linux regardless, so judge only the tests under investigation.

**Fix:** commit 61a1a8fc canonicalizes the temp home in the three tests.
