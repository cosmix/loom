# Knowledge Cli Gaps

> Knowledge CLI gaps: no delete-section, CRLF, backlog

## Knowledge Signals Never Teach Tier-2 (2026-07-28)

`orchestrator/signals/` generates `loom knowledge update <tier-1-file>` guidance, but **no
prefix teaches the `category/slug` tier-2 form**. Verified functionally: tier-2 works
(`loom knowledge update patterns/lock-ordering` creates the file and `INDEX.md` picks it up on
the next knowledge write) — it is simply never advertised to an orchestrated knowledge stage.

Consequence: the hierarchy grows only through the interactive `bootstrap`/`gc` paths, not during
`loom run`. Not a defect in what landed; follow-up stage material.

## `loom knowledge` Has No Delete-Section Verb (2026-07-28)

`update` appends and `replace-section` replaces, but nothing removes a section. Consolidating
several tier-1 sections into one tier-2 topic therefore cannot be completed with the CLI alone —
the migration in this plan replaced the lead section with a summary and had to strip the
remaining N-1 headings with an external script. A `loom knowledge drop-section` (or a
`replace-section --delete`) would close the gap.

## `loom knowledge update` Cannot Set a Topic Blurb (2026-07-28)

A new topic is seeded with a fixed scaffold — a title derived from the slug and the blurb
"Topic notes for the `<category>` knowledge area" — and user content is appended _after_ it.
`scan_topics` harvests the **first** `#` and `>` lines for the INDEX.md table, so the generic
seeded blurb wins unless corrected with `loom knowledge annotate <target> --blurb "<text>"` (at
most 80 characters; longer is refused, not truncated). `update` still has no `--blurb` flag of its
own, so seeding and correcting the blurb are two calls, and a leading `>` line inside the supplied
content becomes a second, redundant blurb-shaped paragraph in the body rather than replacing the
scaffold's.

## GC Flags Tier-1 Files for Section Extraction With No Oversized Sections (2026-07-31)

`analyze_gc_metrics` flags a tier-1 file whenever its **total** exceeds `DEFAULT_MAX_TIER1_LINES`
(250), independently of whether any individual section exceeds the section threshold. All six
tier-1 files here currently report `0 oversized sections` yet appear as extraction targets, and
the GC system prompt's first instruction is "Extract oversized tier-1 sections into tier-2 topic
files" — sections the analyzer itself says do not exist.

The agent is left to invent a split with no guidance on where the seams are, which is exactly the
condition under which a restructuring run drops content.

**Fix:** when a file is over budget but has no oversized section, say so in the prompt and ask for
a split proposal by topic cohesion instead of naming a section-extraction target that isn't there.

## `loom knowledge` Cannot Rename a Section Heading (2026-08-17)

`loom knowledge replace-section <file> <heading> [content]` replaces a section's **body** and
keeps the existing heading line. `splice_section` (`fs/knowledge/dir.rs:278`) matches the EXISTING heading and
always re-emits that same heading text — there is no way to change the heading itself through
the CLI, so marking an entry resolved in the repo's `~~strikethrough~~ (RESOLVED date)`
convention still requires a direct file edit for the heading line, even though the body can now
be corrected in place.

**Fix:** either accept a `--heading <new>` flag, or match the OLD heading and re-emit whatever
heading line the content passed in.

## Two Fenced-Code-Block Models Disagree in `fs/knowledge/` (2026-08-26)

`splice.rs`'s `fence_mask` (the level-agnostic `replace-section` splicer) requires a closing
fence's run length to be at least as long as the opener's — CommonMark-correct. `chunker.rs`'s
`fence_marker` (used for retrieval chunking, `~:153-222`) closes on ANY line whose first
non-whitespace characters start with the same delimiter, regardless of run length. They disagree
on, e.g., a ` ``` ` line inside a ```` ```` ```` fence: `chunker.rs` treats it as closing the
fence early (so a `##` line just past it can be lexed as a heading) while `splice.rs` keeps
reading through to the real closer. Consequence: the span `loom knowledge context` reads a
heading from is not necessarily the span `replace-section` will overwrite. Should become one
shared scanner.

## `replace-section` Silently Converts CRLF to LF and Can Drop Trailing Blank Lines (2026-08-26)

`splice.rs::assemble_replacement` rebuilds the file from `base.lines()` (which strips both `\n`
and any preceding `\r`) joined back with plain `\n`, so a CRLF knowledge file is silently
rewritten to LF on any `replace-section` write. When the replaced section runs to EOF, any
trailing blank lines that followed the old section body are also dropped, since the tail beyond
the match is not copied forward. Harmless today — this tree's knowledge files are LF — but worth
knowing so a surprising whitespace-only diff after a `replace-section` call is explainable rather
than alarming.

