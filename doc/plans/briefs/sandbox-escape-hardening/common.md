# Common rules for every worker of PLAN-sandbox-escape-hardening

Read this file and your own brief in full before anything else. The plan is
`doc/plans/PLAN-sandbox-escape-hardening.md` (`loom run` commits its rename to
`doc/plans/IN_PROGRESS-PLAN-sandbox-escape-hardening.md`; read whichever exists);
its YAML is authoritative where a brief and the plan differ. `R` below is the
main checkout (the parent of the git common directory), `T` a stage worktree
`R/.worktrees/<stage-id>`, `W` its git administrative directory
`R/.git/worktrees/<admin>`, and `C` a session's cache directory (decision D3).

## What you own

- You write only the files your row of the stage's worker table lists. A file
  you need to change that no row lists is an ownership gap: stop, record it with
  `loom memory note "found: <file> needs <change> for <reason>"`, and name it in
  your report. Never edit another worker's file, and never a file another
  stage of this plan owns (the ownership map below).
- You never run git, never spawn subagents, never write `.loom/` or
  `doc/loom/knowledge/`, and never use Claude Code auto-memory.
- Verification is the main agent's job. Run at most one narrowly scoped check
  over your own files, once (`cargo test --lib <your_module>::` or
  `cargo test --test <your_contract_file>`), and skip it when your brief says
  the crate cannot compile until another worker finishes. A compile error in a
  file you do not own is not yours: report it with its file:line.
- Leave formatting to the main agent (`cargo fmt --all` once after the last
  wave); keep every line under 100 columns anyway.

## This stage runs under the OLD capsule

Your session was spawned by the loom binary installed before this plan. Its
sandbox still has every hole this plan closes, and `git commit` still works in
it. Nothing you build takes effect on your own session. Never test a change by
observing your own sandbox; test it through the code (`build_settings`, the
capsule writer, the wrapper script text, the daemon apply function) or through
the srt harness, which self-skips inside a stage.

## Anchors

Line numbers in the briefs were read at `3fc28031` and are advisory; the plan
runs after PLAN-stage-exits-and-environment merges, so they have moved. Locate
every edit by symbol (`loom map --outline <file>`, or `rg -n '<symbol>'`).

## Decisions every stage shares

