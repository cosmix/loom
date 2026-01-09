# Subagent Prompt Templates

## Standard Subagent

```
** READ CLAUDE.md IMMEDIATELY AND FOLLOW ALL ITS RULES. **

## Assignment: [task description]
## Files You Own: [paths this agent can modify]
## Files Read-Only: [paths for reference only]
## Acceptance: [criteria that must pass]
```

## Senior Agent (High Complexity)

Use for: merge conflicts, debugging, architecture, algorithms.

```
** READ CLAUDE.md IMMEDIATELY AND FOLLOW ALL ITS RULES. **

## Assignment: [task description]
## Complexity: HIGH - Use extended thinking (ultrathink)
## Files You Own: [paths]
## Files Read-Only: [paths]
## Acceptance: [criteria]
```

## When to Use Senior Agents

| Scenario | Reason |
|----------|--------|
| Git merge conflicts | Complex diff analysis |
| Debugging | Root cause tracing across files |
| Architecture decisions | System-wide impact analysis |
| Algorithm design | Correctness and complexity analysis |

## Parallelization Rules

- Same files? SERIAL
- Task B needs Task A output? SERIAL
- Otherwise? PARALLEL
