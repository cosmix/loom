# gate-noise-and-review W4: the Performance dimension, signal wording and doctrine

Tier: sonnet (`loom-software-engineer`). Wave 2. Read
`doc/plans/briefs/fewer-disputes/gate-noise-and-review/common.md` first. W1 changes the detector
and W2/W3 add the hook; you write the text that describes them, so write from the design in
common.md and the decided phrases below, not from code you have not seen.

## Files owned

`src/orchestrator/signals/cache/blocks.rs`, `src/orchestrator/signals/tests_doctrine_prefixes.rs`,
`src/orchestrator/signals/v2_section_review.rs`, `src/orchestrator/signals/v2_section_tests.rs`,
`src/orchestrator/signals/tests_doctrine_v2.rs`, `src/cli/types_stage_review.rs` (one doc comment),
`../agents/loom-code-reviewer.md`, `../skills/loom-plan-writer/references/v2-contracts.md`,
`../skills/loom-plan-writer/references/bookend-stages.md`, `../skills/loom-orchestration/SKILL.md`
(the "Test integrity" paragraph only), `../README.md` (the test-integrity sentence only).
Never touch `REVIEW_ORDER` (`v2_section_review.rs`), `STAGE_EXIT_RULES` (`helpers.rs`) or their
copies: `tests_doctrine_v2.rs` pins BLOCK-E and BLOCK-F byte for byte across surfaces, and the
stage `verdict-completion` rewrites BLOCK-F after this stage. Do not edit `skills/loom-plan-writer/
SKILL.md` or Sections 1-3 of `v2-contracts.md` (`criteria-authoring` owned them).

## Steps

1. `cache/blocks.rs::append_review_dimension_details`: the doc comment says "four"; add a fifth
   item after **Functional** and change the doc to "five". Decided text:
   `5. **Performance** — hot paths, unbounded loops or reads, N+1 queries, repeated work per request, synchronous I/O on a latency path, unbounded memory or lock hold time; reason from the plan's stated load or deployment evidence and name a measurement or a bounded-cost argument. Your reviewer brief lists the plan-wide change: review it against the plan's stage briefs, not against your own expectation.`
   The IV prefix has no size ceiling test (`tests_size.rs` bounds `generate_stable_prefix`, the
   standard one); run `cargo test --lib orchestrator::signals::` to confirm.
2. `tests_doctrine_prefixes.rs::integration_verify_prefix_retains_required_review_and_doctrine_guards`:
   add `"**Performance**"` to the first needle list and a second assertion that the prefix holds
   `"review it against the plan's stage briefs"`.
3. `v2_section_review.rs::append_review_gate`: add one bullet to the gate list (before
   `append_dispute_commands`), decided text:
   `- The spawn guard appends a `loom-reviewer-brief` section to every reviewer brief: this stage's current test-integrity events with their lost assertion statements and, in integration-verify, the files the plan changed, the plan's briefs directory and every `consumer_deferred`. Do not paste them. A reviewer reports a finding for each lost guard that no equal or stronger check replaces; fix it, or dispute the event, before the final round.`
   `v2_section_tests.rs`: extend `review_gate_block_carries_block_e_verbatim` (or add a sibling
   test) so both Standard and IntegrationVerify signals contain `loom-reviewer-brief` and the
   KnowledgeDistill signal does not.
4. `tests_doctrine_v2.rs`: add `v2_contracts_reference_describes_the_statement_detector`
   (add `const V2_CONTRACTS: &str = include_str!("../../../../skills/loom-plan-writer/references/v2-contracts.md");`
   and the same for `bookend-stages.md` beside the existing constants; the file's `section(markdown,
   heading)` helper returns a `## <heading>` body): the section `5. What completion enforces in v2`
   of `v2-contracts.md` contains `assertion statement`, `loosens`, `loom hook reviewer-brief`; and
   `bookend_stages_lists_the_signals_review_dimensions`: `bookend-stages.md` contains `performance`
   and `build/test/sandbox` and no longer contains `test coverage`.
5. `../agents/loom-code-reviewer.md`: add a section `## Loom briefs` before `## Constraints`,
   at most 8 lines: a brief may end with a `loom-reviewer-brief` section computed by loom; for each
   lost assertion statement report a finding unless a check that is equal or stronger replaces it,
   and name that check with `file:line`; in integration-verify judge the diff against the plan's
   briefs it lists, treat a `consumer_deferred` line as a declared gap, and review performance.
   Keep the phrases `acceptance criterion` and `intent` and `acceptance criterion in your brief`
   (pinned by `tests_doctrine_gate.rs`).
