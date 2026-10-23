# Code Review: Remove the container execution backend

**Plan:** PLAN-remove-container-backend | **Generated:** 2026-05-14 16:34 UTC

## Summary

This plan removes Loom's container execution backend—uneconomical due to Anthropic's shift to metered usage charges—by systematically deleting container code, CLI surface, and documentation. It collapses the multi-backend abstraction (BackendType, TerminalBackend trait, BackendDispatcher) back to a single concrete NativeBackend type. The plan preserves all unrelated bugfixes and ensures the native execution path remains unchanged throughout the removal process.

## Changes by Stage

### Bootstrap Knowledge Base (knowledge-bootstrap)

**Status:** completed  
**Purpose:** doc/loom/knowledge/ is already populated. This stage confirms coverage
and produces a verified container-removal surface map for the two big
code stages.

Use parallel subagents and skills to maximize performance.

Tasks:

- Run loom knowledge check. If coverage < 50%, run loom map --deep.
- Verify the container-removal surface map. The human-readable Stages
  section of this plan file (doc/plans/PLAN-remove-container-backend.md,
  or the IN_PROGRESS-/DONE- renamed variant — check .work/config.toml
  source_path) lists every file. Confirm those files/symbols still exist.
- Resolve and record to memory the three open items:
  1. The exact daemon/server/client.rs arms for CompleteStageContainer
     (auth-token arm, dispatch arm, complete_stage_container helper).
  2. Whether is_cwd_relative_base in fs/work_dir.rs becomes dead once the
     /repo LOOM_WORK_DIR fallback is removed.
  3. Whether loom/tests/stage_completion.rs exercises the CONTAINER
     completion-delegation path (safe to delete) or native completion.
- Refresh stale notes in architecture.md / entry-points.md that the
  removal will invalidate. Do NOT delete container knowledge sections —
  that is the strip-container-docs stage's job; deleting here causes a
  merge conflict.

Use loom knowledge CLI commands, NOT Write/Edit, for knowledge files.