D1 **Daemon-owned commits.** A session of kind Stage, Contract or Adjudication
(whatever the adjudication's cwd: a judge never writes git) gets the whole git
common directory in `denyWrite`, resolved at spawn with
`git rev-parse --path-format=absolute --git-common-dir` run in `R`. A deny
always wins over Claude Code's automatic linked-worktree grant of that
directory, so `W`, `refs/`, `objects/`, `HEAD`, `index` and `packed-refs` are all
read-only in the session. Agents commit with:

    loom commit -m "<type(scope): description>" -- <path> [<path> ...]

in ONE Bash call, then confirm in the NEXT Bash call (the relay hook fires only
after the first call returns):

    loom request status <request-id> --wait 90

The daemon applies the request with pinned git (`WorktreeGit::pinned`) in the
stage's own worktree, and never runs `git add` on agent-written paths: it resets
the index with `git read-tree --reset HEAD` (that keeps the stat data of untouched
entries; a plain `read-tree HEAD` would zero every entry's stat data, which no one
can refresh under D1), opens every path with a no-follow walk from the worktree
root (a regular file with `O_NOFOLLOW|O_NONBLOCK`, size-capped; a symlink leaf
stored as a link; a FIFO, device, directory or nested repository refuses the
request), copies each regular file's bytes through the descriptor into a temp
file in its own state directory, hashes it with
`git --attr-source=HEAD hash-object -w --path=<path> <temp file>` (attributes
come from HEAD's tree, never from the worktree's `.gitattributes`, which could be
a FIFO), and stages entries with
`git update-index --add --cacheinfo <mode>,<sha>,<path>` (`WorktreeGit::run`
takes no stdin). It refuses any gitlink, then commits. The CLI expands a
directory argument into files in-session. Checkout-rooted sessions (Knowledge
stages, Merge and BaseConflict resolution) keep `git add` and `git commit`; an
Adjudication session never writes git, whatever its cwd. Subagents never run
`git commit` or `loom commit`.

The exact doctrine sentence every worktree-session surface uses (signals,
CLAUDE.md.template, skills, hook messages):

    Commit with `loom commit -m "<type(scope): description>" -- <files>`
    in one Bash call, then run `loom request status <id> --wait 90` in the
    next call; `git add` and `git commit` fail in a stage worktree because its
    git directory is read-only.

D2 **Pinned host git.** Every git command loom or a loom hook runs OUTSIDE the
session sandbox against a stage worktree runs pinned: `GIT_DIR=W`,
`GIT_COMMON_DIR=R/.git`, `GIT_WORK_TREE=T`, derived from loom's own state and
`R/.git/worktrees/*/gitdir`, never from `T/.git` (a file inside the
agent-writable worktree). Rust uses `WorktreeGit::pinned` /
`pinned_in_project_of` (`loom/src/git/worktree/pinned.rs`); shell hooks derive
`R` from `LOOM_WORK_DIR`. A path is a stage location when its parent directory
is named `.worktrees` and the directory above holds a `.git` entry of any type
(a directory, or the gitfile of a `--separate-git-dir` checkout); the OUTERMOST
such ancestor decides, a stage location that cannot be pinned is an error (never
discovered git), and a checkout inside another stage worktree is refused. Every
loom git call never recurses into submodules
(`git/runner.rs::NO_HOOKS_ARGS` carries `-c diff.ignoreSubmodules=all
-c submodule.recurse=false -c status.submoduleSummary=false`, and the shell helper
`loom_pinned_git` carries the same) and sets `GIT_NO_REPLACE_OBJECTS=1`; a
production `Command::new("git")` outside the runner is converted or classified
`host-direct`.

D3 **Per-session caches.** Every session gets `C =
<user cache dir>/loom/session-caches/<project-key>/<stage-id>-<session-kind>`,
where `<project-key>` is the first 16 hex characters of the SHA-256 of the
canonical main checkout (two repositories on one host never share a cache
directory). A session without a stage uses its session id; `dirs::cache_dir()`
is `$XDG_CACHE_HOME` or `~/.cache` on Linux, `~/Library/Caches` on macOS; under
`cfg(test)` it is a directory under the work dir, like the scratch root. The PATH
is stable across the sessions of one stage, so cargo does not rebuild registry
dependencies after a handoff or retry; the CONTENT is per session: at spawn the
daemon removes any existing `C` (never following a symlink), recreates it 0700,
writes the owner marker `C/.loom-session` holding the session id, seeds it, and
grants it in the capsule's `allowWrite`. A seed never reproduces a tool's control
or temp entries (bun `.tmp`, go `trim.txt`). Retiring a session removes `C` only
when the marker names that session; `loom init` and `loom clean` remove the
project's namespace. The operator's real caches get no grant at all, so they are
read-only in every session: a spawn whose capsule's final `allowWrite` holds an
entry equal to, inside or above a resolved real cache (lexically or after
resolving symlinks; entries at or inside `C` excepted) fails
(`session_fs::refuse_real_cache_grants`), and every resolved real cache that
exists at spawn and is no ancestor of the home directory, `R`, `T` or `C` is
write-denied in both layers. Each session's
environment (wrapper `exec env -i` list AND `STAGE_HOST_ENV_ALLOWLIST`):

    CARGO_HOME=C/cargo            RUSTUP_AUTO_INSTALL=0
    BUN_INSTALL_CACHE_DIR=C/bun   npm_config_cache=C/npm
    npm_config_store_dir=C/pnpm-store                YARN_CACHE_FOLDER=C/yarn
    GOPATH=C/go  GOMODCACHE=C/go/pkg/mod  GOCACHE=C/go-build  GOFLAGS=-modcacherw
    GOPROXY=file://<real GOMODCACHE>/cache/download,<daemon GOPROXY or https://proxy.golang.org,direct>
    UV_CACHE_DIR=C/uv  UV_LINK_MODE=copy  PIP_CACHE_DIR=C/pip  DENO_DIR=C/deno
    XDG_CACHE_HOME=C/xdg-cache
    GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=gc.auto GIT_CONFIG_VALUE_0=0
                       GIT_CONFIG_KEY_1=maintenance.auto GIT_CONFIG_VALUE_1=false
    CODEX_HOME=C/codex-home       (codex-licensed sessions only)

`RUSTUP_HOME` is unchanged and read-only: a stage cannot install a toolchain.
Real cache locations come from the daemon's own environment (each tool's
override variable), falling back to the tool's default under
`dirs::home_dir()` / `dirs::cache_dir()`. Nothing is a literal path.

D4 **Credential reads.** `CREDENTIAL_DENY_READ_PATHS` gains plain paths (no
globs; an absent path mounts nothing, a file is masked by `/dev/null`, a
directory by a tmpfs), plus paths derived at spawn from the daemon's
environment (`GH_CONFIG_DIR`, `DOCKER_CONFIG`, `KUBECONFIG`,
`NPM_CONFIG_USERCONFIG`, `CARGO_HOME`, `GNUPGHOME`, `PASSWORD_STORE_DIR`,
`AWS_SHARED_CREDENTIALS_FILE`, `AWS_CONFIG_FILE`, `CLOUDSDK_CONFIG`,
`AZURE_CONFIG_DIR`, `CODEX_HOME`). `~/.codex/auth.json` is denied to every
session the codex lane is not licensed for. Every capsule also read-denies the
existing top-level files `R/.env` and `R/.env.*` (enumerated at spawn).

