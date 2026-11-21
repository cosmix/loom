# Ci Gate Rollout

> CI gate rollout slips: untested strict gates, shared index, known-red runs

## Strict CI Gates Reached Main Untested, and the macOS Job Grew to an Hour (2026-11-21)

**What happened:** about 80 commits from 2026-11-19 to 2026-11-21 reached CI in one push. They had made the main test step strict (`LOOM_TEST_REQUIRE_SANDBOX_FREE=1`), added strict confined toolchain probes and a macOS host-job lane that reruns the lib suite confined. No CI run had validated any of them. The first run failed on both OSes: the toolchain seeds were exported after the strict step, zsh was missing on Linux, kickoff-draft fixtures lacked a field made required, a new pass-cache test was not `#[serial]`, a hook-test helper raced on a broken pipe, and the confined bun, Go, Maven and Gradle probes could not reach toolchains installed under the runner's home. `Test (macos)` took 56 minutes, 19 of them compiling.

**Why:** strictness turns every skip into a failure, so each missing tool, unseeded cache or step-order slip fails, and nothing ran these gates before main. The macOS lib test build needs about 10 GB for one rustc while the runner has 7 GB, so it swaps; the earlier "twice as slow as Linux" reading came from one older run and was wrong by a factor of two to three.

**Prevention:** a commit that makes a CI gate strict or adds a CI lane is pushed alone and watched green before the next one builds on it. A step that exports variables through `GITHUB_ENV` comes before every step that reads them. Compare CI durations from at least two runs of the current workflow, and measure memory before blaming cores or the linker.

**Fix:** seeding and the strict toolchain probes, the macOS suites and the host-job lane run in `.github/workflows/nightly.yml`; `.github/workflows/ci.yml` runs Linux lib and integration jobs with `CARGO_PROFILE_DEV_DEBUG=0` and cancels superseded runs. The confinement gap for toolchains under `$HOME` is open (see [CI macOS Test Build Time](../concerns/ci-macos-test-build-time.md) for the memory finding).

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

**Fix:** `GRADLE_OPTS=-Xmx1g` only (`stage_cache/environment.rs::GRADLE_IN_PROCESS_OPTS`), derived from `BuildActionsFactory::canUseCurrentProcess` and `CurrentProcess::isLowMemoryProcess` at v8.10.2. That made the build run in-process, but the next CI failure was then misreported as "still forked": a "single-use Daemon" line from another part of the log was taken for the probe's, while the probe's own stack ran through `RunInProcess`. The real blocker was Gradle's UDP lock socket (`FileLockCommunicator.java:48`) and, for workers, loopback TCP; the loopback grant that followed was removed again and confined Gradle is unsupported on macOS ([Seatbelt loopback grant](seatbelt-loopback-grant.md)). Read the failing test's own output, not a grep across the whole job log.
