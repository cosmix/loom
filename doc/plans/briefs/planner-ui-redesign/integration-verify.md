# integration-verify executor brief

Opus owns verification and code review. Sol/high executors only implement assigned fixes.

Read common.md in this directory first. Model: `gpt-6.1-sol`; effort: `high`.

Stage working directory: `loom`. Each worker receives a bounded subset and explicit
owned files from the orchestrator, not the whole stage. The source plan defines design,
acceptance and stage boundaries.

## Stage requirements

Final verification of doc/plans/IN_PROGRESS-PLAN-planner-ui-redesign.md. Verify FUNCTIONAL INTEGRATION, not just tests passing. NEVER Claude Code auto-memory.
CONTEXT: read the plan, loom memory show --all, doc/loom/knowledge/INDEX.md and only the relevant sections.
BUILD & TEST (zero tolerance): loom check integration-verify once.
CODE REVIEW: parallel loom-code-reviewer subagents: (1) the planning model's feature rules against the updated doc/product sections 01 and 03 (classifier, ids, loose plans, retired path, duplicate slugs); (2) web security (Untrusted, inline Markdown path, no dangerouslySetInnerHTML); (3) layout and accessibility in a real browser: home, feature screen, product overview with a run opened two levels deep, a plan in file mode, the new-feature flow, at 390px and 1280px in Ledger, Aubergine, Pacific and Graphite (no sideways scroll, focus order, the fold buttons and Esc reachable by keyboard, hover visible in every theme). Fix every finding through an engineer agent or dispute it; never defer one.
WEB FIXES: a review fix in web/src rebuilds web/dist after the final review round and commits it last as chore(web).
Record discoveries to loom memory, including stale knowledge: loom memory note "stale-knowledge: ...".

Worker restrictions in common.md govern executor actions. Instructions above about
verification, git, memory and dispatch belong to the orchestrator or daemon.