MEMORY RECORDING (use loom memory and loom knowledge ONLY — never
Claude Code auto-memory under ~/.claude/projects/*/memory/):

- Record the resolved open items: loom memory note "found: ..."
- Record surface-map discrepancies: loom memory note "..."

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- strip-container-docs: subagents with Edit/Write tools are blocked by worktree sandbox — main agent must do all file edits directly with dangerouslyDisableSandbox when needed
- found: loom/tests/stage_completion.rs is ENTIRELY container-specific — safe to delete in Stage 3. The file header (lines 3-7) explicitly describes it as testing container-private clone completion via Request::CompleteStageContainer. Module is container_rpc (line 16), imports from orchestrator::terminal::container::git_bridge, and the single standalone test at line 278 exercises is_container_completion() which keys on LOOM_BACKEND=container env var. This file exercises ZERO native completion paths — delete without replacement.
- found: is_cwd_relative_base (fs/work_dir.rs:91-99) becomes dead after Stage 3. It is ONLY called at line 145 to gate loom_work_dir_from_env(). The plan removes loom_work_dir_from_env() and its call site (lines 145-149); once that block is deleted, is_cwd_relative_base has zero callers and must also be deleted. Additionally: commands/common/mod.rs has a DUPLICATE local work_dir_from_env() (lines 49-55) that also checks for /repo/ paths — this is the duplicate fallback the plan mentions at line 210 ('remove the duplicate /repo-restricted LOOM_WORK_DIR fallback in commands/common/mod.rs::work_dir_from_env') and must be deleted too.
- found: daemon/server/client.rs CompleteStageContainer has THREE sites to remove. (1) Auth-token arm (lines 136-138): in the per-request auth extraction match, Request::CompleteStageContainer { auth_token, .. } => (auth_token, "CompleteStageContainer") — feeds verify_for_capability(). (2) Dispatch arm (lines 221-247): destructures stage_id, session_id, target_branch, expected_base_oid, calls complete_stage_container(), writes response, breaks. (3) complete_stage_container helper (pub(crate), lines 302+): uses git_bridge::bundle_staging_path to find staged bundle, calls git_bridge::validate_bundle then imports. All three must be removed together in Stage 3.

### Remove Container Code (Keep Scaffolding) (remove-container-keep-scaffolding)

**Status:** completed  
**Purpose:** Delete every container-only module, the loom container CLI, all container
branches/fields, and the BackendType::Container variant — while KEEPING
BackendType (now a single Native variant), the TerminalBackend trait,
BackendDispatcher, and LivenessService intact. This intermediate state
MUST compile and leave the native execution path behavior-identical.

Read the human-readable "Stage 3" section of this plan file
(doc/plans/PLAN-remove-container-backend.md — check .work/config.toml
source_path for the renamed variant) for the COMPLETE file-by-file
breakdown. Key points below.

Use parallel subagents and skills to maximize performance.

Critical correctness items (pressure-test findings):

- sandbox/config.rs is a FAIL-CLOSED edit, not a no-op: validate_config
  currently rejects BypassPermissions only when backend == Native. Change
  it to reject BypassPermissions UNCONDITIONALLY with a clear error.
  Update sandbox tests to assert rejection.
- The container main-session hook overlay must be removed as a SET:
  container_main_settings_path, setup_container_main_session_settings,
  their hooks/mod.rs re-exports, the knowledge-stage branch in
  stage_executor.rs that calls it, and the container tests in
  hooks/generator.rs + hooks/tests.rs.
- Keep BOTH settings.local.json git-exclude helpers
  (add_settings_local_to_worktree_gitignore AND
  add_settings_local_to_main_gitignore).
- Remove handlebars from loom/Cargo.toml (container-only). Keep sha2, hex,
  shell-escape.
- Also remove the duplicate /repo-restricted LOOM_WORK_DIR fallback in
  commands/common/mod.rs::work_dir_from_env.
- clean.rs --sessions: recommended — reword to drive native session
  termination via loom sessions kill logic; if out of scope, make it an
  explicit no-op with corrected help. Record the decision in memory.

Scaffolding KEPT in this stage (removed in the next stage): BackendType
enum (single Native variant), TerminalBackend trait, create_backend,
BackendDispatcher, LivenessService, and all backend: BackendType
params/fields. Keep permission_mode everywhere — it is NOT container.

SUBAGENT FILE ASSIGNMENTS (each owns disjoint files; the MAIN AGENT does
the final integration compile + commit — subagents never run git commit
or loom stage complete):
  Subagent 1 — Bulk deletion + Cargo.toml (loom-software-engineer):
    Files Owned: loom/src/orchestrator/terminal/container/**,
      loom/src/commands/container/**, loom/resources/**,
      loom/tests/container_smoke.rs, loom/tests/container_adversarial.rs,
      loom/tests/log_format_test.rs, loom/tests/stage_completion.rs,
      loom/tests/fixtures/stream-json/**,
      loom/src/completions/dynamic/tests/tests_container.rs,
      loom/Cargo.toml
    Files Read-Only: none
  Subagent 2 — CLI surface (loom-software-engineer):
    Files Owned: loom/src/cli/types.rs, loom/src/cli/dispatch.rs,
      loom/src/commands/init/execute.rs,
      loom/src/commands/init/plan_setup.rs,
      loom/src/completions/dynamic/commands.rs,
      loom/src/completions/dynamic/mod.rs
    Files Read-Only: none
  Subagent 3 — Schema + models container fields (loom-senior-software-engineer):
    Files Owned: loom/src/plan/schema/execution.rs,
      loom/src/plan/schema/types.rs,
      loom/src/models/session/**, loom/src/models/stage/**
    Files Read-Only: none
  Subagent 4 — Orchestrator + daemon container branches (loom-senior-software-engineer):
    Files Owned: loom/src/orchestrator/core/stage_executor.rs,
      loom/src/orchestrator/core/completion_handler.rs,
      loom/src/orchestrator/core/merge_handler.rs,
      loom/src/orchestrator/core/event_handler.rs,
      loom/src/orchestrator/monitor/handlers.rs,
      loom/src/orchestrator/terminal/mod.rs,
      loom/src/orchestrator/terminal/dispatcher.rs,
      loom/src/orchestrator/terminal/native/pid_tracking.rs,
      loom/src/daemon/protocol.rs, loom/src/daemon/server/client.rs,
      loom/src/daemon/server/lifecycle.rs,
      loom/src/commands/stage/complete.rs
    Files Read-Only: none
  Subagent 5 — Hooks/sandbox/fs/git container branches (loom-senior-software-engineer):
    Files Owned: loom/src/hooks/generator.rs, loom/src/hooks/config.rs,
      loom/src/hooks/mod.rs, loom/src/hooks/tests.rs,
      loom/src/fs/work_dir.rs, loom/src/fs/permissions/**,
      loom/src/git/worktree/**, loom/src/sandbox/config.rs,
      loom/src/sandbox/settings.rs, loom/src/commands/common/mod.rs
    Files Read-Only: none
  Subagent 6 — Commands cleanup (loom-software-engineer):
    Files Owned: loom/src/commands/clean.rs, loom/src/commands/stop.rs
    Files Read-Only: none
NO FILE OVERLAP between subagents. The deletions (Subagent 1) and the
reference removals (2-6) cannot each compile alone — the MAIN AGENT runs
the final cargo build/test/clippy, resolves any straggler references not
in the assignment table, and commits.

MEMORY RECORDING (loom memory ONLY — never loom knowledge in a standard
stage, never Claude Code auto-memory under ~/.claude/projects/*/memory/):
Record IMMEDIATELY when they happen:

