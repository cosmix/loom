---
description: Distill important session insights into doc/loom/knowledge
---
Add any information that is deemed important for a future agent or human engineer working on this project to `doc/loom/knowledge`, focusing on architectural insights, conventions, and mistakes made (and their resolution if available).

Route each insight to the right file (the files are append-only — add, don't rewrite):

- `architecture.md` — component relationships, data flow, module/dependency graph
- `patterns.md` — reusable architectural patterns found in the codebase
- `conventions.md` — naming, structure, and coding standards
- `mistakes.md` — what went wrong, the misleading signal, root cause, prevention rule, and fix
- `concerns.md` — tech debt, warnings, known issues
- `stack.md` / `entry-points.md` — dependencies/tooling and key files to read first

If `doc/loom/knowledge/INDEX.md` exists, the layout is hierarchical: each file above is a tier-1 summary, and per-category directories (`architecture/`, `mistakes/`, ...) hold tier-2 topic files for deeper detail. Route by size — a finding that fits in roughly 40 lines or fewer goes inline into the tier-1 file above; anything larger goes to `loom knowledge update <category>/<slug>`, leaving a 2-4 line summary plus a link in the tier-1 file. Run `loom knowledge index` last, after all knowledge writes, to regenerate `INDEX.md`.

Only record what is non-obvious and durable. Do NOT record ephemeral task details, procedural steps, or anything already captured in the code, git history, or these files. Convert any relative dates to absolute.
