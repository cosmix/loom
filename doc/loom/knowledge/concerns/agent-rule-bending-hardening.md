# Agent Rule-Bending Hardening

> Checks an agent can bend, and the hardening backlog

## Why This Entry Exists (2026-09-16)

A review put the claim: *"some operations are forbidden if `LOOM_*` env vars are set, and an agent
is smart enough to unset them, so loom has no security."* Examined against the tree at base
`7504369c` on 2026-09-16.

The claim is accurate for one of three enforcement classes and wrong about the other two. The
classes are below, then the gaps the review actually turned up. **No incident is reported here** —
this is a read of the code, and every item states what was verified and what was not.

## The Three Enforcement Classes

| Class | Where the check runs | Whose environment it sees | What it can be trusted for |
| --- | --- | --- | --- |
| 1 | The loom CLI, invoked from a Bash tool call | The agent's shell — agent-controlled | Routing, error messages |
| 2 | A hook script | Claude Code's process — not agent-controlled, but the hook reads agent-authored command text | Workflow, accidents, deterrence |
| 3 | The daemon, or `loom hook relay` inside a hook | Irrelevant: identity comes from process ancestry and file ownership | Authority |

### Class 1 — env read in the agent's own shell

Every loom CLI call an agent makes reads its identity from its own process environment:
`commands/stage/control_session.rs:124-131` (`WrapperEnv::from_process_env`), `relay/emit.rs:68-75`,
`commands/memory/handlers/record.rs:120`, `commands/handoff/create.rs:287-299`,
`commands/subagents/render.rs:61`. `env -u LOOM_STAGE_ID loom …` changes what all of them see. The
review's claim holds here exactly.

It buys nothing, because none of them grants a capability. `route_control_session`
(`commands/stage/control_session.rs:161-209`) uses the variables to choose *between* the broker
route and the in-process route; unset them and the in-process route is taken, which then fails
writing `.loom/work` because the capsule mounts it read-only. **Env selects a route; the sandbox
and the ancestry checks grant the right.**

### Class 2 — hook process

Hooks are spawned by the Claude Code process and inherit *its* environment — the wrapper script's
exports — not the Bash tool's shell. An `unset` inside a tool call mutates one shell and dies with
it; the PreToolUse hook inspecting that same call still sees the real values.

Hooks also run *outside* the OS sandbox. Internal proof: `loom hook relay` writes `W/inbox/`, which
the session's own capsule denies — if hooks ran under the capsule the relay could not function at
all.

The reachable attack on class 2 is the settings `env` block, which overrides process env (see
[Session Identity Env](../mistakes/session-identity-env.md)). It is closed by
`fs/permissions/settings.rs::scrub_session_identity_env` and by every capsule denying `.claude` in
both layers (`sandbox/control_surfaces/session_denies.rs:89-103`, `sandbox/control_surfaces.rs:56-69`).

What remains is command-text matching, which is bypassable and documented as such —
`loom-hooks/commit-filter.sh:12-28`: *"command substitution that builds `git` from pieces, `$IFS`
tricks, `base64|sh`, or spawning git from a child interpreter… The DURABLE guarantee is
architectural."* Its anti-evasion guard covers two spellings of the gate unset
(`gate_var_unset_intent`, `commit-filter.sh:179-190`) and bare `eval`.

**Observed gap in that guard:** nothing checks `git commit --no-verify`, which skips the installed
pre-commit hook (`loom-hooks/git-pre-commit-hook.sh`, the staged-path check) outright.

### Class 3 — trusted host

- `daemon/server/peer_identity.rs:157-185` — `SO_PEERCRED` pid, a pid-file start-time match so a
  recycled pid cannot stand in, then an ancestry walk. Fails closed on `Unverifiable`, deliberately
  the opposite of the liveness helpers.
- `commands/hook/relay.rs:206-246` — the hook process must sit inside the claimed session's process
  tree, the scratch dir must be the one loom derives (0700, uid-owned), and the ticket must match
  the hash printed in the relay line.

Forging `LOOM_SESSION_ID` to name another session fails the ancestry walk. Env supplies the claim;
process ancestry and file ownership supply the proof.

### The rule that falls out

If the agent controls a value, the worst it may buy is a wrong route or a worse error message,
never a capability. Every new `LOOM_*` consumer should be answerable against that question *at its
call site*, without reading three modules to find out which class it is in.