- MISTAKE: loom memory note "mistake: tried X, failed because Y, fix Z"
- DECISION: loom memory decision "chose X" --context "why"
- SURPRISE/GOTCHA: loom memory note "found: ... in file:line"
Do NOT record procedural actions. Every subagent Task prompt MUST instruct
the subagent to record mistakes/decisions/surprises via loom memory and to
NEVER run git commit or loom stage complete.

#### Files Changed

No changes recorded.

#### Key Decisions

- made --sessions in loom clean a no-op with redirect message *(clean_sessions() was 100% container-focused (docker/podman/container runtimes). Reusing kill_single_session from sessions.rs would require importing session parsing, BackendDispatcher, and iterating all session files — effectively duplicating loom sessions kill. Since clean.rs had zero native session logic, explicit no-op with redirect to loom sessions kill is cleaner and avoids cross-cutting coupling.)*
- kept validate_git_identity + GIT_IDENTITY_MAX_LEN in plan/schema/execution.rs *(task said remove if only container callers, but grep shows non-container callers: src/fs/work_dir.rs:522 (config.toml hand-edit sanitisation) and src/commands/init/execute.rs:308 (loom init git identity capture). Kept the fn, the const, and all 5 of its tests.)*
- kept require_admin_capability and the admin-token gate *(subagent 4 kept it; the gate is not container-only (it guards --no-verify bypass flags generally), so it survives the container removal. Renamed its test from cli_invoked_from_within_container_fails_admin_check to cli_without_admin_token_fails_admin_check to drop stale container framing.)*

#### Notes

