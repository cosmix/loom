# Code Review: Default to Accept-Edits and Enable Claude Code Remote Control

**Plan:** PLAN-accept-edits-and-remote-control | **Generated:** 2026-05-14 17:27 UTC

## Summary

This plan changes loom's native backend to default Standard and IntegrationVerify stages to `AcceptEdits` permission mode (instead of `Auto`), and adds Claude Code Remote Control support that auto-enables when prerequisites are met (claude ≥2.1.51, claude.ai login, workspace trust). The implementation includes capability detection, a one-time preflight that warns operators advisorily, and a fallback mechanism that disables Remote Control for a run if a spawned session fails early. Documentation is updated to reflect the new defaults and Remote Control behavior.

## Changes by Stage

### Bootstrap Knowledge Base (knowledge-bootstrap)

**Status:** completed  
**Purpose:** Targeted recon + verification stage. Knowledge coverage is populated but
partly STALE - the doc/loom/knowledge files still describe a container
backend that was removed (commits 5bcf5d8 / c2f16bb). Do NOT trust
knowledge prose about backends; verify everything against live source.

Use parallel subagents and skills to maximize performance.

Read doc/loom/knowledge/*.md for general context, then VERIFY against the
current source and pin exact file:line for each of:

1. sandbox/config.rs: the PermissionMode enum import, default_mode_for()
   (takes only StageType - there is NO BackendType in this checkout),
   merge_config(), validate_config(), and the tests
   test_default_mode_for_stage_type and
   test_merge_config_permission_mode_precedence.
2. sandbox/settings.rs: where permissions.defaultMode is emitted
   (apply_default_mode).
3. orchestrator/terminal/native/mod.rs: the four claude command-build
   sites - spawn_session, spawn_merge_session,
   spawn_base_conflict_session, spawn_knowledge_session.
4. orchestrator/terminal/native/pid_tracking.rs: create_wrapper_script and
   the wrapper template (LOOM_MAIN_AGENT_PID=$$, echo $$ > pid_file,
   exec {claude_cmd}).
5. fs/work_dir.rs: the centralized .work/config.toml API (read_config,
   write_config, the private read_section/write_section generics, and
   read_plan_sandbox / write_plan_sandbox as the pattern to mirror).
6. commands/init/plan_setup.rs: initialize_with_plan and where it writes
   config.toml sections.
7. commands/run/mod.rs: execute_background; commands/run/foreground.rs:
   execute and execute_foreground.
8. orchestrator/core/crash_handler.rs: handle_session_crashed (failure
   classification + retry).
9. src/claude.rs: find_claude_path().

Then add a section titled exactly:

## Remote Control & Permission Mode Integration Points

to doc/loom/knowledge/entry-points.md (via loom knowledge update
entry-points) with the confirmed file:line references, plus a one-line
note that container-backend references elsewhere in knowledge are stale.

MEMORY & KNOWLEDGE RECORDING:

- Record every pinned location: loom memory note "found: <thing> at file:line"
- Record any place where knowledge prose contradicted the live code
- Use loom knowledge update entry-points for the new section
- Before completing: loom memory list (verify the pins are captured)

Do NOT use Claude Code auto-memory (~/.claude/projects/*/memory/).
Use loom knowledge / loom memory commands only.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- stale knowledge prose: entry-points.md Sandbox section says 'MergedSandboxConfig, merge_config(), expand_paths()' but is missing PermissionMode / default_mode_for detail. WorkDir section says disputes_dir/plan_versions_dir are MISSING but both are now implemented at :239-247. Knowledge files may still reference container-backend which was removed in commits 5bcf5d8/c2f16bb.
- found: src/claude.rs find_claude_path() :10-33; uses which::which first then checks ~/.claude/local/claude, ~/.local/bin/claude, ~/.cargo/bin/claude, /usr/local/bin/claude, /opt/homebrew/bin/claude
- found: orchestrator/core/crash_handler.rs handle_session_crashed() :15-137; classify_failure() called :49; should_auto_retry() called :65; try_mark_blocked() at :114; sync_worktree_permissions_with_working_dir best-effort sync :90-110
- found: commands/run/mod.rs execute_background() :31-84; commands/run/foreground.rs execute() :17-36 (public, calls execute_foreground), execute_foreground() :39-end (private, builds OrchestratorConfig with sandbox_config from plan_sandbox)
- found: commands/init/plan_setup.rs initialize_with_plan() :27-214; writes [plan] table via toml_edit at :139-157; writes [plan_sandbox] via work_dir::write_plan_sandbox at :160-162; create_stage_from_definition() :217-288
- found: fs/work_dir.rs centralized config.toml API: read_config() :356-366; write_config() :370-381; private read_section<T>() :383-402; private write_section<T>() :404-429; read_plan_sandbox() :435-437; write_plan_sandbox() :440-442 (pattern to mirror for new sections)
- found: orchestrator/terminal/native/pid_tracking.rs create_wrapper_script() :250-368; wrapper template format\!() :321-350; LOOM_MAIN_AGENT_PID=$$ at :332; echo $$ > pid_file at :337; exec {claude_cmd} at :340
- found: orchestrator/terminal/native/mod.rs four command-build sites: spawn_session :91-165 (claude_cmd :121-129), spawn_merge_session :167-239 (claude_cmd :199-204 hardcoded opus[1m]/xhigh), spawn_base_conflict_session :241-314 (claude_cmd :275-280 hardcoded opus[1m]/xhigh), spawn_knowledge_session :316-389 (claude_cmd :346-353)
- found: sandbox/settings.rs:15-26 apply_default_mode() is the single place that maps PermissionMode -> permissions.defaultMode camelCase; called at :300 inside generate_settings_json()
- found: sandbox/config.rs:1-4 PermissionMode enum imported from plan::schema (with StageType, no BackendType); default_mode_for() at :49-54 takes only StageType; merge_config() at :60-95; validate_config() at :102-112; test_default_mode_for_stage_type :461-475; test_merge_config_permission_mode_precedence :478-511

