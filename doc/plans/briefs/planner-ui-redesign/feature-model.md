# feature-model executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `loom`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements Design section 1 and the pace offer of section 8 of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md. Read that plan's Design and "Stage feature-model" sections first; they list every file and the decisions (D1, D2, D4, D10) this stage assumes.
PATHS: working_dir is loom/; YAML paths are package-relative (../ outside the crate).
KNOWLEDGE: architecture/planning-document-model.md (Module Map; Kinds, Ids and Slugs; Project View and Checklist), architecture/summary-store.md, architecture/planning-guidance.md.
ORDER: (1) kinds, slug and discover with their tests, as a foundation worker; (2) checklist, offers, problems, implements, summary store ids, plan_lifecycle retired path, in parallel; (3) plan_server payloads, the /api/features route, guidance opening and plan-plugin skills, plan check output and its JSON fixture.
RULES: the top level of doc/plans/ holds standalone plans only; a top-level vision, section or metaplan is a needs-feature problem, never a fallback feature; plan ids stay plan/<slug> and slugs stay unique repository-wide (a duplicate slug across features is a problem, never a silent winner). No backwards-compatibility shims: this repository's own docs move in migrate-repo-docs.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- loom/src/planning/**
- loom/src/plan_server/**
- loom/src/commands/plan/**
- loom/src/fs/plan_lifecycle.rs
- loom/src/fs/plan_lifecycle/**
- loom/src/fs/plan_lifecycle_tests.rs
- loom/tests/planning_*.rs
- loom/tests/fixtures/planning/**
- plan-plugin/skills/**
