# Knowledge Hierarchy

> Tier-1/tier-2 knowledge mechanics: layout predicate, target parsing, INDEX.md generation, audit link rules, coverage blast radius, opt-in migration, lock ordering.

## Module Layout (`fs/knowledge/`)

Split by concern when the tiering work pushed `dir.rs` past the 400-line cap; every public
method signature was kept stable so no caller changed.

| File | Owns |
| --- | --- |
| `types.rs` | `KnowledgeFile`, `KnowledgeTarget`, `KnowledgeLayout`, `INDEX_FILENAME`, the tier-1 alias table |
| `dir.rs` | `KnowledgeDir` — `initialize`, `append_target`, `replace_section_target`, `splice_section`, layout detection |
| `index.rs` | `scan_topics`, `generate_index`, `write_index` |
| `gc.rs` | `analyze_gc_metrics`, `find_oversized_sections`, orphan and broken-link detection, thresholds |
| `summary.rs` | signal-facing knowledge summary |
| `templates.rs` | tier-1 and tier-2 file scaffolds |

The alias table lives in `types.rs`, not the CLI layer: `commands/knowledge/mod.rs::parse_file_type`
delegates to `KnowledgeFile::parse` so the data layer and the CLI cannot drift.

## Layout Predicate

A knowledge directory is **`Hierarchical` iff `INDEX.md` exists**, otherwise `Legacy`
(`dir.rs::layout`). Nothing else is consulted — not topic directories, not content, not links.
Creating `INDEX.md` flips the layout; deleting it downgrades instantly.

## Targets: Tier-1 vs Tier-2

`KnowledgeTarget::parse` splits on the first `/`. No slash → a tier-1 file (resolved through the
alias table: `arch`/`map`/`overview` → `architecture.md`, `lessons` → `mistakes.md`, and so on).
One slash → a tier-2 topic at `<category-dir>/<slug>.md`. A second slash is rejected: topics are
exactly one level deep.

Slugs go through `validation::validate_id` — ASCII alphanumeric plus `-` and `_` only (no dots),
128 chars max, no reserved device names, and **no leading two-digit `NN-` prefix**. A trailing
`.md` is stripped, so `architecture/foo` and `architecture/foo.md` are the same target. The
category directory is created automatically on first write.

## INDEX.md Generation

`loom knowledge index` regenerates `INDEX.md` from the on-disk tree: a generated-file marker, a
reading-protocol blurb, a **Tier 1** table (file, description, line count) and a **Tier 2**
table grouped by `### <category>` (topic path, title, blurb, line count). The `Tier 2` section is
omitted entirely when no topics exist.

`scan_topics` is **non-recursive** — it reads `<root>/<category>/*.md` only, skipping dotfiles
and non-`.md` entries. Nested subdirectories under a category are ignored completely. Title is
the first `#` line, blurb the first `>` line, falling back to the slug and an empty string.

Regeneration is idempotent and does a full atomic overwrite, so hand edits to `INDEX.md` are
silently destroyed. Every `update` / `replace-section` also refreshes the index — but **only
once the directory is already hierarchical**.

## Audit Rules — the Two Checks Disagree About Link Form

This is the single most surprising part of the system, and it drives how every link must be
written.

- **Orphan detection** (`gc.rs`): a topic is an orphan unless its relative path
  (`architecture/foo.md`) appears as a **plain substring** in one of the **seven tier-1 files**.
  `INDEX.md` is *not* in that set — a topic linked only from the generated index is still an
  orphan. The `.md` extension is required.
- **Broken-link detection** (`gc.rs`): a stricter regex that only matches the **inline markdown
  form** `](category/slug.md)` with a literal `)` immediately after `.md`. A leading `./` or a
  trailing `#anchor` makes a link invisible to this check. Only tier-1 files are scanned;
  topic-to-topic links are never validated.

**Therefore the one link form that satisfies both checks is `[Title](category/slug.md)` —
relative, no `./`, with `.md`, no anchor, written in a tier-1 file.**

Index staleness is a third, separate check: `INDEX.md` must textually contain every tier-1
filename and every topic path. Line counts in the index are *not* compared, so stale numbers are
never flagged.

## Thresholds

| Constant | Value | Meaning |
| --- | --- | --- |
| `SECTION_EXTRACT_THRESHOLD` | 40 | a tier-1 `##` section with **more than** 40 body lines is flagged for extraction |
| `DEFAULT_MAX_TIER1_LINES` | 250 | per tier-1 file (`--max-file-lines`) |
| `DEFAULT_MAX_TOPIC_LINES` | 500 | per tier-2 topic (`--max-topic-lines`) |
| `DEFAULT_MAX_PROMOTED_BLOCKS` | 3 | `## Promoted from Memory` blocks per file |

**There is deliberately no aggregate line cap.** Total lines are computed and printed for
information only. A total budget punishes a growing codebase for recording what it learned; the
per-file and per-section limits shape *structure* instead, which is the thing that actually
degrades retrieval.

## Coverage Blast Radius (`commands/knowledge/check.rs`)

`architecture_coverage_text()` concatenates tier-1 `architecture.md` **plus every tier-2 topic
whose category is `Architecture`** before matching `src/` directories. Without it, the first
restructuring that moved prose out of the tier-1 summary would have collapsed the number that
plans gate on with `loom knowledge check --min-coverage`.

The filter is `category == Architecture`. **Architecture prose relocated into a different
category directory silently stops counting toward coverage.** Keep architecture content under
`architecture/`, and re-run `loom knowledge check --min-coverage` after any move.

## Migration Is Opt-In (a Deliberate Backwards-Compatibility Exception)

This project otherwise forbids compatibility shims and migration routines. The knowledge layout
is the documented exception: `KnowledgeDir::initialize()` captures `let fresh = !root.exists()`
and writes `INDEX.md` **only for a directory it just created**. An existing flat knowledge base
is never auto-migrated, and `KnowledgeLayout` keeps its `Legacy` arm indefinitely.

The reason is that a knowledge base is **user-curated prose, not code**. Silently restructuring
thousands of lines of someone's writing as a side effect of an unrelated command is destructive
and unreviewable; a wrong migration cannot be recovered by re-running a build. Upgrading is
therefore explicit — `loom knowledge index` for structure only, or `loom knowledge gc` for an
agent-driven compaction. As a nudge, a `Legacy` directory always reports `gc_recommended`, so on
a flat directory that flag means "not yet migrated", not "something is wrong".

## Locking

`fs/locking.rs` locks a file's **parent directory**, not the file. `INDEX.md` and all tier-1
files share the knowledge root, so an index refresh must run **after** `locked_read_modify_write`
returns — calling it inside the closure self-deadlocks on the same thread, because `flock` is
per open file description. Tier-2 writes lock `<root>/<category>/` and therefore never collide
with an index write. Refresh failures warn to stderr and return `Ok`, so that a successful
content write is never retried into a double append.