D5 **Sibling worktrees are unreadable.** A worktree-rooted capsule denies reads
of `R/.worktrees` and re-allows `T` (`sandbox.filesystem.allowRead`). Reads of
the main checkout stay open (in-session loom resolves config, retrieval and
knowledge state through `main_project_root`); the plan records that as an
accepted gap.

D6 **Codex lane.** A codex-licensed capsule no longer grants `~/.codex` or
`~/.claude/plugins/data/codex-openai-codex`. It grants `C` (which holds
`C/codex-home`), `~/.codex/plugin-data` (the companion's job state, read by
host-side receipt code), and the single file `~/.codex/auth.json` (codex writes
it in place: `FileAuthStorage::save` opens it with truncate+write).

D7 **Checkout-session integrity.** `G`, the main checkout's git directory, is
`R/.git` when that is a directory and, when `R/.git` is a gitfile (a
`--separate-git-dir` checkout), the directory its `gitdir:` line names; anything
else is an error. B's `checkout_integrity::main_git_dir`, C's shell
`loom_main_git_dir` and the capsule's `resolve_git_common_dir` agree on it.
Checkout-rooted agent sessions (Knowledge, Merge, BaseConflict) keep write access
to `G` by design. The capsule denies the entries of the common dir that exist at
spawn and a commit in `R` never writes (`info`, `objects/info`, `worktrees`,
`modules`, `refs/replace`, `hooks`, `config`); a deny
on an absent path would put an empty placeholder file on the host, so absent
redirect files (`commondir`, `gitdir`, `shallow`, `info/grafts`) are detected
instead. The daemon snapshots the repository's refs, pack files and control files
when it spawns such a session, and verifies them before it accepts the session's
result: refs outside the session's target branch and outside `refs/heads/loom/`
(which the daemon itself moves meanwhile) unchanged, every new or changed pack
passing `git verify-pack`, the control files unchanged, no in-progress
operation file (`MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`, `BISECT_LOG`,
`sequencer/`, `rebase-merge/`, `rebase-apply/`), the staged set of the main
checkout's index a subset of the snapshot's with identical entries, and
`git fsck --no-full` clean (every loose object re-hashed). A difference routes the stage to human
review. Loom's own git runner refuses a main git directory holding `commondir`.

## Ownership map (stage → files)

Only the files below may be written by that stage. A worker's brief narrows
this to its own rows.

- `commit-relay`: `loom/src/relay/**`, `loom/src/commands/commit/**` (new),
  `loom/src/commands/request/**`, `loom/src/commands/hook/relay*`,
  `loom/src/cli/{types.rs,types_ops.rs,dispatch.rs,dispatch_stage.rs}`,
  `loom/src/commands/mod.rs`, `loom/src/orchestrator/core/inbox_drain.rs`,
  `loom/src/orchestrator/core/inbox_drain/**` except `sweep.rs`, `tests_sweep.rs`,
  `tests_sweep_cache.rs` and `tests_merge.rs`,
  `loom/src/orchestrator/signals/{helpers.rs,cache.rs,tests_commit_timing.rs}`,
  `CLAUDE.md.template`,
  `skills/loom-orchestration/SKILL.md`, `skills/loom-usage/SKILL.md`,
  `skills/loom-git-workflow/SKILL.md`, `loom-hooks/_subagent-preamble.txt`,
  `loom-hooks/commit-filter.sh`, `loom-hooks/post-tool-use.sh`,
  `loom-hooks/git-add-guard.sh`,
  `loom-hooks/loom-relay.sh`, `loom/src/commands/stage/contracts/freeze.rs` (one
  refusal string), their tests, `loom/tests/commit_relay_contracts.rs`.
