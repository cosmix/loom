# kickoff-session executor brief

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `loom`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Implements "Stage kickoff-session" and Design section 8 of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md. Decisions assumed: D10, D11.
PATHS: working_dir is loom/; YAML paths are package-relative (../ outside the crate).
KNOWLEDGE: architecture/planning-session-bridge.md, architecture/planning-session-sandbox.md, architecture/planning-guidance.md, doc/product/section_12.md (write rules are literal paths; the tool table), doc/product/section_11.md "How the server runs it" (the agent-team launch pressure sessions use).
RULES: until the size and name are confirmed the kickoff writes nothing under doc/; the grant covers one target that did not exist when the session started (doc/product/<name>/index.md for a feature, doc/plans/PLAN-<slug>.md and its brief bundle for a standalone plan); a confirmed name that already exists is refused; the planning hook, not the prompt, enforces it.
TEAM RULES: only a draft-all session launches with agent teams enabled and keeps SendMessage and ListAgents; teammates stay in-process; the hook allows writes only to the documents the run was started for; the bridge reads teammate transcripts for tokens and current document and renders none of their messages; at most five teammates.
Use parallel subagents and skills to maximize performance.
MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.

## Stage file boundary (repository-relative)

- loom/src/plan_session/**
- loom/src/plan_server/chat/**
- loom/src/planning/guidance/**
- plan-plugin/skills/kickoff/**
- plan-plugin/skills/section/**
- plan-plugin/skills/plan/**
