# Sandbox Tooling And Network

> Stage-sandbox tool failures: sccache, cargo audit, loopback

## The Sandbox's AF_UNIX Denial Also Kills sccache, Breaking Every Cargo Command (2026-09-04)

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
`process`/`require`/`import`, only `page`.

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