## Gaps Found

### G1 — Commits are policed by text matching rather than by who can commit

`commit-filter.sh`, `commit-guard.sh`, `git-add-guard.sh` and the installed pre-commit hook all
exist to police who commits and which paths get staged. All four are class 2: text matching, best
effort by their own headers, and `--no-verify` walks past the last one.

**Fix direction:** route commits through the relay that already exists (`relay/*`, `fs/inbox/*`,
`orchestrator/core/inbox_drain/*`). The agent requests "commit these paths, this message"; the
daemon commits on the host from the worktree diff. Subagent commits become impossible rather than
discouraged, `git add -A` staging `.loom` becomes impossible, `--no-verify` becomes meaningless, and
path policy applies where the commit actually happens. This retires a hook family rather than
growing one.

### G2 — The worktree git-directory surface is not denied

- srt denies `.git/hooks` and `.git/config` **only when `.git` is a directory**
  (`sandbox/linux-sandbox-utils.js:185-205` in `@anthropic-ai/sandbox-runtime` 0.0.76, read from the
  bun cache; Claude Code bundles its own copy, whose version is unconfirmed). In a stage worktree
  `.git` is a regular file, so srt contributes no git denies there at all, and `DANGEROUS_FILES`
  (`sandbox/sandbox-utils.js:10-20`) does not list `.git`.
- Loom denies `<repo>/.git/hooks` and `<repo>/.git/config`
  (`sandbox/control_surfaces/session_denies.rs:81-82`) and nothing else under the git directory. The
  worktree's own `.git` pointer file and `<repo>/.git/worktrees/<id>/{commondir,gitdir}` each
  redirect where host-side git resolves its config, and the latter directory has to stay writable
  for commits to work.
- Host-side git runs inside the worktree from **unsandboxed** shell hooks: `commit-guard.sh:119` and
  `:136` (bare `git status --porcelain`), plus `stage-terminal-guard.sh`, `poll-guard.sh`,
  `no-preexisting-failures.sh` and `_progress-classification.sh`. `git/runner.rs`'s
  `-c core.hooksPath=/dev/null` applies to loom's own Rust git calls and covers hooks only — not
  `core.fsmonitor`, `diff.external`, `filter.*.clean`, `core.sshCommand`, aliases or `core.pager`.
- **Unverified:** no working gadget was built. A local check did not get `core.fsmonitor` to fire on
  git 2.43 through a repointed worktree gitdir. Treat the gadget as unproven and the structural gap
  as real.
- **Unverified, and the first thing to answer:** what grants a worktree session write access to
  `<repo>/.git` at all. `session_writable_roots` (`sandbox/control_surfaces.rs:93-111`) starts its
  list from `repo_root`, but that list feeds deny-skipping, not the capsule's `allowWrite`. Commits
  demonstrably work, so something grants it. The same answer decides whether `refs/heads/main` is
  writable from a stage — if it is, the merge gate
  (`orchestrator/core/merge_handler/merge_gate.rs:218-226`, screening `.claude/`, `.mcp.json`,
  `.loom/` and the tracked hooks directory) can be walked around rather than defeated.

**Fix direction:** deny the worktree `.git` pointer file and the two redirect files; narrow the
git-directory grant to what a commit needs (objects, `refs/heads/loom/<id>`,
`worktrees/<id>/{index,HEAD,logs}`); give every host-side git call in agent territory one shared
safe-flag wrapper instead of the hooks flag alone. G1 removes most of the need for in-worktree git
in the first place.

### G3 — No end-to-end proof that any denial holds

Everything above describes configuration, not observed behavior.
[Sandbox and Confinement Gaps](sandbox-and-confinement-gaps.md) records that CI has no credentialed
runtime and denial verification is manual release validation. The harness exists and passed manually
on 2026-09-14 (`orchestrator/terminal/native/tests_confinement_srt.rs`, run with
`LOOM_TEST_REQUIRE_SANDBOX_FREE=1` and a PATH shim for `srt`).

**Fix direction:** run it in CI with that shim and add a probe per deny worth keeping — the state
root, the credentials, the attestation key, `.claude`, and whatever G2 closes. Each probe needs the
matched positive/negative control pair
[Execution Containment](../architecture/execution-containment.md) prescribes, or a green probe
cannot distinguish "denied" from "the harness never ran".