6. `v2-contracts.md` Section 5 (`## 5. What completion enforces in v2`): replace the "Test-integrity
   events" bullet with the decided content below, and add the reviewer-brief bullet after it.
   Section 4 (`ratchet_files`, the sentence "Any change to a listed file ... tightening included,
   raises `TI-ratchet-<path>`"): replace with "A change that loosens a listed file raises
   `TI-ratchet-<path>` (Section 5); a change that only tightens it, such as a lower value or a
   removed entry, raises nothing, so a stage that shrinks a function lowers its ledger row without a
   dispute." (This one sentence is the only Section 4 edit; the plan assigns it here because the
   rule changed with the detector.) Also change the table row "Test integrity ... an event is not
   accepted, or is worse than accepted" only if it says tests or lines; otherwise leave it.
   Decided Section 5 content (write it as prose in the file's voice, one bullet with sub-bullets):
   - `TI-decl-<lang>`, `TI-assert-<lang>`: unchanged counts.
   - `TI-edit-<path>`: a test file that existed at base lost or changed an assertion STATEMENT. A
     statement starts at a match of the language's assertion pattern and ends at the first `;` or
     line break at bracket depth zero outside string literals; comments, whitespace and trailing
     commas outside strings do not count. A reflow, a rewrap beside a real edit and a statement
     moved within its file raise nothing; a changed or deleted statement does, a changed
     continuation line of a multi-line assert included.
   - Moved tests: a lost statement cancels against an added one in another changed or new test file
     of the same language when both sit in a function the source-graph extractor finds in both files
     under the same name with an identical normalized body. A language without a grammar (Kotlin,
     Scala, Swift, Elixir), a parse error or an oversized file gets no cross-file cancellation; a
     move that also edits the body raises on the source file. An accepted event stays pinned to the
     file's sha256 and to every file that cancelled one of its statements.
   - `TI-ratchet-<path>`: a file parses only when EVERY non-comment, non-blank line ends in an
     unsigned integer; the rest of the line is the key and the integer the value. A new key or a
     higher value loosens and raises; a removed key or a lower value tightens and raises nothing. Values are read as ceilings: write a floor (a
     coverage minimum) so its last token is not a bare integer (`80%`), and every change to it
     raises. A file with even one non-comment line whose last token is not an unsigned integer (a
     knowledge check baseline, `coverage 80%`) is unparsable, and any change to it raises.
   Reviewer-brief bullet: "**Reviewers see the events.** The spawn guard appends the output of
   `loom hook reviewer-brief` to every `loom-code-reviewer` spawn of a stage session, under a 5 s
   bound and a nonce envelope, and fails open: the stage's current integrity events with their lost
   statements and each event's accepted state, and for integration-verify also the files the plan
   changed, the plan's briefs directory and every stage's `consumer_deferred`. The reviewer reports
   a finding for each lost guard that no equal or stronger check replaces, so fix the guard or
   dispute the event before the final review round." Keep the existing paragraph for
   `loom stage review integrity`.
7. `bookend-stages.md`, integration-verify "Code review" bullet: replace "(security via ...;
   architecture; test coverage)" with the five dimensions the signal injects as Review Dimension
   Details (security via the loom-security-audit skill, architecture, build/test/sandbox,
   functional, performance), keep "fix all findings with an engineer agent", and replace the
   parenthetical about the six mini dimensions with: "The six mini adversarial dimensions and the
   five parallel dimensions are already injected at the signal layer, and the spawn guard appends
   the stage's integrity events (and, here, the plan-wide change and briefs) to each reviewer brief;
   do not restate them. To require specific dimensions use plan-level `code_review` config, not
   prose." Add to the `version: 2` list: "**Reviewer brief** — integration-verify reviewers receive
   the files the plan changed, the plan's briefs directory and every `consumer_deferred`; they judge
   the change against the briefs."
8. `../skills/loom-orchestration/SKILL.md` "Test integrity" paragraph: say assertion statements,
   "a reflow, a move or a lowered ledger entry does not count", and add one sentence: "Your
   reviewers get these events in their brief; fix what they report before the final round."
9. `../README.md` (line starting "In a version 2 plan, `loom stage complete` also runs"): replace
   "removed or changed assertion lines, or a changed `ratchet_files` entry" with "removed or changed
   assertion statements, or a loosened `ratchet_files` entry".
10. `src/cli/types_stage_review.rs`: the `Integrity` doc comment says "lost assertion lines,
    changed ratchet files": make it "lost assertion statements, loosened ratchet files".
11. Run once: `cargo test --lib orchestrator::signals::` (it covers the agent and skill pins in
    `tests_doctrine_gate.rs`; `token_optimization_contracts` runs in the orchestrator's gate).

## Traps

- Every fenced block in markdown names a language (`text`, `yaml`, `rust`).
- No LLM-isms in doctrine: no "not X but Y", no "robust", "crucial", "leverage", "seamless".
- `tests_doctrine_gate.rs` and `tests/token_optimization_contracts.rs` pin phrases and the
  frontmatter of the reviewer agent and the skills; keep the frontmatter and the pinned phrases.
- INTEGRITY: no existing assertion should need to change. The new needles are additions.
