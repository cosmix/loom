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

### 4. Context Management & Session Continuity (NON-NEGOTIABLE)

**YOU MUST TERMINATE THE CURRENT TASK WHEN CONTEXT UTILIZATION APPROACHES 85%.** This is absolute and non-negotiable.

**Continuous Session Recording Requirements:**

Throughout EVERY session, you MUST maintain a `## Session State` section in the project's `CLAUDE.md` file (create if it doesn't exist) with:

1. **Work Completed**: Detailed list of what was accomplished this session
2. **Files Modified**: Every file created, edited, or deleted with brief description of changes
3. **Documentation Read**: Full paths to all documentation, specs, or reference files consulted
4. **Key Decisions Made**: Technical decisions and their rationale
5. **Current State**: Where you stopped, what's in progress
6. **Next Steps**: Explicit, actionable items for continuation
7. **Blockers/Issues**: Any unresolved problems or questions

**Update Frequency**: Update `CLAUDE.md` after EVERY significant action (file read, code change, decision). Do NOT batch updates.

**At 85% Context Utilization**:

1. IMMEDIATELY stop current work
2. Write comprehensive handoff notes to `CLAUDE.md`
3. Ensure all work in progress is saved and documented
4. List exact next steps with file paths and line numbers where relevant
5. Inform the user that context limit is approaching and task must be paused
6. DO NOT attempt to "finish quickly" - stop and document
7. If there is additional context left, attempt completion of the task. DO NOT, UNDER ANY CIRCUMSTANCES START A NEW TASK.

**On Task Completion (CLEANUP REQUIRED)**:

When a task is FULLY completed (not paused due to context limits):

1. **Remove the `## Session State` section entirely** - it served its purpose
2. **Keep only permanent project knowledge** in CLAUDE.md:
   - Architecture decisions that affect future work
   - Non-obvious patterns or conventions discovered
   - Known issues or technical debt to track
3. **Do NOT leave stale session data** - old "Work Completed", "Files Modified", etc. from finished tasks pollute the file
4. **CLAUDE.md should be concise** - if it exceeds ~100 lines, prune aggressively

**Lifecycle**: Task starts → Create Session State → Update continuously → Task completes → DELETE Session State (keep only permanent learnings)

**Session State Format** (in project CLAUDE.md):

```markdown
## Session State (Last Updated: [timestamp])
**Completed**: [items with file:line refs] | **Modified**: [files + changes]
**Docs Read**: [paths] | **Decisions**: [decision: rationale]
**Current**: [where stopped] | **Next**: [actionable items] | **Blockers**: [issues]
```

### 5. Dependency Management (Package Managers ONLY)

**NEVER manually edit dependency files.** This includes:

- `package.json` / `package-lock.json` (Node.js)
- `Cargo.toml` / `Cargo.lock` (Rust)
- `pyproject.toml` / `requirements.txt` / `poetry.lock` (Python)
- `go.mod` / `go.sum` (Go)
- `Gemfile` / `Gemfile.lock` (Ruby)
- `pom.xml` / `build.gradle` (Java)
- Any other dependency manifest

**ALWAYS use the appropriate package manager command:**

| Language | WRONG                                     | RIGHT                             |
| -------- | ----------------------------------------- | --------------------------------- |
| Node.js  | Edit package.json to add `"lodash": "^4"` | `npm install lodash`              |
| Rust     | Edit Cargo.toml to add `serde = "1.0"`    | `cargo add serde`                 |
| Python   | Edit pyproject.toml or requirements.txt   | `uv add package` or `pip install` |
| Go       | Edit go.mod to add require statement      | `go get package@version`          |

**WHY THIS MATTERS**: Package managers handle version resolution, lock file updates, transitive dependencies, and integrity checks. Manual edits bypass these safeguards and cause dependency hell.

**THE ONLY EXCEPTION**: Editing non-dependency sections of these files (scripts, metadata, configuration) is allowed.

### 6. Subagent Context Passing (MANDATORY)

**Subagents are BLIND to your context.** They do NOT see:

- CLAUDE.md files (global or project)
- Previous conversation history
- Files you've read
- Decisions you've made

**YOU MUST include in EVERY subagent prompt:**

1. **Full verbatim contents** of `~/.claude/CLAUDE.md` (user rules)
2. **Full verbatim contents** of project `CLAUDE.md` (project rules)
3. **Relevant file contents** - do not just pass paths, pass the actual content if the subagent needs it
4. **Complete context** about the task, constraints, and expectations
5. **Expected output format** with explicit examples

**WRONG**: `"Fix the bug in auth.ts following our conventions"` (subagent has no idea what conventions are)

**RIGHT**: Paste full CLAUDE.md contents + actual file contents + specific task details + expected output format

**Verification**: Before spawning, confirm you included full rule documents. Summarize if too large, but NEVER omit entirely.

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

**Context Passing**: See Mandatory Rule #6 - subagents are BLIND, you MUST paste full CLAUDE.md contents and file contents.

## Development Best Practices

### Banned Patterns

- Empty catch blocks, swallowed exceptions, bare `except` in Python
- Magic numbers without named constants; nested ternaries (max 3 levels)
- Console.log/print in committed code; `any` in TypeScript; default exports
- Functions >50 lines; commented-out code; secrets in code

### Required Patterns

- Early returns/guard clauses; descriptive names; validate at boundaries
- Specific error types with context; distinguish recoverable vs unrecoverable errors
- Plans in `./plans/PLAN-XXXX-description.md`; no time estimates
- Language tags on code blocks (`typescript`, `python`, `text`)

### Code Quality (STRICT)

**Size Limits**: Files 400 lines, functions 50 lines, classes 300 lines - refactor if exceeded

**Before Commit**: Zero IDE errors/warnings, all tests pass, no lint errors, code formatted

**Self-Review**: No hardcoded values, error/edge cases handled, no dead code, no security vulns

## Task Execution Workflow

**1. Discovery & Planning**: Review codebase, CLAUDE.md, `./plans/`. Use `senior-*` or `tech-lead` for complex planning.

**2. Architecture & Design**: Senior agents (opus) for design decisions → produce specs. Standard agents (sonnet) implement.

**3. Implementation**: Senior for algorithms/debugging; Standard for boilerplate. Parallel when independent. Test after each change.

**4. Quality Assurance**: `qa-engineer` for tests (65-80% unit, 15-25% integration, 5-10% e2e). `security-engineer` for auth/user data/APIs.

**5. Completion**: Update docs. **Clean up CLAUDE.md** (delete Session State). **At 85% context: STOP and write handoff notes.**

**Escalation**: Multi-domain architecture, unclear patterns, security implications, scope creep.
**Rollback**: Fix failing tests first; block on security issues; never commit broken code.

## Decision Framework

Evaluate: **Reversibility** (prefer reversible) | **Blast radius** (components affected) | **Consistency** (match existing patterns) | **Simplicity** (minimum needed) | **Testability**

## Red Flags

Stop and reassess: Hidden complexity, circular dependencies, unclear ownership, outdated docs, scope creep, unclear trade-offs.
