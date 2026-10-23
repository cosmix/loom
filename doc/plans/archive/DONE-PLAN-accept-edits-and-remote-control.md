# Plan: Default to Accept-Edits and Enable Claude Code Remote Control

## Overview

This plan reconfigures how loom invokes Claude Code so that:

1. **Every spawned session defaults to `acceptEdits` permission mode** instead of the
   research-preview `auto` mode. `auto` stays a valid value users can still select
   per-stage or per-plan — only the *defaults* change.
2. **loom's native terminal sessions are launched with Claude Code's Remote Control
   feature enabled when the host supports it**, so operators can steer their loom
   sessions from a phone, browser, or another machine.

> **Note on scope:** loom's container backend was removed (commits `5bcf5d8` /
> `c2f16bb`); the native terminal backend is the only backend in this checkout. Some
> `doc/loom/knowledge/*.md` files still describe the container backend and are stale —
> implementation agents must verify against live code, not knowledge prose. This plan
> is native-only by construction.

Remote Control is auto-enabled when supported: loom probes the local `claude` binary
(version + login type + disqualifying env vars) and only passes `--remote-control`
when prerequisites are met. `loom run` (both background and `--foreground`) performs a
one-time preflight; if Remote Control cannot work it prints a clear operator warning
and runs sessions without the flag — sessions never crash because of an unsupported
flag.

Because the login-type check is only a heuristic, an **orchestrator-level fallback**
makes it non-load-bearing: if a native session that was spawned with
`--remote-control` exits non-zero shortly after spawn, loom's crash handler writes a
`.work/remote_control-unsupported` marker before the existing retry runs. `resolve()`
reads that marker, so the retry — and every later spawn this run — omits the flag. The
wrapper script is unchanged (still a plain `exec`), so all PID / hook / liveness
invariants are preserved.

## Background (verified against this checkout, 2026-05-14)

- `PermissionMode` (`loom/src/plan/schema/types.rs:~12`) has exactly five variants:
  `Default`, `AcceptEdits`, `Auto`, `Plan`, `BypassPermissions`. `as_settings_value()`
  maps them to the camelCase strings Claude Code expects in
  `permissions.defaultMode`.
- The per-stage-type default is `default_mode_for(stage_type: StageType)` in
  `loom/src/sandbox/config.rs:~49` — it takes ONLY a `StageType` (there is no
  `BackendType` in this checkout). It currently returns `Auto` for `Standard` /
  `IntegrationVerify` and `AcceptEdits` for `Knowledge` / `KnowledgeDistill`.
  `merge_config()` resolves `stage > plan > default_mode_for`. `validate_config()`
  rejects only `BypassPermissions`, so explicit `Auto` stays allowed.
- loom's native backend spawns `claude` as an interactive TTY session (it passes
  `--model` / `--effort` + the prompt, NOT `--print`), so Remote Control applies.
  Command strings are built at four sites in
  `loom/src/orchestrator/terminal/native/mod.rs`: `spawn_session` (~120),
  `spawn_merge_session` (~195), `spawn_base_conflict_session` (~271),
  `spawn_knowledge_session` (~345).
- Claude Code Remote Control is enabled per-invocation with `--remote-control`
  (alias `--rc`), combines cleanly with `--model` / `--effort` + an initial prompt,
  and requires `claude` >= 2.1.51, a claude.ai subscription login (NOT an API key),
  and accepted workspace trust. If a prerequisite is missing, `claude
  --remote-control` exits non-zero — so the flag must be capability-gated, never
  passed unconditionally. `CLAUDE_REMOTE_CONTROL_SESSION_NAME_PREFIX` sets the
  remote session name prefix.
- `.work/config.toml` has a single centralized read/write API in
  `loom/src/fs/work_dir.rs` (`read_config`/`write_config` + the private
  `read_section`/`write_section` generics; `read_plan_sandbox`/`write_plan_sandbox`
  are the pattern to mirror). `LoomConfig` has no general execution-config slot, so
  the `[remote_control]` setting is a runtime-only `.work/config.toml` section
  (Option A), not plan YAML.

## Goals

- Change the per-stage-type default `PermissionMode` for `Standard` and
  `IntegrationVerify` from `Auto` to `AcceptEdits` in `default_mode_for()`.
- Keep `PermissionMode::Auto` selectable as an explicit plan/stage value.
- Add a `remote_control` module: capability detection, preflight, and a per-spawn
  `resolve()` gate.
- Add a `[remote_control]` section to `.work/config.toml` (`mode = "auto"` default,
  or `"off"`), read/written through `fs/work_dir.rs`.
- Make all four native command-construction sites append `--remote-control` only
  when `resolve()` is true, via one shared, testable command-builder helper.
