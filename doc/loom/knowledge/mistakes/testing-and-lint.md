# Testing And Lint

> Lint and test discipline: --all-targets, ambient git config in shelling tests, the stub checker, and reviewer claims.

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

**What happened:** `git::merge::tests::merge_stage_refuses_when_merge_head_set` passed locally but failed on CI with `MERGE_HEAD must be set` at the *setup* assertion. The setup used a `run` closure that called `Command::new("git")…output().unwrap()` — `.unwrap()` only catches spawn failure, not a non-zero exit. So when an ambient git config broke a setup commit, every step silently no-op'd and the failure only surfaced lines later as a confusing MERGE_HEAD assertion. Reproduced exactly by setting `commit.gpgsign=true` (no key) in the global config: the seed commit fails → `checkout main` fails → the "merge" runs on the wrong branch → no conflict → no MERGE_HEAD.

**Misleading signal:** "Passes locally, fails in CI" on a pure-logic test that *must* be deterministic. The panic points at the symptom (`MERGE_HEAD` absent), not the cause (a swallowed setup-commit failure several lines up). Tests that shell out to `git` inherit the runner's global/system config — `commit.gpgsign`, `core.hooksPath`, templates — none of which exist on a clean dev box.

**Why:** Two compounding defects: (1) the helper discarded git exit status, so setup failures were invisible; (2) the repo was not isolated from ambient git config, so a hostile global setting could break commits/merges. Note `#[serial]` only serializes against other `#[serial]` tests — the merge tests are non-serial and run alongside `repository.rs`'s `GIT_CONFIG_GLOBAL`-mutating tests, another reason to pin config per-Command rather than rely on the process environment.

**Prevention:** Any test that shells out to `git` must (a) assert each setup command's exit status and surface stderr — never `output().unwrap()` and drop the status; and (b) neutralize ambient config by setting `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` to nonexistent paths and `GIT_CONFIG_NOSYSTEM=1` on the `Command` (so it survives a polluted process env too). Set identity via local config. ~10 test files here use the same `init_repo`/`run_git` shape (`in_progress.rs`, `merge_attribution.rs`, `recovery.rs`, `merge_verify.rs`, …); the asserting ones at least fail loudly, but none isolate ambient config — port the `isolated_git`/`git_ok` helpers from `git/merge/mod.rs` if they ever flake.

**Fix:** Added `isolated_git`/`git_ok` helpers in `git/merge/mod.rs` tests: every setup step asserts success, the conflicting merge dumps stdout/stderr if MERGE_HEAD is absent, and all invocations run with global/system config disabled. Verified green under a forced-`gpgsign` global config that previously reproduced the failure.
