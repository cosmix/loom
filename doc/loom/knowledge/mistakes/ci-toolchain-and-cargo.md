# Ci Toolchain And Cargo

>

## CI's Clippy Tracks Rustup `stable`, So a New Rust Release Breaks Main With No Code Change (2026-08-26)

**What happened:** three consecutive pushes to main failed CI with only the `Clippy` job red —
build, both test matrices, docs, fmt, maintainability, audit and deny all green, and the same
`cargo clippy --all-targets -- -D warnings` passed locally. The first failure was on 2026-08-21;
the last green run was 2026-08-20. Nothing in those commits touched Rust — they were
`docs(knowledge)` commits. The real trigger was Rust **1.98.0**, released 2026-08-20, whose new
`chunks_exact_to_as_chunks` (style, warn-by-default) fires on `ticks.chunks_exact(2)` in
`src/context/lexical/evidence.rs`. The local toolchain was still 1.97.1, which has no such lint.

**Why:** `.github/workflows/ci.yml` installs `dtolnay/rust-toolchain@stable` and the repo pins no
toolchain file and no `rust-version`. CI therefore silently follows the newest stable, while
a developer machine sits on whatever `rustup update` last fetched. Every six weeks a new stable can
turn previously-clean code into `-D warnings` errors, and the offending commit will be whichever
one happened to push next — usually one that changed nothing relevant.

**Prevention:** when Clippy alone fails and the diff cannot explain it, check the toolchain gap
first — do not read the diff for a cause it does not contain:

```bash
rustc --version                                                   # local
curl -sS https://static.rust-lang.org/dist/channel-rust-stable.toml | rg -m1 '^version = "1\.'
```

If they differ, read the lint list for the intervening release before anything else — the
`## Rust <version>` section of
`https://raw.githubusercontent.com/rust-lang/rust-clippy/master/CHANGELOG.md` names every new lint
and every widened one. Only `style`, `complexity`, `suspicious`, `correctness` and `perf` additions
can break this gate; `pedantic` and `nursery` entries are allow-by-default and irrelevant here.

**Reproducing the newer toolchain without touching `~/.rustup`:** the sandbox denies writes there,
so `rustup update` fails with `Read-only file system`. Redirect all three homes into scratch space
instead — the toolchain download and the crate re-fetch both go over the proxy fine:

```bash
export RUSTUP_HOME=$TMPDIR/rustup CARGO_HOME=$TMPDIR/cargo CARGO_TARGET_DIR=$TMPDIR/target198
~/.cargo/bin/rustup toolchain install 1.98.0 --profile minimal --component clippy --no-self-update
# rustup installs NO cargo/clippy proxies into a redirected CARGO_HOME - call the toolchain's own
# binaries and put its bin dir on PATH so `cargo clippy` finds `cargo-clippy`:
TC=$RUSTUP_HOME/toolchains/1.98.0-x86_64-unknown-linux-gnu
PATH=$TC/bin:$PATH "$TC/bin/cargo" clippy --all-targets -- -D warnings
```

Use a separate `CARGO_TARGET_DIR`: sharing `loom/target/` between two toolchains invalidates every
artifact on each switch.

**The annotations workaround does not help for this failure mode.** The entry above recommends
`gh api .../check-runs/<id>/annotations` when `--log-failed` returns 403. For a Clippy failure the
only annotation is `Process completed with exit code 101` — no lint name, no file. Reproducing
locally against the CI toolchain is the only route to the actual diagnosis.

**Fix applied:** `backtick_spans` now uses `as_chunks::<2>().0.iter()` with a `&[open, close]`
pattern. `as_chunks` is stable since 1.88 and discards a trailing odd element exactly as
`chunks_exact` did, so the unpaired-backtick behaviour is unchanged. There is no `rust-toolchain.toml`
in this repo — pinning one would trade these surprise breakages for silently ageing lint coverage,
and that trade was deliberately rejected.

## `install.sh` Aborts With No Controlling TTY, After All Real Work Already Succeeded (2026-08-30)

**What happened:** `install.sh`'s `cleanup_backups()` does `read -r response </dev/tty`
unconditionally. In a sandbox or CI with no controlling TTY this aborts the WHOLE script (`set
-e`) with a raw `No such device or address` error — even though every real installation step
(skills, agents, hooks, `CLAUDE.md`, commands) had already completed successfully by that point.
Pre-existing; not introduced by any specific stage.

**Prevention:** to test `install.sh` non-interactively in a throwaway `HOME`, override
`HOME=$TMPDIR/...` (`CLAUDE_DIR=$HOME/.claude` is computed at runtime from `HOME`, never
hardcoded) and pipe `y` to stdin for `confirm_overwrites` (`install.sh::confirm_overwrites`, which reads plain
stdin, not the tty) — but the run will still abort at `cleanup_backups()`'s tty read at the very
end unless a tty is attached, so treat a `No such device or address` failure AFTER the install
steps' own success output as a harness artifact, not evidence the install failed.