- found: integration compile surfaced 7 straggler files not in any subagent's assignment table — commands/mod.rs (mod container), completions/dynamic/tests.rs (mod tests_container), plan/schema/mod.rs (re-export of removed types), commands/handoff/create.rs + commands/stage/tests/session.rs (Session.runtime/container_name fields), orchestrator/preflight.rs (BackendType::Container + ProjectExecutionConfig.container digest block). Rule: after a parallel-deletion stage, always run cargo build AND cargo test --no-run as main agent — test-only files won't show up in a lib build.
- found: PlanExecutionConfig becomes a fieldless struct after removing container field; kept as 'pub struct PlanExecutionConfig {}' per scaffolding requirement. Its round-trip test was reduced to just ::default().
- gotcha: PLAN-remove-container-backend.md is NOT in the worktree git tree (only PLAN-fix-sandbox-parent-traversal-denywrite.md is committed). The stage file .work/stages/02-remove-container-keep-scaffolding.md embeds the Key Points but NOT the full file-by-file Stage 3 breakdown. Subagents must explore the codebase to find all container references.

### Collapse the Backend Scaffolding (collapse-backend-scaffolding)

**Status:** completed  
**Purpose:** Remove the now-vestigial multi-backend abstraction. NativeBackend becomes
a plain concrete type; LivenessService is rerouted to hold it directly;
the execution schema module is deleted; all backend params/fields are
removed. Includes one adjacent bugfix. This stage MUST compile and pass
the full test suite.

Read the human-readable "Stage 4" section of this plan file for the
COMPLETE file-by-file breakdown. Key points below.

Use parallel subagents and skills to maximize performance.

Critical items:

- Concrete NativeBackend: change "impl TerminalBackend for NativeBackend"
  to an inherent impl; every former trait method becomes pub fn; drop
  backend_type() + its unit test.
- ADJACENT BUGFIX (in scope — tracking_key is a keeper and the bug is in
  the code being collapsed): kill_session / is_session_alive derive the
  window title + PID-file key from stage_id as loom-{stage_id} /
  {stage_id}.pid, which is wrong for merge/knowledge/base-conflict
  sessions (their spawns use prefixed titles). Use session.tracking_key
  when present, with a legacy fallback to loom-{stage_id} /
  {stage_id}.pid. Add merge + knowledge session regression tests.
- Delete dispatcher.rs entirely; reroute LivenessService from
  Dispatcher(Arc<BackendDispatcher>) to Native(Arc<NativeBackend>), keep
  the Fixed(bool) test seam.
- Orchestrator core, hooks, sandbox, and git form ONE build unit:
  backend_type / stage_backend removal cannot land without simultaneously
  removing the backend params they feed in sandbox/hooks/git signatures.
- auto_merge.rs: attempt_auto_merge takes BOTH dispatcher AND
  merge_backend — remove both plus the session.set_backend call.
- preflight.rs: delete resolve_project_backend / resolve_stage_backend;
  delete the module + "pub mod preflight;" if empty; update all six
  callers.
- Delete plan/schema/execution.rs entirely; ripple into schema/mod.rs,
  schema/types.rs (execution: fields), fs/work_dir.rs (the
  project_execution / plan_execution config-IO API), plan/graph/loader.rs.
- Keep permission_mode everywhere; keep the unconditional bypass rejection
  from the previous stage; keep both gitignore helpers.
