# Ci Gate Rollout

> CI gate rollout slips: untested strict gates, shared index, known-red runs

## Strict CI Gates Reached Main Untested, and the macOS Job Grew to an Hour (2026-11-21)

**What happened:** about 80 commits from 2026-11-19 to 2026-11-21 reached CI in one push. They had made the main test step strict (`LOOM_TEST_REQUIRE_SANDBOX_FREE=1`), added strict confined toolchain probes and a macOS host-job lane that reruns the lib suite confined. No CI run had validated any of them. The first run failed on both OSes: the toolchain seeds were exported after the strict step, zsh was missing on Linux, kickoff-draft fixtures lacked a field made required, a new pass-cache test was not `#[serial]`, a hook-test helper raced on a broken pipe, and the confined bun, Go, Maven and Gradle probes could not reach toolchains installed under the runner's home. `Test (macos)` took 56 minutes, 19 of them compiling.

**Why:** strictness turns every skip into a failure, so each missing tool, unseeded cache or step-order slip fails, and nothing ran these gates before main. The macOS lib test build needs about 10 GB for one rustc while the runner has 7 GB, so it swaps; the earlier "twice as slow as Linux" reading came from one older run and was wrong by a factor of two to three.

**Prevention:** a commit that makes a CI gate strict or adds a CI lane is pushed alone and watched green before the next one builds on it. A step that exports variables through `GITHUB_ENV` comes before every step that reads them. Compare CI durations from at least two runs of the current workflow, and measure memory before blaming cores or the linker.

**Fix:** seeding and the strict toolchain probes, the macOS suites and the host-job lane run in `.github/workflows/nightly.yml`; `.github/workflows/ci.yml` runs Linux lib and integration jobs with `CARGO_PROFILE_DEV_DEBUG=0` and cancels superseded runs. The confinement gap for toolchains under `$HOME` is open (see [CI macOS Test Build Time](../concerns/worktree-build-cold-start.md) for the memory finding).

## A Commit Took Another Session's Staged Files (2026-11-21)

**What happened:** `git commit` of one test file in the main checkout committed 18 files: another session had staged its own changes, and the pre-commit hook re-adds every staged file.

**Why:** the index is shared by every session working in one checkout; staging only your own path does not exclude what is already staged.

**Prevention:** before committing in a shared checkout, run `git diff --cached --name-only`; when other paths are staged, commit from a temporary index (`GIT_INDEX_FILE=<tmp> git read-tree HEAD`, `git add <own paths>`, `git write-tree`, `git commit-tree`, `git update-ref`), then `git reset -q HEAD -- <own paths>` so the shared index matches the new HEAD for those paths.

**Fix:** the commit was undone with `git reset --soft HEAD~1` and rebuilt from a temporary index; the other session's staging stayed untouched.

## A CI Run Was Started on Gates Known to Fail (2026-11-21)

**What happened:** with two failure classes still open (confined toolchain probes under `$HOME`, hook policy tests under the host-job lane), `.github/workflows/nightly.yml` was dispatched anyway, committing about an hour of macOS runner time to failures already known.

**Why:** the goal of one full green run was treated as a reason to start the run, ahead of fixing what was known to fail.

**Prevention:** start a CI run only when every failure it can show is unknown. Reproduce and fix known failures locally first; the macOS host-job lane and the confined probes both run on a Mac (`cargo test --lib -- --ignored --exact process::sandbox_probe::host_job_lane::the_lib_tests_pass_under_host_job_confinement`).

**Fix:** the dispatch was cancelled; the known failures are fixed locally before the next nightly dispatch.

## A Tool's Behaviour Was Taken From Memory, Costing a CI Round (2026-11-21)

**What happened:** to keep confined Gradle from forking a daemon (which needs loopback TCP that Seatbelt denies), `GRADLE_OPTS` was set to `-Xmx1g -Dorg.gradle.jvmargs=-Xmx1g` on a recollection of Gradle's launcher. A 30-minute nightly round showed it still forked: in Gradle 8.10 setting `org.gradle.jvmargs` makes `canUseCurrentProcess` demand exact equality of immutable JVM args, which the client cannot meet.

**Why:** the fix and its explanation came from memory; no source, doc or run confirmed them before the push.

**Prevention:** verify a third-party tool's behaviour before relying on it: read its source at the exact version (`gh api repos/<owner>/<repo>/git/trees/<tag>?recursive=1`, then `contents/<path>?ref=<tag>` with the raw media type) or run it. State in the code comment which source was read.

**Fix:** `GRADLE_OPTS=-Xmx1g` only (`stage_cache/environment.rs::GRADLE_IN_PROCESS_OPTS`), derived from `BuildActionsFactory::canUseCurrentProcess` and `CurrentProcess::isLowMemoryProcess` at v8.10.2. That made the build run in-process, but the next CI failure was then misreported as "still forked": a "single-use Daemon" line from another part of the log was taken for the probe's, while the probe's own stack ran through `RunInProcess`. The real blocker was Gradle's UDP lock socket (`FileLockCommunicator.java:48`) and, for workers, loopback TCP; the loopback grant that followed was removed again and confined Gradle is unsupported on macOS ([Seatbelt loopback grant](native-macos-delivery.md#a-seatbelt-localhost-listener-was-re-added-as-if-it-held-binds-to-loopback)). Read the failing test's own output, not a grep across the whole job log.

## Forwarding the Host's GOPROXY and MAVEN_OPTS Into Sessions Leaked Credentials

