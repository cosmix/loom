# Agent Rule-Bending Hardening

> Bendable checks backlog

## Why This Entry Exists

An agent can unset or forge `LOOM_*` variables in its own shell. That defeats the checks of one of
the three enforcement classes below and none of the other two; the classes follow, then the open
gaps. **No incident is reported here**: each item states what the tree shows and what is
unverified.

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
police who commits and which paths get staged. All four are class 2: text matching, best effort by
their own headers.

Commits are daemon-owned ([Daemon-Owned Commits](../architecture/daemon-owned-commits.md)). A
session stages with `git add` and runs `loom stage commit`, which runs the pre-commit and
commit-msg hooks in the session's own sandbox and relays the staged tree (`git write-tree`) with
the HEAD it saw. The daemon writes the commit outside the sandbox with `commit-tree` (signed when
`commit.gpgsign` is set) and moves the one ref the session's scope allows with a compare-and-swap
`update-ref` (`git/stage_commit.rs`, `orchestrator/core/inbox_drain/commit.rs`), refusing a staged
path under `.loom/`, `.work/` or `.worktrees/` and any gitlink. The daemon runs no repository hook
and stages nothing itself: it checks that the index still writes the tree the session named and
commits that tree, so it never reads an agent-written file through a symlink swapped in between
git's `lstat` and `open`.

What stays class 2: `commit-filter.sh` blocks a direct `git commit` in a stage session
(`git_runs_commit`), and the relay mode comes from the session's own environment, so a session
that unsets `LOOM_HOOK_CONTEXT` and `LOOM_SCRATCH_DIR` sends `loom stage commit` down
`commit_in_process` with its own privileges. It still cannot sign there, so the daemon-owned
commit is doctrine plus a best-effort hook, not a boundary.

### G2 — The worktree git-directory surface is not denied

- **The grant.** Claude Code adds the whole git common directory (`R/.git`) to the `allowWrite`
  of every session whose cwd is a linked worktree, denying only `hooks`, `config*`, `commondir`
  and the alternates files ([Execution Containment](../architecture/execution-containment.md),
  "Claude Code Grants a Linked Worktree Its Whole Git Common Directory"). Loom adds
  `R/.git/hooks`, `R/.git/config` and the `config.worktree` of every worktree registered at spawn
  (`sandbox/control_surfaces/session_denies.rs`).
- **What a stage can do**, each shown in a scratch repository: move `main` or any branch and
  rewrite sibling stages' `refs/heads/loom/*`; replace a loose object that `git show` and
  `git checkout` then serve with exit 0 (only a full `git fsck` notices); plant `MERGE_HEAD` so the
  operator's next plain `git commit` in `R` becomes a merge with the agent's history; rewrite
  `R/.git/index`, which that commit then records. The merge gate fails open besides
  (`merge_handler/merge_gate.rs` and the `commits_ahead_of` probe in `merge_handler.rs` log and
  proceed when they cannot compute the diff).
- **The target ref is guarded.** A move of `refs/heads/<target>` loom did not make is attested by a
  `reference-transaction` hook and held by the daemon until the operator accepts or restores it
  ([Target Guard](../architecture/target-guard.md)). Loom's own git ignores session-planted replace
  refs, grafts and commit-graphs ([Sandbox gaps](sandbox-and-confinement-gaps.md)).
- **Still open:** sibling branches, tags and other refs; objects; sibling worktrees'
  administrative directories apart from `config.worktree`; `R`'s index, `HEAD` and `MERGE_HEAD`; a
  planted `commondir`; host-side git following `T/.git` (below). The guard's own residuals:
  [Target Guard, Residuals](../architecture/target-guard.md).
- **No deny-list narrows it yet:** denies cannot be reopened for the stage's own ref, write globs
  are skipped on Linux, and an absent-path deny shows the host an empty placeholder file. Commits
  are daemon-owned ([Daemon-Owned Commits](../architecture/daemon-owned-commits.md)), but sessions
  still write the object store and their own index through `git add`, and a merge resolver runs
  `git merge --no-commit --no-ff` in its worktree, so those paths stay writable; whether a `refs/`
  deny holds under the current grant is unmeasured.
