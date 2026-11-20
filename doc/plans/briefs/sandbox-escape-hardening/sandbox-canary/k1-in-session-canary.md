# K1: the in-session sandbox canary and the stage-sandbox probe surface

Stage `sandbox-canary`, one wave with K2, tier sonnet. Read `../common.md` first.

## Why

Unit tests on `build_settings` output prove what the capsule says, never what a live sandbox
does with it (`concerns/sandbox-and-confinement-gaps.md`, "Sandbox Denial Has No End-to-End CI
Canary"). You write a test that runs inside a real loom stage session and proves, from the
session's own capsule, that every write and read this plan closes is refused while the paths
the session needs stay open. Outside a live stage sandbox it prints a loud skip line and
passes; `LOOM_TEST_REQUIRE_STAGE_SANDBOX=1` turns that skip into a failure. Integration-verify
runs it with that flag after the operator reinstalls loom; in THIS stage it must skip (your
session's capsule was built by the pre-plan binary and has every hole: "This stage runs under
the OLD capsule" in common.md).

You also write the library surface K2's srt harness and the stage's contracts call.

You own: `loom/src/process/sandbox_probe.rs`, `loom/src/process/sandbox_probe/srt.rs` (new),
`loom/tests/sandbox_canary.rs` (new), `loom/tests/sandbox_canary/probes.rs`,
`loom/tests/sandbox_canary/layout.rs`, `loom/tests/sandbox_canary/checks.rs`,
`loom/tests/sandbox_canary/probe_tests.rs` (all new). Never write
`loom/tests/sandbox_canary_contracts.rs` (frozen contract file).

## What capsule-policy provides (merged before you start)

You use no Rust symbol capsule-policy adds. You read its effects from the session:

- D1: a worktree-rooted capsule lists the git common directory in
  `sandbox.filesystem.denyWrite`.
- D3: the session environment sets `CARGO_HOME=C/cargo` and the other D3 path variables, with
  `C = <user cache dir>/loom/session-caches/<stage-id>-<session-kind>` (shared by a stage's
  sessions; `C/.loom-session` names the session it was seeded for); `C` is in `allowWrite`; no real
  package cache is granted.
- D4: `denyRead` holds plain credential paths, `~/.codex/auth.json` for sessions the codex lane
  is not licensed for.
- D5: a worktree-rooted capsule has `R/.worktrees` in `denyRead` and `T` in
  `sandbox.filesystem.allowRead`.
- D6: a codex-licensed capsule grants `C`, `~/.codex/plugin-data` and `~/.codex/auth.json`,
  never `~/.codex`.

Derive every path at runtime: the capsule JSON, `git rev-parse`, `dirs::home_dir()`, the
environment. No literal home, uid, repository or cache path anywhere.

## Anchors (read at `3fc28031`; locate by symbol)

- `loom/src/process/sandbox_probe.rs` (196 lines): `skip_unless` :154 (the
  `LOOM_TEST_REQUIRE_SANDBOX_FREE` skip-or-panic), its tests :165-196.
- `loom/src/orchestrator/terminal/native/tests_confinement_srt.rs`: `srt_settings` :127,
  `srt_document` :142, `srt_path` :161 move into your `sandbox_probe/srt.rs` (K2 deletes them
  there and calls yours).
- `crate::orchestrator::terminal::native::session_settings_path(work_dir, session_id)`
  (`native/session_settings.rs:44`, `pub(crate)`, re-exported in `native/mod.rs:47`): the
  capsule path, `<work_dir>/capsules/<session_id>.settings.json`. Call it; do not rebuild it.
- `loom::git::runner::run_git_checked(args: &[&str], repo_root: &Path) -> Result<String>`
  (`git/runner.rs:129`): trimmed stdout of a successful git command.
- Dependencies already in `Cargo.toml`: `nix` with feature `fs` (`nix::sys::statfs::{statfs,
  TMPFS_MAGIC}`), `libc`, `dirs`, `serde_json`, `tempfile`. Add none.

## 1. The library surface (`sandbox_probe.rs`)

Refactor `skip_unless` into a private `skip_or_panic(probe_ok, test_name, why, require_var,
demand)` that both skips share; `skip_unless`'s text and behaviour stay byte-identical (its
existing tests and 18 callers must not notice). Then add, with doc comments:

    /// What a test process can see of the loom stage sandbox around it.
    #[derive(Debug, Clone, Default)]
    pub struct StageSandboxEvidence {
        pub stage_id: Option<String>,     // LOOM_STAGE_ID
        pub session_id: Option<String>,   // LOOM_SESSION_ID
        pub work_dir: Option<PathBuf>,    // LOOM_WORK_DIR
        pub capsule: Option<String>,      // the session capsule's text, when readable
        pub pid1_comm: Option<String>,    // /proc/1/comm trimmed; None off Linux or unreadable
        pub cargo_home: Option<PathBuf>,  // CARGO_HOME
        pub cache_owner: Option<String>,  // <CARGO_HOME's parent>/.loom-session, trimmed,
                                          // read only when it is a regular file
    }

    impl StageSandboxEvidence {
        /// Reads the process environment, the capsule, /proc/1/comm and the session
        /// cache's owner marker; an empty variable counts as unset. stage_id, session_id
        /// and work_dir come from LOOM_STAGE_ID, LOOM_SESSION_ID and LOOM_WORK_DIR, or,
        /// when one is unset, from LOOM_ACCEPTANCE_STAGE_ID, LOOM_ACCEPTANCE_SESSION_ID and
        /// LOOM_ACCEPTANCE_WORK_DIR: `loom stage complete` runs acceptance through
        /// spawn_confined, which withholds the LOOM_* names and exports these copies
        /// (capsule-policy, process/environment.rs).
        pub fn from_env() -> Self
    }

    /// Ok when the evidence shows a live loom stage sandbox under this plan's policy;
    /// Err naming the first missing piece. Never touches the filesystem.
    pub fn stage_sandbox_live(evidence: &StageSandboxEvidence) -> Result<(), String>

    /// false when `evidence` is live. Otherwise prints
    /// "SKIP <test_name>: <reason> (set LOOM_TEST_REQUIRE_STAGE_SANDBOX=1 to fail instead)"
    /// and returns true, or, when LOOM_TEST_REQUIRE_STAGE_SANDBOX=1, panics with
    /// "<test_name>: <reason> (LOOM_TEST_REQUIRE_STAGE_SANDBOX=1 demands a run inside a loom
    /// stage sandbox)".
    pub fn skip_unless_stage_sandbox(test_name: &str, evidence: &StageSandboxEvidence) -> bool

    /// One `sandbox.filesystem` entry as the path Claude Code reads it: `~/x` under `home`,
    /// `//x` and `/x` absolute, anything else under `cwd`; a trailing `/**` cut; None for any
    /// other glob (`*`, `?`, `[`, `{`).
    pub fn capsule_path(entry: &str, cwd: &Path, home: &Path) -> Option<PathBuf>

    /// The default locations under `home` of every package cache decision D3 relocates, with
    /// their Linux and macOS spellings, whether or not they exist.
    pub fn real_package_cache_dirs(home: &Path) -> Vec<PathBuf>

    pub mod srt;

`stage_sandbox_live` checks, in this order, with a reason naming the variable or file:
`stage_id`, `session_id` and `work_dir` present; `capsule` present and parsing as JSON with an
object at `sandbox.filesystem`; `pid1_comm == Some("bwrap")`; `cargo_home` present and
`cache_owner == session_id` (D3: `C` is `<stage-id>-<kind>`, a path shared by a stage's
sessions, and its owner marker `C/.loom-session` holds the id of the session it was seeded for;
never compare `C`'s directory name with the session id). A missing `CARGO_HOME` or a missing or
mismatched marker gives the same reason: "CARGO_HOME is not this session's cache (C/cargo with
C/.loom-session naming this session, decision D3): the session was spawned by a loom that
predates the sandbox-escape-hardening policy". That check is what makes the canary skip in this
stage and run in integration-verify; the flag turns it into a failure there.

The detector requires `/proc/1/comm == bwrap`. That has been measured only on the host
(`systemd`), never inside `loom stage complete`'s confined acceptance (one historical test,
`fs/permissions/hooks/policy_tests_stage_gate.rs`, saw stage evidence missing there). The plan
adds a sandbox-canary acceptance line `test "$(cat /proc/1/comm)" = bwrap`; if it fails in the
stage, the detector premise is wrong: block the stage naming it, never weaken the detector.

