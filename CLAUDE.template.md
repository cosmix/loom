# Claude Code Rules

---

## ⚠️ MANDATORY RULES (ALL AGENTS MUST FOLLOW)

These rules are **NON-NEGOTIABLE** and apply to the main agent AND all subagents. Violations are unacceptable.

### 1. NEVER Use CLI Commands for File Operations

**DO NOT USE** these commands under any circumstances:

- `cat`, `head`, `tail` → Use the **Read** tool
- `grep`, `rg`, `ag` → Use the **Grep** tool
- `find`, `ls`, `fd` → Use the **Glob** tool
- `sed`, `awk` → Use the **Edit** tool
- `echo >`, `cat <<EOF`, `printf >` → Use the **Write** tool
- `curl`, `wget` (for fetching) → Use the **WebFetch** tool

**WHY THIS MATTERS**: Native tools are faster, provide better output formatting, handle errors gracefully, and integrate properly with the Claude Code system. CLI commands bypass these benefits and produce inferior results.

**THE ONLY EXCEPTIONS**:

- Actual shell operations: `git`, `npm`, `docker`, `make`, `cargo`, etc.
- Complex piped sequences that genuinely require shell orchestration
- When the user explicitly requests CLI usage

**WHEN YOU MUST USE CLI** (for allowed operations):

- ALWAYS use `rg` over `grep`
- ALWAYS use `fd` over `find`

### 2. No Incomplete Code

- **NEVER** leave TODO/FIXME comments
- **NEVER** create placeholder stubs or deferred implementations
- **NEVER** write "implement later" or similar comments
- Every piece of code you write must be complete and production-ready

### 3. Quality Gates

Before marking any task complete:

- Zero IDE diagnostics errors/warnings
- All tests pass
- No linting errors

---

## Agent Orchestration

### Delegation Strategy

- ALWAYS delegate work to specialist agents when the task matches their expertise
- Use the `tech-lead` agent for complex multi-domain projects requiring coordination
- Provide thorough context to each subagent to allow them to do their job properly

### When to Use Senior vs Standard Agents

- **Senior (opus)**: Planning, architecture, debugging, design patterns, code review, strategic decisions
- **Standard (sonnet)**: Implementation, boilerplate, well-defined tasks, routine operations

### Parallel Execution

When tasks are INDEPENDENT, spawn agents IN PARALLEL:

- Different files or components with no shared dependencies
- Separate analyses or reviews
- Multiple skill applications to different areas
- Research tasks that don't depend on each other

### Sequential Execution

Use sequential execution when:

- Task B depends on Task A's output
- Shared state or resources require coordination
- Order matters (e.g., schema before data, interface before implementation)

### Context Passing

**CRITICAL**: Subagents do NOT automatically inherit CLAUDE.md rules. You MUST explicitly pass them.

Always provide subagents with:

- **Full contents of `~/.claude/CLAUDE.md`** (user rules) - copy verbatim into prompt
- **Full contents of project `CLAUDE.md`** (project rules) - copy verbatim into prompt
- Clear objective and scope
- Relevant file paths and context
- Constraints and requirements
- Expected output format

Example subagent prompt structure:

```text
## Rules (MUST FOLLOW)
[Paste contents of ~/.claude/CLAUDE.md here]
[Paste contents of project CLAUDE.md here]

## Task
[Your specific task description]
```

## Development Best Practices

### Banned Anti-Patterns

NEVER use these:

- CLI for file ops: `cat`, `head`, `tail`, `grep`, `find`, `sed`, `awk`, `echo >` (use native tools)
- TODO/FIXME comments, placeholder stubs, or deferred implementations
- Empty catch blocks, swallowed exceptions, bare `except` in Python
- Magic numbers without named constants
- Nested ternaries or deep conditionals (max 3 levels)
- Console.log/print in committed code
- `any` in TypeScript
- Default exports (use named exports)
- Functions/classes exceeding 50 lines
- Commented-out code
- Secrets, credentials, or .env values in code

ALWAYS do these:

- Early returns and guard clauses to reduce nesting
- Descriptive names; self-documenting code
- Validate inputs at boundaries; fail fast

### Error Handling

- Create specific error types, not generic exceptions
- Preserve error context when wrapping (use `from e` in Python, `cause` in JS)
- Log errors with context (request IDs, relevant state)
- Distinguish recoverable vs unrecoverable errors
- Never expose internal error details to users

### Planning

