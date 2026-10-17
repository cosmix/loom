# Sandbox Tooling And Network

> Sandbox tool failures

## The Sandbox's AF_UNIX Denial Also Kills sccache, Breaking Every Cargo Command (2026-09-04)

**Current state (2026-09-25):** the export described below is now gated. `build_cache::sccache_usable_in` (`orchestrator/terminal/native/build_cache.rs:145`) is `!sandbox.enabled || network.allow_all_unix_sockets`; `launch.rs:337` and `wrapper.rs:308` withhold `RUSTC_WRAPPER` from a sandboxed session whose seccomp filter denies `AF_UNIX`, and `process/environment.rs` starts acceptance commands from `env_clear()`. The `env -u RUSTC_WRAPPER` workaround and the `LOOM_SCCACHE=0` restart are needed only for a sandbox that allows all unix sockets yet still breaks sccache, or if that gate is removed. A 2026-10-14 probe under the host-socket hiding (`sandbox/host_sockets.rs`) ran `sccache rustc -vV` and `sccache --start-server` inside a flagged session, so the gate's "on" side holds. Verification v2's D4 plan-verify "Rustc wrapper" lint was dropped for this reason: as specified it warned on every cargo plan whenever `/usr/bin/sccache` existed, so `plan verify --strict` failed for plans that could not hit the failure.

The rest is the 2026-09-04 history, true of the tree before the gate.

**What happened:** loom exports `RUSTC_WRAPPER=/usr/bin/sccache` into every stage session
and into the confined acceptance environment (`process/environment.rs` allowlists
`RUSTC_WRAPPER`) whenever sccache is found on the host. Every `cargo build`/`clippy`/`doc`/
`test` then fails before a single crate compiles: `error: process didn't exit successfully:
/usr/bin/sccache rustc -vV` / `sccache: error: Operation not permitted (os error 1)`.

**Why:** the same AF_UNIX `socket()` denial documented above — sccache's client reaches its
server over a Unix domain socket, and `sccache --start-server` cannot even bind one.
`find_sccache_path()` (`orchestrator/terminal/native/build_cache.rs`) only proves the binary
EXISTS, never that it can run where it is being exported to. NOT a cache-directory
permission issue: a writable `SCCACHE_DIR` fails identically. Note `sccache --version`
SUCCEEDS, so any probe weaker than starting the server passes and proves nothing.

**Detection:** a cargo failure whose FIRST line names sccache, before any "Compiling" line,
is this — not a build defect.

**Workaround inside a session (no daemon change):** prefix the command with
`env -u RUSTC_WRAPPER`. This also fixes `git commit`/`git push`, since `loom/.githooks/
pre-commit` and `pre-push` run cargo without that prefix and git hooks inherit the git
process's environment — `env -u RUSTC_WRAPPER git commit ...` is enough.

**Fix that needs an operator, not the stage session:** restart `loom run` with
`LOOM_SCCACHE=0` so the daemon stops exporting the wrapper at all — the acceptance criteria
themselves carry no prefix and the completion-guard hook pins the stage-completion command
to one exact invocation, so nothing inside the session can add a prefix there. Never amend a
shared plan's criteria to carry the prefix; that bakes a machine-specific workaround into the
plan.

## `cargo audit` and `cargo deny` Fail in a Stage Sandbox for Two Unrelated Reasons (2026-09-04)

**What happened:** `cargo audit -f Cargo.lock -d target/advisory-db` and `cargo deny check`
both fail verbatim in a stage session, and neither failure is the sccache/AF_UNIX one above.

**`cargo audit`:** the operator's global `~/.gitconfig` carries `url.ssh://git@github.com/
.insteadof https://github.com/`, so `cargo audit`'s https clone of `RustSec/advisory-db` is
silently rewritten to `ssh://`, and the sandbox has no ssh key or `known_hosts`:
`ssh_askpass: exec(/usr/bin/ssh-askpass): No such file or directory` / `Host key
verification failed`. Detection: an https git fetch failing with `ssh_askpass` or "Host key
verification failed" is an `insteadOf` rewrite, never a proxy/allowlist denial —
`git ls-remote https://github.com/...` succeeds once the rewrite is off. Workaround:
`GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null <command>` (`GIT_CONFIG_COUNT`/
`GIT_CONFIG_KEY_0` do NOT work — `insteadOf` picks the longest matching prefix and the
existing rule still wins).