`real_package_cache_dirs` lists exactly (home-relative): `.cargo/registry`, `.cargo/git`,
`.rustup/downloads`, `.rustup/tmp`, `.rustup/toolchains`, `.bun/install/cache`, `.npm`,
`.local/share/pnpm/store`, `.cache/pnpm`, `.local/state/pnpm`, `Library/pnpm/store`,
`Library/Caches/pnpm`, `.cache/yarn`, `Library/Caches/Yarn`, `.yarn/berry`, `.cache/deno`,
`Library/Caches/deno`, `.cache/uv`, `Library/Caches/uv`, `.cache/pip`, `Library/Caches/pip`,
`go/pkg`, `.cache/go-build`, `Library/Caches/go-build`. It is an oracle: never build it from
`sandbox/package_caches.rs` or any capsule-policy resolver, or a path dropped from both goes
unseen. Never use `dirs::cache_dir()` for it: inside a session `XDG_CACHE_HOME` points into `C`.

Add one unit test to the existing `tests` module: `capsule_path_reads_entries_the_way_claude_code_does`
(`~/a/**`, `//abs/b`, `/abs/c`, `rel/d/**`, and `a/*.rs` giving None).

## 2. The srt translation (`sandbox_probe/srt.rs`, new)

Move the translation out of `tests_confinement_srt.rs` (`srt_settings`, `srt_document`,
`srt_path` there; `srt_path` becomes your `capsule_path`) and extend it:

    /// The canonicalized git common directory of `cwd` when `cwd` lies in a linked worktree
    /// (`git rev-parse --path-format=absolute --git-common-dir` differs from `--git-dir`);
    /// None in a main checkout, outside git, or when git fails.
    pub fn linked_worktree_common_dir(cwd: &Path) -> Option<PathBuf>

    /// `capsule`'s `sandbox.filesystem` block as srt settings for a session in `cwd` with
    /// `home` as its home: {"filesystem": {"denyRead", "allowRead", "allowWrite",
    /// "denyWrite"}, "network": {"allowedDomains": [], "deniedDomains": []}}. Each list maps
    /// the capsule's list through `capsule_path` (None dropped, a missing list empty).
    /// allowWrite starts with `cwd`, then `linked_worktree_common_dir(cwd)` when Some (Claude
    /// Code grants a linked worktree its common directory; srt does not), then the capsule's.
    pub fn srt_settings(capsule: &Value, cwd: &Path, home: &Path) -> Value