- Create implementation plans in `./plans/PLAN-XXXX-description.md`
- Never use ~/.claude for plans
- Always reference plans by their project-relative path
- Never provide time estimates for tasks

### Documentation

- When creating markdown files and adding code blocks, ALWAYS specify the language for syntax highlighting (e.g., `typescript`, `python`, or if no language applies, `text`)

### Tool Usage (CRITICAL)

**NEVER use CLI commands when native tools exist.** Native tools are faster, safer, and provide better output.

| Task                | WRONG (CLI)           | RIGHT (Native Tool) |
| ------------------- | --------------------- | ------------------- |
| Read file           | `cat`, `head`, `tail` | `Read` tool         |
| Search file content | `grep`, `rg`          | `Grep` tool         |
| Find files          | `find`, `ls`, `fd`    | `Glob` tool         |
| Edit file           | `sed`, `awk`          | `Edit` tool         |
| Create file         | `echo >`, `cat <<EOF` | `Write` tool        |
| Fetch web content   | `curl`, `wget`        | `WebFetch` tool     |
| Search the web      | -                     | `WebSearch` tool    |

**Only use CLI for:**

- Actual shell operations (git, npm, docker, make, etc.)
- Piped command sequences where multiple tools chain together
- When CLI is explicitly required by the task

**When CLI is necessary:**

- Use `rg` instead of `grep`
- Use `fd` instead of `find`

**Fallbacks when native tools fail:**

- If `WebFetch`/`WebSearch` fail (blocked, restricted), use `curl` or `wget` as fallback

### Code Quality

- No file should exceed 400 lines. Refactor by breaking up large files into smaller modules
- Ensure no errors or warnings from IDE diagnostics before completing tasks

### Dependency Management

- NEVER add dependencies manually by editing package.json, Cargo.toml, pyproject.toml, or equivalent
- ALWAYS use package managers (npm, cargo, uv, etc.) for dependency management

### Progress Tracking

- Record progress in the project CLAUDE.md when working on significant tasks
- Reference phases/tasks found in project documentation
- When tasks are complete, remove detailed progress records to keep CLAUDE.md concise

## Task Execution Workflow

For non-trivial tasks, follow these phases using appropriate subagents:

### 1. Discovery & Planning

- Review codebase patterns, project CLAUDE.md, and existing plans in `./plans/`
- Use `senior-software-engineer` or `tech-lead` for complex planning
- Break work into atomic subtasks with clear inputs, outputs, and acceptance criteria
- Use `senior-infrastructure-engineer` if new services/databases/cloud resources needed

### 2. Architecture & Design

- Use senior agents (`senior-software-engineer`, `senior-data-engineer`, `senior-ml-engineer`, `senior-infrastructure-engineer`) for design decisions
- Senior agents produce specifications; standard agents implement
- Document architectural decisions and rationale

### 3. Implementation

- **Senior (opus)**: Complex algorithms, debugging, design patterns
- **Standard (sonnet)**: Well-defined implementation, boilerplate
- Parallel when independent; sequential when dependencies exist
- Run tests/lints after each significant change; verify zero IDE errors

### 4. Quality Assurance

- Use `qa-engineer` for unit/integration tests (pyramid: 65-80% unit, 15-25% integration, 5-10% e2e)
- Use Playwright MCP for web e2e tests (request from user if unavailable)
- Use `security-engineer` for security review; never skip for user data, auth, or external APIs
- Provide remediation plans to parent agent for resolution

### 5. Completion

- Update documentation with `technical-writer`; include verified examples
- Record progress in CLAUDE.md; remove extraneous detail when complete
- When context < 25%: write detailed handoff notes, list blockers, end task cleanly

### Escalation & Rollback

- **Escalate** when: multi-domain architecture, unclear patterns, security/performance implications, scope creep
- **Rollback**: Fix failing tests before proceeding; block on security issues; never commit broken code

## Decision Framework

When making technical decisions, evaluate:

- **Reversibility**: Prefer reversible decisions; be careful with irreversible ones
- **Blast radius**: How many components does this change affect?
- **Consistency**: Align with existing patterns unless compelling reason to deviate
- **Simplicity**: Choose the simplest solution that meets requirements
- **Testability**: Can this be validated with automated tests?

## Red Flags

Stop and reassess when you encounter:

- Tasks that seem simple but hide complexity
- Circular dependencies between components
- Unclear ownership of shared resources
- Missing or outdated documentation
- Scope creep during implementation
- Multiple valid approaches with unclear trade-offs
