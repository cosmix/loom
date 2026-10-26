# W2: heading sections, GitHub anchors, span hashes

Tier: opus (`loom-senior-software-engineer`). Read `doc/plans/briefs/plan-documents/common.md` first.

## Files you own

`loom/src/planning/sections.rs`, `loom/src/planning/sections/tests.rs`,
`loom/src/fs/knowledge/splice.rs` (visibility only). Section 3 below (plan metadata spans) is codex
unit C4 (`sections/metadata.rs`), which runs in the same wave as you against pinned signatures:
`sections_of` calls `metadata::metadata_spans(text, &lines) -> (Option<Span>, Vec<(String, Span)>)`,
and C4 uses your `Span { start_line, end_line, hash }` and `normalized_hash(text: &str) -> String`
exactly as common.md pins them. Until both files exist the build may fail in the other file: wait
and rebuild, never edit it.

## 1. Widen the splice scanners

In `loom/src/fs/knowledge/splice.rs` change `fn atx_heading` (`:133`), `fn fence_mask` (`:220`) and
`fn section_end` (`:162`) to `pub(crate)`, nothing else. `section_end` already ends a section at the
first non-fenced heading of the same or a shallower level (`l <= level`), so it serves level 1
unchanged; `find_section`/`heading_level_of` keep their level 2-6 rule because
`loom knowledge replace-section` never targets a title. The existing splice tests must pass
untouched. `fence_mask`'s unclosed-fence degrade (`:239-251`) applies to planning documents too:
keep it.

## 2. `sections.rs`

Signatures in common.md ("Heading sections, anchors, span hashes"). Algorithm:

1. Split `text` into lines on `\n` (strip one trailing `\r` per line for scanning only).
2. `fence_mask(&lines)`; every non-fenced line where `atx_heading` is `Some((level, text))` with
   non-empty text is a heading (an empty `##` is not a heading for anchors: skip it). Strip an ATX
   closing sequence (` ##` at the end) from the title.
3. Extent: `section_end(&lines, &mask, i, level)`. Preamble: lines before the first heading
   (`0..first`, possibly empty).
4. Anchor: `github_slug(title)`, then de-duplicate in document order: the first use of a slug is
   bare, the next `slug-1`, then `slug-2`. A generated `foo-1` that collides with a real heading
   slug `foo-1` keeps counting (GitHub behaviour: track every emitted anchor in a set).
5. `github_slug`: lower-case (Unicode `to_lowercase`), drop every char that is not alphanumeric
   (Unicode), space, `-` or `_`, then turn each space into `-` (do not collapse runs). Strip inline
   Markdown first: backticks, `*`, and a link `[text](url)` becomes `text`. Examples (pin each in
   your tests): `4.2 Line Grammar` -> `42-line-grammar`; `Plans and their lifecycle` ->
   `plans-and-their-lifecycle`; `` `loom plan check` `` -> `loom-plan-check`;
   `01 — Workflow Model` -> `01--workflow-model` (em dash removed, both spaces kept);
   `Symlinks under `doc/`` -> `symlinks-under-doc`.
6. Hashes: `normalized_hash(span_text)` where span text is the span's lines joined with `\n`
   (plus a final `\n` when the span is non-empty). `normalized_hash`: replace `\r\n` and lone `\r`
   with `\n`, trim trailing whitespace (spaces and tabs) on every line, keep a single final newline
   state as it was, then `crate::context::source_graph::body_hash(bytes)`. `file_hash` is
   `normalized_hash(text)`.

`DocSections::find(anchor)`: `""` -> preamble, `loom-metadata` -> `metadata`, `stage:<id>` -> the
stage span, else the heading whose `anchor` equals it. Anchors are compared exactly (they are already
lower-case).

## 3. `sections/metadata.rs`

Plan metadata anchors (section 03 "Plan metadata"). Use
`crate::plan::parser::extraction::extract_yaml_metadata_with_ranges(text)` (`extraction.rs:72`,
`pub`): on `Err` (no block) both `metadata` and `stages` are empty. Convert
`metadata_block_range` (byte offsets) to lines: the `loom-metadata` span runs from the line holding
`<!-- loom METADATA` through the line holding `<!-- END loom METADATA` inclusive. Stages: inside
`yaml_fence_range`, every line matching `^(\s*)- id:\s*["']?([A-Za-z0-9_-]+)["']?\s*$` begins a
stage; it ends at the next such line with the same indent or at the closing fence line (exclusive).
Hash each span with `normalized_hash`. Do not parse YAML here and add no regex dependency beyond the
crate's existing `regex` (`loom/Cargo.toml` `regex = "1.13.1"`); a hand-written matcher is fine.

The metadata block lines also stay inside whatever heading section encloses them; the app-only
anchors are additional addresses, not a re-partition.

## Tests (yours, `sections/tests.rs`)

Every slug example above; repeats with collisions (`## A`, `## A`, `## A-1`); a heading as the
first line (empty preamble); a document with no headings (whole text is preamble); a fence opened
with `~~~~` and closed with `~~~~~`; an unclosed fence; CRLF input giving the same spans and hashes
as LF; a plan with two stages where the second stage id is quoted; `find` for each anchor kind.

## Proof

`cargo test --lib planning::sections::`, `cargo test --lib fs::knowledge::splice::`,
`cargo clippy --all-targets -- -D warnings`. Report the final `github_slug` rules and any example
that differs from GitHub's renderer that you checked by reasoning (state it as unchecked).