- `host-git-integrity`: `loom/src/git/**` (incl. `worktree/pinned.rs`),
  `loom/src/handoff/session_content.rs`, `loom/src/verify/before_after.rs`,
  `loom/src/verify/criteria/cache_fingerprint.rs`,
  `loom/src/verify/criteria/cache_ignore.rs`,
  `loom/src/verify/wiring_detection.rs`, `loom/src/verify/duplicate_detection.rs`,
  `loom/src/orchestrator/adjudication/prompt/sources.rs`,
  `loom/src/orchestrator/core/provision_gate.rs` (created by
  PLAN-stage-exits-and-environment),
  `loom/src/orchestrator/monitor/parked.rs`,
  `loom/src/orchestrator/merge_lifecycle/**`,
  `loom/src/orchestrator/core/merge_handler*`,
  `loom/src/orchestrator/core/inbox_drain/tests_merge.rs` (one setup line),
  `loom/src/orchestrator/terminal/backend.rs`,
  `loom/src/daemon/server/control_complete.rs`, `control_complete_tests.rs`,
  `loom/src/daemon/server/completion_dispatch/tests.rs` (one setup line),
  `loom-hooks/_common.sh`, `loom-hooks/commit-guard.sh` (its git calls AND its D1
  message), `loom-hooks/stage-terminal-guard.sh`, `loom-hooks/codex-forward-guard.sh`,
  `loom-hooks/tests/run-all.sh` (the only stage that registers new hook tests),
  `loom/src/context/worktree_graph.rs`, `loom/tests/host_git_call_sites.rs` and
  `loom/tests/host_git_call_sites.txt` (new), their tests,
  `loom/tests/host_git_integrity_contracts.rs`.
- `capsule-policy`: `loom/src/sandbox/**`, `loom/src/fs/permissions/state_root.rs`,
  `loom/src/codex.rs`, `loom/src/process/environment.rs`,
  `loom/src/orchestrator/terminal/native/**` except `tests_confinement_srt.rs` and
  `tests_confinement_escape.rs` (capsule-policy may make compile-only edits to `fixture()` in
  `tests_confinement_e2e.rs` when it adds fields to `LaunchHost`, `HostFacts` or
  `WritableRootInputs`; sandbox-canary runs after it, so the two never write it at once),
  `loom/src/orchestrator/core/inbox_drain/sweep.rs`, `inbox_drain/tests_sweep_cache.rs`
  (new), `loom/src/models/stage/types.rs` (a doc comment only),
  `loom/src/orchestrator/signals/format/helpers.rs`,
  `skills/loom-plan-writer/references/sandbox.md`,
  `loom-hooks/credential-guard.sh` and its tests,
  `loom/src/daemon/server/environment.rs`, `loom/src/commands/init/cleanup.rs`,
  `loom/src/commands/clean/relay_dirs.rs`, `loom/src/process/mod.rs` (one re-export line),
  `loom/src/verify/criteria/confine.rs` (the `Confined` arm of `prepare_confined` only),
  `loom/tests/capsule_policy_contracts.rs`.
- `sandbox-canary`: `loom/tests/sandbox_canary.rs` and `loom/tests/sandbox_canary/**`,
  `loom/src/orchestrator/terminal/native/tests_confinement_e2e.rs`,
  `loom/src/orchestrator/terminal/native/tests_confinement_srt.rs`,
  `loom/src/orchestrator/terminal/native/tests_confinement_escape.rs` (new),
  `loom/src/process/sandbox_probe.rs`, `loom/src/process/sandbox_probe/**` (new),
  `loom/tests/sandbox_canary_contracts.rs`.
- `loom/tests/host_git_call_sites.txt` belongs to host-git-integrity;
  integration-verify adds the entries for the files the other stages create in
  parallel.

## The maintainability ledger

`loom/maintainability-baseline.txt` records exact line counts for files over 400
lines and functions over 50 lines; `cargo test --test maintainability` fails
when a recorded entry changes or a new item crosses a limit. It is a plan
`ratchet_files` entry: a change to it raises `TI-ratchet-...`, which the stage
disputes once after its final review round. Keep new files under 400 lines and
new functions under 50; keep ledgered items net-zero unless your brief says
otherwise.

## Tests and integrity

- Never edit an existing assertion line in a test file (`TI-edit`). Add tests.
- Contract files are frozen before you start: never edit one.
- A boundary test asserts the allowed case beside the denied case, or it cannot
  fail when the boundary silently stops applying
  (`architecture/execution-containment.md`, "The Test Pattern That Makes A
  Boundary Test Able To Fail").
- Tests build every path from a TempDir, `dirs::home_dir()` or an injected
  home: no literal home directory, uid or repository path.

## Style

Match the surrounding code: `anyhow` with `context`, its comment density, its
naming. Doc comments state what the code does now. No TODO, no stub. Plain
sentences in user-facing text.

## Memory

    loom memory note "mistake: ... Why: ... Prevention: ..."
    loom memory decision "chose X over Y" --context "because Z"
    loom memory note "found/gotcha: ... in <file>:<line>"

## Report

End with: files changed, the check you ran and its result (or why you skipped
it), assumptions you made, and anything unresolved. Nothing else.