**`cargo deny`:** not installed, and `cargo install cargo-deny --locked` fails with
`Read-only file system (os error 30)` — the sandbox allows `~/.cargo/registry` and
`~/.cargo/git` but not `~/.cargo/bin`. Install with `--root $TMPDIR/tools` (and
`CARGO_TARGET_DIR` under `$TMPDIR`), prepend to `PATH`. Once running, `cargo-deny` shells out
to `cargo metadata`, which inherits `RUSTC_WRAPPER` and dies on the same sccache EPERM —
`cargo-deny` reports that as "failed to fetch crates", which reads like a network denial and
is not one; prefix with `env -u RUSTC_WRAPPER` too.

**Prevention:** when N acceptance criteria fail together, attribute each one INDIVIDUALLY —
a shared symptom ("all cargo commands fail") is not a shared cause, and inheriting a prior
session's attribution without re-testing propagates a wrong diagnosis.

## A Bash Tool Call Is Its Own Network Namespace — a Server Started in One Call Is Unreachable From Another (2026-09-04)

**What happened:** a server started with `cargo run status --web` (or `bun run dev`) in one
Bash call is unreachable from a later Bash call, and from a Playwright MCP browser, because
each Bash invocation gets its own network namespace.

**Prevention:** for any visual/browser check of a locally-served app, start the server AND
make every request against it inside ONE shell invocation
(`(loom status --web PORT & sleep 2; curl ...; kill $!)`), or serve pre-captured content to
the browser directly — e.g. build to `$TMPDIR/dist` and have the Playwright process itself
serve fixtures via `page.route`/`page.routeWebSocket` rather than proxying to a live server
in a different Bash call. `browser_run_code_unsafe`-style sandboxes have no
`process`/`require`/`import`, only `page`. Capsule sessions load no MCP servers
(`--strict-mcp-config`), so the Playwright case applies to interactive sessions.

## A Sandboxed `git merge` That Aborts Still Leaves the Branch's New Files Untracked (2026-09-10)

**What happened:** a merge-resolver session ran `git merge loom/memory-events` in the main
checkout. Git failed with `unable to unlink old '.gitignore': Device or resource busy` (and the
same for `CLAUDE.md.template`, `commands/distill.md`, `skills/loom-plan-writer/SKILL.md`, plus
`Read-only file system` for `loom-hooks/pre-compact.sh`) and printed `Merge with strategy ort
failed.` HEAD, the index and every tracked file were unchanged, but the ten files the branch
ADDS had already been written as untracked files. The later `git merge --ff-only` aborted with
`untracked working tree files would be overwritten by merge`.

**Why:** the Bash sandbox bind-mounts each individually allow-listed path (`.gitignore`,
`CLAUDE.md.template`, the hook and skill files), so git cannot unlink and recreate them. Ort
checks out new paths before it reaches the busy ones and does not remove them on abort. The
resolver's clean-tree check filtered `??` lines, so the strays went unseen.

**Prevention:** after any failed merge or checkout in the main repo, compare
`git diff --name-only --diff-filter=A <base> <branch>` against the working tree, not just
tracked status. When the branch touches a bind-mounted path, merge in a detached temporary
worktree (`git worktree add --detach <scratch> main`), commit there, and have the user
fast-forward main from outside the sandbox. In the pre-commit hook and cargo, export
`RUSTC_WRAPPER=` (see the sccache entry above).

**Fix:** confirm each stray is byte-identical to the merge commit
(`git hash-object <f>` equals `git rev-parse <commit>:<f>`), remove it, then fast-forward.

## A Stage Sandbox Can Deny Loopback TCP Even While the Server Reports Listening — but Not Always (2026-09-12)

**What happened:** in the settings-lanes stage, `loom status --web 7373` and
`vite --port 5173` both reported listening, but `curl --noproxy '*'` to `127.0.0.1`
returned HTTP 000 from the same Bash command, and headless Chrome rendered an empty
`<html>`. The same day, in the integration-verify stage for the same plan,
`scripts/smoke-web-dashboard.sh` against `loom/target/debug/loom` bound `127.0.0.1` and
every `curl` in it succeeded.

