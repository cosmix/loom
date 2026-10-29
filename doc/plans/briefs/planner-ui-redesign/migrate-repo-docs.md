# migrate-repo-docs executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `"."`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements "Stage migrate-repo-docs" of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md, decision D3. Runs only after IN_PROGRESS-PLAN-feat-planner-11-execution-handoff.md is done.
Use git mv for every tracked move. Rewrite doc/product/index.md and doc/product/section_NN.md references in loom/src comments, doc/loom/knowledge and unlocked plans; leave locked plans (IN_PROGRESS-, DONE-, RETIRED-, archive/) and their briefs unchanged. Print the mv commands for gitignored DONE- plans for the operator.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- .gitignore
- doc/product/**
- doc/plans/**
- doc/loom/summaries/**
- loom/src/**