- **Host-side git follows the agent's pointer.** A rewritten `T/.git` naming a directory whose
  config defines a clean filter makes a host `git status` in `T` run it
  (`verify/contracts/test_support.rs::plant_foreign_git_dir`); `git/runner.rs` disables hooks and
  `core.fsmonitor`, not filters, and the shell hooks run bare git. Discovering callers:
  `monitor/parked.rs`, `handoff/session_content.rs`, `verify/before_after.rs`,
  `merge_lifecycle/containment.rs`, `git/cleanup/removal.rs`, `git/cleanup/worktree.rs`,
  `git/merge/in_progress.rs`; on an operator's host `loom stage complete` also
  `verify/criteria/cache_fingerprint.rs`, `verify/wiring_detection.rs`,
  `verify/duplicate_detection.rs`, `context/worktree_graph.rs`; hooks `commit-guard.sh`,
  `stage-terminal-guard.sh`, `codex-forward-guard.sh` (`poll-guard.sh`,
  `no-preexisting-failures.sh` and `_progress-classification.sh` run no git).
- **A planted `commondir` in `R/.git`** redirects every discovery-based git call in `R`;
  `GIT_COMMON_DIR` stops it, `GIT_DIR` alone does not. Checkout-rooted sessions (knowledge stages,
  merge resolution, an adjudicator in `R`) can plant it.

**Fix direction for the rest:** metaplan section 7 (`doc/plans/optim-token-metaplan.md`) plans
whole-directory denies on sibling worktrees' administrative directories. Not planned: narrowing
the common-directory grant to what `git add` and a merge resolver need once a `refs/` deny is
measured, pinning host-side git, verifying checkout-rooted sessions' effect on `R/.git`, and
failing the merge gate closed.

## Gaps Found (continued): G3

### G3 — No end-to-end proof that any denial holds

Everything above describes configuration, not observed behaviour.
[Sandbox and Confinement Gaps](sandbox-and-confinement-gaps.md) records that CI has no
credentialed runtime and denial verification is manual release validation. The srt harness
(`orchestrator/terminal/native/tests_confinement_srt.rs`, run outside any sandbox with
`LOOM_TEST_REQUIRE_SANDBOX_FREE=1` and a PATH shim for `srt`) passed manually on 2026-09-14, and
it cannot test the git directory: srt (`@anthropic-ai/sandbox-runtime` 0.0.78) shares Claude
Code's bind logic but not its linked-worktree grant, so `R/.git` is read-only under srt whatever
the capsule says. It probes no `denyRead` path either.

**Fix direction:** run it in CI with that shim and add a probe per deny worth keeping — the state
root, the credentials, the attestation key, `.claude`, the git common directory. The srt
translation must add the common-dir grant Claude Code makes, or a refusal there proves nothing.
Each probe needs the matched positive/negative control pair
[Execution Containment](../architecture/execution-containment.md) prescribes, or a green probe
cannot distinguish "denied" from "the harness never ran". Fix direction (not planned): an
in-session canary that reads the session's own capsule and runs inside a live stage sandbox in
integration-verify.

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

Plan-authored commands (acceptance, setup, truth checks, wiring tests, dead-code checks) run
agent-authored code: `cargo test` runs the stage's own `build.rs`. Where they run decides the
exposure.

- **Default boundary.** `CommandConfinement::Confined` (the default,
  `models/stage/sandbox_config.rs:17`) does more than rebuild a minimal environment:
  `verify/criteria/confine.rs::prepare_confined` wraps the child in an OS boundary
  (`verify/criteria/confine_host.rs`: Linux bubblewrap with `--unshare-all` and a read-only root,
  macOS `sandbox-exec`), allows worktree writes and selected toolchain reads, and denies network.
  Missing containment support is an error. [Execution Containment](../architecture/execution-containment.md)
  is the owner of this contract.