### Implement Accept-Edits Default + Remote Control (implement-defaults-and-remote-control)

**Status:** completed  
**Purpose:** Change the per-stage-type permission-mode default and add Claude Code
Remote Control support for the native backend (the only backend in this
checkout).

Use parallel subagents and skills to maximize performance. Spawn BOTH
subagents in ONE message. They own strictly non-overlapping files.

Read the knowledge-bootstrap memory pins first (loom memory show --all)
so you work from confirmed file:line references, not stale knowledge prose.

SUBAGENT FILE ASSIGNMENTS:
  Subagent 1 - Permission Mode Default (loom-software-engineer):
    Files Owned:
      - loom/src/sandbox/config.rs
    Files Read-Only:
      - loom/src/plan/schema/types.rs
  Subagent 2 - Remote Control (loom-senior-software-engineer):
    Files Owned:
      - loom/src/remote_control.rs (NEW)
      - loom/src/lib.rs
      - loom/src/fs/work_dir.rs
      - loom/src/commands/init/plan_setup.rs
      - loom/src/commands/run/mod.rs
      - loom/src/commands/run/foreground.rs
      - loom/src/orchestrator/terminal/native/mod.rs
      - loom/src/orchestrator/terminal/native/pid_tracking.rs
      - loom/src/orchestrator/core/crash_handler.rs
    Files Read-Only:
      - loom/src/claude.rs
      - loom/src/models/session/types.rs
      - loom/src/sandbox/settings.rs

NO FILE OVERLAP between subagents confirmed: Subagent 1 touches only
loom/src/sandbox/config.rs; Subagent 2 never touches sandbox/config.rs.

SUBAGENT 1 TASKS (Permission Mode Default):

- In loom/src/sandbox/config.rs, default_mode_for() (~line 49): change the
  match arm
    StageType::Standard | StageType::IntegrationVerify => PermissionMode::Auto
  to => PermissionMode::AcceptEdits. Leave the Knowledge | KnowledgeDistill
  arm unchanged.
- Update the doc comment above default_mode_for() that states
  "Standard / IntegrationVerify -> Auto".
- Update test_default_mode_for_stage_type() (~line 461) and the
  StageType::Standard default-case assertion inside
  test_merge_config_permission_mode_precedence() (~line 492) to expect
  PermissionMode::AcceptEdits.
- DO NOT remove the Auto variant from the PermissionMode enum. Confirm
  validate_config() still accepts Auto (it only rejects BypassPermissions).
- Add a test standard_stage_defaults_to_accept_edits asserting
  default_mode_for() returns AcceptEdits for both StageType::Standard and
  StageType::IntegrationVerify.

