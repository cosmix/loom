# R2 — every overview expansion shows more than the sentence (issue 5)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first, D3 in particular.

## Why

Level 0 is an overview sentence. Opening it shows its cited member sentences (level 1, `web/src/plan/concept/run.tsx` `InlineRun`). The stored overviews at the time of the review, measured with a script over `doc/loom/summaries/overview/**`:

| Overview | Sentences citing members | Citing exactly one member sentence |
| --- | --- | --- |
| `feat-planner/product` | 25 | 7 |
| `feat-planner/engineering` | 27 | 12 |
| `optim-token/engineering` | 6 | 0 |

A sentence citing one member sentence opens to one sentence, which is often a paraphrase of it. That is the operator's "nearly the same text, and the second click shows the longer text". The existing check (`seal/restate.rs`, Dice ≥ 0.75, `RESTATE_THRESHOLD`) catches only close copies.

## Change

- `loom/src/planning/summary/seal/links.rs`: in the sentence loop, after a sentence's links are checked, count its `LinkTarget::Sentence` links to members.
  - Exactly one is a problem, unless the cited member's current summary holds exactly one sentence (`Rules.evaluation.summaries[member].sentences`).
  - The message is `sentence <id> cites one member sentence; an overview sentence draws on at least two`.
  - Vision source citations are not member sentences and are unaffected.
  - Keep the restatement check as it is.
- `plan-plugin/agents/overview.md`: state the rule, with the words "at least two": "An overview sentence that cites member sentences cites at least two of them, and says what they add up to." R1 adds the same line to `brief.rs` `write_rules` (R1 owns `brief.rs`); do not edit it.
- `plan-plugin/agents/summary.md`:
  - Replace "Use at most 15 sentences" with the per-kind cap: a section or plan summary at most 15 sentences; a vision summary (R1's id `<f>/product/vision`) at most 8 sentences in at most 3 paragraphs, distilling the vision so a reader grasps it in under a minute. The paragraph limit is a prompt rule only; no parser checks it.
  - Add for every document summary: "A sentence compresses its cited section; never copy a sentence of the source."
- `loom/src/planning/guidance/text/summary_codex.md` (the codex lane's summary prompt): its id list gains `<feature>/product/vision`, and its "at most 15 sentences" line gains the vision's 8-sentence, 3-paragraph cap, worded as in `summary.md`.

## Tests

- The stage contract `overview-cites-two-member-sentences` (frozen) drives the check.
- Add the passing case `an_overview_sentence_citing_two_member_sentences_seals` (a plan `after_stage` greps the name).
- Add the exemption case `a_sentence_citing_the_only_sentence_of_a_member_seals`: a sentence citing the one sentence of a one-sentence member seals (also grepped).
- `loom/tests/planning_summary_contracts.rs` and `summary_engine_contracts.rs` run in the main agent's stage gate. Their fixture overviews (`tests/planning_summary_support/fixtures.rs` `overview_of`, `product_overview`) cite one-sentence members, so the exemption should keep them green. If you see a frozen contract go red, stop and report the test and message to the main agent. Do not edit it.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib planning::summary::seal::` (one command).