## CI Clippy Failures That Don't Reproduce Locally = Toolchain Drift (2026-07-22)

**What happened:** CI's Clippy job failed on main while `cargo clippy --all-targets -- -D warnings` passed locally with zero warnings. Local toolchain was 1.95.0; CI installs latest stable via `dtolnay/rust-toolchain@stable`, which had moved to 1.97.1 and shipped new lints (`useless_borrows_in_formatting`, broader `question_mark`) that fired on 21 existing sites.
**Why:** The workflow floats on `@stable` while local toolchains only move on explicit `rustup update`. Every ~6-week Rust release can introduce lints that break CI with `-D warnings` even though no code changed.
**Prevention:** When a CI clippy failure doesn't reproduce locally, check `rustup check` FIRST — if stable has moved, `rustup update stable` and re-run before hunting for any other cause. Most new-lint fallout is machine-applicable: `cargo clippy --fix --all-targets --allow-dirty`, then review the diff (non-trivial rewrites like `question_mark` can leave awkward leftover blocks worth hand-cleaning).
**Fix:** Updated local stable to 1.97.1, applied `cargo clippy --fix`, hand-simplified the `?`-operator rewrite in `fs/work_dir.rs`, verified clippy + fmt + full test suite green.

## `libc::mode_t` Width Differs by Platform — `.into()` Is a Clippy Error on Linux (2026-08-10)

`libc::mode_t` is `u32` on Linux and `u16` on macOS, while the std APIs we call
(`DirBuilderExt::mode`, `PermissionsExt::from_mode`, our own `safe_fs::open_safely`) all take `u32`
unconditionally. So `const MODE: libc::mode_t = 0o700; builder.mode(MODE.into())` compiles on macOS
and fails on Linux with `useless_conversion`, which `-D warnings` promotes to an error — it blocked
`git push` (`loom/src/daemon/server/storage.rs`, the private-mode constants). The same trap applies to any
platform-width alias: `c_int`, `off_t`, `nlink_t`, `time_t`.

**Prevention:** declare permission constants as `u32` (the type every Rust-side API wants) and cast
at the raw-libc boundary only: `libc::fchmod(fd, MODE as libc::mode_t)`. A cast to an alias is exempt
from `unnecessary_cast`, so it is lint-clean on both platforms, whereas `.into()`/`u32::from()` is
lint-clean on exactly one.

**Also:** the pre-push hook runs Clippy, the pre-commit hook does not. A lint-broken commit lands
locally and only surfaces at push time. Run `cargo clippy --all-targets -- -D warnings` before
committing, not after.

## A Test That Assumed a Dev Build Blocked the Release-Tag Push (2026-09-14)

**What happened:** `tests/integration/update_notice.rs::test_dev_build_prints_no_update_notice_and_json_stdout_stays_pure` passed on every commit and in CI on `main`. After `v0.8.0` was tagged on that green HEAD, the pre-push hook of the tag push failed it: with `99.0.0` on record in the test's scratch `update-state.json`, the binary printed ``loom 0.8.0 is out of date (latest 99.0.0) - run `loom update` to upgrade.`` on stderr.

