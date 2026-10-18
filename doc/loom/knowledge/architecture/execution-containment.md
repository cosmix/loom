# Execution Containment

> Sandboxed command containment, limits

## Read This First: What "Containment" Means In Loom

`CommandConfinement::Confined` prepares an OS boundary in `verify/criteria/confine_host.rs` as well as rebuilding the child environment. Fresh host verification commands use macOS Seatbelt or Linux bubblewrap, allow selected runtime/toolchain reads and worktree writes, and deny network access. Missing containment support is an error.

The writable tree and the working directory are separate inputs: a command starts in its acceptance or package directory but binds the whole stage worktree root, so monorepo siblings, root `node_modules` and editable path dependencies stay visible. Callers pass the root explicitly (`CriteriaConfig::with_tree_root`, `run_spec_in_optional_tree`), never inferred from a `.git` walk; a working directory outside the tree is refused. With no tree (provisioning, knowledge stages, `spawn_confined`) the working directory is the tree.

Approved plan provision entries use `run_provision_with_timeout`: the same filesystem and environment boundary, with host networking on Linux and outbound networking on macOS. Linux provisioning binds the canonical target of a symlinked `/etc/resolv.conf` read-only before remounting the sandbox root, preserving DNS without exposing host sockets.

Private per-command scratch stays outside the working tree so recursive linting does not traverse Cargo cache symlinks. Both acceptance and provisioning direct temporary files, Bun, uv, and XDG caches into this scratch, granted writable as one exact path. Selected installed Bun/uv caches and uv Python runtimes remain read-only; the real home stays protected. Cargo registry/git sources are linked read-only into the private Cargo home. Owned process retirement releases scratch. See [the Biome traversal regression](../mistakes/sandbox-and-settings.md#verifier-scratch-participates-in-project-lint-traversal).

The confined command preparer derives `LOOM_BIN` from the running executable, prepends its directory to `PATH`, and exposes only that binary as a read-only runtime path. Ambient `LOOM_BIN` cannot select another binary. The containment cache policy revision is `host-os-boundary-v7`; randomized scratch path arguments are normalized for stable cache identity.

On macOS, an already sandboxed stage can reuse its inherited capsule only through `daemon/inherited`: kernel sandbox evidence, protected daemon and stage/session/capsule records, authenticated socket peer identity, live process ancestry and reciprocal stage ownership must agree. Environment strings and tokens alone cannot authorize inheritance. Session settings grant the exact daemon socket; existing sessions without that grant require relaunch.

Inherited execution retains approved capsule permissions, including network/shared-cache grants. Fresh host verification uses the stricter command policy. Stage `NetworkConfig` controls the session capsule, not a per-command network override. `ConfinedChild` and the criterion executor retain resources until owned-process retirement.

## The Two Confinement Levels

`CommandConfinement` lives in `models/stage/sandbox_config.rs` and uses kebab-case YAML.

| Level | YAML | Behavior |
| --- | --- | --- |
| `Confined` | `confined` (default) | Rebuild the allowlisted environment and apply the host OS boundary or authenticate an existing macOS stage boundary. |
| `Inherit` | `inherit` | Explicitly retain the invoking environment and containment. |

The plan sets `command_confinement`; a stage may override it. `resolve_confinement` and `plan_confinement` centralize precedence.

## What Goes Through `spawn_confined`

Plan-authored acceptance, setup, truth, wiring and dead-code commands use the shared command preparation and criterion executor in `verify/criteria`. `spawn_confined` exposes an owned `ConfinedChild`; executor paths retain `PreparedCommand` resources while managing process retirement and bounded output. Environment allowlisting prevents ambient credentials from becoming plan privileges.

## The Host Environment Allowlist

Three allow-only lists decide which host variables reach which process; anything absent is dropped.
The principle is **locations and login identity yes, live credentials no**. In `process/environment.rs`,
`STAGE_HOST_ENV_ALLOWLIST` serves `apply_stage_environment` (`spawn_confined`, the native spawner's
`spawn_in_terminal`, the tmux server commands) and `AGENT_SESSION_ENV_NAMES` serves the stage
wrapper's `exec env -i` list; `HOST_ENV_ALLOWLIST` in `daemon/server/environment.rs` serves the daemon
child ([Launching the Daemon](daemon-launch.md)).

`STAGE_HOST_ENV_ALLOWLIST` forwards `HOME`, `PATH`; `USER` and `LOGNAME` (they name the operator, and
on macOS the `claude` CLI finds its Keychain login by `$USER`, so a session without it reads "Not
logged in"); `CARGO_HOME`, `RUSTUP_HOME` (locations CI images relocate); `SCCACHE_DIR`,
`SCCACHE_CACHE_SIZE` (inert without a selected wrapper, whose policy is in
`orchestrator/terminal/native/build_cache.rs`; `RUSTC_WRAPPER` is not forwarded); `LANG`, `LC_ALL`,
`LC_CTYPE`, `TERM`, `TERMINFO`, `TERMINFO_DIRS`, `COLORTERM`, `TERM_PROGRAM`, `SHELL`; `DISPLAY`,
`WAYLAND_DISPLAY`, `XAUTHORITY`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR`; `TMUX_TMPDIR`, `TMUX`,
`TMUX_PANE`, `TMPDIR`; the proxy variables in both cases (`HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`,
`ALL_PROXY`); and the CA bundle locations `SSL_CERT_FILE`, `SSL_CERT_DIR`, `NIX_SSL_CERT_FILE`.
`TERMINFO*` pair with `TERM`: without the capability database the name is unresolvable (kitty), and
a tmux probe on it exits non-zero like "the server is not accepting clients".

`AGENT_SESSION_ENV_NAMES` is the **single source for the wrapper**: `wrapper/script_text.rs` renders
its shell loop with `AGENT_SESSION_ENV_NAMES.join(" ")`, and the wrapper writes `HOME` and `PATH`
itself (`PATH` falls back to `/usr/bin:/bin`). It holds the locale, terminal, display, session and tmux
names, `TMPDIR`, `SCCACHE_DIR`, `SCCACHE_CACHE_SIZE`, `USER` and `LOGNAME`; the proxy, CA bundle,
`CARGO_HOME` and `RUSTUP_HOME` names are not forwarded to sessions
([Three Stage Environment Allowlists](../concerns/sandbox-and-confinement-gaps.md#three-stage-environment-allowlists)).
Tests require every session name in `STAGE_HOST_ENV_ALLOWLIST` (the half-fix that left `USER` unset
on the tmux and native spawn paths) and in the daemon list.

**Withheld from every stage list:** `SSH_AUTH_SOCK`, a live credential-agent socket, so a criterion
needing SSH auth fails by design. The list must still let a build toolchain find itself: a criterion
that cannot run `cargo` fails the stage as loudly as a real defect.

## Honest Limits

Fresh host containment and authenticated inherited containment have different permissions. Inheritance preserves the stage capsule, including its approved network and package-cache grants. `Inherit` is an explicit opt-in to the caller environment/boundary.

The policy is OS-specific and permits the runtime/toolchain paths needed to execute commands. It is not a CPU/memory quota or an immutable-input snapshot. Generated dependencies can remain ambient, and before/after verification snapshots do not detect transient mutate-and-restore behavior. Boundary tests must exercise actual launchers with readable/writable outside canaries and report platform skips; unit tests of environment construction alone do not prove confinement.

## Sandbox Settings Emission — `Edit(path)`, Never `Write(path)`

`sandbox/settings.rs` generates the per-stage Claude Code `settings.json` that
bounds the agent **session** (a different mechanism from `spawn_confined`, which
bounds plan-authored commands — do not conflate them).

**Claude Code's file permission check consults only `Edit(path)`.** A `Write(path)`
rule — allow or deny — is inert. `settings.rs:240-244` carries an explicit
`IMPORTANT` comment recording this, and `settings.rs:181` pushes an `Edit(...)`
rule for the handoffs directory. Loom's generated stage settings are therefore
clean of the inert form.

Two things not to "fix" without reading the reasoning first:

- **Deny beats allow.** A blanket `Edit` deny paired with a narrower `Edit` allow
  blocks the directory the session needs. Scope carefully.
- **Pre-existing user rules are carried forward verbatim on purpose**
  (`settings.rs:399-411`), and a test at `settings.rs:1221` asserts that a
  user-authored `Write(~/.bashrc)` **survives**. Inert user rules are kept
  deliberately; stripping them would break a passing test and silently discard
  the developer's own configuration. This is the opposite policy from loom's own
  emitted rules, and both are correct.

`allow_write` rules also have parent traversal filtered out at the emitter, which
is what actually closes the path-escape hole at the point of use.

## Sandbox-List vs Permission-Rule Path Syntax, and Where Each Resolves From (2026-09-13)

Two settings surfaces read a leading `/` differently, confirmed against the Claude Code docs
(<https://code.claude.com/docs/en/settings-reference.md#sandbox-path-prefixes>, <https://code.claude.com/docs/en/permissions.md#read-and-edit>):

- **`sandbox.filesystem` lists** (`allowWrite`, `denyWrite`, `denyRead`, ...): `/p` and `//p` are
  both absolute, `~/p` is under home, and a bare path is relative to the project root (to
  `~/.claude` in user settings).
- **`Edit`/`Read`/`Write`/`NotebookEdit` permission rules**: `//p` is absolute, `~/p` is under home,
  a bare path is cwd-relative, and a single leading `/` is relative to the SETTINGS SOURCE — the
  directory of the `--settings <file>` that recorded it, which for a capsule is `W/capsules/`.

`sandbox/config.rs::expand_paths`'s doc comment used to claim Claude Code prepends the project root
to an absolute sandbox path. It does not, and the comment is corrected in place — a bare (no-slash)
sandbox path is project-relative, but a `/`-prefixed one is already absolute, matching the docs
above. The docs suggest an OLDER Claude Code version read a single-slash sandbox path as
project-relative, which is why some older loom code favors `./path` there.

Two more facts from the same pass:

- On Linux and WSL2, a `denyWrite` entry for a path that does not exist yet is still enforced: the
  sandbox mounts a 0-byte read-only placeholder for it while the sandboxed command runs.
- `--setting-sources` that excludes a settings source drops that source's `sandbox.filesystem`
  entries, `Edit` rules and `Read` denies from the sandbox — not just its `permissions.*` block.

Open: how a `/`-prefixed permission or sandbox path resolves in a `--settings` file OUTSIDE the
project is unconfirmed. Live checklist item.

Also unconfirmed: whether Claude Code resolves settings only from the session's start directory, so
a nested `x/.claude/` several levels below the worktree root would never be read (cited:
`anthropics/claude-code#74023`, `#37344`, `#12962` — not independently verified against the
installed bundle). Loom passes `--settings <file>` explicitly on every spawn, so root-level gating
does not depend on the answer either way.

## A Denied Missing Path Shows Up in the Session's `git status` (2026-09-13)

Inside a Claude Code Bash sandbox, a write-denied path that does not exist is held by a read-only
bind mount of `/dev/null`. Observed at `<cwd>/.mcp.json`: `stat` reports a character special file
(size 0, owner `nobody`, mtime of the device node), and `/proc/self/mountinfo` lists it as a
`devtmpfs udev` mount. On the host the mount point is an empty regular file that exists while a
sandboxed command runs and is removed afterwards
([Claude Code Grants a Linked Worktree Its Whole Git Common Directory](#claude-code-grants-a-linked-worktree-its-whole-git-common-directory)).

Consequences for a stage session, which runs git inside its sandbox:

- `git status` lists the mount point as untracked (`?? .mcp.json`), so an agent checking that the
  tree is clean before `loom stage complete` sees noise.
- `git add` of a character device fails, which is one more reason the git-add guard forbids `-A`
  and `.`.
- The daemon's own git runs outside the sandbox; between sandboxed commands it sees no entry, and
  during one it can see an empty file.

Do not commit or delete such an entry; confirm it with `stat -c %F <path>` first. The same
mechanism explains the 0-byte placeholders the Claude Code docs describe for missing `.claude`
settings files (sandboxing.md, Troubleshooting).

## Confinement E2E Lives Outside the Sandbox (2026-09-13)

Status (2026-09-14): passed outside the sandbox. The operator ran it with
`LOOM_TEST_REQUIRE_SANDBOX_FREE=1` and a PATH shim for `srt` (`exec bunx @anthropic-ai/sandbox-runtime "$@"`).
The srt tests are `#[serial]`: when four ran in parallel, two srt CLI instances died with an uncaught
Node.js exception, and the exact error was never captured. Loom's parallel stages each run their own
Claude Code sandbox and have not shown this. Treat it as an srt CLI concurrency issue until proven
otherwise, and keep the probes' sentinel rule (`mistakes/verification-harness.md`, "A Must-Fail Probe
That Counts Any Non-Zero Exit Passes When the Harness Never Started").

`orchestrator/terminal/native/tests_confinement_e2e.rs` and its `srt`-backed sibling
`tests_confinement_srt.rs` (loaded via `#[path]`, registered from `native/launch.rs`), plus
`tests/integration/confinement_status.rs`, exercise the OS-level sandbox denies end to end. Run with
`env -u LOOM_WORK_DIR LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test confinement` outside the Claude
Code Bash sandbox: `srt` (`@anthropic-ai/sandbox-runtime`, runnable via `bunx`) cannot run INSIDE
that sandbox because it binds a Unix socket (`srt-mux-*.sock`) and AF_UNIX is blocked there.

`srt` behavior worth knowing when reading a probe result: a denied path that does not exist yet
inside an otherwise-writable directory is mounted from `/dev/null`, so a write to it exits 0 and
lands nowhere — `bwrap` creates an empty host mount-point file for it and `srt` removes that file on
exit. A refusal against a fake-home path or `.git/hooks`/`.git/config` does not by itself prove a
capsule rule is doing the work; both are denied by `srt`'s own built-in defaults regardless of
loom's rules.

## Package-Manager Caches Are Granted To Every Stage

`sandbox::PACKAGE_MANAGER_CACHE_WRITE_PATHS` (`sandbox/package_caches.rs`) lists the
per-user cache directories of bun, npm, pnpm, yarn, deno, cargo, rustup, uv, pip and
go, in tilde form. It is emitted into `sandbox.filesystem.allowWrite` of every
session capsule through `sandbox/settings/policy.rs::filesystem_settings` (order:
plan `allow_write` entries, then the package caches, then codex's own state paths
when that lane is licensed). The capsule is their only carrier: nothing merges them into
the main checkout's `.claude/settings.local.json`.

**Cache-only policy.** Only cache directories are listed, never a
credential-bearing parent — `~/.cargo/registry` and `~/.cargo/git` are granted,
`~/.cargo` as a whole is not (`~/.cargo/credentials.toml` lives there); same
reasoning excludes `~/.rustup`, `~/.bun`, `~/.yarn`, `~/go` as whole directories.

**Two limits, same as any `allowWrite` entry:** (1) a cache directory that does
not exist on the host at session start is skipped by the sandbox, not created —
a manager used for the first time on that machine still fails until the
directory exists; (2) a cache relocated by an env var (`XDG_CACHE_HOME`,
`CARGO_HOME`, `BUN_INSTALL_CACHE_DIR`, `UV_CACHE_DIR`, ...) is not covered and
needs an explicit plan `allow_write` entry.

**Detection rule:** `EROFS` / `Read-only file system` from a package manager
inside a stage means one of those two limits, not a code bug — check whether the
cache dir exists on the host, and whether an env var relocated it, before
assuming the grant is missing.

## The Test Pattern That Makes A Boundary Test Able To Fail

This is the most reusable thing the containment work produced, and it belongs on
every future boundary test in this repo.

`verify/criteria/tests/confine_tests.rs` ships a **matched positive and negative
control**: `confined_shell_command_does_not_see_ambient_secret` **and**
`inherited_shell_command_does_see_ambient_secret`. The pair distinguishes "the
scrub works" from "the canary was never set" — which a single negative assertion
cannot do. `process/environment.rs:92` does the same at unit level by actually
exec'ing `/usr/bin/env` and asserting the canary string is absent from real child
output, rather than inspecting a `Command` struct.

**Rule: a boundary test needs the inherit/allow case asserted alongside the deny
case, or it cannot fail when the boundary silently stops applying.** See
`mistakes/tests-that-cannot-fail.md` for the counter-example this plan also
produced.

## Entry Points

| Path | Why |
| --- | --- |
| `loom/src/verify/criteria/confine.rs` | `spawn_confined`, `resolve_confinement`, `plan_confinement` — start here |
| `loom/src/process/environment.rs` | the allowlist and `apply_stage_environment` |
| `loom/src/models/stage/sandbox_config.rs:17` | `CommandConfinement`; `:100` `NetworkConfig` |
| `loom/src/plan/schema/types.rs:52` | plan-level `command_confinement` |
| `loom/src/sandbox/settings.rs` | per-stage session sandbox emission |
| `loom/src/orchestrator/terminal/native/wrapper.rs:181` | the **second**, diverging copy of the allowlist (see `concerns.md`) |
| `loom/src/verify/criteria/tests/confine_tests.rs` | the matched-control test pattern |

## Missing Grants Are Reported Before the Session Starts (2026-09-13)

A plan or stage `allow_write` path that does not exist on the host when a session starts is not
bound, and a `mkdir` inside the session cannot create it: the directory it would create is the one
the sandbox skipped, so the call fails with `Read-only file system`. `sandbox::missing_grant_paths`
(`sandbox/grant_paths.rs`) resolves each absolute or `~/` entry, skips globs and relative entries,
and returns the ones missing on the host. Both consumers read the merged plan and stage grants:

- STALE (corrected 2026-09-13): this named `write_required_sandbox_settings`
  in the former `sandbox_grants` module, which the state-confinement plan's B1 phase deleted along
  with the rest of that module. The warning now comes from `sandbox::warn_missing_grants`
  (`sandbox/grant_paths.rs:64`), called from `orchestrator/core/spawn_setup.rs:139` and
  `orchestrator/core/stage_executor.rs:654`, one per missing path at spawn, for worktree and
  knowledge stages alike.
- The stage signal lists them under "Missing on the host, so NOT writable this session" with a
  stop-and-report instruction, whether or not the stage has deny rules
  (`missing_allow_write_from_merged` in `orchestrator/signals/generate.rs`).

The remedy is always on the host: create the path, then restart the stage's session.
Package-manager cache paths are not checked here; their own signal note covers them.

## `loom status` Marks the Caller's Own Executing Stage "Orphaned" Inside Its Own Sandbox (2026-09-14)

Running `loom status` FROM INSIDE a stage's own sandboxed session reports that stage's session as orphaned/dead (`session_alive` false, `render/graph.rs:133-137`), even though it is the live session asking the question. **Why:** bubblewrap's PID namespace hides the host PID from the sandboxed process, so the liveness check (which compares against a host PID) cannot see its own process as alive. This is a sandbox artifact of the caller inspecting itself from inside its own namespace, not evidence the session actually died — treat a self-reported "orphaned" from inside a stage sandbox as uninformative, never as a signal to intervene.

**Boundary-aware tests:** macOS capsules grant the exact daemon socket for authenticated inherited-confinement requests; sessions started without that grant require relaunch. Unix-socket availability on other sandbox paths depends on their emitted policy, so tests must report the actual boundary they exercise. A live companion, local TCP listener or tmux server can still require host execution. Use focused transport/route fixtures where the caller sandbox prevents the real boundary, and do not count a skipped boundary test as successful execution.

## macOS: the stage sandbox refuses a nested Seatbelt, so the wrapper runs codex exec directly (2026-09-02)

**Symptom.** Five `loom-codex-forwarder` spawns in a stage session each reached gpt-5.6-terra and the
companion exited 0, but zero files were written: every shell command codex ran, even `pwd`, died with
`sandbox-exec: sandbox_apply: Operation not permitted`. The 67b97114 state-root redirect (previous
section) had worked; this failure sits one layer below it.

**Cause.** Stage Bash calls already run inside Claude Code's own Seatbelt sandbox on macOS. Codex's
`workspace-write` and `read-only` modes wrap each command it runs in `sandbox-exec` too, and macOS
refuses a second profile on an already-sandboxed process. Codex still exits 0 when the model's turn
ends, whatever its tools did.

```bash
sandbox-exec -p '(version 1)(allow default)' /bin/pwd                # sandbox_apply: Operation not permitted, rc 71
codex sandbox -- /bin/pwd                                            # same error
codex sandbox -c sandbox_mode="danger-full-access" -- /bin/pwd       # prints the cwd
```

**Why no config knob helps.** The companion hardcodes
`sandbox: request.write ? "workspace-write" : "read-only"` (`codex-companion.mjs:491`) into
`thread/start`, overriding `~/.codex/config.toml`'s `sandbox_mode` with no flag or env override, and
`read-only` seatbelts too. `dangerouslyDisableSandbox` is refused by the auto-mode classifier; there is
no macOS equivalent of Linux's `exclude_slash_tmp` — the nesting itself is refused.

**Fix.** `loom-hooks/codex-forward.sh` probes `sandbox-exec -p '(version 1)(allow default)' /usr/bin/true`
(PATH lookup, so tests can stub it) and, only when refused, bypasses the companion and runs `codex exec
--sandbox danger-full-access --skip-git-repo-check --model <model> -c
model_reasoning_effort="<effort>" -- "<preamble + task>" </dev/null`. The `</dev/null` is required;
see "Direct-lane runs" below. The outer stage sandbox — worktree plus granted write paths,
domain allowlist, credential read denies — remains the boundary, same as a sonnet subagent's Bash call.

The evidence trailer now always carries `exit:` and `mode:`. `mode: companion` lists the newest
`state/*/jobs/*.json` records, globbed from the state root the companion actually used (including the
redirected `~/.codex/plugin-data` — the earlier wrapper globbed the original root and printed `jobs:
none found` on redirected machines). `mode: direct` lists the `session:` rollout path
(`~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`) that `codex exec` writes.
`signals/format/codex.rs` accepts either.

| Platform | Where | Lane | Inner sandbox |
| --- | --- | --- | --- |
| Linux | inside a stage sandbox | companion | bubblewrap, nested; needs `exclude_slash_tmp` |
| macOS | inside a stage sandbox | direct `codex exec` | none |
| macOS | outside any sandbox | companion | codex's own Seatbelt applies |

**Given up, deliberately.** No companion job record, so `/codex:status`/`/codex:result` cannot see
direct runs. Codex's Seatbelt no longer masks `.git` on macOS, so the preamble's no-git rule and the
orchestrator's post-run `git status --short` are the remaining guards. Codex's inner network cut-off is
gone, but the stage sandbox's domain allowlist still applies.

See [Codex Lane Rogue Wrapper](../mistakes/codex-lane-rogue-wrapper.md) for the verification gap that
let this ship.

## Claude Code Grants a Linked Worktree Its Whole Git Common Directory

Measured against Claude Code 2.1.286 (its bundle, and live `claude -p` runs in a scratch
repository), these sandbox rules decide what loom's capsule can and cannot express on Linux:

- **The git-dir grant.** For a session whose cwd is a linked worktree, Claude Code adds the whole
  git common directory to `allowWrite` itself. Its detector reads the worktree's `.git` file,
  requires the `gitdir` to lie in `<common dir>/worktrees/` with a back-pointer naming the
  worktree, and adds the common directory; it then denies `hooks`, `config`, `config.lock`,
  `config.worktree`, `commondir`, `objects/info/alternates`, `objects/info/http-alternates` and
  each `worktrees/*/{config.worktree,commondir}`. No setting turns the grant off. A session whose
  `.git` is a directory gets `.git/hooks` and `.git/config` denied instead.
- **Bind order.** Every `allowWrite` entry is bound first and every `denyWrite` entry after, so a
  deny always wins and an allow nested inside a deny never reopens it. `allowRead` reopens a path
  inside a `denyRead` directory; nothing reopens a write deny.
- **Globs.** `allowWrite` and `denyWrite` glob entries are skipped on Linux. `denyRead` globs are
  expanded into one mount per match, with a warning above 256.
- **Absent paths.** A `denyWrite` path that does not exist is bound from `/dev/null` (or an empty
  directory at its first missing ancestor). bwrap creates the mount point on the host filesystem
  when the parent is a host directory bound into the sandbox, so host processes see an empty
  regular file there while each sandboxed command runs; Claude Code removes it afterwards (a killed
  sandbox can leave it). A `denyRead` path that does not exist mounts nothing.
- **Reads.** A `denyRead` file is bound from `/dev/null`; a directory is covered by a tmpfs with
  the `allowRead` and `allowWrite` paths inside it re-mounted.
- **Environment.** A top-level `env` in a `--settings` file reaches Bash commands; Claude Code
  sets `TMPDIR` per command to `<CLAUDE_CODE_TMPDIR or /tmp>/claude-<uid>`.
- **srt.** `@anthropic-ai/sandbox-runtime` 0.0.78 shares this bind logic but not the git-dir
  grant, so an srt test of a linked worktree must add the common directory to `allowWrite` itself.

What this exposes, and why no deny-list can narrow it, is in
[Agent Rule-Bending Hardening](../concerns/agent-rule-bending-hardening.md), G2.
Loom's own `config.worktree` deny and check: [Loom's `config.worktree` Deny and Check](security-and-isolation.md#looms-configworktree-deny-and-check).

## In-Tree `allow_write` Entries Are Not Emitted

`sandbox::build_settings` (`sandbox/settings.rs`) drops every non-glob `allow_write` entry whose resolved path is the session's working directory or inside it, so it reaches neither `sandbox.filesystem.allowWrite` nor an `Edit(...)` allow. The predicate is `sandbox/grant_paths.rs::is_inside_cwd`: `~/`, `/` and `//` entries resolve as absolute, a bare entry against the cwd, and both sides through symlinks, so an entry reaching through a link to a target outside the cwd (a worktree's `.loom/work/...`) is kept; globs (`*`, `?`, `[`, `{`) and entries that fail to resolve are kept too. `SettingsTarget::cwd` carries the working directory, and the session capsule (`orchestrator/terminal/native/session_settings/contents.rs::capsule_settings`) always sets it. `loom plan verify` warns on a relative non-glob entry outside `.loom/` and `.work/` (`plan/schema/validation/v2_lints/sandbox_capability.rs::report_in_tree_grants`).

Why: the cwd is writable already, so the entry grants nothing, while Claude Code's Linux sandbox bind-mounts every listed non-glob path on its own and the kernel refuses to unlink or rename a mount point: git replacing the file, or `rm -rf` of a listed directory, fails with `Device or resource busy`. Deny rules still win, so effective permissions do not change. One side effect: under `permission_mode: default` a dropped exact-path entry no longer yields an `Edit(...)` allow, so a file-tool edit there can prompt; a glob keeps its rule.
