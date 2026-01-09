# Loom Plan Format

Plans must wrap YAML in HTML comment markers for the parser.

## Required Structure

```markdown
<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-id
      name: "Stage Name"
      dependencies: []  # or ["other-stage-id"]
      parallel_group: "group-name"  # optional
      acceptance:
        - "cargo test"
        - "cargo clippy"
      files:
        - "src/**/*.rs"
```

<!-- END loom METADATA -->
```

## Stage Fields

| Field | Required | Description |
|-------|----------|-------------|
| `id` | Yes | Unique identifier (kebab-case) |
| `name` | Yes | Human-readable name |
| `dependencies` | Yes | Array of stage IDs (use `[]` if none) |
| `parallel_group` | No | Group stages that can run concurrently |
| `acceptance` | No | Commands that must pass for completion |
| `files` | No | Glob patterns for files this stage owns |

## Execution Diagram

Include a simple dependency diagram in your plan:

```
[stage-a] --> [stage-b] --> [stage-d]
          \-> [stage-c] -/
```

Legend: `-->` = depends on. Stages at same depth with same deps run in parallel.

## Important

Without the `<!-- loom METADATA -->` markers, `loom init` cannot parse the plan.
