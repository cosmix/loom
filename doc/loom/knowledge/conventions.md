# Coding Conventions

> Discovered coding conventions in the codebase.
> This file is append-only - agents add discoveries, never delete.

## File Naming Conventions

### Stage Files

- Pattern: `{depth:02}-{stage-id}.md` (e.g., `01-knowledge-bootstrap.md`)
- Depth 1-indexed in filename (depth 0 in code becomes `01-` prefix)
- Located in `.work/stages/`

### Session Files

- Pattern: `{session-id}.md`
- Session ID format: `session-{uuid_short}-{timestamp}`
- Located in `.work/sessions/`

### Signal Files

- Pattern: `{session-id}.md`
- Located in `.work/signals/`

### Handoff Files

- Pattern: `{stage-id}-handoff-{NNN:03d}.md` (e.g., `feature-auth-handoff-001.md`)
- Sequential numbering per stage
- Located in `.work/handoffs/`

### Plan Files

- Pattern: `PLAN-{description}.md` (initial)
- Lifecycle: `PLAN-*` → `IN_PROGRESS-PLAN-*` → `DONE-PLAN-*`
- Located in `doc/plans/`

### Branch Naming

- Stage branches: `loom/{stage-id}`
- Base branches: `loom/_base/{stage-id}` (for multi-dependency merges)

## Error Handling Conventions

### Result Type

All fallible functions return `anyhow::Result<T>`:

```rust
use anyhow::{Context, Result};

fn example() -> Result<()> {
    do_thing().context("Failed to do thing")?;
    Ok(())
}
```

### Context Chaining

Add context at each layer for debugging:

```rust
fs::read(path)
    .with_context(|| format!("Failed to read file: {}", path.display()))?
```

### Git Command Errors

Include command, directory, exit code, stdout, stderr:

```rust
anyhow::bail!(
    "git {} failed (exit code {}): Directory: {}, Stdout: {}, Stderr: {}",
    command, exit_code, dir, stdout, stderr
)
```

## Serialization Conventions

### YAML Frontmatter in Markdown

All state files use markdown with YAML frontmatter:

```markdown
---
id: stage-1
status: Executing
dependencies: [bootstrap]
---

# Stage: Feature Implementation

Description and content...
```

### Serde Attributes

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Stage {
    #[serde(default)]
    pub retry_count: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_info: Option<FailureInfo>,

    #[serde(alias = "pending")]  // Backward compatibility
    pub status: StageStatus,
}
```

### DateTime Format

All timestamps use `DateTime<Utc>` from chrono:

```rust
use chrono::{DateTime, Utc};

pub created_at: DateTime<Utc>,
pub updated_at: DateTime<Utc>,
```

## Module Organization

### Standard Module Structure

```text
module/
├── mod.rs          # Public exports and module docs
├── types.rs        # Data structures
├── methods.rs      # Impl blocks for types
├── transitions.rs  # State machine logic (if applicable)
└── tests.rs        # Unit tests
```

### Public API in mod.rs

```rust
// mod.rs
mod types;
mod methods;
mod transitions;

pub use types::{Stage, StageStatus};
pub use transitions::try_transition;
```

## Testing Conventions

### Test Isolation

Use `tempfile::TempDir` for filesystem tests:

```rust
#[test]
fn test_stage_file() {
    let temp = tempfile::tempdir().unwrap();
    // Test with temp.path()...
}
```

### Serial Tests

Use `#[serial]` from `serial_test` crate for tests that cannot run in parallel:

```rust
#[test]
#[serial]
fn test_git_operations() {
    // Git operations need exclusive access
}
```

### Test Naming

```rust
#[test]
fn test_transition_from_executing_to_completed() { }

#[test]
fn test_stage_file_with_missing_dependencies() { }
```

## ID and Key Validation

### Stage ID Rules

