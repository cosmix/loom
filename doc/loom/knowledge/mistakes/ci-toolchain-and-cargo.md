# Ci Toolchain And Cargo

> CI clippy drift, offline cargo audit, install.sh

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
hardcoded) and pipe `y` to stdin for `confirm_overwrites` (`install.sh:486`, which reads plain
stdin, not the tty) — but the run will still abort at `cleanup_backups()`'s tty read at the very
end unless a tty is attached, so treat a `No such device or address` failure AFTER the install
steps' own success output as a harness artifact, not evidence the install failed.

## CI Clippy Failures That Don't Reproduce Locally = Toolchain Drift (2026-07-22)

**What happened:** CI's Clippy job failed on main while `cargo clippy --all-targets -- -D warnings` passed locally with zero warnings. Local toolchain was 1.95.0; CI installs latest stable via `dtolnay/rust-toolchain@stable`, which had moved to 1.97.1 and shipped new lints (`useless_borrows_in_formatting`, broader `question_mark`) that fired on 21 existing sites.
**Why:** The workflow floats on `@stable` while local toolchains only move on explicit `rustup update`. Every ~6-week Rust release can introduce lints that break CI with `-D warnings` even though no code changed.
**Prevention:** When a CI clippy failure doesn't reproduce locally, check `rustup check` FIRST — if stable has moved, `rustup update stable` and re-run before hunting for any other cause. Most new-lint fallout is machine-applicable: `cargo clippy --fix --all-targets --allow-dirty`, then review the diff (non-trivial rewrites like `question_mark` can leave awkward leftover blocks worth hand-cleaning).
**Fix:** Updated local stable to 1.97.1, applied `cargo clippy --fix`, hand-simplified the `?`-operator rewrite in `fs/work_dir.rs`, verified clippy + fmt + full test suite green.

## `cargo audit` Git-Fetches Its Database First — It Cannot Pass in a No-Network Stage Sandbox (2026-09-12)

**What happened:** a plan copied `cargo audit -f loom/Cargo.lock -d loom/target/advisory-db`
verbatim from a sibling plan's pre-push gate into a stage whose sandbox declares "No
network access." `cargo-audit` git-fetches the RustSec advisory database into that path
before scanning anything; `loom/target/advisory-db` does not exist in a fresh worktree,
so the fetch fails ("couldn't fetch advisory database: git operation failed") before the
audit itself ever runs — the criterion fails regardless of whether the dependency tree
has any advisories.

**Why:** the criterion was copied from a plan whose stage DOES have network access,
without checking the destination stage's own network policy.

**Prevention:** a plan step that runs `cargo audit` inside a no-network stage must use
`cargo audit --no-fetch -d "$HOME/.cargo/advisory-db"` against a database path already
present on the machine (populated by an earlier `cargo audit` run outside a stage
sandbox), never a fresh in-worktree path the offline run cannot populate itself. When
copying a gate list between plans, re-check each criterion against the destination
stage's own sandbox network policy — a criterion that passed in the source plan is not
evidence it will pass in the copy.

## `cargo audit` Also Cannot Pass Inside the Agent's Own Bash-Tool Sandbox (2026-09-13)