**Why:** loopback TCP denial is a property of that stage's sandbox network policy, not
a fixed platform behaviour — two stages in the same plan, run the same day, saw
opposite results.

**Prevention:** probe loopback with the smoke script (or a plain `curl`) at session
start, before planning work around its absence. Do not treat one stage's denial as a
standing rule for the next stage, or one stage's success as proof a sibling stage can
reach loopback too.

**Workaround when loopback IS denied and a plan step needs the dev server (e.g. a
visual review):** either add a sandbox network allowance for `127.0.0.1`, or render the
built bundle via `file://` with `fetch`/`WebSocket` stubbed — see
[patterns.md](../patterns.md#offline-file-harness-for-a-visual-review-under-a-no-network-sandbox).
`google-chrome`/headless-shell also needs `XDG_CONFIG_HOME`/`--user-data-dir` pointed at
the scratchpad (its crashpad handler writes to `~/.config` and dumps core otherwise),
and the Read tool's worktree guard only opens images inside the worktree — a
screenshot directory under `node_modules/` gets ignored by tools that read images, so
write screenshots inside the worktree proper.

`mkdir -p /tmp/loom-pre-commit-plan` was placed in the plan's `integration-verify` stage `setup:`
list, and `stage_executor.rs` prepends `setup:` with `&&` to _every_ acceptance criterion. Inside
the stage sandbox the `mkdir` failed with `Read-only file system`: the sandbox only binds a
`sandbox.filesystem.allow_write` grant path that already **exists at session start** — a path a
plan step creates for the first time is never bound, `setup:` included. All 10 criteria therefore
failed instantly under the stage-completion and stage-check commands, each printing only `FAILED
[criterion n]` with no stdout/stderr (`acceptance_runner.rs:201-216`), while every one passed when
run by hand outside the sandbox.

**Prevention:** never create a sandbox grant path in a plan's `setup:` (or anywhere else inside a
stage). The operator must create the directory on the host before the run starts, or before the
stage session starts if the plan was already running; a plan's Verified Baseline section should
include one sandboxed check run so this surfaces before execution, not after. This is a different
failure than the bare `mktemp -d` case above — that one produces an empty `HOME`; this one
produces a grant path the sandbox never binds at all.

## A Sandbox Bind Mount Makes `git merge` and `git stash` Fail in an Interactive Session (2026-09-13)

**What happened:** in interactive Claude Code sessions, `git stash` (11:19:50) and `git merge` (13:33:43) in the main checkout failed with `error: unable to unlink old 'agents/loom-codex-forwarder.md': Device or resource busy`. On the `ort` strategy failure, git reset to HEAD and reapplied its auto-stash, rewriting dirty files with the same content and a new mtime. That made the failure look like a content change during a later investigation.

**Why:** a single file in the sandbox's write allowlist, such as `README.md` or `agents/loom-codex-forwarder.md`, is bind-mounted, and git cannot unlink or rename over a mount point.

**Prevention:** before a merge, fast-forward or checkout from a sandboxed shell, check that the incoming change touches no single-file bind (`rg -F <repo> /proc/self/mountinfo`). If it does, run that git step outside the sandbox.

**Workaround when the git step cannot run outside the sandbox (a manual merge-resolver session):** build the merge without `git merge` unlinking anything — `git merge-tree --write-tree main <branch>` computes the merged tree without touching the working tree, `git read-tree <tree>` loads it into the index, `git checkout-index -f` materializes every OTHER changed path, and the bind-mounted path itself is written in place with `git cat-file blob <tree>:<path> > <path>` (a plain redirect over the bind-mounted file, not an unlink/rename). Then `git update-ref MERGE_HEAD <branch-sha>` so the commit records a real merge, resolve any conflicts with `git merge-file --ours/--union` on temp copies when one side should deterministically win, `git add`, `git commit`.

**Fix:** none in loom. The state-confinement merge (2026-09-14) checked its incoming paths against the binds before fast-forwarding main.

## Worktree Test Runs Resolve node_modules From the MAIN Repo When the Worktree Has None (2026-08-11)

