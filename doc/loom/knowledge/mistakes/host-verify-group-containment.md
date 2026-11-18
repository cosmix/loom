# Host Verify Group Containment

>

## Group guardian assumed bubblewrap payload stayed in its host group

**What happened:** A recovery guardian initially used its host process group to retire every criterion descendant without checking the Linux bubblewrap wrapper.

**Why:** The wrapper uses --new-session, so sandbox payload processes leave the host group. The guardian still reaches bubblewrap itself; the wrapper --die-with-parent contract is what retires the detached sandbox process tree.

**Prevention:** Trace every wrapper and session boundary before claiming process-group containment, and run a confined-path lifecycle test beside shell stand-ins.

**Fix:** Keep bubblewrap --new-session and --die-with-parent; test that guardian retirement stops a live sandbox descendant, and treat that test as required acceptance evidence.

## A skipped bubblewrap regression looked like an actual pass

**What happened:** The focused group test printed an outer pass after its inner confinement probe skipped under sandbox restrictions. I initially reported that output as proof of real bubblewrap cleanup.

**Why:** The test uses a child test process for observer isolation, and the outer assertion saw the child exit successfully even when the child skipped. I read the exit status without forcing the probe to run.

**Prevention:** For acceptance that depends on host sandbox facilities, run with LOOM_TEST_REQUIRE_SANDBOX_FREE=1 and a writable TMPDIR; inspect the inner test output, not only the outer status.

**Fix:** The forced run failed inside the managed sandbox as expected, then passed outside it with a live confined heartbeat that stopped after group retirement.

## Hook and Evidence Tests Failed Wherever Loom Was Installed

**What happened:** two hook policy tests (`policy_tests::forward_guard_allows_only_exact_forward_wrapper_command`, `policy_tests::file_guard_rejects_absolute_host_paths_and_credentials`) and 19 of 21 `tests/codex_evidence` tests failed on any host with `loom` installed or a Claude Code Bash `TMPDIR`.

**Why:** the forward guard (`loom-hooks/codex-forward-guard.sh`) asks `${LOOM_BIN:-loom} hook codex-backend`. A fixture `HOME` with no user config makes `configured_backend` default to native, so the guard answered with the native-lane refusal. The codex_evidence fixture strips `LOOM_*` and then asks the `loom` on `PATH`. Separately, a fixture `HOME` under `/tmp/claude-<uid>` falls under `allow_tmpdir` (`worktree-file-guard.sh:210`), which lets Read open owned plain files, so the credentials Read exited 0.

**Prevention:** a test that drives a hook or wrapper pins every setting the hook reads: the fixtures write `[codex] backend = "legacy"` into the work dir's config file (`policy_tests.rs:172`, `codex_evidence/fixture.rs:74`). Fixture roots go through `fixture_base` (`policy_tests_fixture.rs:61`), which swaps a TMPDIR under `/tmp/claude-<uid>` or a `.worktrees/<id>` checkout for the closed-session directory `/tmp/claude-<uid>/loom-fixtures/<closed-session>`, as `loom-hooks/tests/_fixture_base.sh` does. Inside the Claude Code bwrap sandbox the codex_evidence tests still fail (`fake companion did not write exact job file`): nested bubblewrap is refused and `codex-forward.sh:123` takes the direct lane. The `loom check` host job is the judge.

## The Default 300 s Criterion Timeout Cannot Hold `cargo test`

**What happened:** an integration-verify criterion running the whole `loom` `cargo test` hit the default timeout. The library suite alone took 289 s on a warm build (13894 tests) and the integration binaries pushed the run past 300 s.

**Why:** an extended criterion without `timeout_secs` runs under `DEFAULT_EXTENDED_CRITERION_TIMEOUT` (`verify/criteria/config.rs:26`, 300 s).

**Prevention:** a plan whose acceptance runs the full `loom` `cargo test` sets an explicit `timeout_secs` (1200 or more).

## Unsafe Blocks Need a SAFETY Comment

**What happened:** a test fixture wrote `unsafe { libc::getuid() }` bare and failed clippy.

**Why:** `src/lib.rs:2` denies `clippy::undocumented_unsafe_blocks`.

**Prevention:** every `unsafe` block, test code included, carries a `// SAFETY:` line directly above it (`relay/scratch.rs:68`, `policy_tests_fixture.rs:70`).
