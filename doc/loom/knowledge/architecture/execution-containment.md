# Execution Containment

> What sandboxed command containment means in loom, its two confinement levels, and what routes through spawn_confined.

## Read This First: What "Containment" Means In Loom

The plan that built this was named "context retrieval and containment", and the
name oversells the second half. **Loom's execution containment is environment
scrubbing. Nothing else.** It is least-privilege hygiene, not a security
boundary.

There is no namespace isolation, no seccomp filter, no landlock, no cgroup, and
no network restriction applied to any command loom spawns. Three independent
proofs, established at the plan's verification gate:

1. An exhaustive `rg 'unshare|CLONE_NEW|netns|seccomp|landlock'` over `loom/src`
   returns only comments and unrelated matches — no syscall, no crate.
2. `verify/criteria/confine.rs` `spawn_confined` has exactly TWO levels (below);
   neither touches namespaces, the filesystem, or sockets.
3. Empirically, `readlink /proc/self/ns/net` is byte-identical between the parent
   and a `sh -c` child — the exact spawn shape used for `CommandSpec::Shell` — so
   the child shares the host network namespace.

**Consequence for plan authors:** do not write an acceptance criterion that
presumes a containment level loom never implemented. "Prove an outbound
connection is denied" cannot be satisfied here, and the honest verdict for such
an item is "did not run", not "the host could not provide it".

**There is also no `network: none` syntax for a spawned command.**
`models/stage/types.rs:340` `NetworkConfig` carries `allowed_domains`,
`additional_domains`, `allow_local_binding` and `allow_unix_sockets`, and an
empty `allowed_domains` does mean "no network allowed" — but that config is only
emitted into the Claude Code `settings.json` sandbox for the agent **session**.
It never reaches `spawn_confined`, so a plan-authored command is not
network-restricted by loom at all.

## The Two Confinement Levels

`CommandConfinement` (`models/stage/types.rs:255-263`, serde `kebab-case`):

| Level | YAML | What it mechanically does |
| --- | --- | --- |
| `Confined` | `confined` (**default**) | `command.env_clear()`, then re-adds only the variables on the host allowlist, via `crate::process::apply_stage_environment` |
| `Inherit` | `inherit` | no-op — the child gets loom's ambient environment. Explicit plan opt-in only |

Configured plan-wide as `command_confinement` (`plan/schema/types.rs:52`,
defaulted) and overridable per stage as `command_confinement`
(`models/stage/types.rs:305`, `Option<_>` — unset means the plan-level value
applies). `confine.rs` owns the policy half too: `resolve_confinement` and
`plan_confinement` answer "which level applies to this stage?" so no caller
reimplements the precedence.

## What Goes Through `spawn_confined`

`spawn_confined` is the **single leaf primitive** for the whole family of
plan-authored commands: every acceptance criterion, setup command, truth check,
wiring test, dead-code check and change-impact command in a loom plan becomes a
process through it (`verify/criteria/confine.rs:1-14`).

The rationale is worth keeping: plans are trusted artifacts, but **trusted is not
privileged**. A plan line should not be able to read `GITHUB_TOKEN`, `AWS_*` or
`ANTHROPIC_API_KEY` merely because loom happened to be started from a shell that
had them.

## The Host Environment Allowlist

`process/environment.rs:14-59`, `STAGE_HOST_ENV_ALLOWLIST`. Allow-only; anything
absent is dropped. Forwarded:

- `HOME`, `PATH`
- `CARGO_HOME`, `RUSTUP_HOME` — toolchain *locations*, not credentials. Usually
  absent (both default under `HOME`), but CI images that relocate them leave
  `cargo` unable to find its registry without them.
- Locale/terminal: `LANG`, `LC_ALL`, `LC_CTYPE`, `TERM`, `COLORTERM`,
  `TERM_PROGRAM`, `SHELL`
- Display/session: `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`,
  `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR`
- tmux: `TMUX_TMPDIR`, `TMUX`, `TMUX_PANE`; plus `TMPDIR`
- Proxies, both cases: `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`, `ALL_PROXY`,
  `http_proxy`, `https_proxy`, `no_proxy`, `all_proxy`
- CA bundle *locations*: `SSL_CERT_FILE`, `SSL_CERT_DIR`, `NIX_SSL_CERT_FILE` —
  paired with the proxy vars, because a host behind a corporate MITM proxy
  usually also needs a custom CA bundle or the TLS handshake fails.

**Deliberately withheld:** `SSH_AUTH_SOCK`. It is a live credential-agent socket,
not a location, so an acceptance criterion needing SSH auth fails by design
rather than silently inheriting host agent access.

The governing principle is **locations yes, live credentials no**. The list must
also carry enough for a build toolchain to find itself — an acceptance criterion
that cannot run `cargo` fails the stage just as loudly as a real defect.

## Honest Limits

A `Confined` command **cannot** read an ambient environment variable outside the
allowlist — that is the entire guarantee. It **can** still:

- open arbitrary outbound network connections (shares the host network namespace);
- read and write any path the invoking user can, including outside `allow_write`;
- connect to any Unix socket on the host;
- signal or inspect other processes owned by the user;
- reach `org.freedesktop.secrets` and the X11 session via the forwarded
  `DBUS_SESSION_BUS_ADDRESS` and `XAUTHORITY`.

That last point is a real inconsistency, not a hypothetical:
`DBUS_SESSION_BUS_ADDRESS` is a live credential surface, which is the same
argument used to withhold `SSH_AUTH_SOCK`. Root cause: **one allowlist serves two
consumers with different needs** — the terminal spawner genuinely needs
display/session variables, `spawn_confined` does not. See `concerns.md`.

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

## Package-Manager Caches Are Granted To Every Stage

`sandbox::PACKAGE_MANAGER_CACHE_WRITE_PATHS` (`sandbox/package_caches.rs`) lists the
per-user cache directories of bun, npm, pnpm, yarn, deno, cargo, rustup, uv, pip and
go, in tilde form. It is emitted into `sandbox.filesystem.allowWrite` on TWO
surfaces: `sandbox/settings/policy.rs::filesystem_settings` for every worktree
stage's settings (order: plan `allow_write` entries, then the package caches,
then codex's own state paths when that lane is licensed), and
`fs/permissions/codex_sandbox.rs::ALLOWANCES` for the MAIN repo's
`.claude/settings.local.json` on `loom init`/`loom repair`.

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
| `loom/src/models/stage/types.rs:255` | `CommandConfinement`; `:340` `NetworkConfig` |
| `loom/src/plan/schema/types.rs:52` | plan-level `command_confinement` |
| `loom/src/sandbox/settings.rs` | per-stage session sandbox emission |
| `loom/src/orchestrator/terminal/native/wrapper.rs:181` | the **second**, diverging copy of the allowlist (see `concerns.md`) |
| `loom/src/verify/criteria/tests/confine_tests.rs` | the matched-control test pattern |
