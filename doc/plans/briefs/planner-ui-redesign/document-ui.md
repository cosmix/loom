# document-ui executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `web`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements Design sections 7 and 12 of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md. The header is settled with the user and revisited once testing starts.
PATHS: working_dir is web/.
KNOWLEDGE: conventions/plan-web.md (Chat Panel and Review UI Rules), doc/product/section_11.md "Pressure testing from the app".
RULES: the document header replaces doc-header.tsx, plan-header.tsx and page-header.tsx, and the replaced files are deleted; the pressure restyle changes presentation only, never the run's behaviour or messages.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- web/src/plan/ui/doc-heading.tsx
- web/src/plan/ui/doc-heading.test.tsx
- web/src/plan/files/doc-header.tsx
- web/src/plan/files/file-mode.tsx
- web/src/plan/files/plan/**
- web/src/plan/files/tree*.tsx
- web/src/plan/concept/page-header*.tsx
- web/src/plan/markdown/render.tsx
- web/src/plan/markdown/markdown-render.test.tsx
- web/src/plan/chat/pressure/**
- web/src/plan/run/**
- web/src/plan/chat/panel.tsx
- web/src/plan/chat/run-header.tsx