## `loom knowledge update` Trims Stdin Content But Not Inline Content (2026-08-26)

`commands/knowledge/mod.rs::resolve_content` trims stdin input (`read_content_from_stdin` calls
`buffer.trim()`) but passes an inline CLI argument through untrimmed (`Some(c) => c`). An inline
`loom knowledge update <file> "<content with trailing blank lines>"` call therefore widens the gap
before the next appended section, while the same content piped via stdin would not. Minor, but the
two paths should agree.

## Tier-1 Knowledge Housekeeping Backlog

`loom knowledge check --strict` enforces 250 lines per tier-1 file, 40 lines per tier-1
section, and 12 KB for `INDEX.md` (`fs/knowledge/catalog/size.rs`). The remaining work is a
dedicated knowledge-reorganization project, not part of token-governor correctness.

- **All six tier-1 files remain oversized.** Their individual sections are compact; the
  overage is cumulative volume. Moving roughly 80-120 sections safely into tier-2 topics
  should be done file-by-file, preserving links and checking for duplicate headings.
- **`MissingSourceRef` remains the dominant finding.** Resolution needs the FULL path relative to a
  package's src root (e.g. commands/status/data/collector.rs, loom-hooks/spawn-guard.sh) — a bare
  filename (collector.rs) or a partial suffix (ledger/legend.rs, ui/tui/app.rs) fails even when
  that suffix is unique in the tree; only the fully-qualified relative path resolves. The residual is
  mostly bare filenames, hook filenames written without their `loom-hooks/` prefix, and genuinely stale
  citations that cannot be assigned to one package root safely. Canonicalize them to the full
  src-relative form; ambiguity must continue to fail closed.
- **Tier-2 topics with generic blurbs are unfixable from inside a stage session.** A stage session's
  `loom-hooks/worktree-file-guard.sh` hook denies Edit/Write on any path under `doc/loom/knowledge/`, and
  there is no `loom knowledge` CLI verb for the blurb line specifically (only `update`, which appends,
  and `replace-section`, which needs an existing `#{2,6}` heading — the blurb is a bare `>` line under
  the H1). A knowledge-distill stage that creates a new tier-2 topic via
  `loom knowledge update <category>/<slug>` therefore cannot repair its own auto-scaffolded "Topic
  notes for the `<category>` knowledge area." blurb; that repair needs either a `--blurb` flag on
  `update`/a dedicated verb, or a direct file edit from an interactive (non-stage) session.
- **The generated index remains oversized.** This is low-priority navigation cleanup.
- **2026-09-04 reconfirmation (web-dashboard plan's knowledge-distill stage):** the backlog is at
  728 `MissingSourceRef`/oversized-file issues under `./doc/loom/knowledge` on a tree with none of
  this plan's own additions applied (verified against the unmodified HEAD tree via a temporary
  stash), essentially unchanged from prior counts. This plan's own new/edited knowledge content adds
  ZERO net new issues (verified: every bare filename this stage introduced was corrected to its
  fully-qualified src-relative path before completion). `loom knowledge check --strict` — the
  canonical knowledge-distill acceptance criterion per `skills/loom-plan-writer/SKILL.md` — therefore
  still fails on this tree for reasons entirely predating this plan; fixing the backlog itself needs
  the dedicated reorganization project named above, not a per-plan knowledge-distill stage. A stage
  hitting this should confirm (as here) that its own additions are clean, then treat the residual
  count as this pre-existing, already-tracked concern rather than attempting to clear it inline.

## File Tools Are Blocked on Knowledge Files, and No Command Renames a Heading (2026-09-13)

The worktree file guard refuses Edit and Write under `doc/loom/knowledge/` ("knowledge files are
recorded through `loom knowledge update`, not file tools"), in a knowledge-distill stage too. The
only in-place channel is `replace-section`, which rewrites a whole section body and keeps its
heading. Nothing renames or deletes a heading, so a count or status in a heading goes stale: the
knowledge-hierarchy page still says "Nine" issue kinds, with a correction in its body. A surgical
fix inside a long section means regenerating the whole body (extract it, apply exact-once
substitutions, pipe the result to `replace-section` from a file). `loom-control-complete.sh` also
rejects a Bash command line that merely resembles a completion command, so a long body belongs in a
file fed on stdin, not an inline heredoc.

`INDEX.md` sits at its 16 384-byte `OversizedIndex` budget: it was 1 byte over before the
token-optimization distill wrote anything. Every new tier-2 topic adds a row, so each one has to be
paid for with shorter blurbs (`loom knowledge annotate <target> --blurb`).
