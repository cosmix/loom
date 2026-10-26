# concept-data W1: wire model, key, links, state

Lane note: codex units own `wire/concept.rs` (C1), `concept/state.rs` (C2), `concept/links.rs`
(C3) and `concept/key.rs` (C4); the sections below that describe those files are the reference
their briefs build on, not your work. C1 is in the tree when you start.

Working dir `loom/`. You own `src/plan_server/wire/tests_concept.rs`, the one
`ConceptChanged(ConceptChanged)` variant in `src/plan_server/wire/message.rs` (plan 2's file:
add the variant, change nothing else), and the fixtures `../web/src/plan/api/fixtures/concept-{overview,summary}.json`. The stage
description's PUBLIC SURFACE lists every wire type and field; it is authoritative. Do
`wire/concept.rs` first and report when it compiles: W2 and W3 start from it.

## wire/concept.rs

Serde types exactly as the stage description lists them, `#[serde(rename_all = "snake_case")]`
and `#[serde(tag = "kind", rename_all = "snake_case")]` on `Block`, `Citation`,
`ImplementsView`. String enums (`SummaryView.state`, `OutOfDate.reason`, `EmptyState.kind`) are
Rust enums serialised snake_case. Derive `Serialize, Deserialize, Debug, Clone, PartialEq`. Every path field is
repo-relative (`doc/...`); `ConceptPage.file_hash: Option<String>` is the summary file's
`normalized_hash`. Fixtures use repo-relative paths throughout.

## Fixtures and parity tests

Write both fixture JSON files by hand, oxfmt-clean (`cd ../web && node_modules/.bin/oxfmt` on
them): an overview with three key chips (one out of date, reason `source_changed`), two
paragraphs, a sentence citing two summary sentences of two documents (two marks), a sentence
citing the vision directly (one mark, colour 0), a sentence with no citations; a summary page for
`section_01` with three sentences citing two anchors and `implemented_by` one plan.
`wire/tests_concept.rs` follows plan 2's `wire/tests.rs` pattern (`<name>_fixture_matches_serde_output`,
`<name>_fixture_deserializes`). The enum-value check against the TypeScript schema
(`assert_all_in_schema`) is not written here, because `schema-concept.ts` does not exist until
concept-web; instead add a third fixture-free test that serialises every variant of every enum
in `wire/concept.rs` and asserts the snake_case strings, and concept-web's schema test parses
those same strings.

## key.rs

`pub(crate) fn key_chips(project: &Project, eval: &Evaluation, scope: Scope, ctx:
&PageContext) -> Vec<KeyChip>`: one chip per id in `store::scope(project).product` (or
`.engineering`), in that order; colour `(index % 8 + 1) as u8`; `label` the section number
(`01`) or plan slug; `doc` the source document (title from `ProjectDoc.title`, page key
`product/section_01` or `engineering/<slug>`); `out_of_date` from `state::out_of_date_reason`;
`no_plan` for a product chip whose number has no entry in `project.implemented_by`, set only when
the engineering scope is non-empty. `pub(crate) fn colour_of(chips: &[KeyChip], doc_path: &str)
-> Option<u8>`.

## links.rs

`pub(crate) fn implements_view(project: &Project, slug: &str) -> ImplementsView` from
`project.implements[slug]`: `Implements::Unmapped` gives `NotStated`; `NoneDeclared` gives
`Sections` with no links; `Sections(n)` one `SectionLink` per number (`doc` `None` when no product
section `section_<nn>` exists, `name` the section's title, or `section_<nn>` as written when
broken) plus one `SectionLink { number: None, name: <token>, doc: None }` per `bad_tokens` entry.
`pub(crate) fn implemented_by(project: &Project, number: u32) -> Vec<PlanLink>` from
`project.implemented_by`, with each plan's title and state, in slug order.

## state.rs

`pub fn out_of_date_reason(id: &str, state: &SummaryState, ctx: &PageContext) -> Option<OutOfDate>`
(`id` is the summary id; plan 10 uses it with a field it adds to `PageContext`):
`Current` gives `None`; `Stale` or `Incomplete` gives `source_changed`; `Rebuilding` gives
`rebuilding`; `Missing` gives `None` (the page shows the empty or building state); with
`ctx.read_only` `Some(reason)` every non-`None` result becomes `cannot_rebuild` with
`detail: Some(reason)`. Plan 10 extends this function with `awaiting_approval` and `failed`; keep
it one `match`.

Unit tests in each file's `#[cfg(test)]` module. Proof: `cargo test --lib plan_server::concept::`
and `cargo test --lib plan_server::wire::`.

## Required test cases (coverage audit; each a named test)

- `state.rs`: "current gives none", "stale and incomplete give source_changed", "rebuilding gives
  rebuilding", "missing gives none", "read-only turns every reason into cannot_rebuild".
- `key.rs`: "no_plan only with an engineering scope", "chip carries colour, label and title".
- `links.rs`: "unmapped is not stated", "none declared is an empty list", "bad token kept as
  written", "implemented_by in slug order with states".