**What happened:** the plan's design table forwarded the host's `GOPROXY`, `GOFLAGS` and `MAVEN_OPTS` to every consumer. Integration-verify review found two defects: the seed and override variables never reached any consumer (the daemon allowlist stripped them), and once forwarded, a `GOPROXY` with URL userinfo and a `MAVEN_OPTS` holding proxy passwords would ride into every session wrapper. The code's own comment claimed "a credential never rides along".

**Why:** the design copied the host's proxy settings as a unit without asking which of them can hold a secret.

**Prevention:** forward cache LOCATION variables freely and settings only after checking them for credentials. `HOST_ENV_NAMES` reads `GOPROXY` and `GOFLAGS` (a `GOPROXY` with userinfo in any entry is dropped whole) and never `MAVEN_OPTS`. Two frozen contracts asserted the old behaviour, so three test-integrity events stayed on purpose (the session contract's socket path and two host-`MAVEN_OPTS` assertions). Open remainder: [the daemon environment](../concerns/state-confinement-gaps.md#open-reviewer-suggestions-from-the-stage-cache-plan).

**Fix:** [Stage Build Cache](../architecture/stage-build-cache.md#host-seeds-come-from-looms-own-environment).

## One sccache Socket in the Shared Root Let a Host Job Reach the Session's Server

**What happened:** the first design put every consumer's socket in the stage cache root (`s.sock`, `w.sock`, `h.sock`) and relied on Seatbelt granting `h.sock` alone. Review found that bubblewrap cannot filter path-based Unix sockets and binds the whole root for Linux host jobs and workers, so a host job could connect to the session's server, which runs compilers outside the host job's confinement. macOS provisioning's blanket outbound allow reached it too.

**Why:** the Seatbelt rule was read as the boundary, and the Linux half was never compared with it.

**Prevention:** when a rule is expressed per platform, check what the other platform's mechanism can filter. Session and worker sockets now live in a consumer-private directory (`StageCache::with_socket_dir`) and `environment` omits `SCCACHE_SERVER_UDS` without one.

## Seeds Read From a Confined Command's Environment Were Silently Empty

**What happened:** host-job seeds resolved from the confined command's filtered environment, which lacks `GOPATH`, `GOMODCACHE`, `CCACHE_DIR` and `GRADLE_USER_HOME`, so every seed fell back to the HOME default and the environment branches never ran.

**Prevention:** read seeds from `stage_cache::host_env_from_process()` for every consumer. A test that sets a variable and asserts the seed uses it is the only thing that catches the fallback.

## Running an Editing Subagent Beside Reviewers Records No Review Round

**What happened:** an editing subagent (a functional test writer) ran in parallel with three `loom-code-reviewer` spawns; all three stops were skipped with `review_harvest_failed.worktree_changed`, so no round was recorded.

**Why:** the review hook requires the worktree fingerprint to be unchanged from spawn to stop.

**Prevention:** spawn reviewers only while no other agent writes the tree; run editing work in a separate wave. The same rule holds for the gate: each `loom check` call compares the worktree digest (`host_verify/wait.rs::state_matches`), so a rerun while a subagent edits relays a new request and the daemon cancels the previous host job (three runs were lost). Run the gate once every editing subagent has returned.

## Smaller Findings From the Stage Cache Plan

- **A signal's scaffold section can disagree with the freeze.** The contract signal for `stage-cache-sessions` said "No scaffold edits: every contract file is already collected", yet `stage_cache/mod.rs` lacked `mod session_contract_tests;`; the freeze pre-check named it. Trust the freeze pre-check over the signal and add the declaration (`#[cfg(test)] mod ...;`). The contract writer of the first stage likewise needed `pub mod stage_cache;` in `lib.rs` and a `mod.rs` holding only the contract test module.
- **A goal-backward `reachable` check cannot follow a path-qualified call.** `host::wrap(...)` leaves the name `wrap` ambiguous; a `use` import plus a bare call gives the graph its edge (`apply_stage_cache` reachable from `run_job_bound` needed this).
- **rustfmt expands long calls one argument per line.** With `fn_call_width` 60, `assert_eq!` and `var(...)` calls whose arguments exceed 60 characters grow by several lines each; count them before budgeting a test file under 400 lines (`stage_cache/tests.rs`).
- **An unquoted YAML `run:` value ending in `::` is a mapping error** (`.github/workflows/ci.yml`, `cargo test ... stage_cache::`). Quote it.
- **A long `TMPDIR` turns the sccache socket off.** Under `loom check` and inside a temp-dir `HOME`, `<root>/h.sock` or the scratch socket exceeds SUN_LEN, `SCCACHE_SERVER_UDS` is omitted and sccache falls back to TCP, which Seatbelt denies; the session-wrapper contract then fails. Tests that need the socket resolve the cache under `tempdir_in("/tmp")` and assert the length first.
- **Nested Seatbelt fails every confined test inside a sandboxed agent session.** `sandbox-exec: sandbox_apply: Operation not permitted` (exit 71) fails the confined contract tests, the wrapper-run tests (also bash process substitution under `/dev/fd`) and the `host_verify` runner tests there, and the completion host job of a stage can hit it too. Classify by that text, run the stage's non-confined tests for evidence, and dispute the criteria that fail only because of where they run ([Seatbelt denials](sandbox-tooling-and-network.md#seatbelt-denials-read-as-build-warnings-and-test-failures-that-are-environment-not-code-2026-10-15)). `boundary/linux.rs` cannot be compiled on macOS (no `x86_64-unknown-linux-gnu` std), so Linux-only edits need the operator's Linux probe or CI.
