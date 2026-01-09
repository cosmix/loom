# Signal Template

Location: `.work/signals/<session-id>.md`

## Template

```markdown
# Signal: [session-id]

- **Stage**: [stage-id] | **Plan**: [plan-id]

## Tasks
[from stage definition]

## Context Restoration
[file:line refs to read first]
```

## Purpose

Signals tell a new session what work to resume. Created by:
- The daemon when spawning new sessions
- Handoffs that need continuation

## On Session Start

1. Check `.work/signals/` for your session ID
2. If found, execute the signal's tasks immediately
3. If not found, ask the user for direction