A different failure mode from the one above: run interactively (not as a plan's acceptance
criterion), `cargo audit` fails inside the Claude Code Bash sandbox with `~/.cargo/advisory-db`
read-only, and `--db` fetch fails on host-key verification. The repo's pre-push hook runs `cargo
audit` outside that sandbox; do not add it to a stage's own gate script or try to make it pass
inside an agent session — treat a red `cargo audit` from inside the sandbox as expected, not a
regression, and rely on the pre-push hook for the real check.

## `libc::mode_t` Width Differs by Platform — `.into()` Is a Clippy Error on Linux (2026-08-10)

`libc::mode_t` is `u32` on Linux and `u16` on macOS, while the std APIs we call
(`DirBuilderExt::mode`, `PermissionsExt::from_mode`, our own `safe_fs::open_safely`) all take `u32`
unconditionally. So `const MODE: libc::mode_t = 0o700; builder.mode(MODE.into())` compiles on macOS
and fails on Linux with `useless_conversion`, which `-D warnings` promotes to an error — it blocked
`git push` (`loom/src/daemon/server/storage.rs:14`). The same trap applies to any
platform-width alias: `c_int`, `off_t`, `nlink_t`, `time_t`.

**Prevention:** declare permission constants as `u32` (the type every Rust-side API wants) and cast
at the raw-libc boundary only: `libc::fchmod(fd, MODE as libc::mode_t)`. A cast to an alias is exempt
from `unnecessary_cast`, so it is lint-clean on both platforms, whereas `.into()`/`u32::from()` is
lint-clean on exactly one.

**Also:** the pre-push hook runs Clippy, the pre-commit hook does not. A lint-broken commit lands
locally and only surfaces at push time. Run `cargo clippy --all-targets -- -D warnings` before
committing, not after.

## Every Stage Worktree Compiled Its Dependencies From Scratch (2026-09-02)

**What happened:** each stage worktree has its own `target/` directory, so every stage spent minutes recompiling the same dependency crates before its first test ran.

**Why:** a shared `CARGO_TARGET_DIR` across worktrees is unsafe here — parallel stages would overwrite each other's `debug/loom`, which acceptance criteria invoke by relative path — and nothing else shared compiled output between worktrees.

**Prevention:** share rustc output through `sccache`, which caches per input hash and leaves every worktree its own `target/` untouched.

**Fix:** `orchestrator/terminal/native/build_cache.rs` locates `sccache` (`LOOM_SCCACHE=0` disables it, `LOOM_SCCACHE=<path>` pins it, otherwise `which` then `~/.cargo/bin`, `~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`). The session wrapper exports `RUSTC_WRAPPER=<path>` for every session kind when found, and forwards an operator's own `RUSTC_WRAPPER`, `SCCACHE_DIR`, `SCCACHE_CACHE_SIZE`; the confined acceptance environment allows the same three. `loom run` and `loom doctor` print one line stating whether sccache was found.

**2026-09-04 correction:** sccache IS installed on this machine (0.7.7 at `/usr/bin/sccache`) and IS exported into every session, but it fails closed inside the stage sandbox — see "The Sandbox's AF_UNIX Denial Also Kills sccache" in [sandbox-and-settings.md](sandbox-and-settings.md) for the root cause and the `env -u RUSTC_WRAPPER` / `LOOM_SCCACHE=0` workarounds. The prior "not installed" note was wrong and led two separate stages to misattribute the same failure.

## The Same Suite Ran Once Per Stage, Per Check, Per Judge (2026-09-02)

**What happened:** five stages of one plan each carried the unfiltered `cargo test --all-targets` gate as an acceptance criterion. Each copy ran once in the agent's own `loom check`, again in `loom stage complete`, and again for every adjudication of that criterion; integration-verify then ran the whole suite once more.

**Why:** the plan proved the entire repository at every stage instead of proving each stage's own code, and nothing remembered a pass already recorded against an unchanged tree.

**Prevention:** run the full suite once, in integration-verify. A standard stage's acceptance criterion should be `cargo test --lib <module>::`, `cargo test --test <target>`, or an equivalent name filter. `loom plan verify` now warns on a full-suite run outside integration-verify (`plan/schema/validation_suite.rs::is_full_suite_run`), and the plan-writer skill states the rule as item 6 of its acceptance checklist.

**Fix:** `verify/criteria/cache.rs` caches criterion passes under `<work_dir>/acceptance-cache/<sha256>.json`, keyed by the criterion text, the acceptance directory, `git rev-parse HEAD`, the raw `git status --porcelain=v2 --untracked-files=all -z` output, and the content hash of every listed path. Failures are never cached. A command mentioning `$HOME`, `~/`, `mktemp`, or `LOOM_HOME` is never cached. `loom check --no-cache`, `loom stage complete --no-cache`, or `LOOM_ACCEPTANCE_CACHE=0` bypass the cache; a cached pass prints as `✓ passed (cached)`. A command that references any git-ignored path (a built binary under target/, for instance) is never cached, because the digest covers the tracked tree only; cargo test and cargo build stay cacheable since they rebuild from that tree.

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