SUBAGENT 2 TASKS (Remote Control):

- Create loom/src/remote_control.rs and register it in lib.rs with
  pub mod remote_control;. It must provide:
  - RemoteControlMode (enum Auto | Off, serde kebab-case, default Auto)
      and RemoteControlConfig { mode: RemoteControlMode } (serde,
      Default = Auto).
  - claude_supports_remote_control(claude_path) -> bool: runs
      "<claude_path> --version", parses the semver, returns true when
      version >= 2.1.51. Treat exec/parse failure as false.
  - remote_control_eligible() -> Result<(), String>: heuristic auth
      check. Positive signal: ~/.claude/.credentials.json exists.
      Disqualifying signals (return Err with a non-secret reason):
      ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN, CLAUDE_CODE_USE_BEDROCK,
      CLAUDE_CODE_USE_VERTEX, CLAUDE_CODE_USE_FOUNDRY set in the
      environment. Consult the current Remote Control docs for the
      authoritative disqualifying set. NEVER log the values of these
      vars - only their names.
  - preflight(claude_path) -> RemoteControlStatus where
      RemoteControlStatus is Enabled or Disabled { reason: String }.
      Combines version + eligibility.
  - resolve(work_dir) -> bool: the per-spawn gate used by the native
      backend. Returns false when read_remote_control_config() mode is
      Off OR the .work/remote_control-unsupported marker file exists;
      otherwise returns preflight(find_claude_path()?).is_enabled().
      Memoize the version subprocess with std::sync::OnceLock so it runs
      at most once per process, but always re-check the config + marker
      (cheap file reads) so a mid-run marker takes effect immediately.
  Use anyhow::Result and the error-handling conventions in
  conventions.md. Keep the file under 400 lines.
- In loom/src/fs/work_dir.rs: add a REMOTE_CONTROL_SECTION const and
  read_remote_control_config(work_dir) -> Result<RemoteControlConfig> /
  write_remote_control_config(work_dir, &RemoteControlConfig) helpers,
  mirroring read_plan_sandbox / write_plan_sandbox exactly (toml_edit via
  the existing read_section / write_section generics). A missing section
  must default to RemoteControlConfig::default() (mode = Auto).
- In loom/src/commands/init/plan_setup.rs, initialize_with_plan: after the
  existing config.toml writes, persist the default [remote_control]
  section via work_dir::write_remote_control_config.
- Add a shared preflight helper (e.g. remote_control::run_startup_preflight
  or a helper in commands/run) and call it from BOTH
  commands/run/mod.rs::execute_background AND
  commands/run/foreground.rs::execute_foreground. It runs preflight()
  ONCE, and on Disabled prints a clear operator warning naming the reason
  (e.g. "Remote Control disabled: claude 2.0.x < 2.1.51" or
  "... claude.ai login not found"). Advisory only - it must NOT abort
  startup. When config mode is Off, skip probing entirely.
- In loom/src/orchestrator/terminal/native/mod.rs: extract a pure,
  testable command-builder helper, e.g.
    fn build_claude_command(claude_path, model, effort,
                            remote_control_enabled, escaped_prompt) -> String
  and route ALL FOUR spawn sites (spawn_session, spawn_merge_session,
  spawn_base_conflict_session, spawn_knowledge_session) through it.
  Append " --remote-control" (before the prompt positional, alongside
  --model/--effort) only when remote_control_enabled is true. Each site
  computes remote_control_enabled = remote_control::resolve(work_dir).
- In loom/src/orchestrator/terminal/native/pid_tracking.rs: in the wrapper
  script template, export CLAUDE_REMOTE_CONTROL_SESSION_NAME_PREFIX=loom.
  Export it unconditionally - it is inert when Remote Control is off and
  makes RC sessions identifiable when it is on. DO NOT change the
  exec {claude_cmd} line or any other wrapper behavior - the exec
  invariant and PID-file semantics must stay exactly as they are.
- In loom/src/orchestrator/core/crash_handler.rs, handle_session_crashed:
  implement the orchestrator-level fallback. If remote_control::resolve()
  currently returns true AND the crashed session is a native session that
  exited non-zero within ~15s of session.created_at, write the
  .work/remote_control-unsupported marker file (best-effort, ignore write
  errors) BEFORE the existing retry/backoff logic runs. The existing
  retry then respawns the stage, and resolve() will return false because
  the marker exists - so the retry omits --remote-control. Do not add new
  retry paths; reuse the existing classification + backoff.

