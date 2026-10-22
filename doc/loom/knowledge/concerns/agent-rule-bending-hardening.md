# Agent Rule-Bending Hardening

> Enforcement classes, hard-stop matrix

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
`relay/emit.rs::from_process_env` (`LOOM_SESSION_ID`, `LOOM_SCRATCH_DIR`, `LOOM_STAGE_ID` and the
rest), `commands/memory/handlers/record.rs:120`, `commands/handoff/create.rs:287-299`,
`commands/subagents/render.rs:61`. `env -u LOOM_STAGE_ID loom …` changes what all of them see.

It buys nothing, because none of them grants a capability. `relay::emit::mode` (`relay/emit.rs:102`)
reads the same variables to choose the route: without `LOOM_SESSION_ID`, or in a hook context, the
operator route; with a session but no `LOOM_SCRATCH_DIR`, the legacy route, which host-job commands
refuse; with both, a ticket relayed to the daemon. Unset them and the call takes the operator route,
whose direct writes to `.loom/work` fail because the capsule mounts it read-only. **Env selects a route;
the sandbox and the ancestry checks grant the right.**

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

**Hard stops mapped to their enforcement.** Each row of `CLAUDE.md.template`'s hard-stop list, with what holds it, the class above and the test that pins it. "none" marks a gap.

