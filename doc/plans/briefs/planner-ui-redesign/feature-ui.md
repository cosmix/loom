# feature-ui executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `web`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements Design sections 2 and 8 of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md. Decisions assumed: D1, D10 (and the settled layout and D11).
PATHS: working_dir is web/.
KNOWLEDGE: conventions/plan-web.md (Chat Panel and Review UI Rules; New PlanMessage Variants Need Both Schema Arms), architecture/plan-server.md.
RULES: every server string through Untrusted; every storage key through storageKey; read-only browsers see the screens but no write controls (DisabledReason); the kickoff transcript is set in the reading face and the taking-shape panel in Inter.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- web/src/plan/router.tsx
- web/src/plan/home/**
- web/src/plan/feature/**
- web/src/plan/new/**
- web/src/plan/api/**
- web/src/plan/state/**
- web/src/plan/offers/**
- web/src/plan/header/feature-switcher.tsx
