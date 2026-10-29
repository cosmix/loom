# help executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `web`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements Design section 10 of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md. Decision assumed: D12.
PATHS: working_dir is web/.
KNOWLEDGE: conventions/web-dashboard-typography.md (Help uses operational illustrations; Help illustrations use real components; Help content), mistakes/web-dashboard-server.md (Help illustrations must match production UI).
RULES: every scene renders production components in an isolated Jotai store with sample state, never redrawn lookalikes; one sentence per pin; facts with no control go in Good to know; type at least 13px; the Legend topic is complete; preview the rendered dialog at 390px and 1280px before committing.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- web/src/plan/help/**
