# W4: the contract writer's signal and docs

Tier: sonnet (loom-software-engineer). Wave 1. Read `common.md` first.

## Files you own

`src/orchestrator/signals/contract.rs`, `src/orchestrator/signals/contract_tests.rs`, `../skills/loom-plan-writer/references/v2-contracts.md`, `../README.md`.

## Steps

1. **Feedback in the signal.** In `generate_contract_signal` (`contract.rs:66`), read `crate::orchestrator::adjudication::feedback::read_feedback(work_dir, &stage.id)` and, when present, append `## Adjudicator Feedback (from the refused freeze)` with the notice, placed before `## Your Job`. Mirror how the stage signal does it (`signals/generate.rs:316-352`, `append_stage_feedback` and `has_disputes`), including when the notice is cleared; read that code first.
2. **Rules.** Rewrite `append_rules` (L239-266) items 7-8 and add one item, keeping the scaffold wording stage 1 added:
   - "Finish with `loom stage contracts freeze <id>`, then stop. If it refuses, fix what it names and run it again. If you believe the stage's contract setup (its contracts or harness) or a format criterion makes the freeze impossible, record why with `loom memory note \"freeze-dispute: <evidence>\"`, run the freeze anyway, and stop: every refused freeze goes to an adjudicator, who reads your note and decides."
   - "You cannot change the plan, and neither can the operator's answer to a question: do not ask the operator to amend contracts, harness or criteria, or to run commands for you. A refused freeze reaches the adjudicator on its own. Ask the operator only what only a person can decide."
   - "If the work needs something only a person can provide (a credential, a host install), run `loom stage block <id> \"<what and why>\"`."
   `AskUserQuestion` stays available: a question the writer does ask genuinely needs a person, and the existing `ask-user-pre.sh` behaviour is unchanged.
3. **Docs.** `v2-contracts.md` Section 3 sequence step 2 (about L73): a refusal no longer parks the stage `WaitingForInput`; it becomes a `freeze-refusal` dispute an adjudicator decides (reject sends a fresh writer with instructions; accept amends contracts, harness or a format criterion with a justification; three refusals go to human review). Section 5 "Disputes" bullet: add `freeze-refusal` (filed by loom, never by a stage agent). `../README.md`: the refused-freeze text (about L854) and the dispute-kinds text (about L922) say the same. Locate lines with `rg -n` first.

## Tests

`contract_tests.rs`: the signal carries the feedback section when a notice exists and not otherwise; the rules name freeze-and-stop and the memory-note route for a setup dispute. Run `cargo test --lib orchestrator::signals::contract::` and clippy once each.
