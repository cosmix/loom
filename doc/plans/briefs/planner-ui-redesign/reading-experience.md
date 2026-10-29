# reading-experience executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `web`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements Design section 6 and the reading-face half of section 4 of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md (ink wash and inline caps, both settled with the user; the cap sizes, tints and tooltip are specified there).
PATHS: working_dir is web/.
KNOWLEDGE: conventions/plan-web.md (Markdown renderer rules; Concept Mode Test Rules, including that the fold control's accessible name stays exactly Fold).
RULES: inline Markdown in summary sentences goes through src/plan/markdown/inline.tsx, never dangerouslySetInnerHTML; colours only from CSS variables; reduced motion turns transitions off; the tests that pinned the gutter handle change in the same commit.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- web/src/plan/concept/concept.css
- web/src/plan/concept/concept.contracts.test.tsx
- web/src/plan/concept/column-keys.ts
- web/src/plan/concept/document-key.tsx
- web/src/plan/concept/fold-handle.tsx
- web/src/plan/concept/fold-handle.test.tsx
- web/src/plan/concept/fold-note.tsx
- web/src/plan/concept/paragraph.tsx
- web/src/plan/concept/passage.tsx
- web/src/plan/concept/run.tsx
- web/src/plan/concept/sentence.tsx
- web/src/plan/concept/sentence-text.tsx
- web/src/plan/concept/sentence-text.test.tsx
- web/src/plan/concept/reading-column*.tsx
- web/src/plan/concept/reading-column*.ts
- web/src/plan/markdown/markdown.css
- web/src/plan/markdown/inline.tsx
- web/src/plan/files/reading-column.tsx
