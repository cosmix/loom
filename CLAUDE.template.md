# Claude Code Rules

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

Always provide subagents with:

- Clear objective and scope
- Relevant file paths and context
- Constraints and requirements
- Expected output format
- All the information in this file and other project-specific CLAUDE.md files

## Development Best Practices

### Implementation Standards

- NEVER add TODO comments or stubs - always write production-ready implementations
- NEVER defer implementation of features or components

### Planning

- Create implementation plans in `./plans/PLAN-XXXX-description.md`
- Never use ~/.claude for plans
- Always reference plans by their project-relative path
- Never provide time estimates for tasks

### Documentation

- When creating markdown files and adding code blocks, ALWAYS specify the language for syntax highlighting (e.g., `typescript`, `python`, or if no language applies, `text`)

### Code Quality

- No file should exceed 400 lines. Refactor by breaking up large files into smaller modules
- Ensure no errors or warnings from IDE diagnostics before completing tasks
- ALWAYS Prefer internal tools over CLI tools when possible (but use `rg` over `grep` and `fd` over `find` when CLI is needed, e.g. piped sequences)

### Dependency Management

- NEVER add dependencies manually by editing package.json, Cargo.toml, pyproject.toml, or equivalent
- ALWAYS use package managers (npm, cargo, uv, etc.) for dependency management

### Progress Tracking

- Record progress in the project CLAUDE.md when working on significant tasks
- Reference phases/tasks found in project documentation
- When tasks are complete, remove detailed progress records to keep CLAUDE.md concise

## Security Practices

### Routine Security Checks

Run security scans frequently during development using the `security-scan` skill:

- **Before commits**: Check for hardcoded secrets
- **After dependency updates**: Run `npm audit`, `pip-audit`, etc.
- **During PR review**: Quick vulnerability scan

### When to Involve Security Engineer

Use the `security-engineer` agent (opus) for:

- Threat modeling new features or systems
- Security architecture decisions
- Comprehensive security audits
- Incident response and CVE analysis
- Compliance-related questions

### Security Skills Reference

| Skill | Use Case |
|-------|----------|
| `security-scan` | Quick routine checks (pre-commit, PR review) |
| `threat-model` | Architecture planning, STRIDE/DREAD analysis |
| `security-audit` | Comprehensive vulnerability assessment |
| `dependency-scan` | CVE scanning, license compliance |
| `auth` | Authentication/authorization implementation |
