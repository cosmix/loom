# Plan: Fix Handoff Continuation & macOS Terminal Detection

## Overview

Two orchestrator bugs that degrade the loom experience:

1. **Handoffs don't trigger session restarts** — When a stage needs a handoff (context exhaustion or budget exceeded), the orchestrator marks the stage `NeedsHandoff` but never spawns a continuation session. The stage sits idle until the user manually runs `loom resume`.

2. **Wrong terminal opens on macOS** — Terminal detection doesn't check the `TERM_PROGRAM` env var (set by all major terminals), causing fallback to `Terminal.app` when running in iTerm2/Ghostty/etc.

## Goals

- Stages that need handoff should automatically re-queue and spawn a new session with handoff context
- macOS terminal detection should reliably detect the user's actual terminal
- No regressions in existing stage lifecycle or terminal spawning

## Execution Diagram

```
[knowledge-bootstrap] --> [fix-handoff-continuation, fix-terminal-detection] --> [integration-verify]
```

Stages in `[a, b]` notation run concurrently in separate worktrees.

---

## Stages

### 1. Knowledge Bootstrap

**Purpose:** Verify existing knowledge is sufficient for these fixes. Knowledge files already exist and are comprehensive.

**Tasks:**

- Run `loom knowledge check` to verify coverage
- Review mistakes.md for any relevant prior issues
- Fill gaps only if needed

**Files:** `doc/loom/knowledge/**`

**Acceptance:** Knowledge check passes with coverage >= 50%.

---

### 2. Fix Handoff Continuation (parallel with stage 3)

**Purpose:** Make the orchestrator automatically re-queue NeedsHandoff stages so they get a new session with handoff context on the next poll cycle.

**Dependencies:** knowledge-bootstrap

**Current behavior:**
- `on_needs_handoff()` marks stage NeedsHandoff and saves — but doesn't remove from active_sessions, doesn't close the terminal, doesn't re-queue
- `handle_budget_exceeded()` generates a handoff file and marks NeedsHandoff — but also doesn't re-queue
- `start_stage()` never checks for existing handoffs when generating signals
- Result: stage sits in NeedsHandoff forever unless user runs `loom resume`

**Fix — three changes:**

1. **`event_handler.rs` — `on_needs_handoff()`**: After marking NeedsHandoff, close old session/terminal, remove from active_sessions, transition NeedsHandoff -> Queued, update graph
2. **`event_handler.rs` — `handle_budget_exceeded()`**: After existing NeedsHandoff logic, also transition to Queued and update graph (handoff file already generated)
3. **`stage_executor.rs` — `start_stage()`**: Before generating signal, call `find_latest_handoff(stage_id, work_dir)` and pass the result as `handoff_file` parameter to `generate_signal_with_skills()` (parameter already exists but is always passed as `None`)

**Files:** `src/orchestrator/core/event_handler.rs`, `src/orchestrator/core/stage_executor.rs`

**Acceptance:** `cargo test`, `cargo clippy -- -D warnings`

**Verification:** Signal generation passes handoff reference; NeedsHandoff transitions to Queued.

---

### 3. Fix macOS Terminal Detection (parallel with stage 2)

**Purpose:** Use the `TERM_PROGRAM` environment variable for reliable terminal detection on macOS, reducing fallback to Terminal.app.

**Dependencies:** knowledge-bootstrap

**Current behavior:**
- Detection priority: LOOM_TERMINAL env → TERMINAL env → parent process walk → binary check → app check → Terminal.app fallback
- `TERM_PROGRAM` is never checked (set by all major macOS terminals)
- `detect_parent_terminal()` walks process tree with `ps` which can fail after daemon fork
- `from_name()` doesn't handle TERM_PROGRAM values like "Apple_Terminal" or "iTerm.app"

**Fix — two changes:**

