# Parser rules (reference for codex units C1 to C7)

This file is no longer a worker brief: units C1 to C7 (`c1-vision.md` to `c7-xref.md`) each own one
parser and its test file, and read the matching section below for its rules. The Tests section at
the end lists cases each unit covers in its own test file.

All parsers are pure functions of `&str` (plus `&DocTree` where they resolve names); none reads a
file except `briefs`, which calls `collect_inputs`. Heading detection always goes through
`crate::planning::sections::sections_of` (fence-aware); never scan `#` lines yourself. Lines in
your public types are 0-based (the problem layer converts).

## `vision.rs` (section 01 "Vision")

`parse_sections_table(text) -> Result<Vec<VisionRow>, VisionProblem>`. Find the first heading whose
title equals `Specification sections` ignoring ASCII case; within its span (outside fences, from
`fence_mask`), the first GFM table: a header row and a delimiter row (`| --- |`, cells of `-` with
optional `:`), then body rows until a line that is not a table row. Rows whose first cell (trimmed,
backticks stripped) is all digits become `VisionRow { number, name: second cell trimmed, line }`.
`VisionProblem::{NoHeading, NoTable { heading_line }, NoNumberedRows { heading_line }}`. Never look
outside that heading's span. Duplicate numbers: keep the first, ignore the rest.

## `metaplan.rs` (section 01 "Metaplans", section 03 "Metaplans")

`is_metaplan(rel)`: `doc/plans/index.md` or a `.md` file directly in `doc/plans/` whose name
contains `metaplan`. `entries(text)`: every level-2 heading (from `sections_of`) in order; the first
backticked token in the heading text that `slug::plan_slug` accepts (any lifecycle spelling) makes
it an entry with `plan_file` and `slug`; a heading whose text starts with digits and `.` but names no
plan file is returned with `plan_file: None` (the problem layer flags it); any other heading is
prose and is not returned. A numbered heading whose text contains `no plan file` (ASCII
case-insensitive), the opt-out this repository's `doc/plans/optim-token-metaplan.md:387` uses
(`## 6. Policy notes (no plan file)`), is prose too.

## `implements.rs` (section 03 "Product-to-plan links")

`parse(text)`: only lines BEFORE the `<!-- loom METADATA` marker and outside fences; a line counts
when it starts with `Implements:` at column 0. The first such line is the value; later ones go to
`duplicate_lines`. Value tokens split on `,`, trimmed: `none` (case-insensitive, alone) ->
`NoneDeclared`; `section_<digits>`, `section_<digits>.md`, or a Markdown link whose text or target
file stem is `section_<digits>` -> that number; anything else -> `bad_tokens` as written. An empty
value is a bad token `""`. No line -> `Unmapped`.

## `pressure.rs`

`records(text)`: the heading titled `Pressure tests` (level 2) and, within its span, every list
item `- YYYY-MM-DD: ...` (date validated as digits in that shape) -> `PressureRecord { date, line,
text }`. `open_questions(text)`: for each heading titled `Open questions` (ASCII case-insensitive),
the number of list items starting at column 0 (`- `, `* `, `<digits>. `) inside its span; sum over
headings.

## `xref.rs` (section 03 "Cross-references")

`references(doc_rel, text, tree) -> Vec<Reference>` with `RefForm::{RelativeLink, PathToken, PlanFile}`:

- Relative Markdown links `[text](target)`: skip URLs with a scheme (`http:`, `https:`, `mailto:`)
  and pure fragments `#x` (resolve those against the document itself). Resolve the path relative to
  the document's directory lexically (no filesystem); a result outside `doc/` is a code link and is
  skipped. `target` = the document's `DocId` and the fragment; `None` when no discovered document has
  that path, or when the fragment is not an anchor of the target (`sections_of` on the target's
  text, read by the caller: take a `&dyn Fn(&Path) -> Option<DocSections>` lookup parameter so this
  module stays pure).
- Path tokens: refactor `plan/schema/structural_checks.rs::extract_brief_paths` (`:41`) into
  `pub(crate) fn extract_prefixed_paths(text: &str, prefix: &str) -> Vec<String>` with
  `extract_brief_paths(text)` calling it with `"doc/plans/briefs/"` (behaviour unchanged; its
  existing tests must pass untouched). Call it with `"doc/"`; skip tokens holding `*`, `?`, `<` or
  `>` (placeholders and globs); strip a trailing `:<digits>` line suffix.
- Plan file names: backticked tokens that `slug::plan_slug` accepts resolve by slug through
  `tree.plan(slug)`, from anywhere in `doc/`.

## `briefs.rs` (section 03 "Briefs")

`plan_briefs(root, entry: &PlanEntry, stage_ids: &[String])`: `crate::plan::inputs::collect_inputs(root, &entry.path)`;
group every input other than the plan: under `doc/plans/briefs/<slug>/<stage-id>/` -> `by_stage`
when the directory name is one of `stage_ids`; `common.md` directly in the bundle -> `common`;
everything else -> `other`; `invalid` entries and inputs that do not exist -> `broken` (their token
and reason). Stage ids come from the plan text: the caller passes `DocSections::stages` ids.
`orphan_bundles(tree)`: bundle directory names under `doc/plans/briefs/` whose slug matches no
`tree.plans` key.

## Tests (yours)

`parse_tests.rs`: this repository's vision table shape (copy rows 01-03 of
`doc/product/index.md` "Specification sections", lines 175-180, into the fixture string), a table
under the wrong heading, a heading inside a fence, numbered rows with backticks, `Implements:`
inside a fence and after the metadata marker, `none`, link forms, metaplan entries with every
lifecycle spelling and this repository's metaplan heading form
(`` ## 1. Document model and project checklist (`PLAN-feat-planner-01-documents.md`) `` from
`doc/plans/metaplan-planner-ui.md:134`) and the opt-out heading above, pressure records with a malformed date, open questions
under two headings. `xref_tests.rs`: each form resolving, each form broken, a code link skipped,
a placeholder token skipped, a fragment that is not an anchor.

## Proof

`cargo test --lib planning::`, `cargo test --lib plan::schema::`, `cargo clippy --all-targets -- -D warnings`.
Report every public item you added.