Paths become JSON strings with `display()`. `srt_settings(&Value::Null, dir, dir)` must work (K2's
host probe uses it). Git goes through `run_git_checked`. Keep the module doc comment from
`tests_confinement_srt.rs` lines 5-7 (how the block is read).

## 3. The canary target

`tests/sandbox_canary.rs` is the crate root. Cargo discovers `tests/*/main.rs` as its own
target, so the helper directory holds no `main.rs` and no `mod.rs`: declare each helper with
`#[path = "sandbox_canary/<file>.rs"] mod <file>;` from the root.

The root holds the module doc (what it proves, why it skips, the flag) and:

    #[test]
    fn a_live_stage_sandbox_enforces_its_capsule() {
        let evidence = StageSandboxEvidence::from_env();
        if skip_unless_stage_sandbox("a_live_stage_sandbox_enforces_its_capsule", &evidence) {
            return;
        }
        // layout (an Err from SessionLayout::discover panics with its error), report,
        // checks::run_all, print report.tried, one assert listing every violation and
        // every required proof that is missing (see "Strict live canary")
    }

`layout.rs`: `SessionLayout { capsule: Value, session_id, home, toplevel, git_dir, common_dir,
checkout, cache }` built by `SessionLayout::discover(&StageSandboxEvidence) -> Result<Self,
String>`. `toplevel` is `git rev-parse --show-toplevel` run in `env!("CARGO_MANIFEST_DIR")` (the
session cwd: relative capsule entries such as `loom/src/**` are relative to it); `git_dir` and
`common_dir` are `--path-format=absolute --git-dir` / `--git-common-dir`, canonicalized;
`checkout` (R) is `common_dir`'s parent; `cache` (C) is `CARGO_HOME`'s parent; `home` is
`dirs::home_dir()`. Methods: `worktree_rooted()` (`git_dir != common_dir`), `entries(key) ->
Vec<(String, Option<PathBuf>)>` (resolved with `capsule_path` against `toplevel` and `home`),
`codex_licensed()` (any `allowWrite` entry resolves under `home/.codex`).

