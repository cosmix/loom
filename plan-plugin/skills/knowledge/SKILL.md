---
name: knowledge
description: Record the durable knowledge or correction named in the user's message through the knowledge CLI.
---

# Record knowledge

Use the user's message as the worklist of what to record.
Do not run `loom memory pending` or `loom memory resolve`.

Write only under `doc/loom/knowledge/`, through `loom knowledge update`,
`loom knowledge replace-section`, or `loom knowledge annotate`; never use a file tool.
Run `loom knowledge delete-section` only to remove a section that a `replace-section`
heading miss appended.
Let those commands regenerate `doc/loom/knowledge/INDEX.md` automatically.
Never stage, commit, branch, or run `loom init`, `loom run`, or `loom pressure`.

1. Read `doc/loom/knowledge/INDEX.md` if it exists, then only the relevant knowledge
   sections and source ranges needed to establish the current facts. Record only
   non-obvious, durable insights. Omit ephemeral task details, procedural steps, and
   information already captured in code, version history, or the knowledge base.
   Convert relative dates to absolute dates.
2. Apply corrections first. Correct an existing claim in place with
   `loom knowledge replace-section <file> "<heading>" "<body>"`, passing the body
   without its `##` heading line. Before running it, read the file's headings and copy
   the heading verbatim: a heading that does not match makes the command append a new
   section instead of replacing one. Never use `loom knowledge update` to correct an
   existing claim: appending leaves the wrong claim in place. Read the command's
   result; if it reports an append, run
   `loom knowledge delete-section <file> "<appended heading>"`, then run
   `replace-section` again with the heading copied exactly from the file.
3. Keep current truth only outside `mistakes.md` and `mistakes/*`. Rewrite dated
   headings and "was", "used to", or "supersedes" narratives into current-state
   text with `replace-section`. Move history that carries a lesson into a mistakes
   topic and link to it from the current-state section.

Route each new insight to the right file:

- `architecture.md` — component relationships, data flow, module/dependency graph
- `patterns.md` — reusable architectural patterns found in the codebase
- `conventions.md` — naming, structure, and coding standards
- `mistakes.md` — what went wrong, the misleading signal, root cause, prevention rule, and fix
- `concerns.md` — tech debt, warnings, known issues
- `stack.md` / `entry-points.md` — dependencies/tooling and key files to read first

1. When `INDEX.md` exists, treat these files as tier-1 summaries and the category
   directories as tier-2 topics. Put a new finding of roughly 40 lines or fewer
   inline in the relevant tier-1 file; put larger findings in
   `loom knowledge update <category>/<slug> "<entry>"` and leave a 2–4 line summary
   and link in the tier-1 file. Reuse a matching topic before creating another.
   Give a new topic a blurb of at most 80 characters with
   `loom knowledge annotate <category>/<slug> --blurb "<blurb>"`.
2. Write each finding as a self-contained `##` section with concrete symbols and
   paths inline, so retrieval can select it without surrounding sections.
   If a mistake repeats one already recorded, propose a hook or a
   `loom plan verify` check in `concerns.md` instead of adding another mistake.
   Use `update` for new sections and `replace-section` for existing sections,
   including edits to tier-1 summaries and links.
3. Report the knowledge paths and headings changed, and any unresolved evidence.
