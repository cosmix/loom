# Testing And Lint

> Lint and test discipline: --all-targets, --no-fail-fast, headless CI, ambient git config and inherited descriptors in tests, the stub checker, the maintainability ledger, and reviewer claims.

## Test Code: Struct Init Without Default

**Mistake:** Stage struct tests use explicit constructors without `..Default::default()`. Adding new fields breaks ~10 locations.
**Fix:** Use `..Stage::default()` pattern. Also check `tests/` directory (not just `src/`) when adding fields.

## Debug Output in Production

**Mistake:** `eprintln!` with `Debug:` prefix left in production code.
**Fix:** Use `tracing` crate with proper log levels.

## Clippy --all-targets Required to Catch Test-Module Lints (2026-05-12)

**What happened:** `cargo clippy -- -D warnings` (without `--all-targets`) did not compile test modules, so a style lint in `src/hooks/generator.rs` (items after a test module) went undetected during per-stage acceptance and only surfaced at integration-verify.

**Why:** `cargo clippy` without `--all-targets` compiles only the default target (lib + bin). Test code (`#[cfg(test)] mod tests { ... }`) is in a different target and requires `--all-targets` to be included.

**Prevention:** Stage acceptance criteria that include a clippy check should always use:

```bash
cargo clippy --all-targets -- -D warnings
```

Not `cargo clippy -- -D warnings`. The `--workspace` flag is also useful in monorepos.

## Reviewer False Alarm: Verify Behavior Changes Against the Diff (2026-05-12)

**What happened:** An integration-verify reviewer flagged a "HIGH native regression" in `hooks/generator.rs`, claiming the new backend match arm introduced double-firing of global hooks on native worktrees. The claim was false — the native branch was already unconditionally calling `configure_loom_hooks(obj)` before the change; the new commit only added the container arm.

**Why:** The reviewer analyzed the stage description's framing rather than the actual diff. The description said "branching on config.backend" which sounds like it changes native behavior; the diff showed the native arm was structurally identical to the pre-existing unconditional call.

**Prevention:** When a reviewer asserts a behavior change, verify against the actual diff:

```bash
git show <commit>~1 -- <file>  # before
git show <commit> -- <file>    # after
```

Do not trust verbal descriptions of what a commit does — always compare before/after diffs directly.

## TODO in Rust String Literals Triggers ArtifactStub Checker (2026-06-15)

**What happened:** A Rust format string inside a `push_str()` call contained the word "TODO" as a reference to a future task in the documentation text it was generating (not actual stub code). `loom stage complete` rejected it with an ArtifactStub error, blocking completion.

**Misleading signal:** The word appeared in a prompt or documentation string — semantically it was text content, not a code stub. The ArtifactStub checker scans the raw file content without context.

**Prevention:** Before completing a stage, scan your own format strings and string literals with `rg "TODO|FIXME|unimplemented" loom/src/<your-file>`. If the word appears as content in a string (e.g., as part of documentation text), rephrase to avoid the keyword — "fix later", "outstanding item", "remaining task", or similar.

**Fix:** Rephrased the string literal to avoid the TODO keyword.

## Git-Shelling Tests Must Isolate Ambient Config and Assert Setup Steps (2026-06-15)

**What happened:** `git::merge::tests::merge_stage_refuses_when_merge_head_set` passed locally but failed on CI with `MERGE_HEAD must be set` at the _setup_ assertion. The setup used a `run` closure that called `Command::new("git")…output().unwrap()` — `.unwrap()` only catches spawn failure, not a non-zero exit. So when an ambient git config broke a setup commit, every step silently no-op'd and the failure only surfaced lines later as a confusing MERGE_HEAD assertion. Reproduced exactly by setting `commit.gpgsign=true` (no key) in the global config: the seed commit fails → `checkout main` fails → the "merge" runs on the wrong branch → no conflict → no MERGE_HEAD.

**Misleading signal:** "Passes locally, fails in CI" on a pure-logic test that _must_ be deterministic. The panic points at the symptom (`MERGE_HEAD` absent), not the cause (a swallowed setup-commit failure several lines up). Tests that shell out to `git` inherit the runner's global/system config — `commit.gpgsign`, `core.hooksPath`, templates — none of which exist on a clean dev box.

**Why:** Two compounding defects: (1) the helper discarded git exit status, so setup failures were invisible; (2) the repo was not isolated from ambient git config, so a hostile global setting could break commits/merges. Note `#[serial]` only serializes against other `#[serial]` tests — the merge tests are non-serial and run alongside `repository.rs`'s `GIT_CONFIG_GLOBAL`-mutating tests, another reason to pin config per-Command rather than rely on the process environment.

**Prevention:** Any test that shells out to `git` must (a) assert each setup command's exit status and surface stderr — never `output().unwrap()` and drop the status; and (b) neutralize ambient config by setting `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` to nonexistent paths and `GIT_CONFIG_NOSYSTEM=1` on the `Command` (so it survives a polluted process env too). Set identity via local config. ~10 test files here use the same `init_repo`/`run_git` shape (`in_progress.rs`, `merge_attribution.rs`, `recovery.rs`, `merge_verify.rs`, …); the asserting ones at least fail loudly, but none isolate ambient config — port the `isolated_git`/`git_ok` helpers from `git/merge/mod.rs` if they ever flake.

