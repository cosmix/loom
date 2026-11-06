# T2: edge tokens, anchored Implements, metaplan Covers (trace-core, wave 1)

Read `../common.md` first ("Edge lines", "Tokens", "`Implements:` gains anchors"). You run in
wave 1, before T1 and T3, so they build on your types.

## Files you own

`loom/src/planning/trace/tokens.rs` (and `tokens/` for its tests), `loom/src/planning/implements.rs`,
`loom/src/planning/tests/implements_tests.rs`, `loom/src/planning/metaplan.rs`,
`loom/src/planning/tests/metaplan_tests.rs`, `loom/src/planning/problems/plans.rs`,
`loom/src/planning/problems/trace.rs` (new), `loom/src/planning/briefs.rs`,
`loom/src/planning/tests/briefs_tests.rs`. You also create a declaration-only
`loom/src/planning/trace/mod.rs` holding `pub mod tokens;` and add `pub mod trace;` to
`loom/src/planning/mod.rs`; T1 owns both files after you.

## Tasks

1. **`trace::tokens`.** `SectionRef { number, anchor }` (Debug, Clone, PartialEq, Eq, Serialize,
   Ord) and `section_refs(value) -> (Vec<SectionRef>, Vec<String>)`. Then a general token parser
   for `About:` lines: `pub enum Token { Section(SectionRef), Vision(Option<String>),
   Metaplan(Option<String>), Plan { slug: String, anchor: Option<String> } }` and
   `pub fn tokens(value) -> (Vec<Token>, Vec<String>)`. The plan slug comes from
   `planning::slug::plan_slug` (it already strips lifecycle prefixes; read it). The Markdown link
   form reuses the label-or-target rule of `implements::token_number`; move that helper into
   `tokens` and call it from `implements`. Empty tokens (from `a,,b`) are bad tokens, kept as `""`.
   An anchor is accepted when it is non-empty and every char is a lowercase letter, digit, `-` or
   `_`, or when it is `stage:<id>` with a non-empty id of `[A-Za-z0-9_-]`. Any other anchor makes
   the token bad.
2. **`implements`.** Accept `section_NN#<anchor>`. Numbers deduplicated in first-seen order;
   `anchors: Vec<(u32, String)>` deduplicated in declaration order. Update the struct literal in
   `parse` and every constructor the compiler finds (`rg -n 'ImplementsLine \{' loom/src loom/tests`).
   `implemented_section_paths` keeps its contract (spec_check_contracts pins it).
3. **`metaplan`.** Add `covers`, `bad_covers`, `span_hash` to `MetaplanEntry`. The Covers line is
   the first non-fenced line starting `Covers:` inside the entry's span (from its heading line to
   the span's end line, `HeadingSection.span`), so a following entry's line is never read. Use
   `fence_mask` over the document's lines. `span_hash` is `heading.span.hash`.
4. **Stage lines.** `pub fn stage_lines(text) -> Vec<StageLine { stage: String, refs:
   Vec<SectionRef>, bad: Vec<String>, line: usize }>` in `tokens`, for `Implements (stage:<id>):`
   lines (column 0, outside fences, before `<!-- loom METADATA`).
5. **Problems.** In `problems/plans.rs`, report an anchored Implements token whose section or
   heading does not exist (`implements-anchor-missing`) and a stage line whose stage id is not in
   the plan's YAML stages (`stage-line-unknown`; the plan's stage ids are on the parsed
   `DocSections.stages`). In the new `problems/trace.rs`, report bad Covers tokens
   (`covers-bad-token`) and Covers anchors that do not resolve (`covers-anchor-missing`). Follow
   the existing problem struct and how `problems::collect` gathers sub-modules. `problems.rs`
   belongs to T1: send T1 the one `mod trace;` line plus the call you need, or the orchestrator adds
   it.

6. **Bundle helper.** `briefs::bundle_files(project, plan_rel)` exactly as common.md "The bundle"
   says. The run preflight's `plan_briefs` stays as it is. Test: a feature plan, a standalone plan,
   a plan with no bundle, and another plan's bundle beside it (never included).

## Tests

Unit tests beside each change: link-form tokens, an anchor with `..` or uppercase rejected, a
stage token, empty tokens, a Covers line in the next entry ignored, fenced lines ignored,
duplicate anchors deduplicated, and each new problem. Contracts `metaplan-entry-reads-its-covers-line`
and `implements-keeps-heading-anchors` are frozen in `loom/tests/planning_trace_contracts.rs`: make
them pass without editing them.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::` and the two contracts pass; no
file over 400 lines; report every consumer you changed.