- **In a session.** `loom stage complete` runs as a sandboxed control session
  (`commands/stage/control_session.rs::sandbox_control_session`), so those commands run inside
  both the session sandbox and the command boundary. The daemon's `CompleteStage`
  (`daemon/server/control_complete.rs`) runs no plan or worktree command: it reads files and runs
  pinned git.
- **Residual exposure.** A plan that sets `command_confinement: inherit` opts out of the OS
  boundary and hands children loom's ambient environment; on the host (an operator's
  `loom stage complete`/`verify`/`check` outside a session, or the daemon's `before_stage` truth
  checks at spawn) that code then runs with the operator's full access. `before_stage` is skipped
  when `find_prior_stage_work` reports commits or changes in a reused worktree, and that probe
  discovers git through `T/.git` (G2).

**Fix direction:** make `inherit` visible where an operator reviews a plan (a `loom plan verify`
warning naming the stage), and state the boundary in the README, which still describes
environment scrubbing only.

### G7 — Standing accepted risks that deserve a decision

- **Shared package caches** (`sandbox/package_caches.rs::PACKAGE_MANAGER_CACHE_WRITE_PATHS`,
  emitted into every capsule's `allowWrite`) are writable by every session and executed by the
  operator's own next build: cargo does not re-verify extracted sources, so an edited `build.rs`
  under `~/.cargo/registry/src` runs on the host. This is the cheapest escape in the system, far
  easier than defeating any hook. Fix direction (not planned): give each session
  its own cache over read-only views of the real ones; detail in
  [State Confinement Gaps](state-confinement-gaps.md).
- **Three `pub`-but-uncalled path-escape validators** (`sandbox/config.rs`: `detect_path_escape`,
  `validate_paths`, `is_legitimate_work_access`) still read as protection. Wire them in or delete
  them.
- **Attestation key confidentiality** holds in the capsule: every stage capsule's `denyRead` names
  `<state root>/completion-attestation.key` beside `admin.token` and `user.token` (seen in a live
  stage capsule on 2026-10-01), and `loom-hooks/credential-guard.sh` blocks the three for the file
  tools whatever the settings say. No probe from inside a session has confirmed the OS deny; the
  key's confidentiality is what keeps forged completion evidence out.

## Stage Agents Stop and Report Instead of Disputing or Blocking

The doctrine is BLOCK-F (`orchestrator/signals/helpers.rs::STAGE_EXIT_RULES`, byte-identical in `skills/loom-orchestration/SKILL.md`, pinned by `tests_doctrine_v2.rs`): a wrong criterion, wiring check, contract, review finding or test-integrity event gets a dispute, whose filing ends the session by design; a need only a person can meet gets `loom stage block <stage-id> "<reason>"`. A stage agent's block retires the stage's live session and shows the reason in `loom status` (`orchestrator/core/event_handler/blocked.rs`). `loom plan verify` checks the plan's environment (provision entries, registry domains, JS packages without provision, pre-commit hook network needs; see [plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md)). Open gaps:

- **Subagents are told by text only.** `loom-hooks/commit-filter.sh` blocks a subagent's `loom stage complete` and git commands mechanically, but `loom stage block` and `dispute-*` rely on the `_subagent-preamble.txt` line. A subagent block now retires the main session, so the hook branch should refuse them too. `agents/loom-software-engineer.md` and the coordinator preamble in `skills/loom-orchestration/SKILL.md` do not repeat the report-instead-of-file rule, so an untyped spawn that pastes only BLOCK-A never receives it.
- **BLOCK-F and commit timing disagree.** BLOCK-F says to commit before filing; `append_commit_timing_rules` in the same signal prefix says commit only as the final step. The commit-timing text needs an exception for a dispute or block.
- **A surviving agent on a Blocked stage is never retried.** When retirement leaves survivors, `blocked.rs` warns once; `StageBlocked` is not re-emitted. Only `loom stage reset --kill-session` clears it.
- **Duplicated block mutation.** The `NotListening` arm of `commands/stage/state_relay.rs` repeats the `try_mark_blocked` / `close_reason` / `failure_info` / `updated_at` writes of `handle_block_stage`; `event_handler/stage_takedown.rs::retirable_agents` repeats the session assembly of `stage_agents`. Each pair can drift.

## Gaps Found (continued): G8

### G8 — The knowledge-file guard gates tool names over a path the sandbox grants

**The check.** `loom-hooks/worktree-file-guard.sh:326-334` refuses Write, Edit, MultiEdit and
NotebookEdit under `<worktree>/doc/loom/knowledge/`. It is class 2 and the only check on the
directory: `sandbox/config.rs::apply_knowledge_write_grant` adds `doc/loom/knowledge/**` to every
merged config's `allow_write` and strips it from `deny_write`, because `loom knowledge update` is an
ordinary child of the sandboxed shell ([Knowledge Write Channel](../mistakes/knowledge-write-channel.md)).
Any process the agent starts (`python -c`, `sed -i`, a heredoc) writes the same bytes as the CLI.
Agents observed in knowledge-distill worktrees take that route right after the block.

**Why agents take it:**

- **The message reads as "use another tool".** The reason line says "not file tools", and the shared
  `block_path` footer (`worktree-file-guard.sh:124`) says "Use a path inside the current worktree"
  about a path already inside it.
- **The named verb cannot do an Edit's job.** The message names `update`, which appends. The
  in-place verbs, `replace-section` and `delete-section`, go unmentioned.
- **The sanctioned recipe is a shell transform.** [Knowledge CLI Gaps](knowledge-cli-gaps.md) has a
  surgical fix extract the section with a fence-aware scanner, substitute, and pipe the body to
  `replace-section`; writing the file directly skips one hop.
- **It blocks the stage that owns the files.** The guard applies in knowledge-distill worktrees,
  while CLAUDE.md Rule 12 step 4 permits an Edit plus `loom knowledge sync` outside a stage.

**What a bypass costs.** The CLI's write path regenerates `INDEX.md`, handles headings and fences,
and keeps the index under its `OversizedIndex` budget. A search of `verify/` and `commands/stage/`
found no completion-time knowledge check, so a stale index or a doubled heading merges unnoticed.
Those invariants are the value at stake; the channel adds nothing apart from them.

**A contradicted justification.** `plan/schema/validation.rs::warn_ignored_knowledge_denies` says
"every stage records knowledge through `loom knowledge update`". Rule 12 has standard stages record
`stale-knowledge:` memories and never run `loom knowledge update`. Unverified: whether any
standard-stage path (the memory spool drain, an index refresh) writes the directory.

**Fix direction (not planned):**

1. Gate the result: for a stage whose diff touches `doc/loom/knowledge/`, run `loom knowledge check`
   and an index-freshness comparison (regenerate in memory, compare to disk) at completion, outside
   agent control. Then the write route stops mattering, and distill stages can be allowed Edit with
   a sync after it.
2. Scope `apply_knowledge_write_grant` to Knowledge and KnowledgeDistill stages and deny the
   directory to the rest, once the unverified item above is checked. Spell the deny as a directory
   path (write globs are skipped on Linux, G2) and add a probe with matched controls (G3).
3. Rewrite the block message: drop the worktree footer for this reason, name `replace-section`,
   `delete-section` and `update`, and say a shell or interpreter write breaks the same rule. Claim
   detection at completion only once item 1 exists.
4. Add an exact-once `loom knowledge edit <topic> --old <text> --new <text>` verb, which removes the
   shell-transform recipe for most surgical fixes.
5. No Bash-side text matcher: `commit-filter.sh:12-28` records why that class loses to `python -c`.