**Fix:** Added `isolated_git`/`git_ok` helpers in `git/merge/mod.rs` tests: every setup step asserts success, the conflicting merge dumps stdout/stderr if MERGE_HEAD is absent, and all invocations run with global/system config disabled. Verified green under a forced-`gpgsign` global config that previously reproduced the failure.

## `cargo test` Stops at the First Failing TARGET — a Green Tail Is Not a Green Suite (2026-08-10)

**What happened:** A local `cargo test` run was reported as "2048 passed, suite green". It was not: the run aborted in `tests/e2e` and never executed `tests/maintainability`, `tests/phantom_merge`, or six other targets. The push then failed on `maintainability`, which had been failing the whole time.

**Why it is easy to miss:** the output ends with a plausible `test result: ok` line from the last target that _did_ run, and the `error: test failed, to rerun pass --test e2e` line scrolls past. Nothing announces the nine targets that were skipped. It compounds when the aborting failure is environmental (a sandboxed run cannot create `TMUX_TMPDIR`), because that failure looks ignorable — and ignoring it silently discards the rest of the suite.

**Prevention:** verify with `cargo test --no-fail-fast` so every target runs, and count the `Running tests/...` lines against the target list rather than reading the tail. The pre-push hook uses plain `cargo test`, so it stops early too — a hook that passes only proves the targets before the first failure. Never report a suite green off a run that aborted.

## Headless CI Has No Terminal Emulator — Pin `LOOM_TERMINAL` in Tests That Build an Orchestrator (2026-08-10)

**What happened:** `merge_handler_attempt_tests::merge_probe_failure_does_not_consume_resolver_attempt_budget` passed on every dev box and failed in CI with `No terminal emulator found. Set TERMINAL environment variable or install one of: kitty, alacritty, ...`.

**Why:** `Orchestrator::new` eagerly constructs a `NativeBackend` (`SessionBackend::from_config`, `orchestrator/terminal/backend.rs:100`) even when the test never spawns a session, and `detect_terminal` probes the host. A GitHub runner has none installed, so construction fails and the `.unwrap()` panics — a pure state-machine assertion killed by the host environment.

**Prevention:** any test that calls `Orchestrator::new` must pin `LOOM_TERMINAL` (checked first in `detect_terminal`, and the only branch that maps a name straight to an emulator without a `which` probe of the host). Set it across construction, restore it afterwards, and mark the test `#[serial]` — `detection.rs`'s tests mutate the same process-global variable. Precedent: `tests/e2e/daemon_config/stale_project_execution.rs:66`.

**Detection rule:** to reproduce headless failures locally, build a `PATH` of symlinks that excludes every terminal binary and run the prebuilt test binaries with `DISPLAY`/`WAYLAND_DISPLAY`/`TERMINAL`/`LOOM_TERMINAL` unset. A fix verified only on a machine that _has_ a terminal proves nothing.

## An Inherited Descriptor Keeps an flock Alive After the Owner Releases It (2026-08-10)

**What happened:** `daemon::server::lock::tests::held_and_free_lock_states_are_distinct` failed roughly one run in nine under full parallel load, and passed every time in isolation. After `drop(guard)`, `inspect_lock` occasionally still reported `Held`.

**Why:** flock ownership belongs to the open file description, and `fork` duplicates it. Any _other_ test in the same binary that spawns a command inherits the lock descriptor for the window between fork and exec, and that inherited copy holds the lock alive even after the owner closes its own descriptor — `O_CLOEXEC` drops it at exec, not at fork. Demonstrated directly: a child that sleeps without exec'ing leaves the probe reading `HELD`; the same child with `execl` leaves it `FREE`.

**Prevention:** a test asserting "released" against a flock cannot assume the next probe observes it. Poll to a deadline instead of probing once, and report the last observed state (`held` vs `indeterminate`) in the failure message so the next failure is diagnosable. More generally, treat single-probe assertions about process-global OS state as flaky-by-construction in a multithreaded test binary that also spawns processes.

**Note for production:** the same window applies to the daemon singleton lock. A child forked during the microseconds `orchestrator.lock` is open can hold it past daemon exit until that child execs, so an immediate restart could briefly see "another daemon instance holds the singleton lock". Not observed in practice; recorded so the symptom is recognisable.

## Growing a Function That Carries a Maintainability Baseline Entry Breaks the Gate (2026-08-10)

**What happened:** A five-line fix inside `generate_index` (`src/fs/knowledge/index.rs`) pushed it from 53 to 58 lines and failed `cargo test --test maintainability`, blocking the push.

**Why:** `maintainability-baseline.txt` records exact line counts for every function already over the 50-line limit. The ledger is a debt list, not a budget — entries may shrink but never grow, and an entry that no longer corresponds to a violation is rejected as stale.

**Prevention:** before adding lines to a function, `rg '<fn name>' loom/maintainability-baseline.txt`. If it is listed, refactor rather than extend — and when the refactor drops it under the limit, DELETE the entry rather than lowering it. Prove behaviour is unchanged by regenerating the artifact and diffing (for INDEX.md: `loom knowledge index` then `git status --porcelain`, expecting no change).
