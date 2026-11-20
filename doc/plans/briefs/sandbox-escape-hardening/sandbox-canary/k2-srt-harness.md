# K2: the srt harness probes the paths this plan closes

Stage `sandbox-canary`, one wave with K1, tier sonnet. Read `../common.md` first.

## Why

The confinement e2e launches a session through the real launch path, translates the capsule
it wrote into srt settings and probes it from a real bwrap sandbox. It does not probe any
`denyRead`, `R/.git` beyond `hooks`, the real package caches or the codex home today, and its
translator misses a grant Claude Code makes: a session in a linked worktree may write the
whole git common directory. srt (0.0.78) does not make that grant, so under srt `R/.git` is
read-only whether or not the capsule denies it, and a test of D1 would prove nothing. You fix
the fixture and the translation, add six probe tests, and move the existing tests' grant
lines to the grants capsule-policy left.

You own: `loom/src/orchestrator/terminal/native/tests_confinement_e2e.rs`,
`loom/src/orchestrator/terminal/native/tests_confinement_srt.rs`, and
`loom/src/orchestrator/terminal/native/tests_confinement_escape.rs` (new).

## What you use

- From K1 (same wave; code against these, K1 writes them):
  `crate::process::sandbox_probe::srt::srt_settings(capsule: &Value, cwd: &Path, home: &Path)
  -> Value` (your translation, moved, plus the common dir of a linked-worktree `cwd` in
  `allowWrite` and the capsule's `allowRead` passed through),
  `crate::process::sandbox_probe::capsule_path(entry: &str, cwd: &Path, home: &Path) ->
  Option<PathBuf>` (your `srt_path`, returning a path), and
  `crate::process::sandbox_probe::real_package_cache_dirs(home: &Path) -> Vec<PathBuf>`.
- From capsule-policy (merged): its effects on the capsule `prepare_session_launch_with`
  writes, in common.md terms: D1 (the common dir in a worktree-rooted capsule's `denyWrite`),
  D3 (`C` in `allowWrite`, named `<stage-id>-<kind>` with the owner marker `C/.loom-session`
  holding the session id; no real cache granted), D4 (plain
  `denyRead` paths), D5 (`R/.worktrees` in `denyRead`, `T` in `allowRead`), D6 (a
  codex-licensed capsule grants `C`, `~/.codex/plugin-data` and `~/.codex/auth.json`, not
  `~/.codex`), D7 (a checkout-rooted capsule denies the common dir's existing `info`,
  `objects/info`, `worktrees`, `modules` and `refs/replace`, and nothing else in it). Read each
  from the capsule JSON; use no capsule-policy symbol by name. If
  capsule-policy added fields to `LaunchHost`, `HostFacts` or `WritableRootInputs` and made a
  compile-only edit to `fixture()`, keep it; if `fixture()` does not compile, fill the new
  fields from the fake home and the fixture base as production fills them, and report it.

## Anchors (read at `3fc28031`; locate by symbol)

`tests_confinement_e2e.rs` (381 lines, registered by `native/launch.rs:348-350`, unchanged):
`Fixture` :54, `fixture` :75, `fake_home` :122 (directory list :124-130), `repo_with_worktree`
:150 (the `git init` block :156-162), `confine` :200-230, `control_surfaces` :233, the stage
test :251 (grant line :263 `f.home.join(".cargo/registry/x")`), the knowledge test :271 (grant
line :287, same text), the codex test :295 (comment :304, grant line :305
`f.home.join(".codex/x")`).

`tests_confinement_srt.rs` (278 lines): `probe_srt` :49 (`srt_document` call :61),
`write_settings` :123, `srt_settings` :127, `srt_document` :142, `srt_path` :161, `Confined`
:175, `run` :197, `run_alive` :212, `write` :224, `refusals_missed` :236, `writes_missed` :259.

Isolated git in tests: `src/verify/impact_tests_tests.rs::git` :54-60 (`GIT_CONFIG_GLOBAL` and
`GIT_CONFIG_SYSTEM` at missing files, `GIT_CONFIG_NOSYSTEM=1`).

## 1. `tests_confinement_srt.rs`

- Delete `srt_settings`, `srt_document` and `srt_path`. `write_settings` writes K1's
  `srt_settings(capsule, cwd, home)`; `probe_srt` writes `srt_settings(&Value::Null,
  dir.path(), dir.path())`. Point the module doc's translation sentence at
  `process::sandbox_probe::srt`. Drop imports that go unused (clippy denies them).
- `Confined` gains `pub(super) cache: PathBuf` (the session's `C`) and
  `pub(super) capsule: Value` (the capsule it was built from), with doc comments.
- `pub(super) fn session_cache(capsule: &Value, key: &str, session_id: &str, cwd: &Path,
  home: &Path) -> PathBuf`: the `allowWrite` entry, resolved with `capsule_path`, whose file
  name is `key`, D3's `<stage-id>-<session-kind>` (`STAGE_ID` and the kind spelled the way
  capsule-policy spells it: `rg -n 'session-caches' src/`). Never match on the session id: `C`
  is shared by a stage's sessions and only its marker names one. None found panics quoting the
  `allowWrite` list: a capsule without D3's `C` is a regression, never a skip. When
  `C/.loom-session` exists it must hold `session_id`, or the function panics (the marker names
  another session).
- Read probes, each through `run_alive` (the `ALIVE` sentinel) with the command's own exit
  code printed after it (`cat <p>; rc=$?; echo; echo LOOM_PROBE_RC=$rc`, reusing
  `RC_PREFIX` and `write_rc`):
  - `pub(super) fn reads_missed(&self, readable: &[(PathBuf, &str)]) -> Vec<String>`: each
    must give exit code 0 and stdout containing the expected text.
  - `pub(super) fn masks_missed(&self, masked: &[(PathBuf, &str)]) -> Vec<String>`: each must
    give an RC line (the shell lived through the read) and stdout WITHOUT the text.
  - `pub(super) fn listing(&self, dir: &Path) -> Result<Vec<String>, String>`: the names
    `ls -A1` prints, `Err` with `diagnostics` when its exit code is missing or non-zero.
  Failure messages carry `diagnostics(&output)` like the write probes.

## 2. `tests_confinement_e2e.rs`

- Declare `#[path = "tests_confinement_escape.rs"] mod escape;` beside the `srt` module.
- `repo_with_worktree`: replace the `git init` block with
  `escape::init_repo_with_linked_worktree(repo, worktree)`, called before anything is created
  under `worktree` (`git worktree add` needs a missing or empty target), then create
  `worktree/src` and `worktree/.claude` as now. Keep the `.git/hooks` line.
- `fake_home`: add `.codex/plugin-data` to the directory list.
- `confine`: after `write_settings`, set `cache` with `srt::session_cache(&capsule, &key,
  &session.id, &cwd, &f.home)` (`key` built from `stage.id` and `kind` as D3 spells it) and
  `capsule`. After the launch and before `confine` does anything else with `C`, assert that
  `C` exists on the host (the launch creates it; name, in a comment, the production code that
  does so: `rg -n 'session-caches' src/`). `confine` never creates `C` itself, so a launch that
  stops creating it fails the fixture and is not hidden by `confine` creating it.
- Grant lines: :263 and :287 become `confined.cache.join("x"),`; :305 becomes
  `f.home.join(".codex/plugin-data/x")`, and the comment above it says the lane's
  `plugin-data` grant (D6) is live, so the refusal of `hooks.json` is not a dead sandbox.
- Stay at or under 400 lines (381 at HEAD): the new tests and the git helpers live in the
  escape file. Never edit an `assert!` line.

## 3. `tests_confinement_escape.rs` (new)

Module doc: what the six tests prove and that they share the parent's fixture. Helpers:

    /// A git checkout on `main` with one commit (README.md) and a linked worktree at
    /// `worktree` on `loom/<its file name>`; git runs with isolated configuration.
    pub(super) fn init_repo_with_linked_worktree(repo: &Path, worktree: &Path)
    fn add_linked_worktree(repo: &Path, worktree: &Path)
    fn git(dir: &Path, args: &[&str]) -> String   // asserts success, quotes stderr

Every test is `#[test] #[serial]`, starts with `if skip("<its name>") { return; }`, builds
every path under `f.base` (never outside it: skip capsule entries that do not resolve under
`f.home`, `f.repo` or `f.base`), creates each probed path's parent on the host before its srt
run, and ends with one `assert!(missed.is_empty(), "{}", missed.join("\n"))`.

| Test (exact name) | Refused or masked under the capsule | Matched positive |
| --- | --- | --- |
| `a_stage_capsule_refuses_the_git_common_dir_and_writes_its_worktree` | writes to `R/.git/HEAD`, `R/.git/index`, `R/.git/refs/heads/loom-probe`, `R/.git/objects/loom-probe`, `W/HEAD`, `W/loom-probe` (W from `git rev-parse --path-format=absolute --git-dir` in the worktree) | `worktree/src/x` written; AND the same capsule with its `denyWrite` entry for the common dir removed, translated to a second settings file, writes `R/.git/objects/loom-probe-control` (proves srt grants the dir, so the refusal is D1's); a capsule with no such entry is a miss |
| `a_stage_capsule_refuses_the_real_package_caches_and_writes_its_session_cache` | a write to `<dir>/x` for every `real_package_cache_dirs(&f.home)` entry, each created on the host first | `C/x` and `C/cargo/x` written |
| `a_stage_capsule_masks_denied_credentials_and_reads_their_siblings` | every `denyRead` entry under `f.home`: a `/**` entry becomes a directory holding `loom-probe-secret` (listing empty, file masked); any other becomes a file holding a secret (masked); a capsule with none is a miss | `<parent>/loom-probe-visible` of each entry, unless another `denyRead` entry covers it, reads its text |
| `a_stage_capsule_hides_sibling_worktrees_and_reads_its_own` | after `add_linked_worktree(R/.worktrees/other)` and an untracked `other/sibling-only.txt`: `R/.worktrees` lists only the stage worktree (drop `.claude`, `.mcp.json`); `sibling-only.txt` masked | `worktree/README.md` reads; `R/README.md` reads (D5 keeps the checkout readable: the accepted gap, asserted so it stays deliberate) |
| `a_knowledge_capsule_refuses_the_git_control_dirs_and_writes_a_branch_ref` | a knowledge capsule (session kind Knowledge, cwd `R`, D7): a fresh `loom-probe` in `R/.git/info`, `R/.git/objects/info` and `R/.git/worktrees`, each asserted to exist on the host before the launch (git init makes the first two; the fixture's linked worktree makes the third; D7 denies only entries that exist at spawn) | `R/.git/refs/heads/loom-probe` written (knowledge sessions commit in `R`), which also proves the refusals come from D7's entries and not from a read-only `.git` |
| `a_codex_lane_capsule_refuses_the_codex_home_and_writes_its_session_codex_home` | lanes Claude and Codex; `~/.codex/AGENTS.md` (created holding text) and a fresh `~/.codex/loom-probe` refused | `C/codex-home/x`, `~/.codex/auth.json` (created holding `{}` first: srt binds only existing paths) and `~/.codex/plugin-data/x` written |

Keep each test under 50 lines; put shared setup in helpers.

## Traps

- The sentinel rule stays: every probe goes through `run_alive`, a write counts as refused only
  with a non-zero write exit code AND the file unchanged
  (`mistakes/verification-harness.md`, "A Must-Fail Probe That Counts Any Non-Zero Exit
  Passes When the Harness Never Started").
- A write whose parent directory is missing fails with ENOENT and reads as a refusal: create
  every parent on the host first.
- srt binds only paths that exist when it starts; a denied path missing inside a writable
  directory is mounted from `/dev/null`, so a write exits 0 and lands nowhere
  (`architecture/execution-containment.md`, "Confinement E2E Lives Outside the Sandbox"):
  create denied and granted files before the probe, after `confine` returns.
- srt denies `.git/hooks` and `.git/config` by its own defaults: they are no evidence of D1,
  which is why the common-dir test probes HEAD, index, refs, objects and W, with the control.
- `test -w` reports WRITABLE on a `/dev/null` mount: only a real write or read decides.
- The srt CLI crashed when several instances ran at once: every test is `#[serial]`.
- The fixture sees only what `prepare_session_launch_with` writes. Before writing the tests,
  find where capsule-policy adds the common-dir deny and D7's entry denies
  (`rg -n 'git-common-dir|objects/info' src/sandbox src/orchestrator/terminal/native
  src/fs/permissions`) and where it grants and creates `C` (`rg -n 'session-caches' src/`).
  If any of them happens outside the launch path the fixture
  drives, stop and report it with file:line: the fixture then cannot see that decision, and
  fixing it needs a file capsule-policy owns.

## Check

None. Your files compile only in `cargo test --lib`, which needs K1's `sandbox_probe/srt.rs`,
and every srt test self-skips inside a stage session (srt binds a Unix socket; the sandbox
denies AF_UNIX). The main agent builds after both workers return; the operator runs the suite
outside the sandbox (plan prose, "Manual step"). Say in your report that these tests have only
taken their skip path.

The operator's srt command pins `@anthropic-ai/sandbox-runtime@0.0.78` (the version the bind
behaviour and the missing git-dir grant were measured on) and runs
`cargo test confinement -- --nocapture`: cargo captures a passing test's stderr otherwise, so
a `SKIP` line would never show.

```bash
shim="$(mktemp -d)"
printf '#!/bin/sh\nexec bunx @anthropic-ai/sandbox-runtime@0.0.78 "$@"\n' > "$shim/srt"
chmod +x "$shim/srt"
cd loom && env -u LOOM_WORK_DIR PATH="$shim:$PATH" LOOM_TEST_REQUIRE_SANDBOX_FREE=1 \
  cargo test confinement -- --nocapture
```