- Max 128 characters
- Alphanumeric, dashes, underscores only
- No path separators (`/`, `\`)
- No dots (prevents `.`, `..`, `stage.md`)
- No spaces
- No reserved OS names (CON, AUX, NUL, etc.)

### Fact Key Rules

- Max 64 characters
- Alphanumeric, dashes, underscores only

### Acceptance Criteria Rules

- Max 1024 characters per criterion
- No control characters (except tab, newline, CR)
- Non-empty (not whitespace-only)

## Constant Definitions

### Context Thresholds

```rust
pub const DEFAULT_CONTEXT_LIMIT: u32 = 200_000;
pub const CONTEXT_WARNING_THRESHOLD: f32 = 0.75;  // 75%
pub const CONTEXT_CRITICAL_THRESHOLD: f32 = 0.85; // 85%
```

### Timeouts

```rust
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);  // 5 min
pub const DEFAULT_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(30);
pub const HUNG_SESSION_TIMEOUT: Duration = Duration::from_secs(300);  // 5 min
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);
```

### Retry Limits

```rust
pub const DEFAULT_MAX_RETRIES: u32 = 3;
pub const BACKOFF_BASE_SECONDS: u64 = 30;
pub const BACKOFF_MAX_SECONDS: u64 = 300;
```

## Display Conventions

### Status Icons

```rust
match status {
    Completed => "✓",
    Executing => "●",
    Queued => "▶",
    WaitingForDeps => "○",
    Blocked => "✗",
    NeedsHandoff => "⟳",
    MergeConflict => "⚡",
    WaitingForInput => "?",
    Skipped => "⊘",
    CompletedWithFailures => "⚠",
    MergeBlocked => "⊗",
}
```

### Color Scheme (using `colored` crate)

```rust
Executing => blue().bold()
Completed => green()
Blocked => red().bold()
Pending => dimmed()
Queued => cyan()
Warning => yellow()
```

### Context Bar Colors

```rust
usage < 0.60 => green
0.60 <= usage < 0.75 => yellow
usage >= 0.75 => red
```

## Git Operations

### Worktree Commands

```rust
git worktree add .worktrees/{stage-id} -b loom/{stage-id}
git worktree remove --force .worktrees/{stage-id}
git worktree prune
git worktree list --porcelain
```

### Merge Commands

```rust
git merge --no-ff -m "Merge loom/{stage-id}" loom/{stage-id}
git merge --abort  // On conflict
```

### Branch Commands

```rust
git branch -D loom/{stage-id}  // Delete after merge
git rev-parse --abbrev-ref HEAD  // Current branch
git merge-base --is-ancestor {commit} {branch}  // Ancestry check
```

## Plan YAML Schema

### Required Fields

```yaml
loom:
  version: 1 # Only version 1 supported
  stages:
    - id: stage-id # Required, validated
      name: "Stage Name" # Required, non-empty
      working_dir: "." # Required: "." or subdirectory
      dependencies: [] # Required (can be empty)
      acceptance: [] # Required (can be empty)
```

### Optional Fields

```yaml
description: "Optional task description"
parallel_group: "group-name"
setup: ["command to run before"]
files: ["src/**/*.rs"]
auto_merge: true
stage_type: "standard" # or "knowledge"
```

## Dependency Management

### Package Managers

Never hand-edit manifests. Use:

- Rust: `cargo add`
- Node: `bun add` / `npm install`
- Python: `uv add` / `pip install`
- Go: `go get`

## Code Size Limits

- File: 400 lines max
- Function: 50 lines max
- Class/Struct impl: 300 lines max
- Exceed = refactor immediately

## Builder Pattern

Used for complex struct construction:

```rust
impl HandoffContent {
    pub fn builder() -> Self { Self::default() }

    pub fn with_session_id(mut self, id: String) -> Self {
        self.session_id = id;
        self
    }

    pub fn with_stage_id(mut self, id: String) -> Self {
        self.stage_id = id;
        self
    }
}
```

## Enum Conventions

### Status Enums with Display

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum StageStatus {
    WaitingForDeps,
    Queued,
    // ...
}

impl std::fmt::Display for StageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Self::WaitingForDeps => "waiting-for-deps",
            Self::Queued => "queued",
            // ...
        })
    }
}
```

## Hook Conventions

### Hook Location

Hooks installed to `~/.claude/hooks/loom/` (loom subdirectory for isolation)

### Naming

Pattern: `<event>-<action>.sh` (e.g., `session-start.sh`, `post-tool-use.sh`)

## Comment Style

### Module Documentation

```rust
//! Module-level documentation explaining purpose
//! and key concepts.

/// Function-level documentation.
///
/// # Arguments
/// * `stage_id` - The stage identifier
///
/// # Returns
/// Result containing the stage or error
pub fn load_stage(stage_id: &str) -> Result<Stage> { }
```

### Inline Comments

Use sparingly, only for non-obvious logic:

```rust
// Depth 0 in code becomes "01-" prefix in filename (1-indexed for humans)
let prefix = format!("{:02}-", depth + 1);
```

## Skill File Format Convention

### Directory Structure

