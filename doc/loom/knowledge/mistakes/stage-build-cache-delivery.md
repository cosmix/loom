# Stage Build Cache Delivery

> Stage cache delivery: credential forwarding, sockets, seeds, gate timing

## Forwarding the Host's GOPROXY and MAVEN_OPTS Into Sessions Leaked Credentials

**What happened:** the plan's design table forwarded the host's `GOPROXY`, `GOFLAGS` and `MAVEN_OPTS` to every consumer. Integration-verify review found two defects: the seed and override variables never reached any consumer (the daemon allowlist stripped them), and once forwarded, a `GOPROXY` with URL userinfo and a `MAVEN_OPTS` holding proxy passwords would ride into every session wrapper. The code's own comment claimed "a credential never rides along".

**Why:** the design copied the host's proxy settings as a unit without asking which of them can hold a secret.

**Prevention:** forward cache LOCATION variables freely and settings only after checking them for credentials. `HOST_ENV_NAMES` reads `GOPROXY` and `GOFLAGS` (a `GOPROXY` with userinfo in any entry is dropped whole) and never `MAVEN_OPTS`. Two frozen contracts asserted the old behaviour, so three test-integrity events stayed on purpose (the session contract's socket path and two host-`MAVEN_OPTS` assertions). Open remainder: [the daemon environment](../concerns/stage-build-cache-gaps.md#open-reviewer-suggestions-from-the-stage-cache-plan).

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