**What happened:** in a JS-project worktree stage (cartolyth `city-detail-popup`), codex's proof
command `bunx vitest run …` failed with EROFS writing `node_modules/.vite-temp`, with no test
assertions executed — while the file edits themselves landed fine.

**Why:** a fresh git worktree has no `node_modules` (ignored files are not part of the checkout).
Node module resolution walks UP from the worktree — `.worktrees/<stage>/` → `.worktrees/` → the
MAIN repo's `node_modules/` — so test runners load dependencies from the main checkout and write
their caches there too (vite writes `node_modules/.vite-temp/` while loading config). Both the
codex nested sandbox and the stage sandbox refuse that write, correctly: it lands outside the
worktree, in shared mutable state that parallel stages and the operator's checkout depend on.
Proof it really happens: `node_modules/.vite-temp` exists in cartolyth's MAIN repo, created by a
later unsandboxed run.

**Prevention:** a plan whose stages run JS/TS tests in-session MUST provision dependencies inside
the worktree before the first test run — an explicit first task (`bun install` from the worktree
root) in the stage description. `setup:` does NOT cover this: it only prefixes acceptance
commands, which `loom check` runs on the host after the session's work, not inside the session.

**Fix:** never widen a sandbox toward the main repo's `node_modules` — the denial is the system
working. Install dependencies in the worktree, then re-run the tests.

## macOS Aliases `/tmp` to `/private/tmp` — Canonicalize Both Sides Before Comparing (2026-09-13)

**What happened:** a path comparison in `accepted_loom_bin` (is a binary under a session-writable
root?) missed a match on macOS because one side of the comparison held `/tmp/...` and the other
`/private/tmp/...` — the same path, different spelling.

**Why:** macOS symlinks `/tmp` to `/private/tmp`; a path built from one and a path built from the
other are byte-different strings for the same file.

**Prevention:** canonicalize both sides of any path-equality or path-ancestor check before
comparing, not just one.

**Fix:** applied in the `accepted_loom_bin` comparison.

## `-c core.hooksPath=/dev/null` Silently Overrides a Scoped `core.hooksPath` Read (2026-09-13)

**What happened:** `merge_gate.rs::hooks_dir_prefix` read `core.hooksPath` through loom's own git
runner to find the repository's tracked hooks directory for the merge gate's control-path check. It
always read `/dev/null`.