`probes.rs`: a `Report { tried: Vec<String>, violations: Vec<String> }` whose every probe adds
one `tried` line ("<probe> <path>: <outcome>") and, on failure, one violation; and primitives,
each returning `Result<String, String>` (Ok = the outcome proven, Err = the violation):

- `create_refused(dir)`: `OpenOptions::new().write(true).create_new(true)` on a fresh name
  (`.loom-canary-<pid>-<counter>`, checked absent first). Ok only when it fails with EROFS,
  EACCES or EPERM (`raw_os_error` against `libc` constants) AND `symlink_metadata` then reports
  NotFound. A success removes the file and is Err; any other error is Err "unproven".
- `create_allowed(dir)`: the same open succeeds and the file is removed again.
- `append_refused(file)`: bytes read before, `OpenOptions::new().append(true).open`, bytes read
  after. Ok when the open fails with EROFS/EACCES/EPERM and the bytes match. An open that
  succeeds is dropped WITHOUT writing and is Err.
- `masked_file(path)`: Ok when `metadata` shows a character device (`FileTypeExt`) or the read
  fails with PermissionDenied; a regular file that reads is Err "readable (<n> bytes)". Never
  put file content in a report.
- `masked_dir(dir, allowed: &BTreeSet<OsString>)`: Ok when `is_tmpfs(dir)` and every listed
  name is in `allowed`; Err naming the extra names or "not a tmpfs".
- `is_tmpfs(path) -> bool`: `nix::sys::statfs::statfs(path)` type `== TMPFS_MAGIC` on Linux;
  `false` elsewhere (`#[cfg]` pair).

`checks.rs`: `run_all(&SessionLayout, &mut Report)` runs these in order, controls first.