| Hard stop | Enforced by | Class | Pinning test |
| --- | --- | --- | --- |
| 1 PLANS | `loom-hooks/plans-path-guard.sh` denies a plan file outside `doc/plans/` | 2 | `loom-hooks/tests/plans-path-guard-allows-doc-plans.sh`, `plans-path-guard-blocks-claude-plans.sh`, `plans-path-guard-blocks-project-plans.sh`, `plans-path-guard-multiedit.sh` |
| 2 MEMORY | `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` in the stage wrapper (`loom/src/orchestrator/terminal/native/wrapper.rs:364`) and the capsule's `.claude/projects/**` deny (`HOME_SURFACES` in `loom/src/sandbox/control_surfaces.rs`) | 3 | none found for the env line or the deny |
| 3 GIT | `loom-hooks/commit-filter.sh` (blocks `git commit` and a subagent's stage completion), `loom-hooks/git-add-guard.sh` (blanket staging) and the PreToolUse pin in `loom-hooks/loom-control-complete.sh` (class 2); the daemon completes a stage only from its own host job and refuses a completion request from a subagent (`is_control` in `loom/src/relay/kind.rs`, `loom/src/host_verify/apply.rs`) (class 3) | 2 and 3 | `loom/tests/integration/hooks_commit_filter.rs` (`blocks_git_commit_in_a_stage_session`), `loom-hooks/tests/git-add-guard-quoting.sh`, `loom-hooks/tests/loom-control-complete-heredoc.sh`, `loom/tests/daemon_completion_contracts.rs` (`subagents_cannot_relay_completion`) |
| 4 CONTEXT | the `CONTEXT CEILING REACHED` report in `loom-hooks/post-tool-use.sh` (class 2); the daemon backstop in `loom/src/orchestrator/monitor/ceiling.rs`, which writes the handoff a session in the Red band is owed (class 3) | 2 and 3 | `loom-hooks/tests/post-tool-use-ceiling-hard-block.sh`, `post-tool-use-ceiling-warn-once.sh`, `post-tool-use-ceiling-resolution.sh`; `loom/src/orchestrator/monitor/tests/ceilings.rs` |
| 5 BRANCHES | none: no hook matches `git checkout -b`, `git switch -c` or `git branch <name>` | none | none |
| 6 DELEGATION | `_loom_main_edit_advisory` in `loom-hooks/post-tool-use.sh`: one advisory per session past 20 changed lines or 2 files; advisory only, it never denies | 2 | `loom/tests/guidance_gate_contracts.rs` (`main_session_edit_advisory_fires_once_past_the_limit`), `loom-hooks/tests/post-tool-use-main-edit-advisory.sh` |

Stop 5 has no mechanical check, stop 6 only an advisory, and stop 2 has no pinning test.

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

### G2 — The git common directory's refs and objects are not denied

- **The grant.** Claude Code adds the whole git common directory (`R/.git`) to the `allowWrite`
  of every session whose cwd is a linked worktree, denying only `hooks`, `config*`, `commondir`
  and the alternates files ([Execution Containment](../architecture/execution-containment.md),
  "Claude Code Grants a Linked Worktree Its Whole Git Common Directory"). Loom adds
  `R/.git/hooks`, `R/.git/config`, every sibling worktree's administrative directory
  `R/.git/worktrees/<name>` whole, and the session's own `config.worktree`, for the worktrees
  registered at spawn (`partition_admin_dirs` in `git/worktree/config_worktree.rs`, applied in
  `sandbox/control_surfaces/session_denies.rs`; [Where a Session's Write Grants Come
  From](../architecture/security-and-isolation.md#where-a-sessions-write-grants-come-from)).
- **What a stage can still do**, each shown in a scratch repository: move `main` or any branch and
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
- **Still open:** sibling branches, tags and other refs; objects; `R`'s index, `HEAD` and
  `MERGE_HEAD`; a planted `commondir`; host-side git following `T/.git` (below); the residuals of
  the sibling-directory denies ([Sandbox and Confinement Gaps](sandbox-and-confinement-gaps.md),
  "Residuals of the Sibling Administrative Directory Denies"). The guard's own residuals:
  [Target Guard, Residuals](../architecture/target-guard.md).
- **No deny narrows refs and objects:** a deny cannot be reopened for the stage's own ref, write
  globs are skipped on Linux, and an absent-path deny shows the host an empty placeholder file.
  Commits are daemon-owned ([Daemon-Owned Commits](../architecture/daemon-owned-commits.md)), yet
  sessions still write the object store and their own index through `git add`, and a merge
  resolver runs `git merge --no-commit --no-ff` in its worktree, so `refs/` and `objects/` stay
  writable. Ref denies are unmeasured: whether a `refs/` deny holds under the current grant, and
  what it does to the daemon's own `update-ref`, has not been tried.
- **Host-side git follows the agent's pointer.** A rewritten `T/.git` naming a directory whose
  config defines a clean filter makes a host `git status` in `T` run it
  (`verify/contracts/test_support.rs::plant_foreign_git_dir`); `git/runner.rs` disables hooks and
  `core.fsmonitor`, not filters, and the shell hooks run bare git. Discovering callers:
  `monitor/parked.rs`, `handoff/session_content.rs`, `verify/before_after.rs`,
  `merge_lifecycle/containment.rs`, `git/cleanup/removal.rs`, `git/cleanup/worktree.rs`,
  `git/merge/in_progress.rs`, and `verify/impact_tests.rs` `check()`, which reaches git through
  `checkout_git` (`verify/review/observer.rs`). `checkout_git` pins git to a stage worktree's
  registered git directory; for any other directory it discovers git as the `.git` there directs.
  Hooks `commit-guard.sh`, `stage-terminal-guard.sh` and `codex-forward-guard.sh` run bare git
  (`poll-guard.sh`, `no-preexisting-failures.sh` and `_progress-classification.sh` run no git).
- **A planted `commondir` in `R/.git`** redirects every discovery-based git call in `R`;
  `GIT_COMMON_DIR` stops it, `GIT_DIR` alone does not. Checkout-rooted sessions (knowledge stages,
  merge resolution, an adjudicator in `R`) can plant it.

**Fix direction for the rest:** not planned: measuring a `refs/` deny, narrowing the
common-directory grant to what `git add` and a merge resolver need once a `refs/` deny is
measured, pinning host-side git, verifying checkout-rooted sessions' effect on `R/.git`, and
failing the merge gate closed.

## Gaps Found (continued): G3

### G3 — Denials are proven against bubblewrap in CI and by hand against Claude Code

Two probes cover the capsule. The bubblewrap probe `tests_capsule_denies::bwrap`
(`orchestrator/terminal/native/tests_capsule_bwrap.rs`) translates the emitted capsule's
`denyWrite` and `denyRead` lists into bubblewrap binds and runs shell probes inside, each refusal
with a matched control that succeeds; CI runs it with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, so a
skipped probe fails there. It covers the sibling administrative directories and the credential
files. The release checklist `doc/sandbox-release-checklist.md` has a person run fifteen rows
against a live, credentialed Claude Code session.

What neither reaches: the srt harness (`orchestrator/terminal/native/tests_confinement_srt.rs`,
run outside any sandbox with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` and a PATH shim for `srt`) is not
in CI and skips where `srt` is absent. It cannot test the git directory: srt
(`@anthropic-ai/sandbox-runtime` 0.0.78) shares Claude Code's bind logic but not its
linked-worktree grant, so `R/.git` is read-only under srt whatever the capsule says. Denies the
bubblewrap probe does not cover (the state root, the attestation key, `.claude`, the common
directory's `hooks` and `config`) have no CI probe, and the tool-route denies are checked on the
emitted capsule text only. Each new probe needs the matched positive/negative control pair
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
- **In a session.** `loom check` and `loom stage complete` relay a host job. The daemon's host child
  runs the criteria through the same boundary, outside the session sandbox (`host_verify/child.rs`),
  and the daemon's application of a passed run (`host_verify/apply.rs`) reads files and git state and
  runs no plan command.
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

The doctrine is BLOCK-F (`orchestrator/signals/helpers.rs::STAGE_EXIT_RULES`, byte-identical across the standard, integration-verify and knowledge-distill stable prefixes, pinned by `tests_doctrine_v2.rs`): a wrong criterion, wiring check, contract, review finding or test-integrity event gets a dispute, whose filing ends the session by design; a need only a person can meet gets `loom stage block <stage-id> "<reason>"`. A stage agent's block retires the stage's live session and shows the reason in `loom status` (`orchestrator/core/event_handler/blocked.rs`). `loom plan verify` checks the plan's environment (provision entries, registry domains, JS packages without provision, pre-commit hook network needs; see [plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md)). Open gaps:

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

## Sibling Directory Denies: Residuals

`partition_admin_dirs` (`git/worktree/config_worktree.rs`) lists `<common>/worktrees/*` once, when the capsule is built, and denies every entry that is not the session's own whole; the own entry keeps `config.worktree` denied and the rest writable (`git add` writes its `index`). Nothing is denied beneath a directory denied whole, because bubblewrap aborts when a deny's mount point lies beneath a read-only bind ([Execution Containment](../architecture/execution-containment.md#claude-code-grants-a-linked-worktree-its-whole-git-common-directory)). What stays open:

- **Only registry entries are denied.** A session can rename an entry or add a new directory under `<common>/worktrees` itself; the new name is covered by no deny.
- **A worktree registered after the session spawned** keeps its `HEAD`, `index`, `gitdir` and `logs/` writable to that session. Only its `config.worktree` is covered, by the daemon's `WorktreeGit::run` check ([A Check-Then-Use Window Around the `config.worktree` Check](sandbox-and-confinement-gaps.md#a-check-then-use-window-around-the-configworktree-check)).
- **Symlinks and unreadable entries.** `worktree_admin_dirs` keeps only real directories: a symlinked registry entry present at spawn gets no deny, and a `DirEntry` whose `file_type()` errors is silently omitted from the sibling list (benign on Linux, where it means the entry vanished, but security-adjacent).
- **A session can rewrite its own entry's `gitdir`.** `partition_admin_dirs` then finds no claimant for the worktree and refuses that session's own later spawn; when the rewrite names a sibling's worktree, two entries claim it and the sibling's next spawn is refused too. The refusal fails closed and escalates nothing.
- **A worktree directory on disk that no registry entry claims** refuses every worktree-rooted spawn there, the adjudication judge (escalated as `adjudicator_unavailable`) and the merge resolver included, because `judge_cwd` (`orchestrator/adjudication/session.rs`) and `merge_resolver_worktree` (`orchestrator/terminal/backend.rs`) check only `is_dir()`.
- **`target_is_worktree` (`sandbox/settings.rs`) matches any `.worktrees` path component**, so a standalone clone under such a path that has linked worktrees is refused at spawn: the main worktree has no claimant. Unlikely.
- **An empty registry with `Some(worktree)`** returns an empty partition although the doc says an unclaimed worktree is an error; pinned git refuses an unregistered worktree anyway, so nothing escalates. An unreadable registry (a `read_dir` error other than `NotFound`), the third spawn-refusal path, has no test.
- **A directory the host deletes during a running command can be recreated by that command.** A sibling's entry vanishes inside a command already running when its stage merged, and the command may write it back (release checklist row 8 asserts only that the next command succeeds and the host shows no entry).
- **Drift risk.** `registered_admin_dir` (`git/worktree/pinned.rs`) repeats the registry listing loop of `worktree_admin_dirs` and `partition_admin_dirs`; a shared helper would stop the mirror drifting.

## Route-Closure Open Items

- **Ref denies are unmeasured.** Commits are daemon-owned, so the metaplan's reason for leaving ref denies out is gone; sessions still write the object store and their own index, and nobody has measured a `refs/` deny under the current grant (G2 above).
- **`credential-guard.sh` rule (b) reads only an operator-authored `settings.local.json`**, so a checkout-rooted session's file tools read the credential files ([Sandbox and Confinement Gaps](sandbox-and-confinement-gaps.md#credential-reads-denied-files-plain-paths-and-what-stays-readable)).
- **Sibling worktree reads stay open**, and the main checkout stays readable by design.
- **Unmeasured tool routes:** `Monitor`, `AppifactRepl`, the self-hosted-runner tools, `EnterWorktree` and `ExitWorktree` (checklist row 15).
- **`loom knowledge bootstrap` runs unconfined.** `commands/knowledge/bootstrap/mod.rs` starts `claude` through `crate::claude::run_foreground` with no capsule, outside manual mode, so the session it runs carries none of the capsule's denies and no confinement notice.
- **Manual mode prints unquoted paths.** `orchestrator/core/manual_start.rs` prints the worktree, work-directory, repository-root and signal paths bare: a path with a space breaks the printed `cd`, and `$` or backticks in a path expand in the operator's shell. Ids are validated and cannot inject. The builder's text was pinned byte for byte, so `shell_escape::escape` is a later change.
- **Documentation disagrees with the checklist.** `CONTRIBUTING.md` (the release-checklist paragraph near line 192) lists trigger paths that differ from `doc/sandbox-release-checklist.md`, which adds `state_root.rs` and `config_worktree.rs`; neither names `launch.rs`, which carries the Workflow licence; the paragraph also separates the tag sentence from its code block. `README.md` and `CONTRIBUTING.md` are outside the route-closure stages' files, so this stays open until a stage that owns them.
- **Unimplemented reviewer suggestions** are under "Route-Closure Review Suggestions" below.

## Route-Closure Review Suggestions

Reviewer suggestions on the route-closure stages that nobody implemented; none blocks. Paths are under `loom/src/`.

- **Capsule-level denyRead assertion.** `orchestrator/terminal/native/tests_capsule_routes.rs:36`: assert without bubblewrap that the nine credential files and `~/.codex/auth.json` are in `sandbox.filesystem.denyRead` for every session kind, plus a codex-licensed variant. Today only the bubblewrap probe (Stage only, skipped without user namespaces) proves it on an emitted capsule.
- **Pin the Workflow licence.** `tests_launch_capsule.rs:157`: `try_launch_stage` returns the wrapper script and the closure discards it; assert it contains `licensed for ultracode` exactly when the capsule omits the `Workflow` deny. `:149`: the name claims no other kind gets `Workflow` but only Contract is probed; loop over `ALL_KINDS`. `:168`: give the ultracode test's stage the id `stage-1` like its sibling, not `Stage::default()`. The file is at the 400-line limit; move a test to a sibling file before adding one.
- **Tests that read their own subject.** `sandbox/settings/tests.rs:436` builds its expectation from `mandatory_deny_read(false)` (the literal list lives only in `policy/tests.rs:37`); `orchestrator/core/manual_start.rs:100` asserts constant text contains words from the same constant.
- **Fixture and gap tests.** `tests_capsule_routes.rs:78` hand-builds a `CapsuleRequest` that `tests_session_settings::write_capsule` already builds (parameterise `workflow_licensed`). `git/worktree/config_worktree_tests.rs:143` lacks a test for an unreadable registry. `tests_capsule_denies.rs:36` runs `git commit` without `-c commit.gpgsign=false` (see `verify/contracts/test_support.rs:142`), so a host with global signing fails three tests under plain `cargo test`. `tests_capsule_bwrap.rs:42` `resolve()` overlaps `tests_confinement_srt.rs` `srt_path()`; accepted.
- **Code shape.** `session_settings/contents.rs:123`: `capsule_settings` is 46 lines (limit 50) and was meant not to grow from 42; binding `capsule_deny_rules(inputs)` to a local first costs 2 lines. `git/worktree/mod.rs:34`: the `pub` re-export of `worktree_admin_dirs` has no consumer outside `partition_admin_dirs` and its tests. `orchestrator/terminal/native/launch.rs:272`: the reason comment on `#[allow(clippy::too_many_arguments)]` ("Kept separate: ...") does not say what is kept separate or why bundling is not done (precedent `wrapper.rs:65-67`). `git/worktree/config_worktree.rs:100`: import `names_work_tree` like `common_dir`.
- **Comments.** `launch.rs:75` ("that now stands in for the doctrine this stage moved out of the signal") and the assertion message at `fs/permissions/state_root.rs:245` ("is a mirror loom used to write") are history phrases; `session_settings/contents.rs:90` wraps a doc paragraph mid-clause.