CRITICAL: claude --remote-control exits non-zero if prerequisites are not
met. The flag must ONLY be appended when resolve() is true. Never pass it
unconditionally. The wrapper script must NOT be turned into a
run-check-retry script - the fallback lives in the crash handler so the
exec / PID / hook invariants are preserved.

MEMORY RECORDING (use loom memory ONLY - never loom knowledge, never
auto-memory):

- MISTAKE: loom memory note "mistake: tried X, failed because Y, fixed by Z"
- DECISION: loom memory decision "chose X" --context "why"
- SURPRISE/GOTCHA: loom memory note "found: ... at file:line"
- Do NOT record procedural actions.
- Tell BOTH subagents in their Task prompts to record mistakes,
  decisions, and surprises with loom memory.

#### Files Changed

- loom/src/sandbox/config.rs - default_mode_for() Standard|IntegrationVerify now returns AcceptEdits instead of Auto; Knowledge|KnowledgeDistill unchanged; Auto variant retained and still accepted by validate_config()

#### Key Decisions

- Remote Control fallback lives in crash_handler, not the wrapper script *(claude --remote-control exits non-zero when prerequisites unmet. Putting a run-check-retry in the pid_tracking wrapper would break the exec/PID-file/hook invariants. Instead: crash_handler.handle_session_crashed writes .work/remote_control-unsupported marker when a native session crashes within 15s of created_at while resolve()==true; the existing retry/backoff then respawns and resolve() returns false so --remote-control is omitted. No new retry paths.)*
- build_claude_command extracted as a pure helper in native/mod.rs *(Four spawn sites (spawn_session, spawn_merge_session, spawn_base_conflict_session, spawn_knowledge_session) duplicated claude_cmd construction. A single pure fn(claude_path, model, effort, remote_control_enabled, escaped_prompt)->String is unit-testable and guarantees --remote-control is appended consistently (before the prompt positional) and only when resolve(work_dir) is true.)*

#### Notes

- found: resolve(work_dir) in remote_control.rs memoizes only the claude --version subprocess via OnceLock, but always re-reads config + marker file each call — so a mid-run .work/remote_control-unsupported marker (written by the crash handler) takes effect on the very next spawn without a process restart.
- found: clippy never-loops bug in original parse_version draft in remote_control.rs — using '?' on the first split token short-circuits before the multi-token fallback runs. Fix: extract a parse_version_token helper returning Option and handle tokens explicitly. Rule: when a fn has a fallback branch, don't use '?' in the primary branch — it skips the fallback.

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final integration verification - runs AFTER the implementation stage.

Use parallel subagents and skills to maximize performance.

CRITICAL: verify FUNCTIONAL INTEGRATION, not just that tests pass.