| Check | Negative (must be refused or masked) | Matched positive control |
| --- | --- | --- |
| (a) session cache, D3 | every D3 path variable set (CARGO_HOME, BUN_INSTALL_CACHE_DIR, npm_config_cache, npm_config_store_dir, YARN_CACHE_FOLDER, GOPATH, GOMODCACHE, GOCACHE, UV_CACHE_DIR, PIP_CACHE_DIR, DENO_DIR, XDG_CACHE_HOME; CODEX_HOME iff codex-licensed) lies under C, else a violation; a missing one is a violation | `create_allowed` in C and in each variable's directory (`create_dir_all` first); `create_allowed` in `env!("CARGO_TARGET_TMPDIR")` (the worktree's target dir); some `allowWrite` entry resolves to C |
| (b) git common dir, D1 (worktree-rooted only) | `create_refused` in `common_dir`, `common_dir/refs/heads`, `common_dir/objects`, `git_dir` (W); `append_refused` on `common_dir/HEAD`, and on `git_dir/index` when it exists | the `CARGO_TARGET_TMPDIR` control of (a) |
| (c) real caches, D3 | `create_refused` in every `real_package_cache_dirs(home)` entry that is a directory, and in every capsule `sandbox.filesystem.denyWrite` entry that `capsule_path` maps to an existing directory outside C and T (capsule-policy write-denies the daemon's resolved real caches, so a cache relocated by `CARGO_HOME` or `UV_CACHE_DIR`, which the session's variables no longer name, is probed too) | C writable, from (a) |
| (d) credential reads, D4 | every `denyRead` entry that exists: a file or character device is `masked_file`; a directory is `masked_dir`, allowed = the first component below it of each resolved `allowRead`/`allowWrite` entry and `toplevel`, plus `.claude` and `.mcp.json` when the directory is R, `R/.worktrees` or T; AND the fixed oracle list below the table | `read_dir(home)` yields at least one entry; the first non-empty regular file directly in `home` that no `denyRead` entry covers reads at least one byte (none found is a `tried` note) |
| (e) sibling worktrees, D5 (worktree-rooted, T directly in `R/.worktrees`) | `R/.worktrees` lists nothing but T's name, after dropping `.claude` and `.mcp.json` | T's name is listed and `read_dir(toplevel)` is non-empty |
| (f) codex home, D6 | `create_refused` in `home/.codex` when it exists; when not codex-licensed, `masked_file` on `home/.codex/auth.json` when it exists | the (a) controls; when licensed, CODEX_HOME lies under C |
| (g) static grants | no resolved `allowWrite` entry equals, lies inside or contains an entry of `real_package_cache_dirs(home)`, or (worktree-rooted) `common_dir`; glob entries are `tried` notes | none: a pure check of the capsule |

A checkout-rooted session (a Knowledge stage running the suite) skips (b), (e) and (g)'s
common-dir part with a `tried` note: D7 keeps its `.git` writable by design, apart from the
entries D7 denies, which K2's srt test covers (a probe here would need a positive write inside
the operator's real `.git`). An absent optional path (`home/.codex`, `git_dir/index`, a
credential entry, a real cache directory) is a `tried` note, never a violation and never a pass.

Strict live canary. The test runs only in a live stage sandbox, so a required path that is
missing there is a VIOLATION, never a `tried` note: in a live worktree-rooted session the git
common dir, the admin dir `W`, the session cache `C`, the worktree `T` and `R/.worktrees` must
all exist, and `SessionLayout::discover` returning `Err` panics with its error. The final
assertion also requires that each of these was proven (its probe's `tried` line present and no
violation for it): `create_refused(common_dir)`, `create_refused(W)`,
`append_refused(common_dir/HEAD)`, `create_allowed(C)`, `masked_dir(R/.worktrees)` with the
listing showing only the session's own worktree, and `create_refused(home/.cargo/registry)`
when that directory exists. A run that records none of them fails, and so does one missing any
of them.

Check (d) keeps its capsule-driven list AND an independent fixed list, a constant in
`checks.rs`: `.config/gh/hosts.yml`, `.netrc`, `.npmrc`, `.claude.json`, `.git-credentials`,
`.docker/config.json`, `.kube/config`, `.aws/credentials`, `.ssh` (each under
`dirs::home_dir()`). Each that exists must read masked whatever the capsule says (`masked_file`
for a file, `masked_dir` with nothing allowed for `.ssh`). Like `real_package_cache_dirs`, it is
an oracle: never derive it from `CREDENTIAL_DENY_READ_PATHS` or the capsule, so an entry dropped
from both the constant and the capsule cannot go unseen.

## 4. Probe primitive tests (`probe_tests.rs`, always run)