- Fixture sweep: fs/stage_loading.rs, plan/amendment.rs,
  plan/schema/tests/*, commands/init/tests.rs, orchestrator/core/mod.rs
  tests, tests/e2e/criteria_validation/*, tests/e2e/daemon_config/*,
  tests/integration/helpers.rs.
- Add a regression test that a stale [project_execution] table in
  .work/config.toml has no runtime effect (toml_edit round-trip may
  preserve it as an unknown section — harmless).

SUBAGENT FILE ASSIGNMENTS (disjoint; MAIN AGENT integrates + commits —
subagents never run git commit or loom stage complete):
  Subagent 1 — Terminal layer + liveness (loom-senior-software-engineer):
    Files Owned: loom/src/orchestrator/terminal/native/mod.rs,
      loom/src/orchestrator/terminal/mod.rs,
      loom/src/orchestrator/terminal/dispatcher.rs,
      loom/src/orchestrator/mod.rs, loom/src/orchestrator/liveness.rs
    Files Read-Only: none
  Subagent 2 — Orchestrator core + preflight (loom-senior-software-engineer):
    Files Owned: loom/src/orchestrator/core/**,
      loom/src/orchestrator/auto_merge.rs,
      loom/src/orchestrator/continuation/**,
      loom/src/orchestrator/preflight.rs
    Files Read-Only: loom/src/orchestrator/terminal/native/mod.rs
  Subagent 3 — Schema + models + config IO (loom-senior-software-engineer):
    Files Owned: loom/src/plan/schema/**, loom/src/plan/graph/loader.rs,
      loom/src/plan/amendment.rs, loom/src/fs/work_dir.rs,
      loom/src/fs/stage_loading.rs, loom/src/models/**
    Files Read-Only: none
  Subagent 4 — Hooks/sandbox/git backend-param removal (loom-senior-software-engineer):
    Files Owned: loom/src/hooks/**, loom/src/sandbox/**,
      loom/src/git/worktree/**, loom/src/fs/permissions/**
    Files Read-Only: none
  Subagent 5 — Command call sites + fixtures (loom-software-engineer):
    Files Owned: loom/src/commands/stage/merge_resolver.rs,
      loom/src/commands/sessions.rs,
      loom/src/commands/run/foreground.rs,
      loom/src/daemon/server/orchestrator.rs,
      loom/src/commands/repair.rs,
      loom/src/commands/init/**, loom/tests/**
    Files Read-Only: none
NO FILE OVERLAP between subagents. Subagents cannot each compile alone;
the MAIN AGENT runs the final cargo build/test/clippy and resolves any
straggler references.

MEMORY RECORDING (loom memory ONLY — never loom knowledge in a standard
stage, never Claude Code auto-memory under ~/.claude/projects/*/memory/):

- MISTAKE: loom memory note "mistake: ..."
- DECISION: loom memory decision "..." --context "..."
- SURPRISE/GOTCHA: loom memory note "found: ..."
Every subagent Task prompt MUST instruct the subagent to record memories
via loom memory and to NEVER run git commit or loom stage complete.

#### Files Changed

No changes recorded.

#### Key Decisions

- attempt_auto_merge signature: dropped (dispatcher, merge_backend) params, replaced with single (native: &NativeBackend) *(collapse-backend-scaffolding: Orchestrator now holds Arc<NativeBackend> as field 'native' instead of Arc<BackendDispatcher> 'dispatcher')*
- Deleted plan/schema/execution.rs entirely (incl. validate_git_identity + GIT_IDENTITY_MAX_LEN) *(grep confirmed validate_git_identity and GIT_IDENTITY_MAX_LEN had zero non-test callers across loom/src; no relocation needed)*
- Collapsed NativeBackend to a concrete inherent-impl type; LivenessService and Orchestrator now hold Arc<NativeBackend> directly *(TerminalBackend trait + BackendDispatcher + BackendType + preflight + plan/schema/execution.rs were vestigial single-variant scaffolding)*

#### Notes

