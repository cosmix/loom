# Cargo Under Confinement

> Cargo in sandboxes and confined runs: audit fetches, cold builds, Seatbelt

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

## Plain `cargo audit` Cannot Refresh Its Database Inside the Agent's Bash-Tool Sandbox (2026-09-13)

**What happened:** run interactively (not as a plan's acceptance criterion), plain `cargo audit`
fails inside the Claude Code Bash sandbox: `~/.cargo/advisory-db` is read-only there, so the
database refresh fails, and a `--db` fetch fails on host-key verification. A note written then
concluded `cargo audit` cannot pass inside an agent session at all.

**Why that was wrong:** only the fetch cannot run. The sandbox grants write to the lock file
`~/.cargo/advisory-db..lock` and reads the database directory, so `cargo audit --no-fetch` scans
against a database the operator already refreshed on the host. The platform-portability stage of
PLAN-open-issues-19-24 ran it that way as an acceptance criterion; the plan's host prerequisite was
one host `cargo audit` in `loom/` before the run.

**Prevention:** inside an agent session or a no-network stage use `cargo audit --no-fetch`, and
treat a failure naming a missing or stale database as an environment gap to report (the operator
refreshes it on the host), not as an audit result. Plain `cargo audit` stays a pre-push step. Run
it from the directory that holds `Cargo.lock`: the command has no `--manifest-path`, so
`loom plan verify` flags it when the stage's `working_dir` is the repository root.

## Every Stage Worktree Compiled Its Dependencies From Scratch (2026-09-02)

**What happened:** each stage worktree has its own `target/` directory, so every stage spent minutes recompiling the same dependency crates before its first test ran.

**Why:** a shared `CARGO_TARGET_DIR` across worktrees is unsafe here — parallel stages would overwrite each other's `debug/loom`, which acceptance criteria invoke by relative path — and nothing else shared compiled output between worktrees.

**Prevention:** share rustc output through `sccache`, which caches per input hash and leaves every worktree its own `target/` untouched.

**Fix (attempted, inert):** `orchestrator/terminal/launch/build_cache.rs` locates `sccache` (`LOOM_SCCACHE=0` disables it, `LOOM_SCCACHE=<path>` pins it, otherwise `which` then `~/.cargo/bin`, `~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`), and `loom run` and `loom doctor` print whether it was found. It shares nothing in practice: `sccache_usable_in` withholds `RUSTC_WRAPPER` from every sandboxed session whose seccomp filter denies `AF_UNIX` (see "The Sandbox's AF_UNIX Denial Also Kills sccache" in [sandbox-tooling-and-network.md](sandbox-tooling-and-network.md)), and confined host jobs start from `env_clear()` with `process/environment.rs::STAGE_HOST_ENV_ALLOWLIST` forwarding `SCCACHE_DIR` and `SCCACHE_CACHE_SIZE` but never `RUSTC_WRAPPER`. The earlier text here claimed the session wrapper exports `RUSTC_WRAPPER` for every session kind and the confined environment allows it; neither holds. An empty `~/.cache/sccache` on the development machine confirms nothing is cached. The open problem, for every ecosystem, is [concerns/worktree-build-cold-start.md](../concerns/worktree-build-cold-start.md).

## The Same Suite Ran Once Per Stage, Per Check, Per Judge (2026-09-02)

**What happened:** five stages of one plan each carried the unfiltered `cargo test --all-targets` gate as an acceptance criterion. Each copy ran once in the agent's own `loom check`, again in `loom stage complete`, and again for every adjudication of that criterion; integration-verify then ran the whole suite once more.

**Why:** the plan proved the entire repository at every stage instead of proving each stage's own code, and nothing remembered a pass already recorded against an unchanged tree.

**Prevention:** run the full suite once, in integration-verify. A standard stage's acceptance criterion should be `cargo test --lib <module>::`, `cargo test --test <target>`, or an equivalent name filter. `loom plan verify` now warns on a full-suite run outside integration-verify (`plan/schema/validation_suite.rs::is_full_suite_run`), and the plan-writer skill states the rule as item 6 of its acceptance checklist.

**Fix:** `verify/criteria/cache.rs` caches criterion passes under `<work_dir>/acceptance-cache/<sha256>.json`, keyed by the criterion text, the acceptance directory, `git rev-parse HEAD`, the raw `git status --porcelain=v2 --untracked-files=all -z` output, and the content hash of every listed path. Failures are never cached. A command mentioning `$HOME`, `~/`, `mktemp`, or `LOOM_HOME` is never cached. `loom check --no-cache`, `loom stage complete --no-cache`, or `LOOM_ACCEPTANCE_CACHE=0` bypass the cache; a cached pass prints as `✓ passed (cached)`. A command that references any git-ignored path (a built binary under target/, for instance) is never cached, because the digest covers the tracked tree only; cargo test and cargo build stay cacheable since they rebuild from that tree. That key later proved too strict: hashing the commit id made every commit after `loom check` invalidate its passes. The current key (commit 865eed49) is a `(path, mode, blob oid)` content manifest plus the HEAD text, prepared env, executables and containment revision (`verify/criteria/cache_fingerprint.rs`); `--no-cache` now refreshes (stores without reading) and only `LOOM_ACCEPTANCE_CACHE=0` bypasses; any command containing `$` or running `git` is never cached.

## A Seatbelt `file-read*` Deny Did Not Hide a File From `stat` (2026-11-18)

**What happened:** to make an ancestor `Cargo.toml` above a stage worktree look absent under the macOS sandbox, as it is under bubblewrap, the profile added `(deny file-read* (literal P))` after its global `(allow file-read-metadata)`. A unit test confirmed the rule text and order, but under the real `sandbox-exec` `test -e` still saw the file and cargo still failed with "failed searching for potential workspace … Operation not permitted". That took one more CI round to find.

**Why:** in SBPL a rule that names `file-read-metadata` outranks the `file-read*` wildcard, so the earlier named allow won. `access(F_OK)` is a separate operation, `file-test-existence`. Checking the profile text cannot catch this, because the semantics live in the kernel.

**Prevention:** to hide a path from a Seatbelt profile that allows metadata globally, deny the operations by name: `(deny file-read-metadata file-test-existence (literal P))` plus `(deny file-read* (literal P))`, on the canonical `/private/...` path. Test it under the real `sandbox-exec` (`confine_seatbelt_tests.rs`), never by asserting profile text alone. A macOS test that can fail in CI prints its diagnostics in the assertion message, so one CI round settles the cause.

**Fix:** `confine_ancestors.rs` emits the named-operation denies for every spelling `seatbelt_spellings` produces. `confine_seatbelt_probe_tests.rs` prints the profile in use and the results of six rule variants when the absence test fails.

## The Contract Freeze Rebuilt Every Dependency on macOS Because Its Cargo Home Path Differed From the Session's

**What happened:** in stage `kickoff-fixes`, `loom stage contracts freeze` timed out its first Rust contract (`did not finish within 300 s`) twice; the other ten passed. The stage worktree's `target/debug/build` held two hash sets per crate (`serde-373b73f04bf20b5c` from the session's builds, `serde-2aa1669738976979` from the freeze), and the `aws-lc-sys` C build alone took about 6 minutes inside the freeze. A reproduction outside loom gave the same two hashes and `cargo test --lib --no-run -v` reported `Dirty proc-macro2 v1.0.107: the path to the source changed` for all 264 dependencies whenever `CARGO_HOME` alternated between `~/.cargo` and a directory linking `~/.cargo/registry`. Changing only `TMPDIR` left every unit fresh.