- Run a one-time preflight in both `loom run` paths (background + foreground) that
  warns the operator and disables Remote Control for the run when unsupported.
- Add an orchestrator-level fallback: a fast non-zero exit of a remote-control
  session writes `.work/remote_control-unsupported` so the existing retry runs
  without the flag.
- Non-goals: new user-facing CLI commands; removing the `auto` mode; any wrapper
  script behavior change beyond an inert session-name-prefix env var.

## Execution Diagram

```mermaid
graph LR
    knowledge-bootstrap --> implement-defaults-and-remote-control
    implement-defaults-and-remote-control --> integration-verify
    integration-verify --> knowledge-distill
```

Each node is a worktree stage. The single implementation stage runs two parallel
subagents with non-overlapping file ownership.

---

## Stages

### 1. Knowledge Bootstrap (targeted recon + verification)

**Purpose:** Knowledge files are populated but partly stale (they still describe the
removed container backend). This stage verifies the exact integration points this
plan touches against the **live code**, records them to loom memory, and adds a
focused entry-points section so the implementation subagents work from confirmed
references.

**Tasks:**

- Confirm (against current source) and pin: `PermissionMode` enum + `default_mode_for`
  - `merge_config` + `validate_config` in `sandbox/config.rs`; the `permissions.default
  Mode` emission in `sandbox/settings.rs`; the four native command sites in
  `orchestrator/terminal/native/mod.rs`; `create_wrapper_script` + the `exec` template
  in `native/pid_tracking.rs`; the centralized config API + `read/write_plan_sandbox`
  pattern in `fs/work_dir.rs`; `initialize_with_plan` in `commands/init/plan_setup.rs`;
  `execute_background` in `commands/run/mod.rs` and `execute`/`execute_foreground` in
  `commands/run/foreground.rs`; `handle_session_crashed` in
  `orchestrator/core/crash_handler.rs`; `find_claude_path()` in `claude.rs`.
- Add a `## Remote Control & Permission Mode Integration Points` section to
  `doc/loom/knowledge/entry-points.md` with the confirmed `file:line` references, and
  a one-line note that container-backend references elsewhere in knowledge are stale.
- Record every pin to loom memory.

**Files:** `doc/loom/knowledge/**`

**Acceptance:** entry-points.md contains the new integration-points section.

---

### 2. Implement Accept-Edits Default + Remote Control

**Purpose:** Change the permission-mode defaults and add Remote Control end to end.
Two parallel subagents, strictly non-overlapping files.

**Dependencies:** knowledge-bootstrap

**Subagent 1 — Permission Mode Default (loom-software-engineer):**

- In `loom/src/sandbox/config.rs`, `default_mode_for()` (~line 49): change the match
  arm `StageType::Standard | StageType::IntegrationVerify => PermissionMode::Auto` to
  `=> PermissionMode::AcceptEdits`. Leave the `Knowledge | KnowledgeDistill` arm alone.
- Update the doc comment above `default_mode_for()` that states "Standard /
  IntegrationVerify → `Auto`".
- Update `test_default_mode_for_stage_type()` (~line 461) and the
  `StageType::Standard` default-case assertion inside
  `test_merge_config_permission_mode_precedence()` (~line 492) to expect
  `AcceptEdits`.
- Do NOT remove the `Auto` enum variant — confirm `validate_config()` still accepts it.
- Add a test `standard_stage_defaults_to_accept_edits` asserting `default_mode_for()`
  returns `AcceptEdits` for both `StageType::Standard` and `StageType::IntegrationVerify`.

**Subagent 2 — Remote Control (loom-senior-software-engineer):**