- found: plan said delete plan/schema/execution.rs entirely, but validate_git_identity/GIT_IDENTITY_MAX_LEN lived there. Confirmed zero non-test callers so full deletion was safe — but always grep before deleting a module that mixes concerns.
- gotcha: 'loom' CLI fails under the command sandbox with 'bwrap: Cant create file at .work: Is a directory'. Run loom commands with the sandbox disabled.
- found: when removing a struct field across a workspace, subagents scoped to file-ownership lists missed ~25 straggler initializers (execution: None / execution_backend: None / backend: Default::default()) in test fixtures and core/*. Prevention: main agent must rg the WHOLE tree for '<field>:' after field removal, not rely on per-subagent grep.
- found: orchestrator::terminal also re-exports its own BackendType separate from plan::schema::execution::BackendType. Removing plan/schema/execution.rs does not remove the terminal one - that is a different subagent's scope.
- found: 'execution' field on plan schema lives on LoomConfig (types.rs:~287), not LoomMetadata which is just a {loom: LoomConfig} wrapper. Task said ~line 297 LoomMetadata but meant LoomConfig.
- found: setup_worktree_hooks in src/git/worktree/settings.rs:427 takes a backend: BackendType param that must also be dropped by the git/worktree subagent — stage_executor.rs now calls it without that arg, contract-consistent with HooksConfig::new dropping backend
- found: kill_session/is_session_alive in native/mod.rs used format!("loom-{stage_id}") for window title and bare stage_id for PID key — wrong for merge/knowledge/base-conflict sessions whose spawns use prefixed titles/keys. Fix: window_title_and_pid_key helper prefers session.tracking_key. Prevention: any OS-resource lookup keyed on a session must use tracking_key, not stage_id.

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final verification after the docs stage and both code stages are merged.
Verify FUNCTIONAL INTEGRATION, not just that tests pass.

Use parallel subagents and skills to maximize performance.

NEVER use Claude Code auto-memory (~/.claude/projects/*/memory/). ALL
memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST):

- Read this plan file from doc/plans/ (check .work/config.toml source_path)
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md for architecture context and mistakes

BUILD & TEST (zero tolerance — fix ALL warnings/errors, nothing is
"pre-existing"):

- cargo build, cargo test (full suite), cargo clippy -- -D warnings
- cargo fmt --check

CODE REVIEW (MANDATORY — parallel loom-code-reviewer subagents):

- security review (+ /loom-security-audit): the fail-closed
  bypass-permissions change especially — confirm native sessions cannot
  run with bypassPermissions
- architecture review: confirm the abstraction collapse is clean, no
  dead code, no half-removed scaffolding
- test coverage: the new tracking-key liveness tests, the stale-config
  regression test, the session-compat test
Fix ALL issues with loom-senior-software-engineer / loom-software-engineer.

FUNCTIONAL VERIFICATION (MANDATORY):

- loom --help shows no container subcommand
- loom plan verify on an existing plan still parses (proves removing the
  execution: schema fields did not break plan parsing)
- A plan with permission_mode: bypass-permissions is rejected
- Residual sweep returns nothing in live code (loom/src, loom/tests,
  loom/Cargo.toml, README.md, doc/loom/knowledge/ — NOT doc/plans/ archive/):
  BackendType, BackendDispatcher, TerminalBackend, create_backend,
  commands::container, orchestrator::terminal::container, git_bridge,
  ContainerCommands, execution_backend, StageExecutionConfig,
  project_execution, CompleteStageContainer, is_container_completion,
  delegate_completion_to_daemon, LOOM_BACKEND, handlebars, "/repo/"

Record discoveries to loom memory so knowledge-distill can curate them.

#### Files Changed

No changes recorded.

#### Key Decisions

- left mistakes.md:358 and concerns.md:191 container references intact *(both are past-tense historical incident narratives ('the new commit only added the container arm', 'the agent inside the container was genuinely stuck'); rewriting them would falsify the lessons/incident log. Only CURRENT-STATE docs were corrected.)*

#### Notes