**Why:** under Seatbelt the confined command's Cargo home was the per-tree linked home `<temp root>/.loom-cargo-<digest>`. Its sources resolve to the same files but under another absolute path, and Cargo fingerprints registry packages by that path. The home was stable from one confined command to the next, which is all the earlier fix required ([completion-runtime](completion-runtime.md)). It was still a different path from the one the stage session's own `cargo` used, so the first confined build in a worktree the session had warmed recompiled everything, and the next session build recompiled everything again. Only Linux kept the host path (`CargoHome::Mounted`).

**Prevention:** a confined Cargo command must see the session's Cargo home path, not only a stable one. A test of the Cargo home asserts equality with the host path on every platform (`confine_cargo_home_tests.rs::every_confined_command_for_one_tree_shares_one_cargo_home`). When builds look cold, run the same command with `-v` and read the `Dirty <crate>: <reason>` lines before forming a hypothesis.

**Fix:** `CargoHome::Host` (`verify/criteria/confine_scratch.rs`): under Seatbelt the command reads the host Cargo home in place. Nothing in the home is writable, and Cargo only warns `failed to save last-use data`. Cargo's state files are readable, and its config and credentials files are hidden by name (`confine_seatbelt.rs::host_cargo_home_rules`). The linked home remains only for a Cargo home that overlaps the tree. Two confined `cargo test --lib --no-run -v` runs against the session-warmed `loom/target` then reported `Fresh=264 Compiling=0 Dirty=0` in 0.2 s each.

## A Stale Build-Script Output Directory Embeds Another Session's Scratch Paths

**What happened:** in two stages of one plan (`codex-lane-lifetime`, `completion-sandbox-and-impact`) the confined gate failed to compile with `Operation not permitted` on every criterion. A leftover `loom/target/debug/build/loom/<hash>/out/web_assets.rs` (and once the sibling generated assets file) held `include_bytes!` paths into an earlier session's scratchpad stub copy, which the confined check cannot read.

**Why:** `build.rs` rerun keys did not invalidate after the scratch repository vanished, so cargo reused the generated file.

**Prevention:** when the gate shows `include_bytes!` errors under a scratchpad path, delete that one stale `loom/target/debug/build/loom/<hash>` output directory and rerun; touch nothing else in `target/`. A `build.rs` that embeds files by absolute path should key its rerun on those paths existing.

**Fix:** removing the one stale output directory fixed the build both times.

## The Full Suite Has No Temp Directory That Satisfies Every Test Inside a Claude Code Sandbox (2026-11-24)

**What happened:** a full `cargo test --all-targets` inside a Claude Code Bash sandbox failed 17 `hooks_subagent_verify_guard` tests, because they refuse a temp root whose path contains "claude" and the sandbox's `TMPDIR` is `/tmp/claude-<uid>/...`. Rerunning with `TMPDIR` under `loom/target/` put every temp directory inside the loom repository: 114 lib tests that need a directory outside any repository failed, and `codex_evidence` failed with "workspace mismatch". Two `file_mode_contracts` tests also fail there, because `fs::locking::lock_plan` locks under the fixed `/tmp/loom-plan-locks-<uid>` and the sandbox mounts `/tmp` read-only.
**Why:** the sandbox's only writable temp locations either contain "claude" or sit inside the repository, and each class of test rules one of them out.
**Prevention:** run the suite with the sandbox's default `TMPDIR`, never one inside the repository. Treat the `hooks_subagent_verify_guard` and the two plan-lock failures as environmental, and confirm them with the user's own run (`! cd loom && cargo test --test integration hooks_subagent_verify_guard --test file_mode_contracts`). On this host, use `CARGO_BUILD_JOBS=16` and `--test-threads=16`; an unthrottled run alongside a second build ran the machine out of memory.
**Fix:** none in the code. `codex_evidence` now stubs both sandbox probes, so a sandboxed Linux host no longer sends it down the direct lane.
