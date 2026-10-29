# Planner UI redesign executor policy

The user selects `gpt-6.1-sol` with reasoning effort `high` for implementation,
fixture, test-authoring, documentation and review-fix executor subagents.
Verification and code review remain with Opus; Sol/high workers only implement fixes.
Other stage orchestrators retain their configured models. Knowledge distillation
remains single-agent.

## Dispatch

Split each stage into bounded units with explicit disjoint file ownership and pinned shared
interfaces. Finish producers before their consumers. Use `loom-codex-forwarder` for every
implementation unit that would otherwise use Sonnet; never use `codex:codex-rescue`.
Reserve Claude senior engineers for evidence-based escalation of failed fixes or work that
requires tools unavailable to Codex, recording the reason.

Give each forwarder this envelope with its unique stable unit id and bounded task:

```text
LOOM-CODEX-FORWARD-ONLY
--model gpt-6.1-sol --effort high
--unit-id <stage-unit>
Bash timeout: 600000 ms
<task, owned files, interfaces, and absolute brief paths inside this worktree>
```

Spawn from the worktree root and resolve common.md and the stage brief to absolute paths
inside that worktree. Package-relative task paths use the stage's declared working_dir.
Forward in the foreground, at most six disjoint Codex units concurrently. Each forwarder
makes one wrapper call. An active report is not completion: the orchestrator uses the
installed `loom subagents watch` workflow for the exact unit before scheduling another writer.
Inspect partial edits before recovery; split oversized units instead of reforwarding unchanged.
Report and escalate a failed fix rather than blindly retrying it.

Workers never spawn agents, run git or loom CLI commands, write orchestration state, or run
builds, tests, formatting, linting or type checks. They author required regression tests and
report changed files, assumptions, failures and memory-worthy observations. The orchestrator
records observations through loom memory and follows the existing stage/daemon verification
and commit workflow. Preserve all existing acceptance criteria.