**Why:** loom's git runner passes `-c core.hooksPath=/dev/null` on every command it runs
(`git/runner.rs`, `NO_HOOKS_ARGS`, owner decision 10, so loom's own git calls never execute a
worktree's hooks). An unscoped `git config --get core.hooksPath` returns the highest-precedence
value across all `-c`, local, global and system sources, so the safety flag it was reading THROUGH
was also the value it read back.

**Prevention:** any loom read of `core.hooksPath` — or any config key loom itself sets with `-c` —
must use a scoped read (`git config --local/--global/--system --get`), which `-c` does not override.

**Fix:** read local, then global, then system scope explicitly and use the first that resolves. A
relative global `core.hooksPath` (e.g. `.githooks`) resolves inside every repository, so a future
cleanup pass should also check that scope, not just local.

## Every Stage Bash Call Fails Under Ubuntu/Pop 26.04's bwrap AppArmor Profile (2026-09-22)

**What happened:** after an upgrade from Pop!_OS 24.04 to 26.04, every Bash call in a stage session failed with exit 1: `apply-seccomp: write /proc/self/setgroups (nested userns is capability-restricted; caller must provide CAP_SYS_ADMIN): Permission denied`. Plain Claude Code sessions were unaffected.

**Why:** 26.04's `apparmor` package ships and enables `/etc/apparmor.d/bwrap-userns-restrict`. It declares a profile named `bwrap` for `/usr/bin/bwrap`, which replaces a hand-written unconfined `bwrap` profile of the same name, and moves every bwrap child to `bwrap//&unpriv_bwrap`, which carries `audit deny capability`. Claude Code's sandbox runs `apply-seccomp` inside bwrap, and that helper creates a nested user namespace; mapping IDs there needs a capability the profile denies. Plain sessions do not hit it because only the stage capsule enables `sandbox` (with `failIfUnavailable: true`).

**Prevention:** reproduce without Claude Code: `bwrap --ro-bind / / --dev /dev --proc /proc --unshare-user --unshare-pid unshare -U -r true` must exit 0, and `bwrap ... cat /proc/self/attr/apparmor/current` must not print `unpriv_bwrap`.

**Fix:** host configuration, root required: link `bwrap-userns-restrict` into `/etc/apparmor.d/disable/`, remove it with `apparmor_parser -R`, then reload the unconfined `bwrap` profile with `apparmor_parser -r /etc/apparmor.d/bwrap`. Loom has no preflight for this yet.

## Nested subprocess timing can crash Loom in the sandbox

**What happened:** A Python subprocess benchmark of loom map exited with SIGABRT and no result, while direct loom map succeeded.

**Why:** The wait-timeout child-process handler panicked on an EPERM write to its signal pipe in the nested sandbox process.

**Prevention:** Run CLI comparisons directly through the command tool and inspect stderr before treating a nonzero exit as a retrieval failure or timing result.

**Fix:** Discard the nested benchmark and use direct commands only if timing is needed.

## In-Tree `allow_write` Entries Became Bind Mounts That Broke Git, Four Times

**What happened:** on 2026-10-01 two merge resolvers in another project failed with `unable to unlink old 'vitest.config.ts': Device or resource busy`; one aborted merge left 11 untracked branch files in the main checkout. The same error had been recorded three times before ("A Sandboxed git merge That Aborts ..." 2026-09-10, "A Sandbox Bind Mount Makes git merge and git stash Fail ..." 2026-09-13, and `phantom-merges.md` 2026-09-19); each entry recorded a workaround and none removed the cause.

**Why:** every session kind received the stage's sandbox config, and each plan `allow_write` entry was emitted verbatim into `allowWrite` and an `Edit(...)` allow. A single listed file inside the working directory became its own bind mount, which git cannot replace. The plan-writer skill told authors to list every file a command writes, lockfile included, which produced those entries.

**Prevention:** when the same failure is recorded a second time, fix the cause instead of adding another workaround entry.

**Fix:** `build_settings` drops in-tree non-glob entries (see [Execution Containment](../architecture/execution-containment.md), "In-Tree allow_write Entries Are Not Emitted"); the plan-writer and pressure-review guidance now say `allow_write` is for paths outside the worktree. Merge resolution no longer runs in the main checkout ([Merge Flow](../architecture/merge-flow.md)).

## The macOS Sandbox Denies `kern.bootsessionuuid`, and `kern.boottime` Was Not a Substitute (2026-10-06)

**What happened:** inside a stage session on macOS the boot identity that `loom subagents watch` leases
persist could not be read (issue #24): the sandbox's `sysctl-read` allowlist has no `kern.bootsessionuuid`.
An outside PR fell back to `kern.boottime` in `lease.rs`.

**Why:** macOS recomputes `kern.boottime` when the wall clock is stepped, so one boot can show two values,
and a lease written under one source and read under the other looks like another boot.

**Prevention:** read host facts a sandbox denies from the unsandboxed daemon and export them, resolving the
export before any local source; never mix two sources for one identity.

**Fix:** the daemon exports `LOOM_BOOT_ID` in each session wrapper and `process::boot_id::resolve_boot_id`
takes it first, then `/proc/sys/kernel/random/boot_id` or `kern.bootsessionuuid`
([Where the Boot ID Comes From](../architecture/owned-waits.md#where-the-boot-id-comes-from)).

## Stages Stranded on the Sandbox's AF_UNIX Denial (2026-10-14)

**What happened:** troofoo stages kept blocking on Unix sockets: Chrome's singleton and postgres's default socket in plan 1, then SeaweedFS `weed filer`/`weed s3` (which always open one) in plan 2's `local-runtime`, after every other criterion had passed. The agent spent a session discovering the denial, and the operator had no way to grant it.

**Why:** the plan-writer skill called AF_UNIX ungrantable but its environment inventory never asked which command opens a socket; plans shipped `allow_all_unix_sockets: false` and sometimes an `allow_unix_sockets` path list that does nothing on Linux; the stage signal never mentioned the denial; and the one setting that lifts it exposed every host socket (docker, D-Bus, gpg-agent).

**Prevention:** the plan-writer inventory asks about sockets (TCP-only option, a per-stage grant, or an operator-run stage); the signal states the socket policy up front; `plan verify` warns on `allow_unix_sockets` on Linux.

**Fix:** a per-stage `allow_all_unix_sockets` grant now hides host sockets and needs `loom run --accept-unix-socket-risk` ([what it still exposes](../concerns/sandbox-and-confinement-gaps.md#what-allow_all_unix_sockets-still-exposes)).

## Seatbelt Denials Read as Build Warnings and Test Failures That Are Environment, Not Code (2026-10-15)

**What happened:** three stages of one plan met the same macOS Seatbelt denials and each had to classify them: `cargo build` printed two warnings per linked target (`output of xcrun while finding MacOSX.sdk`, linker stderr) because xcrun and cc cannot create their cache file under the Darwin user temp dir (errno Operation not permitted); bash process substitution `2> >(...)` failed with `/dev/fd/62: Operation not permitted`, so the `orchestrator::terminal::native::wrapper` tests (`tests.rs:293`, `tests.rs:330`, `tests_exec_env.rs:32`) failed every run; `ps` was blocked, which breaks the spawn guard's `is_ancestor` (a worker brief never appended) and the relay's `parent_pid` (`peer_identity.rs:111`); and a PTY spawn was PermissionDenied (17 `commands::status::web::terminal` tests, `tests_pty.rs:13`), a SIGTERM to a child was not delivered (`reconcile_graph` `cancel_signals`) and `forward_guard` saw the seatbelt session as a stage. The criterion `cargo test --lib commands::status::` and the impact-selected whole-lib run of `loom stage complete` failed on these alone.

**Why:** the sandbox `TMPDIR` does not cover `confstr(_CS_DARWIN_USER_TEMP_DIR)`, and Seatbelt denies `/dev/fd`, process listing and PTY allocation.

**Prevention:** classify by the denial text ("couldn't create cache file", `Operation not permitted`, `PermissionDenied` on a PTY) before treating a warning or failure as a defect, and confirm the failing tests sit outside the modules the diff touches. Run the stage's own modules (`cargo test --lib tokens::`) for evidence, record the environment failures once, and settle early whether a criterion is satisfiable in the sandbox: see [An Integration-Verify Stage Spent 1.5M Subagent Tokens](subagent-orchestration.md).

## Linux CI Had No bubblewrap After Confined Verification Began Requiring It

**What happened:** `56f4d32d` made `verify/criteria/confine_host.rs` fail closed without `bwrap` on
Linux. The Linux test jobs in `.github/workflows/ci.yml` and `.github/workflows/release.yml`
installed only tmux and ripgrep, so 108 lib tests failed with `confined verification requires
bubblewrap (bwrap)`. A Linux-only compile error in the same push hid this for one run.

**Why:** the host requirement changed in code and the CI images did not follow. The macOS jobs
passed because Seatbelt ships with the OS.

**Prevention:** a change that adds a host tool to a tested code path updates every workflow job
that runs those tests, in the same commit.

**Fix:** both Linux test jobs install `bubblewrap` and set
`kernel.apparmor_restrict_unprivileged_userns=0`, because Ubuntu 24.04 AppArmor denies the
unprivileged user namespace `bwrap` creates. With both steps the confined tests run on
`ubuntu-latest`; whether the sysctl alone is required was not isolated.

## GitHub authentication checks under network restriction

**What happened:** A sandboxed `gh auth status` reported an invalid GitHub token while the user confirmed the account was logged in.

**Why:** Network restrictions can prevent credential validation and make the CLI result misleading.

**Prevention:** Retry the required GitHub read with network access before diagnosing credentials or requesting login.

**Fix:** Retry issue retrieval outside the restricted sandbox.

## Knowledge checks must tolerate restricted process operations

**What happened:** `loom knowledge check` aborted in `wait-timeout` after its SIGCHLD notification socket write failed with `EPERM` in the sandbox.

**Why:** Catalog evidence probes invoke Git through `OwnedProcess`; its dependency installed a process-wide signal handler that panicked from a non-unwinding context when socket writes were restricted.

**Prevention:** Bounded subprocess waiting must not require a notification channel that the execution environment can deny. Exercise a child that exits after waiting begins under the restricted sandbox, using an executable preserved from the intended build.

**Fix:** `OwnedProcess::wait` and retirement use deadline-bounded `Child::try_wait` polling, preserving process-group cleanup. A later apparent recurrence came from another checkout overwriting the shared debug executable; its backtrace names that checkout and the old wait implementation. The direct `wait_timeout` use in `verify/criteria/cache_ignore.rs` remains outside this fix.

## Formatter checks can fail only under the isolated test harness

**What happened:** The supervised all-target run passed 8511 tests but failed `freeze_goes_on_when_the_contract_files_are_formatted` and `a_formatted_file_gives_no_problem`. Both passed individually outside the harness on the same compiled test binary.

**Why:** The failure depends on the harness environment or full-suite execution; the exact cause is not isolated. These tests invoke confined Rust formatter commands, while the harness supplies isolated home directories.

**Prevention:** Preserve the failing suite result and distinguish it from standalone reruns. Do not claim a complete suite pass by combining those results.

**Fix:** Record the limitation separately from the verified dashboard and subprocess-wait fixes; investigate formatter availability and confinement in the isolated harness as a separate task.

## Provisioned Bun Installs Fail Before the Stage Can Start

**What happened:** in `../collaterai`, `workspace-scaffold` failed during the guarded frozen Bun install. Several `registry.npmjs.org` downloads returned `ConnectionRefused` even though the saved plan allowed that registry. The plan author had no provisioning-network choice.

**Why:** `orchestrator/provision.rs::run_entry` used the verification command policy. Fresh host verification denies network access, so package provisioning could not fetch missing dependencies; stage domain permissions govern a different sandbox.

**Prevention:** test provisioning through its real runner against a reachable local server and assert that sibling files stay inaccessible. Smoke-test a real frozen Bun install with an empty cache. Keep verification's network-denial regression.

**Fix:** approved provision entries use `run_provision_with_timeout`, retaining environment and filesystem confinement while permitting host networking on Linux and outbound networking on macOS. Bun receives a private cache in the command's temporary directory, removed when the process retires. A real local-server Bun install and the runner regression pass on Linux.

## Sandboxed gh authentication is inconclusive

**What happened:** I treated a sandboxed `gh auth status` failure and an API connection failure as proof that the user was logged out.

**Why:** The shell can lack access to the credential or network available to the user outside the sandbox.

**Prevention:** Recheck `gh auth status` and a read-only GitHub API request with the required permissions before asking a logged-in user to authenticate again.

**Fix:** Use the verified access path for issue creation; if it remains unavailable, report the exact boundary.

## Provision DNS Requires Resolver Symlink Targets

**What happened:** collaterai workspace-scaffold provisioning could not fetch npm tarballs although the registry was allowed and host curl succeeded.

**Why:** Linux provisioning shares networking but binds /etc without /run. On this system /etc/resolv.conf points to /run/systemd/resolve/stub-resolv.conf, leaving the resolver unreadable inside bubblewrap. The installed binary also predates the source change that grants provisioning host networking.

**Prevention:** exercise the real provisioning boundary with a cold cache and preserve resolver symlink targets when granting provisioning network access.

**Fix:** expose only the canonical resolver file read-only during provisioning, retain verification network denial, and install the rebuilt launcher before retrying.

## Shell setup can prevent read-only queries

**What happened:** A Python here-document could not start because the shell could not create a temporary file; optional zsh wildcard paths prevented rg queries when no match existed. A lint log redirection into /tmp also failed under the outer sandbox.

**Why:** Shell preparation can require writes before launching a tool; the outer sandbox can deny /tmp even when the repository is writable. Zsh rejects unmatched globs before invoking rg.

**Prevention:** Use Python -c for short read-only probes, known directory roots for rg, and an allowed diagnostic destination or approved host execution for build gates.

**Fix:** Reissue affected read queries without shell temporary files or unmatched wildcards; run the lint gate with approved host cache/log access.