**Why:** `loom/build.rs` derives `LOOM_VERSION` from `git describe --tags --exact-match`. Every untagged commit builds a `-dev` prerelease, which `update_check::decide` exempts from the notice; the commit a tag points at builds the bare release version, which gets the notice. The test assumed the binary under test is always a dev build. The first suite run against a tagged HEAD is the pre-push hook of the tag push itself (then `.github/workflows/release.yml`'s test job, which builds the tag ref), so CI on `main` cannot catch this class.

**Prevention:** a test whose expected output depends on build identity reads it with `semver::Version::parse(loom::version::VERSION)` and asserts what each identity must do (`pre.is_empty()` means a release build). Never assume `-dev`, and never read `CARGO_PKG_VERSION`, which is `0.0.0-dev` in every build. A failure that appears only when a tag is pushed points at this class first.

**Fix:** the test, renamed `test_update_notice_stays_off_json_stdout_and_dev_builds_print_none`, branches on `loom::version::VERSION`: a dev build must print no notice on either stream; a release build must print it on stderr, which also gives the stdout-purity assertion a notice to keep out. The dev-build exemption stays pinned independently of build identity by `update_check::tests::dev_build_is_never_notified_and_never_refreshes`.

## Using npx Instead of bunx

**Mistake:** Used npx instead of bunx during implementation.
**Fix:** Always use `bun`/`bunx` per project conventions. Check CLAUDE.md tool preferences before running package managers.

## toml_edit vs toml: Different Use Cases

**Mistake:** Using `toml_edit Item -> serde` for reading nested config sections. `toml_edit` is designed for round-trip writes; its typed access silently drops nested sub-tables.

**Why:** `toml_edit::Item` doesn't implement full `serde::Deserialize` for complex nested structures the same way `toml::Value` does.

**Prevention:** Use `toml_edit` for writes (round-trip safe). Use `toml` (re-parse the full file with `toml::Value`, then `try_into::<T>()` on the section) for typed reads of nested structures.

## Rustc 1.98 Flags Redundant Glob Imports in Test Modules (2026-09-23)

**What happened:** CI's Build job (`cargo build --all-targets`, `RUSTFLAGS=-Dwarnings`) failed on rustc 1.98.1 with `unused import` on `use super::paths::*;` and `use super::spawn::*;` in `commands/pressure/tests.rs`. Local 1.97.1 built clean. Clippy and test jobs were skipped, so the Build job hid any further fallout.
**Why:** Same toolchain drift as above, this time in rustc's own `unused_imports` lint. The file also had `use super::*`, and the parent's named `use paths::{..}` / `use spawn::{..}` imports already supplied every name the tests used. 1.98 counts a glob that contributes no name as unused.
**Prevention:** In a `tests.rs` child module, use `use super::*` alone, and name a submodule item explicitly only when the parent does not import it. To reproduce CI without moving the default toolchain: `rustup toolchain install <ver> --profile minimal -c clippy -c rustfmt`, then `cargo +<ver> build/clippy/test`. Keep `CARGO_TARGET_DIR` out of `/tmp`. `tmux::tests_spawn::a_failed_spawn_aborts_and_leaves_no_pid_file_for_the_native_retry_to_adopt` rejects a `LOOM_BIN` under `/tmp` as session-writable and fails with a false positive.
**Fix:** Dropped the two redundant globs. Build, clippy, fmt, doc, and the full test suite (5842 tests) all passed on 1.98.1.

## Fixtures That Set Git Identity Only in Env Failed in CI, Passed Locally (2026-10-03)

**What happened:** after the target-guard push, eight tests failed on CI's Linux runner and passed on the workstation and in the pre-push hook: six in `fs::plan_lifecycle::commit` panicked with `Author identity unknown ... empty ident name`, and two in `orchestrator::core::inbox_drain::tests_merge` got `Refused` where they expected `Applied`, because the daemon's merge commit failed the same way.

**Why:** the fixtures (`git::target_guard::test_support::repo` and `tests_merge::repository`) gave git an identity only through `GIT_AUTHOR_*`/`GIT_COMMITTER_*` on their own git calls. The production code under test runs git without that environment and falls back to the global config, which the workstation has and the runner does not. Fixtures that only commit through their own helper are unaffected.

**Prevention:** a fixture whose repository receives commits from production code sets `user.name` and `user.email` in the repository's own config right after `git init`. `TestRepo::builder()` / `RepoBuilder` in `loom/tests/test_support/git_repo.rs` (`crate::test_git`) does this for every shared fixture, and `loom/tests/test_git_guard_contracts.rs` fails a test file that sets `user.email` itself unless it carries a `bespoke-git-fixture` marker. Reproduce CI's missing identity locally with `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.useConfigOnly GIT_CONFIG_VALUE_0=true cargo test --all-targets --no-fail-fast`; `useConfigOnly` stops git guessing a name from the passwd entry, which the workstation would otherwise supply.

**Fix:** both fixtures set the identity with `git config` after `init`.

## CI's Build-Once Job Never Fed the Jobs That Waited for It (2026-10-03)

**What happened:** `.github/workflows/ci.yml` ran a `Build` job (`cargo build --all-targets`) that every Rust job waited on through `needs: build`, meant to warm one `actions/cache` entry of `~/.cargo` plus `loom/target/` for them. The `loom` crate still compiled eight times per run (Build, Test linux, Test macOS, Clippy, Maintainability, Flake, Docs, Smoke), and Build added 2-4 minutes before any of them started. The cache had grown to 3 GB, restored seven times per run at 20-97 s each. Release caches added 4.6 GB per tag and pushed total use to 11.3 GB, over the 10 GB repo quota.

**Why:** four separate defects. (1) The key was the `Cargo.lock` hash alone and `actions/cache` never overwrites an existing key, so `target/` stayed frozen at the last lockfile change and Build's output was thrown away on every other run. (2) Even a fresh save cannot carry the workspace crate: checkout gives every source file a new mtime, so cargo rebuilds `loom` whatever `target/` holds (run 36877678619: the smoke job restored the cache Build had saved minutes earlier, then logged `Compiling loom ... 42.59s`). (3) The restore-keys fallback pulled the previous lockfile's `target/` and the save layered the new one on top, so the cache only grew; the key also left out the rustc version, so a new stable "hit" and recompiled every dependency. (4) Release caches were saved under the tag ref, which no later tag's run can read, and the release build first restored CI's debug cache, useless to `--release --target`.

**Prevention:** cache dependencies only, with a key that includes the rustc version and the lockfile, saved from the default branch only (`Swatinem/rust-cache` with `save-if: github.ref == 'refs/heads/main'`). Group checks that share a compile mode into one job (fmt + clippy + doc share check-mode dependencies; test + smoke share the dev build) instead of building once and fanning out through `target/` caches. Never save a cache from a tag-triggered run.

**Fix:** `.github/workflows/ci.yml` now runs `lint`, `test` (which also runs the smoke scripts and, through `--all-targets`, the maintainability target), `flake-check` and `macos` (clippy) in parallel, each with `Swatinem/rust-cache`; the Build and Maintainability jobs are gone. `.github/workflows/release.yml` has no cache steps and no flake re-run.

## Tests That Read the Test Binary's Own Checkout Failed Only in the Tag Build (2026-10-03)

**What happened:** `commands::init::tests::test_initialize_with_plan_creates_config` and `..._creates_stage_files` passed locally, in the pre-push hook and in CI on `main`, then failed in `.github/workflows/release.yml`'s test job for `v1.1.0` with a bare `assertion failed: result.is_ok()`.

**Why:** `initialize_with_plan` read the base branch through `checked_out_branch(std::env::current_dir())`, and the tests ran it from the test binary's cwd: the loom checkout itself. Commit `adcd85ee` made a detached HEAD an error there (it used to be recorded as the branch `HEAD`). A branch push checks out a branch, and so do the local run and the pre-push hook; `actions/checkout` of a tag ref checks out a detached HEAD, so only the release job saw it, the first tag after `adcd85ee`. Cargo then stopped at the failing `--lib` binary, so no integration target ran against the detached checkout either. Same tag-only class as the dev-build entry above.

**Prevention:** code that reads git state takes the repository as a parameter, and its tests hand in a scratch repository (`git::target_guard::test_support::repo()`), never the cwd of `cargo test`. Reproduce a tag-only failure with `git checkout --detach` then `cargo test --all-targets --no-fail-fast`, which also shows every target the first failure hid. Assert on `Result` with `.unwrap()` or a message, never a bare `is_ok()`, so CI prints the error.

**Fix:** `initialize_with_plan` takes `repo_root: &Path` (`execute` passes its own); `init_from_stages` in `commands/init/tests.rs` runs it against a scratch repository on `main`.

## A Review Checkout Under /tmp Fails 14 Tests for Path Reasons (2026-10-13)

**What happened:** reviewing a PR, the full `cargo test --all-targets` ran in a worktree under the
harness scratchpad (`/tmp/claude-1000/...`) with `CARGO_TARGET_DIR` pointed there too. 14 tests in
four targets failed, none of them about the change: `tmux::tests_spawn` (needs `<crate>/target` to
exist, then refuses a `LOOM_BIN` under `/tmp`), `completion_replay::hook_broker`, nine
`hooks_subagent_verify_guard` tests (manifest path contains `claude`), and three relay e2e tests
(scratch root under `/tmp` is refused). The same commit passed all 7941 tests from `~/src/`.

**Why:** these tests deliberately reject a binary, scratch root, or manifest path that a loom
session could write (`/tmp`) or that the claude-name process-tree simulation would misread.

**Prevention:** run the full suite from a checkout outside `/tmp` whose path has no `claude`, with
the default `loom/target`. A detached `git worktree add --detach ~/src/<name> <sha>` does it without
a branch. Without `--no-fail-fast` the first failing target hides every target after it.

**Fix:** none needed in the tree; re-run from such a checkout.

## The Test Suite Never Ran on macOS in CI (2026-10-13)

**What happened:** macOS-only defects reached main and were found only by a contributor running loom on a Mac (issues #19, #21, #23, #24). That contributor could not run the full suite there either: PR #26 reports two `park_disposition_tests` failing on macOS and others failing for local reasons such as a global `core.hooksPath`.

**Why:** CI's only macOS job was compile-only (`cargo test --no-run`, commit a48a0117), then clippy-only once 083f84c1 dropped the build-once job. No test, rustdoc, flake-check or hook-suite run ever happened on Darwin. The hook suite's BSD mode is a Linux shim, and the macOS runner puts a Homebrew bash 5 first on PATH, so bash 3.2, the version the hooks target, never ran.

**Prevention:** every gate whose result can depend on the target OS runs on `macos-latest` as well as Linux; a new such gate gets its macOS job in the same change. A job that forces a tool version asserts the version it got, so it cannot pass vacuously.

**Fix:** `.github/workflows/ci.yml` gained `lint-macos` (clippy and rustdoc), `test-macos` (`cargo test --all-targets --no-fail-fast` and both smoke scripts), `flake-check-macos`, and `hooks-macos` (hook syntax, hook suite, partial-staging regression, with `/bin/bash` symlinked first on PATH and a step that fails unless `$BASH_VERSION` is 3.2).

## Rust Fixtures Assumed Linux Paths, PTYs and Tool Output (2026-10-13)

**What happened:** the first macOS run failed 59 Rust tests that pass on Linux. Most fixtures built
evidence paths from a raw tempdir, and macOS temp dirs sit behind a symlink (`/var` →
`/private/var`, `/tmp` → `/private/tmp`), so the evidence-path checks refused them or a hook's
`pwd -P` spelling did not match. The relay e2e tests hard-coded the XDG scratch layout, but on macOS
the scratch root is `$HOME/Library/Caches/loom/scratch`. The census test created a non-UTF-8 file
name, which APFS refuses. The web-terminal tests assumed Linux PTY buffering and GNU `od` spacing.

**Why:** the suite had only ever run on Linux, where none of these differ.

**Prevention:**

- A fixture canonicalizes its temp root once (`fs::canonicalize(temp.path())`) and builds every path
  from it. A shell test canonicalizes its expectation with `pwd -P` when the hook does. Recurred
  2026-10-24: the `freeze_refusal_tests.rs` fixture passed a raw tempdir work dir to a codex lifecycle
  record, and `validate_current_binding` refused it on macOS only. Reproduce on Linux before pushing
  any fixture that reaches subagent-lifecycle evidence: `ln -s "$(mktemp -d)" /path/link && TMPDIR=/path/link cargo test <filter>`.
- Derive a platform-dependent path through the production function (`relay::scratch_root`), never a
  hard-coded layout.
- A PTY test that needs backpressure puts the child in raw mode: a canonical-mode Darwin PTY never
  blocks the master's writes without a complete line. Squeeze `od` output (`tr -s ' '`): BSD `od` pads
  each byte to four columns.
- Do not size a macOS kernel buffer from source reading alone; measure it on a Mac first. Two fixes to
  the stalled-client web-terminal test failed on the runner. The first shrank the client's
  `SO_RCVBUF` before `connect`, reasoning that TCP never takes back an advertised window. The second
  also set a 1 KiB client `SO_SNDBUF`, reasoning from XNU's `tcp_sbrcv_grow_rwin` that receive
  autosizing ignores a set `SO_RCVBUF` and steps by `16 × t_maxseg`. Measured on macOS (Darwin 27)
  loopback, the client keeps a 16,332-byte MSS whatever its send buffer, and `TCP_MAXSEG` 1024 is
  `EINVAL` (512 is accepted and ignored). A client `SO_RCVBUF` of 8 KiB set before `connect` becomes
  326,640 bytes (20 × MSS) and stays there, so a never-reading client plus an 8 KiB server send buffer
  absorb 331,416 bytes. Set after `connect`, it keeps autosizing (past 530 KB). No socket option bounds
  a stalled loopback client below about 330 KB, so a test that needs one to block a writer must
  measure the absorption and write more than that. The window opens gradually (about 100 KB per half
  second from an initial 128 KiB), so the measurement must wait out a quiet period longer than that
  cadence. A 50 ms pause measured 128 KiB; a write that moves nothing for 500 ms measures the
  ~330 KB. Linux locks a user-set `SO_RCVBUF`.

**Fix:** fixtures canonicalize their temp roots, the relay tests derive the scratch root through
`relay::scratch_root`, the census test is gated off macOS, the PTY tests use raw mode and squeeze `od`
spacing, and the stalled-client test sizes its output from `sockets::stalled_client_capacity`. CI
run 38397957790 is the first with every macOS job green.

## Related Topic

The sweep follow-ups on executable startup, cache coverage, hook-path navigation and fixture or tempdir discipline are in [Test Sweep and Fixture Discipline](test-sweep-and-fixture-discipline.md).

## A Stage Passed Its Gate and Failed the Commit's Maintainability Test

**What happened:** the gate (`loom check`) passed and the review rounds closed, but `loom stage commit`'s pre-commit maintainability test failed on a 67-line test function that W2 added in `orchestrator/adjudication/prompt/contract_tests.rs::the_spec_block_shows_the_plan_and_recorded_cwd`.

**Why:** no acceptance criterion ran `cargo test --test maintainability`, and the signal's extra steps checked only that the baseline file was unchanged.

**Prevention:** run `cargo test --test maintainability` as its own step before the final review round.

## A `run:` Line Ending in `::` Makes the Whole Workflow File Invalid

**What happened:** a CI step was added as `run: cargo test --lib tests_capsule_denies::bwrap::`. GitHub rejected `.github/workflows/ci.yml` (`error in your yaml syntax on line 133`), so every push run failed with zero jobs and `gh run view` printed only "likely failed because of a workflow file issue".

**Why:** in a YAML plain scalar, a colon followed by a space or a line end is a mapping indicator. The trailing `::` ends the line with a colon, which turns the value into a nested mapping (`mapping values are not allowed here`). Nothing local parses the workflow files: the pre-push hook and `loom check` never load them.

**Prevention:** quote any `run:` value containing `:` or ending in `:`, or use a `|` block. After editing a workflow, parse it once: `uv run --with pyyaml python -I -c 'import sys,yaml; yaml.safe_load(open(sys.argv[1]))' .github/workflows/ci.yml`. When a run has zero jobs, the parse error is shown only on the run's web page (`https://github.com/<owner>/<repo>/actions/runs/<id>`); the jobs and check-runs APIs return nothing.

**Fix:** the step's command is double-quoted.

## A Slow Pre-Push Hook Lets the Push Connection Die, and Git Exits Silently

**What happened:** `git push` ran `loom/.githooks/pre-push` for about 13 minutes, printed `All pre-push checks passed.`, then printed nothing more and pushed nothing. The operator had to rerun `git push --no-verify`. An agent asked to diagnose it read the `update by push` entry in `git reflog show origin/main` as proof the hooked push worked; the `--no-verify` rerun writes the same entry.

**Why:** git opens the SSH session to the remote and reads its ref advertisement before it runs the pre-push hook, and keeps that session idle while the hook runs. GitHub closes it during the run: ssh prints `Connection to github.com closed by remote host.` into the middle of the hook's output (seen during `scripts/flake-check.sh`), so a client-side SSH keepalive does not help. The first write after the hook returns hits a closed pipe and git dies of SIGPIPE (exit 141) without a message. Reproduced with a fake SSH transport that drops after 2 s and a hook that sleeps 4 s: the hook's last line prints, exit 141, the remote stays empty.

**Prevention:** a hook cannot reopen git's connection, so a long pre-push hook must make the rerun cheap. A reflog entry only shows a ref changed, not which push changed it; confirm a hooked push from its own output (`To <url>` plus a ref line, or exit status 0).

**Fix:** the hook exits at once when its stdin lists no refs to update. After a full pass on a clean tracked tree it records HEAD in `<git-common-dir>/loom-pre-push-verified` and says so, so rerunning the same push skips the checks; a push touching any non-branch ref (a tag) always runs them. `scripts/push.sh`, which was removed, ran the hook against HEAD before connecting, then `git push`es on a fresh connection. Superseded: see the dirty-tracked-file entry below; the hook keeps no record and runs no tests.

## A Module-Level Import Used Only by a Linux-Gated Test Broke the macOS Build (2026-10-22)

**What happened:** `verify/criteria/confine_cargo_home_tests.rs` imported `std::ffi::OsStr` at module level, but its only user was the `#[cfg(target_os = "linux")]` test. Linux built clean; the macOS Lint, Test and Flake-check jobs failed with `unused import` under `-Dwarnings`.

**Why:** Local clippy runs on Linux only, and no darwin target is installed, so the cfg-dependent dead import was invisible before push.

**Prevention:** A name used only inside a `#[cfg(...)]`-gated item is imported inside that item or written fully qualified, never at module level.

**Fix:** Dropped the import; the Linux test spells `std::ffi::OsStr` inline.

## A Dirty Tracked File Kept the Pre-Push Pass From Being Recorded, So the Rerun Ran Every Check Again

**What happened:** after four local commits the operator ran `git push`; the remote dropped the idle connection during the checks and nothing was pushed. The rerun ran the whole suite again instead of skipping it, and pushed only because the connection survived that time. The agent had told the operator to run `git push` while it knew `CHANGELOG.md` held an uncommitted change from another session.

**Why:** `loom/.githooks/pre-push` records HEAD as verified only when the tracked tree is clean before and after the checks, because the checks read the working tree. The modified `CHANGELOG.md` failed that test, so `loom-pre-push-verified` never got the new HEAD. The hook's fallback hint names `scripts/push.sh` (deleted in the cleanup that followed), which refuses a dirty tracked tree too, leaving only `--no-verify`.

**Prevention:** before telling the operator to push, check `git status --short --untracked-files=no`. With tracked changes present, say the pass will not be recorded and the push may need a second full run; with a clean tree, recommend `scripts/push.sh` (deleted in the cleanup that followed) over `git push`.

**Fix:** `scripts/push.sh` no longer needs a clean tree: it checks HEAD in a persistent detached worktree at `${XDG_CACHE_HOME:-$HOME/.cache}/loom-push-check/<cksum of the git common dir>` (own `loom/target`, so later runs build incrementally), runs that commit's hook there with `LOOM_PUSH_CHECK=1`, refuses to push unless HEAD landed in `loom-pre-push-verified`, then `git push`es on a fresh connection. A `mkdir` lock beside the worktree serializes runs. The hook's "Result not recorded" hint for a dirty tree pointed at `scripts/push.sh`, which was removed. The operator rejected the cached worktree and any push script. `scripts/push.sh` was removed, along with the verified-commit record and the check worktree, and the pre-push hook was cut to fast gates (fmt, markdownlint, clippy, rustdoc, test compilation, the maintainability test, a `loom --version`/`--help` smoke run) so plain `git push` finishes before the remote closes the connection. The full suite, flake-check and audit run in CI.

## Signing test retained porcelain error wording

**What happened:** A completion-commit test expected git commit error wording after the implementation moved to signed commit-tree.

**Why:** The test asserted an incidental command phrase instead of the signing failure contract.

**Prevention:** Update error assertions when changing the owning Git primitive, while retaining checks for unchanged HEAD and index.

**Fix:** Assert the signing failure and preserved state.

## Deprecated module marks generated test cases deprecated

**What happened:** Deprecating the whole public worktree cleanup module caused warnings on generated Rust test-harness constants, including under warnings-as-errors builds.

**Why:** Deprecation propagated through the module hierarchy rather than only to the obsolete exported operations.

**Prevention:** Deprecate the specific legacy exports and allow deprecation within their compatibility tests; leave canonical re-exports available.

**Fix:** Moved the deprecation attributes to legacy exports and retained their tests.

## Create test module directories before splitting files

**What happened:** A test-module split stopped after the implementation move because its destination directory did not exist.

**Why:** The edit assumed a Rust test module already had a directory beside its file.

**Prevention:** Create or verify the destination directory before writing a new nested module.

**Fix:** Create the directory and finish the split before verification.

## Dependency upgrade scope clarification

**What happened:** A dependency update request needed clarification that direct dependencies should reach their latest releases, including breaking versions.

**Why:** Latest compatible lockfile updates can be confused with latest direct dependency releases.

**Prevention:** Check direct dependencies against latest releases separately from transitive resolution, and respect parent constraints for transitives.

**Fix:** Upgrade direct requirements using the ecosystem tool and verify the resulting dependency inventory.

## Linux-Gated Test Helpers Break the macOS Test Build (2026-11-17)

**What happened:** `spawn_payload`/`spawn_payload_with_codex_home` are `#[cfg(all(test, target_os = "linux"))]`, but `runtime_contract_tests.rs` and `supervisor/tests.rs` imported them unconditionally; several other test files kept imports or `mut` bindings used only inside Linux-gated code. The macOS job failed with E0432 and `-D warnings` unused-import errors, which Linux clippy never sees. The same day, after that fix, macOS Lint, Test and Flake check failed again with `dead_code`: `BOUNDARY_TMPDIR`, `SpawnedBoundary::namespace_pid`, `CargoScratch::prepare_private`, `confine_scratch::reclaim::remove`, `parse_cpu_ticks` and `removable::listed::with` were compiled on every platform but read only by Linux-gated code.

**Why:** a `cfg(target_os)` gate on a caller does not gate the items it calls, and the Linux build cannot report the gap. A plain `cargo clippy --target aarch64-apple-darwin` stops in the aws-lc-sys build script, which needs an Apple C toolchain and SDK, so the gap looked impossible to check before pushing.

**Prevention:** when code is gated on Linux, gate every import, constant, field and helper only it uses, or group the gated tests in an inline `#[cfg(target_os = "linux")] mod` with its own `use` lines (as `runtime_contract_tests.rs` `mod bwrap` does). Before pushing a cfg change, `rg` each newly gated caller's callees for other users, and check every module-level import its signature names: gating `reclaim::remove(scratch: &Path)` left `use std::path::{Path, PathBuf}` with no `Path` user on macOS, and that broke all three macOS jobs again. Prefer shadowing (`let x = x || ...;` under cfg) over `let mut` mutated only on Linux.

Run macOS clippy from Linux before pushing a cfg change. Install the std target (`rustup target add aarch64-apple-darwin`) and give aws-lc-sys stub tools: a `cc` that only creates the file named after `-o`, and an `ar` that only creates each `*.a` argument. Clippy never links, so the empty objects are enough:

```bash
CC_aarch64_apple_darwin=<dir>/cc AR_aarch64_apple_darwin=<dir>/ar \
  CARGO_TARGET_DIR=<scratch>/mac-target \
  cargo clippy --target aarch64-apple-darwin --all-targets -- -D warnings
```

This catches macOS dead code and unused imports. It cannot run tests, so macOS runtime behaviour still needs CI.

**Fix:** the bwrap tests moved into `mod bwrap`; the other imports got `#[cfg(target_os = "linux")]`. The six dead items got `#[cfg(target_os = "linux")]`, except `prepare_private`, which a portable test also calls, so it got `#[cfg(any(target_os = "linux", test))]`.

## macOS Test Failures Built Up Unseen Behind Compile Errors and a Hang (2026-11-17)

**What happened:** after the macOS compile errors were fixed, Test (macos) ran for 28 minutes with no output and was cancelled. The full log showed two tests past 60 s and 88 lib-test failures that had accumulated since the last green macOS run on 2026-10-24. The hang kept libtest from printing its failure section, so no panic messages existed. Of the 88, 57 failed because the macOS temp directory sits under the `/var` symlink: `safe_replace_outside_workdir` compared the target and the canonical project root lexically and refused every plan amendment, a production bug for any project under a symlinked parent. The other 31 needed macOS-only behaviour: APFS rejects names that are not UTF-8, a case-insensitive filesystem folds `CONFIG.WORKTREE`, there is no bwrap or `/proc`, and XNU ignores `MSG_DONTWAIT` on a blocking socket, which made the daemon writer able to block forever.

**Why:** every macOS run on the branch failed to compile first, so no test ran there for weeks. GitHub's live log viewer loads only part of a long log, so its search missed the "has been running for over 60 seconds" lines.

**Prevention:** when a CI test job hangs, cancel it and read the raw log with `gh api repos/<owner>/<repo>/actions/jobs/<id>/logs`, searching for `has been running for over`. Reproduce the `/var` trap on Linux by running the failing tests with `TMPDIR` pointing through a symlink. A path check that compares a target with a canonical root canonicalizes the part above the tree on both sides.

**Fix:** `split_under_root` in `loom/src/fs/safe_replace.rs` matches the root across spellings above the tree and walks the rest without following links. The daemon writer polls before each send off Linux. Tests whose setup macOS cannot create are gated or adapted, each with a comment naming the platform fact.

## killpg Answers EPERM on macOS for a Zombie-Only Group, and a CPU-Bound Test Fixture Outran a Timeout (2026-11-19)

**What happened:** CI failed on both platforms with one test each. On macOS, `pressure::codex_run::tests::an_attempts_group_is_dropped_once_empty_and_kept_while_a_child_outlives_it` failed with `failed to stop codex process group: EPERM`. On Linux, `tmux::tests_history::a_scrollback_past_the_capture_limit_keeps_its_tail` found no history file.

**Why:** macOS returns `EPERM` from `killpg` when every member of the group is a zombie (an orphan launchd has not reaped yet); `cancel_groups` treated only `ESRCH` as an empty group. The tmux test's fake `tmux` generated 17 MB with `head -c | tr`, which takes about 0.6 s on an idle machine and can pass the 5 s `TMUX_PROBE_TIMEOUT` on a loaded runner, so the capture timed out and nothing was saved.

**Prevention:** when probing or signalling a process group of our own children, treat `EPERM` like `ESRCH`. A test fixture that must produce a large output prints a file prepared in Rust (`cat`), never shell generation, when a timeout bounds the command.

**Fix:** `src/pressure/codex_run.rs` `cancel_groups` drops a group on `ESRCH | EPERM`; the tmux test writes the scrollback file once and its script `exec cat`s it.

## A Linux-Only Module Used the Dev-Only wait-timeout Crate

**What happened:** `verify/criteria/confine_scratch/overlay.rs` (`#[cfg(target_os = "linux")]`) called `wait_timeout::ChildExt`, but `wait-timeout` is only in `[dev-dependencies]`. Every Linux build of the library failed with E0432/E0599: Lint, Test (linux) and Hook syntax went red, and both Linux web smoke steps failed because there was no binary to run. The macOS jobs and local macOS checks compiled the module out and passed.

**Why:** a dev-dependency resolves under `--all-targets`, but only test code can use it. A Linux-gated library module is invisible to every macOS check, so nothing before CI compiled it. Its author also missed that `process::wait_with_deadline` exists so that library code avoids the `wait-timeout` SIGCHLD handler (see "Knowledge checks must tolerate restricted process operations" in `sandbox-tooling-and-network.md`).

**Prevention:** library code waits with `crate::process::wait_with_deadline`, never `wait_timeout`. Before pushing a change to Linux-gated code from macOS, run Linux clippy in Docker, with the repo mounted read-only and a private target dir so no other checkout shares the build:

```bash
docker run --rm -v "$PWD":/src:ro -w /src/loom -e CARGO_TARGET_DIR=/tmp/target \
  rust:1-bookworm sh -c "rustup component add clippy && cargo clippy --all-targets --locked -- -D warnings"
```

A host `cargo check --target x86_64-unknown-linux-gnu` does not replace this: aws-lc-sys needs `x86_64-linux-gnu-gcc`, and a Homebrew `cargo` ahead of rustup on `PATH` reports a missing `core` even when the target is installed.

**Fix:** `overlay_supported` calls `crate::process::wait_with_deadline`. In ci.yml the web smoke steps of both Test jobs now run only when the `build` and `tools` steps succeed, so one root failure no longer shows up as extra red smoke steps.

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

**What happened:** with two failure classes still open (confined toolchain probes under `$HOME`, hook policy tests under the host-job lane), `nightly.yml` was dispatched anyway, committing about an hour of macOS runner time to failures already known.

**Why:** the goal of one full green run was treated as a reason to start the run, ahead of fixing what was known to fail.

**Prevention:** start a CI run only when every failure it can show is unknown. Reproduce and fix known failures locally first; the macOS host-job lane and the confined probes both run on a Mac (`cargo test --lib -- --ignored --exact process::sandbox_probe::host_job_lane::the_lib_tests_pass_under_host_job_confinement`).

**Fix:** the dispatch was cancelled; the known failures are fixed locally before the next nightly dispatch.
