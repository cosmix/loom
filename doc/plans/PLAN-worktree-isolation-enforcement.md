# Plan: Worktree Isolation Enforcement

## Problem Statement

Claude instances working in worktrees sometimes escape their sandbox and work on files outside the worktree. This causes confusion and incorrect behavior. The sandbox should prevent this from happening.

**Root Cause:** The current isolation model is advisory (CLAUDE.md rules + sandbox permissions) but not strictly enforced.

## Legitimate Access Patterns

| Instance Type | Scope | Spawned For |
|--------------|-------|-------------|
| Worktree Claude | `.worktrees/<stage-id>/` + `.work/` symlink | Stage execution |
| Main-repo Claude | Full project | Merge conflicts, knowledge-bootstrap, integration-verify |

**Worktree Claude should NEVER:**

- Access sibling worktrees (`.worktrees/other-stage/`)
- Use path traversal (`../../`) to escape
- Run git with `-C` or `--work-tree` flags
- Directly modify `.work/stages/` (must use `loom stage complete`)
- Attempt to merge its own work

## Implementation Plan

### Stage 1: knowledge-bootstrap

**Type:** knowledge
**Working dir:** `.`

Research and document the isolation model:

- Read current sandbox implementation
- Read signal generation code
- Read hook system
- Document all legitimate access patterns in `doc/loom/knowledge/`

**Acceptance:**

- `test -f doc/loom/knowledge/architecture.md`
- `grep -q "worktree isolation" doc/loom/knowledge/architecture.md`

---

### Stage 2: sandbox-hardening

**Type:** standard
**Working dir:** `loom`
**Dependencies:** knowledge-bootstrap

Harden default sandbox configuration:

1. **Add path traversal deny rules** to default sandbox config:

   ```yaml
   deny_read:
     - "../../**"
     - "../.worktrees/**"
   deny_write:
     - "../../**"
     - ".work/stages/**"
     - ".work/sessions/**"
   ```

2. **Make sandbox enabled by default** for all stages (currently optional)

3. **Add worktree-aware path expansion** - expand relative paths based on worktree location

**Files:**

- `src/sandbox/config.rs` - Default deny rules
- `src/sandbox/mod.rs` - Enable by default
- `src/plan/schema/types.rs` - Schema updates if needed

**Acceptance:**

- `cargo test sandbox`
- `cargo clippy -- -D warnings`

---

### Stage 3: signal-clarity

**Type:** standard
**Working dir:** `loom`
**Dependencies:** knowledge-bootstrap

Enhance signals to explicitly state boundaries:

1. **Add isolation reminder section** to generated signals:

   ```markdown
   ## Worktree Isolation

   You are working in: `.worktrees/<stage-id>/`

   **ALLOWED:**
   - Files within this worktree
   - `.work/` directory (via symlink)
   - Read CLAUDE.md (symlinked)

   **FORBIDDEN:**
   - Path traversal (`../../`)
   - Sibling worktrees
   - Git operations on main repo
   - Merging your own work (loom handles this)
   ```

2. **Add "what NOT to do" section** with common mistakes

3. **Embed worktree path** so Claude knows exactly where it is

**Files:**

- `src/orchestrator/signals/generate.rs` - Add isolation section
- `src/orchestrator/signals/templates/` - If templates exist

**Acceptance:**

- `cargo test signals`
- `cargo clippy -- -D warnings`

---

### Stage 4: hook-enforcement

**Type:** standard
**Working dir:** `loom`
**Dependencies:** knowledge-bootstrap

Add hooks that block forbidden operations:

1. **Pre-Bash hook** that checks for:
   - `git -C` or `git --work-tree` commands
   - Commands with `../../` paths
   - Commands accessing `.worktrees/` directly

2. **Pre-Edit/Write hook** that validates:
   - Target path is within worktree
   - Not modifying `.work/stages/` or `.work/sessions/`

3. **Hook response** when violation detected:
   - Block the operation
   - Return clear error message explaining what's wrong
   - Suggest correct approach

**Files:**

- `src/hooks/validators/` - New directory for validation hooks
- `src/hooks/generator.rs` - Register new hooks
- `src/hooks/mod.rs` - Export validators

**Acceptance:**

- `cargo test hooks`
- `cargo clippy -- -D warnings`

---

### Stage 5: integration-verify

**Type:** integration-verify
**Working dir:** `loom`
**Dependencies:** sandbox-hardening, signal-clarity, hook-enforcement

Verify the complete isolation enforcement:

1. **Create test worktree** and verify:
   - Sandbox denies path traversal
   - Signals include isolation section
   - Hooks block forbidden git commands

2. **Test escape attempts** are blocked:
   - `../../` path access
   - `git -C ../..` commands
   - Direct `.work/stages/` writes

3. **Verify legitimate access works:**
   - `.work/` symlink access
   - CLAUDE.md reading
   - `loom stage complete` command

**Acceptance:**

- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo build`

---

## Verification

After implementation:

1. Run `loom run` on a test plan
2. Observe that worktree Claude cannot escape sandbox
3. Verify merge conflicts are still handled by main-repo Claude
4. Check signals include isolation warnings

## Open Questions

1. Should we add OS-level enforcement (seccomp, etc.) or is permission-based sufficient?
2. How aggressive should hook blocking be? (warn vs hard block)
3. Should we add a `--strict-isolation` flag for extra paranoid mode?

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  plan_id: worktree-isolation-enforcement

  sandbox:
    enabled: true
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
      deny_write:
        - ".work/stages/**"
    network:
      allowed_domains:
        - "github.com"
        - "crates.io"
    excluded_commands:
      - "loom"

  stages:
    - id: knowledge-bootstrap
      name: "Knowledge Bootstrap"
      stage_type: knowledge
      working_dir: "."
      description: |
        Research and document the worktree isolation model.

        Read and understand:
        - src/sandbox/ - Current sandbox implementation
        - src/orchestrator/signals/ - Signal generation
        - src/hooks/ - Hook system
        - src/git/worktree/ - Worktree creation and isolation

        Document findings in doc/loom/knowledge/architecture.md under a new
        "Worktree Isolation" section.
      acceptance:
        - "test -f doc/loom/knowledge/architecture.md"
        - "grep -q 'Worktree Isolation' doc/loom/knowledge/architecture.md || grep -q 'worktree isolation' doc/loom/knowledge/architecture.md"
      artifacts:
        - "doc/loom/knowledge/architecture.md"
      files:
        - "doc/loom/knowledge/**"

    - id: sandbox-hardening
      name: "Sandbox Hardening"
      stage_type: standard
      working_dir: "loom"
      dependencies:
        - knowledge-bootstrap
      description: |
        Harden default sandbox configuration to prevent worktree escape.

        ## Changes Required

        1. In src/sandbox/config.rs, add DEFAULT deny rules:
           - deny_read: ["../../**", "../.worktrees/**"]
           - deny_write: ["../../**", ".work/stages/**", ".work/sessions/**"]

        2. Ensure sandbox is enabled by default for all stages

        3. Add path validation that detects and blocks relative path escape attempts

        ## Testing

        Add unit tests that verify:
        - Default deny rules are applied
        - Path traversal patterns are caught
        - Legitimate .work/ access via symlink still works
      acceptance:
        - "cargo test sandbox"
        - "cargo clippy -- -D warnings"
      artifacts:
        - "src/sandbox/config.rs"
      files:
        - "src/sandbox/**"
        - "src/plan/schema/types.rs"

    - id: signal-clarity
      name: "Signal Clarity"
      stage_type: standard
      working_dir: "loom"
      dependencies:
        - knowledge-bootstrap
      description: |
        Enhance signals to explicitly communicate isolation boundaries.

        ## Changes Required

        In src/orchestrator/signals/generate.rs, add a new section to generated signals
        with the following content (formatted as markdown):

          ## Worktree Isolation

          You are working in: .worktrees/{stage-id}/

          **ALLOWED:**
          - Files within this worktree
          - .work/ directory (via symlink)
          - Reading CLAUDE.md (symlinked)
          - Using loom CLI commands

          **FORBIDDEN:**
          - Path traversal (../../, ../.worktrees/)
          - Git operations targeting main repo (git -C, --work-tree)
          - Direct modification of .work/stages/ or .work/sessions/
          - Attempting to merge your own branch (loom handles merges)

          If you need something outside your worktree, STOP and explain what you need.
          The orchestrator will handle cross-worktree operations.

        Also embed the absolute worktree path so Claude knows exactly where it is.
      acceptance:
        - "cargo test signals"
        - "cargo clippy -- -D warnings"
      artifacts:
        - "src/orchestrator/signals/generate.rs"
      files:
        - "src/orchestrator/signals/**"

    - id: hook-enforcement
      name: "Hook Enforcement"
      stage_type: standard
      working_dir: "loom"
      dependencies:
        - knowledge-bootstrap
      description: |
        Add pre-execution hooks that block forbidden operations.

        ## Changes Required

        1. Create src/hooks/validators/ directory with validation logic

        2. Add pre-Bash validation that checks for:
           - `git -C` or `git --work-tree` patterns
           - Commands containing `../../` path traversal
           - Commands accessing `.worktrees/` directly (except current)

        3. Add pre-Edit/Write validation that checks:
           - Target path is within worktree (or allowed via .work symlink)
           - Not writing to `.work/stages/` or `.work/sessions/`

        4. When violation detected:
           - Return error with clear message
           - Suggest the correct approach
           - Do NOT silently allow the operation

        ## Hook Response Format

        When blocking, return a message in this format:

          BLOCKED: [reason]

          You tried to: [describe forbidden action]
          Instead, you should: [describe correct approach]
      acceptance:
        - "cargo test hooks"
        - "cargo clippy -- -D warnings"
      artifacts:
        - "src/hooks/validators/mod.rs"
        - "src/hooks/generator.rs"
      files:
        - "src/hooks/**"

    - id: integration-verify
      name: "Integration Verification"
      stage_type: integration-verify
      working_dir: "loom"
      dependencies:
        - sandbox-hardening
        - signal-clarity
        - hook-enforcement
      description: |
        Verify the complete worktree isolation enforcement works end-to-end.

        ## Verification Steps

        1. Build the project and run all tests

        2. Verify sandbox defaults:
           - Check that new sandbox config includes deny rules
           - Verify path traversal patterns are in default deny list

        3. Verify signal generation:
           - Generate a test signal
           - Confirm it includes "Worktree Isolation" section
           - Confirm it lists ALLOWED and FORBIDDEN actions

        4. Verify hook enforcement:
           - Test that validation catches `git -C` commands
           - Test that validation catches `../../` paths
           - Test that legitimate operations pass

        5. Run integration test if available
      acceptance:
        - "cargo test"
        - "cargo clippy -- -D warnings"
        - "cargo build"
      truths:
        - "Sandbox default config denies path traversal"
        - "Signals include worktree isolation section"
        - "Hooks validate and block forbidden operations"
```

<!-- END loom METADATA -->