- New `loom/src/remote_control.rs`: `RemoteControlConfig` / `RemoteControlMode`
  (`auto` default | `off`) types; capability detection (`claude --version` >= 2.1.51);
  an auth/eligibility heuristic that treats `~/.claude/.credentials.json` presence as
  positive and the presence of `ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`,
  `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, or `CLAUDE_CODE_USE_FOUNDRY` as
  disqualifying (consult current Remote Control docs for the authoritative set);
  `preflight()` returning `Enabled` / `Disabled { reason }`; and a memoized
  `resolve(work_dir)` used per-spawn that returns false when config mode is `off` OR
  the `.work/remote_control-unsupported` marker exists. Register in `lib.rs`.
- `fs/work_dir.rs`: add `read_remote_control_config()` / `write_remote_control_config()`
  - a `REMOTE_CONTROL_SECTION` const, mirroring `read/write_plan_sandbox`.
- `commands/init/plan_setup.rs`: write the default `[remote_control]` section in
  `initialize_with_plan`.
- A shared preflight helper called by BOTH `commands/run/mod.rs::execute_background`
  and `commands/run/foreground.rs::execute_foreground`: runs `preflight()` once, logs
  a clear operator warning when `Disabled` (naming the reason), never aborts startup.
- A pure command-builder seam in `native/mod.rs` taking
  `(claude_path, model, effort, remote_control_enabled, escaped_prompt)` and returning
  the command string; all four spawn sites call it. Append `--remote-control` only
  when `remote_control::resolve()` is true.
- `native/pid_tracking.rs`: export `CLAUDE_REMOTE_CONTROL_SESSION_NAME_PREFIX=loom`
  in the wrapper template (unconditional — inert when Remote Control is off). The
  `exec {claude_cmd}` line and all other wrapper behavior stay exactly as-is.
- `orchestrator/core/crash_handler.rs`: in `handle_session_crashed`, if
  `remote_control::resolve()` currently returns true and the crashed native session
  exited non-zero within ~15s of `session.created_at`, write the
  `.work/remote_control-unsupported` marker (best-effort) before the existing retry
  logic runs.

**Files:** `loom/src/sandbox/config.rs` (Subagent 1); `loom/src/remote_control.rs`,
`loom/src/lib.rs`, `loom/src/fs/work_dir.rs`, `loom/src/commands/init/plan_setup.rs`,
`loom/src/commands/run/mod.rs`, `loom/src/commands/run/foreground.rs`,
`loom/src/orchestrator/terminal/native/mod.rs`,
`loom/src/orchestrator/terminal/native/pid_tracking.rs`,
`loom/src/orchestrator/core/crash_handler.rs` (Subagent 2).

**Acceptance:** `cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`.

---

### 3. Integration Verification

**Purpose:** Build, test, code-review, and functionally verify both changes are wired
in. No doc changes here.

**Dependencies:** implement-defaults-and-remote-control

**Tasks:**

- Build & test: full `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build`. Zero tolerance on warnings.
- Code review (parallel loom-code-reviewer subagents): security — confirm
  `--remote-control` is never passed unconditionally, a failed preflight cannot abort
  startup, the crash-handler fallback is best-effort, and no secrets/tokens are logged;
  architecture — module placement, the command-builder seam, config layering;
  test coverage. Fix all findings.
- Functional verification: `default_mode_for()` returns `AcceptEdits` for Standard /
  IntegrationVerify and the generated `settings.local.json` emits `acceptEdits`;
  explicit `Auto` still wins when set in plan/stage YAML; the native command builder
  appends `--remote-control` only when `resolve()` is true (exercise both branches);
  the preflight is invoked from both `loom run` paths and is advisory-only; the
  crash-handler writes the marker on a fast remote-control crash and `resolve()` then
  returns false.

**Files:** `loom/src/**`

**Acceptance:** `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build`.

---

### 4. Knowledge Distillation

**Purpose:** Curate stage memories into permanent knowledge and update user-facing
docs (README / CONTRIBUTING) for the new `acceptEdits` default and Remote Control
behavior. Also correct stale container-backend references in knowledge where this
plan's work touched them.

**Dependencies:** integration-verify

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    auto_allow: true
    excluded_commands:
      - "loom"
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
        - "~/.gnupg/**"
        - "~/.config/gcloud/**"
    network:
      allowed_domains:
        - "crates.io"
        - "index.crates.io"
        - "static.crates.io"
        - "github.com"
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: knowledge-bootstrap
      name: "Bootstrap Knowledge Base"
      stage_type: knowledge
      model: "sonnet"
      reasoning_effort: "high"
      description: |
        Targeted recon + verification stage. Knowledge coverage is populated but
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
      dependencies: []
      acceptance:
        - 'rg -qF "## Remote Control & Permission Mode Integration Points" doc/loom/knowledge/entry-points.md'
      files:
        - "doc/loom/knowledge/**"
      working_dir: "."
      artifacts:
        - "doc/loom/knowledge/entry-points.md"

    - id: implement-defaults-and-remote-control
      name: "Implement Accept-Edits Default + Remote Control"
      stage_type: standard
      model: "opus[1m]"
      reasoning_effort: "xhigh"
      description: |
        Change the per-stage-type permission-mode default and add Claude Code
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
            * RemoteControlMode (enum Auto | Off, serde kebab-case, default Auto)
              and RemoteControlConfig { mode: RemoteControlMode } (serde,
              Default = Auto).
            * claude_supports_remote_control(claude_path) -> bool: runs
              "<claude_path> --version", parses the semver, returns true when
              version >= 2.1.51. Treat exec/parse failure as false.
            * remote_control_eligible() -> Result<(), String>: heuristic auth
              check. Positive signal: ~/.claude/.credentials.json exists.
              Disqualifying signals (return Err with a non-secret reason):
              ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN, CLAUDE_CODE_USE_BEDROCK,
              CLAUDE_CODE_USE_VERTEX, CLAUDE_CODE_USE_FOUNDRY set in the
              environment. Consult the current Remote Control docs for the
              authoritative disqualifying set. NEVER log the values of these
              vars - only their names.
            * preflight(claude_path) -> RemoteControlStatus where
              RemoteControlStatus is Enabled or Disabled { reason: String }.
              Combines version + eligibility.
            * resolve(work_dir) -> bool: the per-spawn gate used by the native
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
      dependencies: ["knowledge-bootstrap"]
      acceptance:
        - "cargo build"
        - "cargo test"
        - "cargo clippy --all-targets -- -D warnings"
      files:
        - "loom/src/sandbox/config.rs"
        - "loom/src/remote_control.rs"
        - "loom/src/lib.rs"
        - "loom/src/fs/work_dir.rs"
        - "loom/src/commands/init/plan_setup.rs"
        - "loom/src/commands/run/**"
        - "loom/src/orchestrator/terminal/native/**"
        - "loom/src/orchestrator/core/crash_handler.rs"
      working_dir: "loom"
      before_stage:
        - command: "test ! -f src/remote_control.rs"
      after_stage:
        - command: "test -f src/remote_control.rs"
      artifacts:
        - "src/remote_control.rs"
      wiring:
        - source: "src/lib.rs"
          pattern: "remote_control"
          description: "remote_control module is registered in lib.rs"
        - source: "src/orchestrator/terminal/native/mod.rs"
          pattern: "remote-control"
          description: "Native command builder appends the --remote-control flag"
        - source: "src/fs/work_dir.rs"
          pattern: "remote_control"
          description: "fs/work_dir.rs has [remote_control] config read/write helpers"
        - source: "src/commands/run/mod.rs"
          pattern: "remote_control"
          description: "Background loom run invokes the Remote Control preflight"
        - source: "src/commands/run/foreground.rs"
          pattern: "remote_control"
          description: "Foreground loom run invokes the Remote Control preflight"
        - source: "src/orchestrator/core/crash_handler.rs"
          pattern: "remote_control"
          description: "Crash handler writes the remote-control-unsupported fallback marker"
        - source: "src/sandbox/config.rs"
          pattern: "standard_stage_defaults_to_accept_edits"
          description: "Test proves Standard/IntegrationVerify default to acceptEdits"

    - id: integration-verify
      name: "Integration Verification"
      stage_type: integration-verify
      model: "opus[1m]"
      reasoning_effort: "xhigh"
      description: |
        Final integration verification - runs AFTER the implementation stage.

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
      dependencies: ["implement-defaults-and-remote-control"]
      acceptance:
        - "cargo test"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo build"
      files:
        - "loom/src/**"
      working_dir: "loom"
      wiring:
        - source: "src/orchestrator/terminal/native/mod.rs"
          pattern: "remote-control"
          description: "Native backend wires the --remote-control flag"
        - source: "src/commands/run/foreground.rs"
          pattern: "remote_control"
          description: "Remote Control preflight is invoked from foreground loom run"
        - source: "src/orchestrator/core/crash_handler.rs"
          pattern: "remote_control"
          description: "Crash-handler fallback marker is wired in"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      stage_type: knowledge-distill
      model: "sonnet"
      reasoning_effort: "high"
      description: |
        Knowledge distillation - runs after integration-verify to curate all
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
            * the new per-stage-type permission-mode default (acceptEdits)
            * the Remote Control capability/preflight/resolve pattern, the
              .work/[remote_control] config section, and the crash-handler
              fallback marker.
        - Where this plan's work touched knowledge sections that still reference
          the removed container backend, correct those references.
        - Run loom review; remove stale or contradicted entries.

        DOCUMENTATION UPDATE (MANDATORY):
        - Update README.md / CONTRIBUTING.md sections describing permission modes
          and session spawning to reflect the acceptEdits default and the Remote
          Control behavior (auto-enabled when supported; prereqs claude >= 2.1.51
          + claude.ai login; [remote_control] config knob; fallback marker).
        - Only touch sections relevant to this plan - do not rewrite files.
        - If a doc area genuinely needs no change, record WHY in loom memory.
      dependencies: ["integration-verify"]
      acceptance:
        - 'rg -q "## " doc/loom/knowledge/architecture.md'
        - 'rg -q "## " doc/loom/knowledge/patterns.md'
      files:
        - "doc/loom/knowledge/**"
        - "README.md"
        - "CONTRIBUTING.md"
      working_dir: "."
```

<!-- END loom METADATA -->