Do NOT use Claude Code auto-memory (~/.claude/projects/*/memory/).
ALL memory goes through loom memory commands.

CONTEXT GATHERING (FIRST):

- Read the plan file from doc/plans/ (check .work/config.toml source_path)
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md for context (note: container-backend prose
  is stale - verify against live code)

ZERO TOLERANCE: every compiler warning, clippy lint, and test failure
must be FIXED, not suppressed. Nothing is "pre-existing".

BUILD & TEST:

- Full cargo test
- cargo clippy --all-targets -- -D warnings
- cargo build

CODE REVIEW (MANDATORY - parallel loom-code-reviewer subagents):

- Security review (with /loom-security-audit): confirm --remote-control is
  never passed unconditionally; a failed preflight cannot abort loom run;
  the crash-handler fallback is best-effort and cannot break retry; no
  secrets or env-var values are logged by preflight or the eligibility
  check.
- Architecture review: remote_control.rs placement and size, the
  command-builder seam in native/mod.rs, the OnceLock memoization,
  config layering through fs/work_dir.rs.
- Test coverage review: version-parsing boundaries (old/exact 2.1.51/
  new/malformed), resolve() with mode Off and with the marker present,
  the four native command sites, the permission-mode default tests.
Fix ALL issues found with loom-senior-software-engineer /
loom-software-engineer (loom-code-reviewer is read-only).

FUNCTIONAL VERIFICATION (MANDATORY):

- default_mode_for() returns AcceptEdits for StageType::Standard and
  StageType::IntegrationVerify, and the generated settings.local.json
  emits "acceptEdits" for those stage types.
- An explicit PermissionMode::Auto set at plan or stage level still wins
  (merge_config precedence) and validate_config still accepts it.
- The native command builder appends --remote-control when resolve() is
  true and omits it when false - exercise BOTH branches via the unit
  tests / a forced status.
- The Remote Control preflight is invoked from BOTH execute_background
  and execute_foreground and is advisory-only (does not abort startup).
- The crash handler writes .work/remote_control-unsupported on a fast
  non-zero exit of a remote-control native session, and resolve() then
  returns false.

Record discoveries to loom memory so knowledge-distill can curate them.

#### Files Changed

No changes recorded.

#### Key Decisions

- integration-verify: applied minimal fixes only *(code-reviewer flagged 3 'important' items; 2 were documented design choices or non-issues (OnceLock process-lifetime memoization is intentional; shell-interp is safe due to upstream validation). Only the real gap — missing version-comparison test coverage — was fixed, plus the cheap consolidation it enabled.)*

#### Notes

- found: version gate (>= MIN_REMOTE_CONTROL_VERSION) was duplicated inline in claude_supports_remote_control and preflight, and had no boundary test coverage despite the signal asking for old/exact/new tests. Fixed: extracted version_supported() helper as single source of truth + added version_supported_covers_boundaries test.
- verified: shell-injection concern in build_claude_command (native/mod.rs:76) is NOT exploitable — effort is constrained by ALLOWED_REASONING_EFFORTS via deserialize_reasoning_effort serde validator (rejects shell metacharacters), claude_path comes from find_claude_path() (not user-controlled), model is escape()d. Boundary validation in plan/schema/types.rs:597-623 holds.

### Knowledge Distillation (knowledge-distill)

**Status:** executing  
**Purpose:** Knowledge distillation - runs after integration-verify to curate all
stage memories into permanent knowledge files and update user-facing
documentation.

Do NOT use Claude Code auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge.

CONTEXT GATHERING (FIRST):

- Read the plan file from doc/plans/ (check .work/config.toml source_path)
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md to understand the current state

MEMORY CURATION:

- Synthesize stage memories - patterns, mistakes, decisions worth keeping.
- Curate mistakes as actionable prevention rules with
  loom knowledge update mistakes. In particular capture the lesson that
  knowledge prose can go stale (container backend) and plans must be
  verified against live code.
- Update doc/loom/knowledge/patterns.md and architecture.md for:
  - the new per-stage-type permission-mode default (acceptEdits)
  - the Remote Control capability/preflight/resolve pattern, the
      .work/[remote_control] config section, and the crash-handler
      fallback marker.
- Where this plan's work touched knowledge sections that still reference
  the removed container backend, correct those references.
- Run loom review; remove stale or contradicted entries.

DOCUMENTATION UPDATE (MANDATORY):

- Update README.md / CONTRIBUTING.md sections describing permission modes
  and session spawning to reflect the acceptEdits default and the Remote
  Control behavior (auto-enabled when supported; prereqs claude >= 2.1.51
  - claude.ai login; [remote_control] config knob; fallback marker).
- Only touch sections relevant to this plan - do not rewrite files.
- If a doc area genuinely needs no change, record WHY in loom memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- update knowledge files directly with Edit/replace-section rather than append-only loom knowledge update *(The stale entries need surgical in-place correction, not appending. Appending would duplicate the section. loom knowledge replace-section is the right tool for section rewrites; Edit is used for targeted line-level corrections.)*

#### Notes

- found: patterns.md Sandbox permission_mode Resolution table still shows auto for Standard/IntegrationVerify — stale. Also entry-points.md Remote Control section §1 says Standard / IntegrationVerify → Auto — stale. Need replace-section for both.
- found: architecture.md line 178 and entry-points.md line 445 both reference Standard/IntegrationVerify → auto for default_mode_for(). This is STALE — default_mode_for() now returns AcceptEdits for ALL four stage types. The change landed in sandbox/config.rs:52-53. Prevention: after changing sandbox defaults, rg knowledge files for the old value.

## Open Questions

No open questions.
