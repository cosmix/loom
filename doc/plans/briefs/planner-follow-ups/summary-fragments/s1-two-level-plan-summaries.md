# S1: two-level plan summaries in the store, the seal and the concept page (R8)

Tier: opus (`loom-senior-software-engineer`). Read `common.md`, then `doc/loom/knowledge/architecture/summary-store.md` sections "File Format", "States" and "Seal" (`rg -n '^## ' <file>` lists them). This brief settles every design decision; do not reopen them.

Line numbers below were read before follow-ups-core merged; it edits `store.rs` (the metaplan id collapses to `<f>/metaplan/index`). Anchor every edit by symbol.

## The design (settled)

1. **One file, two levels.** A plan summary (`plans/<slug>`) is one file. Its body is the **opening level** (the `#` title, optional `##` headings, then paragraphs whose sentences cite only sentences of this same summary, written `[d](#s-xxxxxxxx)`) followed by the **detail level** (paragraphs whose sentences cite only the plan's heading sections, as a plan summary does today). No new summary id, no new front-matter key, no new `SummaryKind`. `cites` records only source links, as today.
2. **Self-links already parse.** `format::classify_href` (`format.rs` ~135) resolves `#s-xxxxxxxx` through `store::resolve_link` (empty path: the summary's own path) to `LinkTarget::Sentence { summary: <own id>, id }`. Pin this with a format test; do not change the parser.
3. **The seal enforces the two levels for plan ids only.** Parsing and evaluation accept any document summary, so a stored one-level plan summary still loads and its page shows one level until the operator rebuilds it after the run. This is the ordinary document path, not a compatibility branch: write no migration and no special case for old files.
4. **Agents learn the detail ids from the seal.** Sentence ids are hashes the agent cannot compute. Every seal rejection about the levels lists the draft's detail sentences as `s-xxxxxxxx "<first six words>"`. The prompts tell the agent: write the detail level, seal, read the ids from the refusal, write the opening level above it, seal again. No new CLI command.
5. **Overviews cite only sentences that cite no sentence** (for a plan: its detail sentences). The overview drill stays overview sentence, then member sentences, then passages; the web client has no third nested level.
6. **Limits.** `MAX_OPENING_SENTENCES = 8` (opening level), the detail level keeps `MAX_DOCUMENT_SENTENCES = 15`, and `sentence_limit` returns `MAX_PLAN_SENTENCES = MAX_OPENING_SENTENCES + MAX_DOCUMENT_SENTENCES` (23) for a plan id so the parser does not refuse a full two-level draft.
7. **The concept page carries both levels.** `ConceptPage.blocks` holds the opening level; a new `ConceptPage.detail: Vec<Sentence>` holds the detail sentences, which the web client looks up when an opening sentence opens. Opening sentences take the overview rendering path (`Citation::Sentence` citations, `rests_on` = the cited detail sentences' anchors, the plan's own mark).

## Build

### Store and format

- `loom/src/planning/summary/store.rs`: add `pub fn is_plan(id: &str) -> bool` (`id.starts_with("plans/")`) beside `is_metaplan`, with a doc comment saying it assumes a validated id.
- `loom/src/planning/summary/format.rs`: the two constants above, `sentence_limit` branch `else if store::is_plan(id) { MAX_PLAN_SENTENCES }`, and `mod levels; pub use levels::{cites_own, split_levels, Levels};`.
- New `loom/src/planning/summary/format/levels.rs` (pure, no I/O):
  - `pub fn cites_own(summary_id: &str, sentence: &Sentence) -> bool`: any link resolves to `LinkTarget::Sentence { summary == summary_id }`.
  - `pub struct Levels<'a> { pub shown: Vec<&'a Block>, pub detail: Vec<&'a Sentence> }` and `pub fn split_levels<'a>(summary_id: &str, blocks: &'a [Block]) -> Levels<'a>`: when no sentence `cites_own`, `shown` is every block and `detail` is empty; otherwise `shown` is every heading plus every paragraph holding a sentence that `cites_own`, in order, and `detail` is the sentences of every other paragraph, in order. Total over any input (a hand edit cannot make it panic).
  - Tests in a new `format/levels_tests.rs` (`#[cfg(test)] mod levels_tests;` in `format.rs`): self-link classification, one-level split, two-level split.
- `format/tests.rs` ~259: the `sentence_limit` table row `("plans/p", MAX_DOCUMENT_SENTENCES)` becomes `MAX_PLAN_SENTENCES`.

### Seal

- New `loom/src/planning/summary/seal/levels.rs`: `pub(super) fn check(id: &str, body: &SummaryBody) -> Vec<FormatError>`, empty unless `store::is_plan(id)`. Opening sentence: every link `cites_own`. Detail sentence: no link `cites_own`. Rules, one `FormatError` per problem with the draft line (1-based, `sentence.line + 1`; line 1 for whole-draft problems). Use these messages; `<detail list>` is `detail sentences: s-aaaaaaaa "First six words of it", s-bbbbbbbb "..."` (`terminal_safe`-clean, text from the draft):
  1. No opening sentence (line 1): `a plan summary opens with sentences that cite its detail sentences as [d](#s-…); <detail list>`.
  2. A sentence with both a self-link and a source link: `sentence <id> cites both this summary and the plan; an opening sentence cites only detail sentences`.
  3. A paragraph mixing opening and detail sentences, an opening paragraph after the first detail paragraph, or a heading after the first detail paragraph: `the opening level comes first: headings and opening paragraphs end where the detail level begins`.
  4. A self-link whose id is not a detail sentence of this draft (unknown, an opening sentence, itself): `sentence <id> cites <target id>, which is not a detail sentence of this summary; <detail list>`.
  5. An opening sentence whose self-links, in order, are not a run of whole detail paragraphs each in paragraph order: `sentence <id> opens part of a detail paragraph; cite every sentence of each paragraph it opens, in order`.
  6. A detail sentence no opening sentence cites: `detail sentence <id> is opened by no opening sentence`.
  7. `more than 8 opening sentences` / `more than 15 detail sentences` (from the constants).
  8. An opening sentence that restates a detail sentence it cites (`restate::restates`, `seal/restate.rs`): `sentence <id> restates detail sentence <did>; say what the paragraph adds up to`.
- `seal.rs` `build_text` (~183): after `links::check`, `errors.extend(levels::check(id, body))`, then sort by line, so all problems report together. Add `mod levels;` (keep `seal.rs` under 400 lines).
- `seal/links.rs`:
  - `sentence_problem` (~140): for a non-overview, a link to `summary == rules.id` when `store::is_plan(rules.id)` returns `None` (levels own those rules; it inserts no member); every other non-overview sentence link keeps "a document summary cites only its source document".
  - For an overview: a cited member sentence whose `SentenceEval.citations` hold any `LinkTarget::Sentence` is refused: `sentence <id> of <summary> opens to detail sentences; cite those detail sentences instead`.
  - `single_member_problem` (~178): the exemption counts the member's sentences whose citations hold no `LinkTarget::Sentence` (citable sentences), not all sentences. The frozen `seal/two_member_contract_tests.rs` must stay green unchanged.
- Tests: a new `seal/levels_tests.rs` (`seal/tests.rs` is at 377 lines): one per rule above, plus an overview citing a plan's opening sentence (refused) and citing its single detail sentence (accepted by the exemption). Update `seal/tests.rs` ~130 (an archived plan sealed one-level) to a two-level draft.

### Evaluation

A self-link must hold, or every two-level plan summary reads stale and the scheduler rebuilds it forever.

- `evaluate/cite.rs` `Context`: add the summary id and `own: BTreeSet<String>`, the ids of this summary's current sentences that cite no own sentence. `holds` gains the arm `(LinkTarget::Sentence { summary, id }, Kind::Document { .. }) if summary == self.id => self.own.contains(id)`.
- `evaluate.rs` `evaluate_one` (~200): first evaluate the sentences that do not `cites_own`, collect the current ones into `own`, then evaluate every sentence in file order with that set. A cycle among opening sentences (a hand edit) reads stale and never recurses.
- Tests in a new `evaluate/levels_tests.rs` (`evaluate/tests.rs` is at 358 lines): a two-level file is current; editing the span one detail sentence cites stales that sentence and every opening sentence citing it, and nothing else.
- `engine.rs` `with_source` (~157) rewrites only `LinkTarget::Source` links, so self-links survive a plan rename. Add one engine lib test proving a renamed two-level plan summary stays current with its `#s-` links unchanged.

### Brief and prompts

- `brief.rs` `write_rules` (~199): for `store::is_plan(id)` add, after the length line:
  - `- A plan summary has two levels. First the opening level: about one sentence per topic, each citing every sentence of the detail paragraph(s) it opens, in order, as [d](#s-xxxxxxxx). Then the detail level: short paragraphs whose sentences cite the plan's heading sections. No heading after the opening level.`
  - `- At most 8 opening and 15 detail sentences.`
  - `- Write the detail level first and seal: the refusal lists each detail sentence's id. Then add the opening level above it and seal again.`
  - Test in `brief/tests.rs` for a `plans/` id; the section and vision assertions stay as they are.
- `plan-plugin/agents/summary.md` and `loom/src/planning/guidance/text/summary_codex.md`: replace "A section or plan summary takes at most 15 sentences" with a section-summary line and a plan-summary paragraph using the words **opening level** and **detail level**, the three brief rules above and the seal-twice workflow. Keep the frontmatter of `summary.md` byte for byte (`loom/tests/planning_guidance_contracts.rs` reads it).
- `plan-plugin/agents/overview.md` and the overview block of `summary_codex.md`: "Cite a plan summary's detail sentences, never its opening level."

### Concept page and wire

- `loom/src/plan_server/wire/concept.rs` `ConceptPage`: add after `blocks`

  `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub detail: Vec<Sentence>,` with a doc comment ("The detail level of a two-level summary, which its opening sentences cite; empty and absent from the JSON for every other page."). Skipping when empty keeps `loom/src/plan_server/wire/fixtures/concept-*.json` and the web copies unchanged.
- `loom/src/plan_server/concept/body.rs`:
  - `blocks` (~45) takes the summary `id` and returns `pub(crate) struct Body { pub blocks: Vec<Block>, pub detail: Vec<Sentence> }`. Call `split_levels(id, &file.blocks)` (import it by name, not as `format::split_levels`); render `shown` as today and each `detail` sentence through `sentence(&cx, file, one, None)`.
  - `sentence` (~93): use `overview_level` when `cx.level == Level::Overview` or any citation is a `Citation::Sentence`; else `document_level`. For a self-citation `citations()` keeps the citation (the plan's `doc_link` exists), `overview_level` pushes the cited detail sentence's source anchors and gives the plan's own mark.
- `loom/src/plan_server/concept/page.rs`: `blank` sets `detail: Vec::new()`; `overview_page` (~164) and `summary_body` (~258) take `Body` (`summary_body` passes `id`; `overview_page` passes its overview id) and set `page.detail`.
- Lib tests in a new `concept/levels_tests.rs` (`concept/tests.rs` is at 340 lines): a one-level plan summary page has an empty `detail` and its JSON has no `detail` key; a two-level page's opening sentence carries the detail anchors as `rests_on`.

### Product spec (`doc/product/feat-planner/section_04.md`)

In "Rules for the body" (~124-144) add a bullet starting **A plan summary has two levels.** stating the opening and detail levels, the whole-paragraph citation, that the seal's refusal lists the detail ids, and that overviews cite only detail sentences. Change the Length bullet to: a section summary at most 15 sentences, a plan summary at most 8 opening and 15 detail sentences. Fix the two-member bullet's exemption to "a single citable sentence". Leave section 05 alone (W2 owns it).

## Contracts (written before you start, by the contract session)

`loom/src/planning/summary/seal/levels_contract_tests.rs` (three tests) and `loom/src/plan_server/concept/levels_contract_tests.rs` (one). Make them pass. Do not edit them or their `mod` lines in `seal.rs` and `concept/mod.rs`.

## Planned edits of frozen tests (yours; report each by file and test)

The seal now refuses one-level plan drafts, so these frozen contract fixtures change. Change only what is listed.

| File | Test or item | Edit |
| --- | --- | --- |
| `loom/tests/planning_summary_support/fixtures.rs` | `DRAFT_X` | `"# X\n\nX is planned. [d](#s-a1edcae5)\n\nX does x. [§](../../../plans/demo/PLAN-x.md#plan-x)\n"` (`s-a1edcae5` is `sentence_id("X does x.")`) |
| `loom/tests/planning_summary_contracts.rs` | `plan_summary_cites_stage_anchor` | draft `"# X\n\nX opens with alpha. [d](#s-63eb2337)\n\nAlpha runs first. [§](../../../plans/demo/PLAN-x.md#stage:alpha)\n"`; `want` becomes `[("X opens with alpha.", "current"), ("Alpha runs first.", "current")]` |
| `loom/tests/planning_summary_contracts.rs` | `deleted_source_deletes_summary` | `y` becomes `"# Y\n\nY is planned. [d](#s-946cc169)\n\nY does y. [§](../../../plans/demo/PLAN-y.md#plan-y)\n"` |
| `loom/tests/plan_concept_contracts/support.rs` | `plan_draft` | an opening line `Plan {slug} is outlined. [d](#{sid})` (`sid(&plan_sentence(slug))`), a blank line, then the existing detail line; `plan_sentence` unchanged |

The ids are `s-` plus the first 8 hex digits of SHA-256 over the text (`printf '%s' 'X does x.' | sha256sum` gives `a1edcae5`). Recompute with `sentence_id` if you change a text.

## Files

`loom/src/planning/summary/**`, `loom/src/plan_server/concept/**`, `loom/src/plan_server/wire/concept.rs`, the three test files in the table, `plan-plugin/agents/summary.md`, `plan-plugin/agents/overview.md`, `loom/src/planning/guidance/text/summary_codex.md`, `doc/product/feat-planner/section_04.md`. Never edit web files or `loom/maintainability-baseline.txt`. New files stay under 400 lines and functions under 50.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::summary:: plan_server::concept:: plan_server::wire::`

## Report

Files changed; the test run's result; each frozen edit from the table; every other pinned test you changed and why; anything unsettled.