1. **`detection.rs` — `detect_terminal()` (macOS)**: Add `TERM_PROGRAM` check as step 1.5 (after LOOM_TERMINAL and TERMINAL, before parent process walk). This is more reliable than process tree walking.
2. **`emulator.rs` — `from_name()`**: Handle common `TERM_PROGRAM` values: "Apple_Terminal" (Terminal.app), "iTerm.app" (iTerm2). Ghostty/kitty/alacritty/wezterm already match via `from_binary()` fallback.

**Files:** `src/orchestrator/terminal/native/detection.rs`, `src/orchestrator/terminal/emulator.rs`

**Acceptance:** `cargo test`, `cargo clippy -- -D warnings`

**Verification:** `from_name("Apple_Terminal")` returns TerminalApp; `from_name("iTerm.app")` returns ITerm2.

---

### 4. Integration Verification

**Purpose:** Verify both fixes work together, run full test suite, code review.

**Dependencies:** fix-handoff-continuation, fix-terminal-detection

**Tasks:**

- Full test suite, clippy, build
- Spawn parallel review subagents (security, architecture, testing)
- Functional verification: confirm NeedsHandoff -> Queued transition works in orchestrator loop
- Curate stage memory into knowledge

**Acceptance:** `cargo test`, `cargo clippy -- -D warnings`, `cargo build`

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
      deny_write:
        - ".work/stages/**"
        - "doc/loom/knowledge/**"
    network:
      allowed_domains: []
  stages:
    - id: knowledge-bootstrap
      name: "Bootstrap Knowledge Base"
      stage_type: knowledge
      description: |
        Lightweight knowledge verification — knowledge files already exist.

        Use parallel subagents and skills to maximize performance.

        Step 0 - CHECK EXISTING KNOWLEDGE:
          Run: loom knowledge check
          If coverage >= 50%, review mistakes.md and move on.
          If coverage < 50%, run: loom map --deep

        Step 1 - REVIEW RELEVANT AREAS:
          Focus on orchestrator event handling and terminal detection.
          Read doc/loom/knowledge/architecture.md for context.

        Step 2 - FILL GAPS (only if needed):
          Use loom knowledge update commands for any missing info about:
          - Handoff/continuation flow
          - Terminal detection on macOS
          - Stage state machine transitions

        MEMORY RECORDING:
        - Record insights: loom memory note "observation"
        - Before completing: loom memory list
      dependencies: []
      acceptance:
        - "test -f doc/loom/knowledge/architecture.md"
        - "rg -q '## ' doc/loom/knowledge/architecture.md"
      files:
        - "doc/loom/knowledge/**"
      working_dir: "."
      artifacts:
        - "doc/loom/knowledge/architecture.md"

    - id: fix-handoff-continuation
      name: "Fix Handoff Continuation"
      stage_type: standard
      description: |
        Fix the orchestrator to automatically re-queue NeedsHandoff stages with handoff context.

        Use parallel subagents and skills to maximize performance.

        CONTEXT: When a stage exhausts its context or exceeds budget, the orchestrator
        marks it NeedsHandoff but never re-queues it. The stage sits idle forever.
        The continuation module exists (continuation/mod.rs) but nothing calls it from
        the orchestrator loop. The simplest fix is to re-queue the stage and pass
        handoff context when the stage is re-started.

        IMPORTANT: Read these files first:
        - src/orchestrator/core/event_handler.rs (on_needs_handoff, handle_budget_exceeded)
        - src/orchestrator/core/stage_executor.rs (start_stage)
        - src/orchestrator/continuation/mod.rs (continue_session - reference only)
        - src/handoff/generator/mod.rs (find_latest_handoff)
        - src/models/stage/transitions.rs (NeedsHandoff -> Queued is valid)

        TASKS:

        Task 1 - Fix on_needs_handoff() in event_handler.rs:
          Current code only marks stage NeedsHandoff and saves.
          After the existing logic, add:
          a) Kill old session via self.backend.kill_session() if session exists in active_sessions
          b) Remove signal file via remove_signal()
          c) Remove from self.active_sessions
          d) Transition stage: NeedsHandoff -> Queued via stage.try_mark_queued()
          e) Save stage
          f) Update graph: self.graph.mark_queued(stage_id)
          The next poll cycle's start_ready_stages() will pick it up.

        Task 2 - Fix handle_budget_exceeded() in event_handler.rs:
          Current code generates handoff, marks ContextExhausted/NeedsHandoff, removes from active.
          After the existing NeedsHandoff transition, add:
          a) Transition stage: NeedsHandoff -> Queued via stage.try_mark_queued()
          b) Save stage again
          c) Update graph: self.graph.mark_queued(stage_id)
          NOTE: handoff file is already generated by handle_context_critical().
          NOTE: session is already removed from active_sessions.

        Task 3 - Pass handoff context in start_stage() in stage_executor.rs:
          In start_stage(), before the call to generate_signal_with_skills():
          a) Import find_latest_handoff from crate::handoff::generator
          b) Call find_latest_handoff(&stage.id, &self.config.work_dir)
          c) Extract filename stem from the result (like extract_handoff_filename in continuation/mod.rs)
          d) Pass as handoff_file parameter to generate_signal_with_skills()
             (currently hardcoded to None)
          Also do the same for start_knowledge_stage() with generate_knowledge_signal()
          if that function accepts a handoff parameter (check signature).

        Task 4 - Add tests:
          Add a unit test verifying that on_needs_handoff transitions the stage
          from NeedsHandoff to Queued and removes from active_sessions.

        MEMORY RECORDING (use memory ONLY - never knowledge):
        - Record insights: loom memory note "observation"
        - Record decisions: loom memory decision "choice" --context "why"
        - Before completing: loom memory list
      dependencies: ["knowledge-bootstrap"]
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
      files:
        - "src/orchestrator/core/event_handler.rs"
        - "src/orchestrator/core/stage_executor.rs"
      working_dir: "loom"
      wiring:
        - source: "src/orchestrator/core/event_handler.rs"
          pattern: "try_mark_queued"
          description: "on_needs_handoff transitions stage to Queued for automatic re-spawn"
        - source: "src/orchestrator/core/stage_executor.rs"
          pattern: "find_latest_handoff"
          description: "start_stage checks for existing handoff to include in signal"
      truths:
        - "cargo test event_handler"
        - "cargo test stage_executor"

    - id: fix-terminal-detection
      name: "Fix macOS Terminal Detection"
      stage_type: standard
      description: |
        Add TERM_PROGRAM environment variable support to macOS terminal detection.

        Use parallel subagents and skills to maximize performance.

        CONTEXT: On macOS, all major terminals set TERM_PROGRAM env var. Loom currently
        ignores it and relies on process tree walking (which fails after daemon fork)
        or app path checks. Adding TERM_PROGRAM as an early detection step fixes the
        common case where iTerm2 users get Terminal.app windows.

        IMPORTANT: Read these files first:
        - src/orchestrator/terminal/native/detection.rs (detect_terminal macOS)
        - src/orchestrator/terminal/emulator.rs (from_name, from_binary)

        TASKS:

        Task 1 - Add TERM_PROGRAM values to from_name() in emulator.rs:
          Add these matches to from_name():
            "Apple_Terminal" => Some(Self::TerminalApp)    // Terminal.app sets this
            "iTerm.app" => Some(Self::ITerm2)              // iTerm2 sets this
          These are the only two that don't already match via existing patterns
          or from_binary() fallback. Ghostty sets "ghostty", kitty sets "kitty", etc.
          which already match.

        Task 2 - Add TERM_PROGRAM check to detect_terminal() in detection.rs:
          In the macOS detect_terminal() function, add a new step between
          "Check TERMINAL environment variable" (step 1) and
          "Detect currently running terminal from parent process chain" (step 2):

            // 1.5. Check TERM_PROGRAM environment variable (set by most terminals)
            if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
                if !term_program.is_empty() {
                    if let Some(emulator) = TerminalEmulator::from_name(&term_program) {
                        return Ok(emulator);
                    }
                }
            }

          This is more reliable than process tree walking and works even
          after daemon fork (env vars survive fork).

        Task 3 - Add tests:
          a) Test from_name("Apple_Terminal") returns TerminalApp
          b) Test from_name("iTerm.app") returns ITerm2
          c) Test that TERM_PROGRAM detection works (similar pattern to
             existing test_loom_terminal_env_var_takes_precedence test)

        MEMORY RECORDING (use memory ONLY - never knowledge):
        - Record insights: loom memory note "observation"
        - Record decisions: loom memory decision "choice" --context "why"
        - Before completing: loom memory list
      dependencies: ["knowledge-bootstrap"]
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
      files:
        - "src/orchestrator/terminal/native/detection.rs"
        - "src/orchestrator/terminal/emulator.rs"
      working_dir: "loom"
      wiring:
        - source: "src/orchestrator/terminal/emulator.rs"
          pattern: "Apple_Terminal"
          description: "from_name handles TERM_PROGRAM value for Terminal.app"
        - source: "src/orchestrator/terminal/native/detection.rs"
          pattern: "TERM_PROGRAM"
          description: "detect_terminal checks TERM_PROGRAM env var on macOS"
      truths:
        - "cargo test emulator"
        - "cargo test detection"

    - id: integration-verify
      name: "Integration Verification"
      stage_type: integration-verify
      description: |
        Final integration verification after both fixes are merged.

        Use parallel subagents and skills to maximize performance.

        CRITICAL: Verify FUNCTIONAL INTEGRATION, not just tests passing.

        CONTEXT GATHERING (FIRST):
        1. Read doc/plans/PLAN-fix-handoff-and-terminal-detection.md
        2. Run: loom memory show --all
        3. Read doc/loom/knowledge/*.md

        BUILD & TEST:
        1. cargo test (full suite)
        2. cargo clippy -- -D warnings
        3. cargo build

        CODE REVIEW (MANDATORY):
        Spawn PARALLEL review subagents:
        - security-engineer: Check for injection risks in shell commands, env var handling
        - senior-software-engineer: Verify state machine transitions are correct,
          no race conditions in active_sessions manipulation
        - /testing skill: Coverage of new code paths

        Fix ALL issues found.

        FUNCTIONAL VERIFICATION (MANDATORY):
        1. Handoff fix: Verify the code path from on_needs_handoff through
           try_mark_queued to graph.mark_queued is wired correctly
        2. Handoff fix: Verify start_stage calls find_latest_handoff and passes
           result to signal generation
        3. Terminal fix: Verify from_name("Apple_Terminal") == TerminalApp
        4. Terminal fix: Verify from_name("iTerm.app") == ITerm2
        5. Terminal fix: Verify detect_terminal checks TERM_PROGRAM

        KNOWLEDGE CURATION (MANDATORY):
        - Read all stage memory: loom memory show --all
        - Update architecture.md with handoff continuation flow changes
        - Record any mistakes to mistakes.md
      dependencies: ["fix-handoff-continuation", "fix-terminal-detection"]
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
        - "cargo build"
      files: []
      working_dir: "loom"
      truths:
        - "cargo test event_handler"
        - "cargo test stage_executor"
        - "cargo test emulator"
        - "cargo test detection"
      wiring:
        - source: "src/orchestrator/core/event_handler.rs"
          pattern: "try_mark_queued"
          description: "NeedsHandoff stages are automatically re-queued"
        - source: "src/orchestrator/core/stage_executor.rs"
          pattern: "find_latest_handoff"
          description: "Signal generation includes handoff context"
        - source: "src/orchestrator/terminal/emulator.rs"
          pattern: "Apple_Terminal"
          description: "TERM_PROGRAM values are handled"
        - source: "src/orchestrator/terminal/native/detection.rs"
          pattern: "TERM_PROGRAM"
          description: "Terminal detection checks TERM_PROGRAM"
```

<!-- END loom METADATA -->