These run in this stage and give each primitive its matched pair; the last one covers
`checks::run_all`. Build every path in a
`tempfile::Builder::tempdir_in(env!("CARGO_TARGET_TMPDIR"))`; restore permissions before the
TempDir drops. Exact names:

- `a_create_probe_is_refused_in_a_read_only_directory_and_lands_in_a_writable_one`: a 0o555
  directory gives Ok from `create_refused` and nothing in it; a writable one gives Err from
  `create_refused`, Ok from `create_allowed`, and is empty afterwards.
- `an_append_probe_is_refused_on_a_read_only_file_and_opens_a_writable_one`: a 0o444 file gives
  Ok and unchanged bytes; a 0o644 file gives Err and unchanged bytes.
- `a_masked_file_probe_accepts_dev_null_and_rejects_a_readable_file`: `/dev/null` is Ok; a file
  holding "secret" is Err and the Err text does not contain "secret".
- `the_tmpfs_probe_agrees_with_the_mount_table` (Linux): the first `tmpfs` mount point in
  `/proc/self/mounts` that is a readable directory gives true (none found: a SKIP line through
  `loom::process::sandbox_probe::skip_unless`); `env!("CARGO_TARGET_TMPDIR")` gives false unless
  its longest-prefix mount in that table is a tmpfs.
- `checks_report_a_writable_git_dir_and_a_real_cache_grant`: `checks::run_all` has no other
  test in this stage (the live test skips here), so this one drives it over a synthetic
  `SessionLayout` built from TempDirs (a hand-written `capsule` value; `git_dir` and
  `common_dir` distinct directories, so `worktree_rooted()` is true). A writable `common_dir`
  gives a check (b) violation and a 0o555 one gives none; an `allowWrite` entry equal to a
  `real_package_cache_dirs` path gives a check (g) violation; a writable directory outside C
  and T listed in the capsule's `denyWrite` gives a check (c) violation and a 0o555 one gives
  none. The synthetic layout fails other
  checks, so each assertion looks only at the violations that name the path under test.

The permission tests (the first two and `checks_report_a_writable_git_dir_and_a_real_cache_grant`)
skip through `skip_unless` when `nix::unistd::geteuid().is_root()`: root ignores permission bits.

## Traps

- The sentinel rule (`mistakes/verification-harness.md`, "A Must-Fail Probe That Counts Any
  Non-Zero Exit Passes When the Harness Never Started"): a refusal counts only with its errno
  AND the path unchanged; the positive controls run first, so a probe that "passes" because
  nothing works is reported as a failed control.
- `test -w` and `access(2)` report WRITABLE on a `/dev/null` bind: never decide writability from
  permissions or metadata. Only a real open decides.
- Never create, truncate or write an existing file: fresh names with `create_new`, append-open
  without a write. Every file a probe created is removed; a failed removal is a violation.
- `Path::exists` hides PermissionDenied; use `metadata` and match the error kind.
- Claude Code mounts `/dev/null` placeholders named `.claude` and `.mcp.json` at R, `R/.worktrees`
  and T: filter those names only there.
- `dirs::cache_dir()` and every D3 variable point into C inside a session; real cache defaults
  come from `dirs::home_dir()` only.
- Report every violation, never the first only; print `report.tried` so a failing run shows
  what each probe tried.
- Keep every file under 400 lines and every function under 50 (`checks.rs` has one function
  per row above).

## Check

Once, at the end: `cargo test --test sandbox_canary` from `loom/`. In this stage the live test
prints its SKIP line ("predates the sandbox-escape-hardening policy") and passes; the four
primitive tests and the `run_all` test run for real. It builds only the library and your target,
so K2's parallel work does not affect it. The stage contracts
(`the_stage_sandbox_detector_needs_every_piece_of_evidence`,
`the_require_flag_turns_the_canary_skip_into_a_failure`,
`srt_translation_grants_a_linked_worktree_its_common_dir`; scenarios in the plan YAML) must pass
against your code.
