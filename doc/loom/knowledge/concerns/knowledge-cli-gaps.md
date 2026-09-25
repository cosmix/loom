# Knowledge Cli Gaps

> Knowledge CLI gaps and housekeeping

## `loom knowledge update` Cannot Set a Topic Blurb (2026-07-28)

A new topic is seeded with a fixed scaffold — a title derived from the slug and the blurb
"Topic notes for the `<category>` knowledge area" — and user content is appended _after_ it.
`scan_topics` harvests the **first** `#` and `>` lines for the INDEX.md table, so the generic
seeded blurb wins unless corrected with `loom knowledge annotate <target> --blurb "<text>"` (at
most 80 characters; longer is refused, not truncated). `update` still has no `--blurb` flag of its
own, so seeding and correcting the blurb are two calls, and a leading `>` line inside the supplied
content becomes a second, redundant blurb-shaped paragraph in the body rather than replacing the
scaffold's.

## `loom knowledge` Cannot Rename a Section Heading (2026-08-17)

`loom knowledge replace-section <file> <heading> [content]` replaces a section's **body** and
keeps the existing heading line. `splice_section` (`fs/knowledge/splice.rs`) matches the EXISTING heading and
always re-emits that same heading text, so a count or status carried in a heading ("Nine issue kinds",
"~~strikethrough~~ (RESOLVED date)") goes stale in place.

**Workaround since 2026-09-19:** `loom knowledge delete-section <file> <heading>` removes a heading and its nested
subsections, so a rename is `delete-section` followed by `update` with the new heading (the section moves to the
end of the file). The knowledge-hierarchy audit-rules section was renamed this way. An in-place rename is still
missing.

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

**Closed by the 2026-09-19 cleanup.** `loom knowledge check --strict` exits 0 on this tree under the tier-1
limits (250 lines per file, 40 per section) AND the tier-2 pair (400 lines per file, 80 per section,
`fs/knowledge/catalog/size.rs`), with no `MissingSourceRef`. The earlier backlog (728 findings, six oversized tier-1
files, bare-filename source references) was cleared by the knowledge-bootstrap stage, and this stage split the five
tier-2 files and eleven sections that the new tier-2 limits flagged (context-retrieval, sandbox-and-settings,
subagent-orchestration, testing-and-lint and codex-plugin, plus eleven sections in other topics).

What remains is discipline, not backlog:

- **A tier-2 file or section that grows past its limit is split, not baselined.** `--write-baseline` exists for
  adopting a limit on a tree that cannot clear it yet; this tree can, so no baseline file is needed while
  `--strict` stays green. After `./dev-install.sh`, run `loom knowledge check --strict`: exit 0 means no baseline
  file to generate. If a newer binary reports issues this one does not, record them with
  `loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt` and gate with `--baseline`.
- **Source references resolve only as a fully qualified path relative to a package's src root** (for example
  `commands/status/data/collector.rs`, `loom-hooks/spawn-guard.sh`). A bare filename or a partial suffix fails even
  when the suffix is unique, and a sentence quoting a path that does not exist reports `MissingSourceRef`; reword it
  with an example marker or "does not exist".
- **A topic's blurb is set with `loom knowledge annotate <target> --blurb "<text>"`** (at most 80 characters), which
  works from inside a stage. `update` still cannot set it, so a new topic takes two calls.
- **Moving a section between topics has no verb.** The split record used `update <dst>` then `delete-section <src>`. An
  extractor that finds the section end MUST skip fenced code: a `# comment` line inside a fence is not a heading, and one
  split truncated a section at such a line and left an unclosed fence that made the checker count 152 lines.

## File Tools Are Blocked on Knowledge Files, and No Command Renames a Heading (2026-09-13)

The worktree file guard refuses Edit and Write under `doc/loom/knowledge/` ("knowledge files are
recorded through `loom knowledge update`, not file tools"), in a knowledge-distill stage too. The
in-place channels are `replace-section` (rewrites a section body, keeps its heading) and, since 2026-09-19,
`delete-section` (removes a heading and its subsections); a heading rename is delete plus `update`. A surgical
fix inside a long section means regenerating the whole body: extract it with a FENCE-AWARE scanner, apply
exact-once substitutions, and pipe the result to `replace-section` from a file. `loom-control-complete.sh` strips
inert heredoc bodies now (quoted delimiter, inert reader), but a long body still belongs in a file fed on
stdin; name that file `.distill-body-*` so the main-agent edit advisory ignores it.

`INDEX.md` has 108 bytes of headroom under its 16 384-byte `OversizedIndex` budget after the 2026-09-19 splits
added seven topic rows and roughly sixty blurbs were shortened to pay for them. Every new tier-2 topic adds a row,
so each one has to be paid for with shorter blurbs (`loom knowledge annotate <target> --blurb`). The next
distillation should expect to shorten more blurbs before adding a topic, or drop rows by merging small topics.