- found+fixed: run_aggregated_wiring_reverification (commands/stage/complete.rs) double-applied working_dir. It was passed acceptance_dir (already = worktree_root + integration-verify.working_dir) then joined each prior stage's working_dir on top, producing loom/loom/src/... -> 'Wiring source file missing'. Fix: pass worktree_root (from StageExecutionPaths) through run_verification_phase into the aggregated re-verifier and join prior-stage working_dir against the worktree root. Detection rule: any path that re-applies a per-stage working_dir must start from the worktree ROOT, never from an already-resolved acceptance_dir.
- gotcha: cargo/loom commands fail under the command sandbox in a loom worktree with 'bwrap: Cant create file at .work: Is a directory' because .work is a symlink to a directory. Run all cargo/loom invocations with the sandbox disabled. loom check must be run from the WORKTREE ROOT, not the working_dir subdir, or it errors '.work directory does not exist'.
- found: container-backend removal left stale CURRENT-STATE references in live code and knowledge docs that the residual sweep caught. Fixed in integration-verify: monitor/{core,handlers,detection}.rs ('dispatcher' -> NativeBackend/LivenessService), daemon/server/client.rs (admin-token rationale was 'containers mount .work' -> '.work is a shared worktree-visible tree'), commands/stage/{complete,knowledge_complete}.rs (isolated-git/container comments), and doc/loom/knowledge/{architecture,entry-points,patterns,mistakes}.md. Prevention: a stage that OWNS doc cleanup (strip-container-docs) must rg the whole tree incl. comments and table cells, not just identifier deletions.
- found: 'loom plan verify' does NOT reject a plan with sandbox.permission_mode=bypass-permissions — it reports 0 errors. Fail-closed rejection happens only at 'loom init' (commands/init/plan_setup.rs validates merged sandbox config via sandbox::config::validate_config) and at spawn time (stage_executor.rs). Verified: 'loom init' on a bypass plan exits 1 with a clear error AND cleans up .work/. Gap worth closing: plan verify could surface the same validation.

### Knowledge Distillation (knowledge-distill)

**Status:** executing  
**Purpose:** Curate all stage memories into permanent knowledge files, and do a final
documentation pass.

Use parallel subagents and skills to maximize performance.

NEVER use Claude Code auto-memory (~/.claude/projects/*/memory/). ALL
memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST):

- Read this plan file from doc/plans/ (check .work/config.toml source_path)
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md to understand current state

MEMORY CURATION:

- Synthesize memories from all stages. Curate mistakes as actionable
  prevention rules via loom knowledge update mistakes "..."
- Curate patterns + architectural decisions: the native-only execution
  model, why the container backend was removed, the NativeBackend concrete
  type, the tracking-key liveness fix.
- Update architecture.md / entry-points.md to reflect the native-only
  structure (the strip-container-docs stage removed container sections;
  this stage ensures what remains is accurate and coherent).
- Run loom review; remove stale entries.

DOCUMENTATION UPDATE:

- Confirm README.md reflects the native-only CLI (strip-container-docs
  already removed container sections — verify coherence, fix any dangling
  references).
- If no further user-facing doc change is needed, record why in memory.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- found: after parallel subagent deletion stage (remove-container-keep-scaffolding), 7 straggler files were NOT in any subagent's assignment table: commands/mod.rs, completions/dynamic/tests.rs, plan/schema/mod.rs, commands/handoff/create.rs, commands/stage/tests/session.rs, orchestrator/preflight.rs. These failed to compile. Prevention: always run cargo build AND cargo test --no-run as main agent after parallel deletion — test-only files don't surface in a lib build.
- found: run_aggregated_wiring_reverification (commands/stage/complete.rs) double-applied working_dir — passed acceptance_dir (already = worktree_root + integration-verify.working_dir) and then joined each prior stage's working_dir on top, producing loom/loom/src/... paths. Fixed by passing worktree_root through run_verification_phase and joining per-stage working_dir against the worktree root.
- found: loom plan verify does NOT reject sandbox.permission_mode=bypass-permissions — that validation only happens at loom init (commands/init/plan_setup.rs validates via sandbox::config::validate_config) and at spawn time. Gap: plan verify could surface the same validation to catch invalid plans earlier.
- found: session liveness in native/mod.rs used format!("loom-{stage_id}") for window title and bare stage_id for PID key — wrong for merge/knowledge/base-conflict sessions whose spawns use prefixed titles/keys. Correct key is session.tracking_key. Prevention: any OS-resource lookup keyed on a session must use tracking_key, not stage_id.

## Open Questions

No open questions.