```text
skills/
├── <skill-name>/
│   └── SKILL.md
```

### SKILL.md Format

```yaml
---
name: <skill-name> # Required: kebab-case identifier
description: <text> # Required: 1-2 sentences + trigger keywords
allowed-tools: Read, Grep... # Optional: comma-separated tool names
trigger-keywords: <csv> # Optional: comma-separated triggers
triggers: # Optional: YAML list of keywords
  - keyword1
  - keyword2
---
# Skill Name

## Overview
## When to Use
## Instructions
```

### Trigger Keyword Formats

Three valid formats (can combine):

1. `triggers:` YAML array (auth skill uses 40+ keywords)
2. `trigger-keywords:` comma-separated string
3. Inline in `description:` field with "Trigger keywords:" prefix

## CLAUDE.md.template Convention

### Section Order

1. Header with timestamp (auto-generated at install)
2. Critical Rules (1-10) - MUST follow exactly
3. Standard Rules (11-15) - Quality guidelines
4. Delegation section - Subagent prompt templates
5. Loom Orchestration section - Session lifecycle
6. Templates section - Handoff and signal formats
7. References section - file:line format
8. Critical Reminders - 6-point checklist

### Required Stage Bookends

- FIRST stage: `knowledge-bootstrap` (unless knowledge exists)
- LAST stage: `integration-verify` (ALWAYS, no exceptions)

### Forbidden Plan Locations

- `~/.claude/plans/` - NEVER write here
- Any `.claude/plans/` path - NEVER write here
- Only valid: `doc/plans/PLAN-<description>.md`

## Re-export Conventions in mod.rs

### Two-Step Pattern

1. Declare submodules: `mod base; mod checks; mod operations;`
2. Re-export public API: `pub use base::Item; pub use checks::func;`

### Re-export Rules

- Group re-exports by source module
- Only export public API items (keep helpers private)
- Use explicit item lists, never wildcards (`*`)
- `pub use` NOT `pub mod` for re-exports

## Test File Conventions

### Inline Tests

- Use `#[cfg(test)] mod tests { }` at end of source file
- For simple unit tests (< 5 tests)

### Separate Test Files

- Use tests.rs in module directory with complex suites
- Declare in mod.rs: `#[cfg(test)] mod tests;`

### Integration Tests

- Located in loom/tests/integration/
- Use `serial_test` crate for test isolation
- Shared helpers in helpers.rs module

## Terminal Module Conventions

### File Organization

- emulator.rs: Enum + command building (no platform-specific code)
- native/detection.rs: Platform-specific detection logic
- native/pid_tracking.rs: Platform-specific PID discovery
- native/window_ops.rs: Platform-specific window management
- native/spawner.rs: Cross-platform spawn orchestration

### Testing Patterns

- Tests are designed to not panic on missing tools
- test_detect_terminal_finds_something allows failure in minimal envs
- Window operation tests check graceful handling of missing wmctrl/xdotool
- Use tempfile::TempDir for PID file tests

## Knowledge File Types

Seven knowledge files (two new):

- architecture.md - Component relationships, data flow
- entry-points.md - CLI commands, key files
- patterns.md - Architectural patterns
- conventions.md - Coding standards, naming
- mistakes.md - Lessons learned
- stack.md (NEW) - Dependencies, frameworks
- concerns.md (NEW) - Technical debt

Aliases for CLI:

- stack: deps, dependencies, tech, tooling
- concerns: debt, issues, warnings

## Code Consolidation Opportunities (2026-01-29)

When a pattern appears 3+ times, extract to a canonical location and import instead of duplicating.

### Critical Duplications Found

- parse_stage_from_markdown: 4 copies, canonical in verify/transitions/serialization.rs
- branch_name_for_stage: 22+ inline format!() calls, canonical in git/branch/naming.rs
- extract_yaml_frontmatter: 2 copies (parser/frontmatter.rs vs skills/index.rs)

### Consolidation Actions Required

- compute_level: 4 copies (status render/ui) → create status/common/levels.rs
- MEMORY_HEADER: 2 copies (fs/memory/) → create fs/memory/constants.rs
- Staleness threshold: 2 copies (status files) → move to models/constants.rs

### Import Pattern Standard

Good: use crate::verify::transitions::serialization::parse_stage_from_markdown;
Bad: fn parse_stage_from_markdown(...) { ... } // duplicate copy

Branch naming: always call git::branch::naming::branch_name_for_stage(stage_id), not format!("loom/{}", id)