## What Hardening Cannot Fix

Agent-written code reaches `main` through auto-merge and then runs unsandboxed on the operator's
machine at the next build. That is the trust boundary of the whole design, and no sandbox change
moves it. The achievable goal is narrower: containment that depends on nothing the agent can type,
and rule-bending that fails loudly and stays visible.

## Related

- [Security and Isolation](../architecture/security-and-isolation.md) — capsules, the relay inbox,
  the merge gate, spawn preflight
- [Execution Containment](../architecture/execution-containment.md) — what confinement does and does
  not guarantee, and the matched-control test pattern
- [Stage-to-Daemon Channels](../patterns/stage-daemon-channels.md) — why the socket route is
  unreachable from a sandbox and what replaces peer identity
- [Session Identity Env](../mistakes/session-identity-env.md) — presence of a variable is not
  membership; settings env shadows wrapper env
- [Sandbox and Confinement Gaps](sandbox-and-confinement-gaps.md),
  [State Confinement Gaps](state-confinement-gaps.md) — the gap inventory this entry extends

## Gaps Found (continued): G4-G7

### G4 — Detection lags prevention

The realistic failure is an overindulgent agent taking a shortcut, not an attacker. Such an agent
leaves evidence the daemon can see from outside the sandbox: a diff touching files outside the
stage's declared `files:` ownership, a commit whose session does not match the stage, a completion
whose gate never ran.

**Fix direction:** extend the merge gate beyond control paths to declared-ownership violations, and
surface them as a stage-level flag in `loom status`. This catches the realistic case somewhere the
agent cannot reach, and it scales better than a ninth text matcher.

### G5 — `spawn_confined`'s allowlist serves two consumers

Already open in [Sandbox and Confinement Gaps](sandbox-and-confinement-gaps.md), restated because
the review confirmed it: `process/environment.rs` withholds `SSH_AUTH_SOCK` as a live credential
socket while forwarding `DBUS_SESSION_BUS_ADDRESS` and `XAUTHORITY`, which reach
`org.freedesktop.secrets` and the X11 session by the same argument. The terminal spawner needs those
variables; `spawn_confined` does not.

**Fix direction:** a base list for confined commands plus a terminal-only extension, and derive the
shell copy in `orchestrator/terminal/native/wrapper.rs:181-195` from the Rust constant with a test
pinning the two together — which also closes the documented divergence in proxy variables, CA bundle
locations and `CARGO_HOME`.

### G6 — Acceptance commands execute agent-written code on the host

`spawn_confined` is environment scrubbing and nothing else — no namespace, no seccomp, no network
restriction ([Execution Containment](../architecture/execution-containment.md) states this with
three proofs). Plan-authored commands are trusted text, but they compile and run agent-authored code
(`cargo test` runs the stage's own `build.rs`). That is the shortest path from a stage to the host.

**Fix direction:** either run acceptance under the stage's own sandbox profile — `srt` is invokable
from the daemon, and doing so would make "prove this outbound connection is denied" an expressible
criterion instead of an unsatisfiable one — or state the boundary where an operator reads it rather
than only in `concerns/`.

### G7 — Standing accepted risks that deserve a decision

- **Shared package caches** (`sandbox/package_caches.rs`) are writable by every session and executed
  by the operator's own next build. This is the cheapest escape in the system and far easier than
  defeating any hook. Per-session caches are the stated follow-up; until then it belongs somewhere
  an operator reads, not only in [State Confinement Gaps](state-confinement-gaps.md).
- **Three `pub`-but-uncalled path-escape validators** (`sandbox/config.rs:192`, `:276`, `:297`) still
  read as protection. Wire them in or delete them.
- **Attestation key confidentiality** appears handled: the array whose tail sits at
  `fs/permissions/state_root.rs:40-42` lists `user.token` and `crate::handoff::ATTESTATION_KEY_FILE`
  (note that `CREDENTIAL_DENY_READ_PATHS` at `:51` is the separate home-credential list).
  [Completion Recovery](../architecture/completion-recovery.md) warns the stage-sandbox read
  deny-list names only the tokens. Confirm with a probe rather than by reading — that file's
  confidentiality is what keeps forged completion evidence out.
