# W4: the contract writer's signal and docs

Tier: sonnet (loom-software-engineer). Wave 1. Read `common.md` first. Stage 1 already edited `orchestrator/signals/contract.rs` (the `## Scaffold` section and rules 1 and 7): locate `generate_contract_signal` and `append_rules` with `rg -n` and keep that wording.

## Files you own

`src/orchestrator/signals/contract.rs`, `src/orchestrator/signals/contract_tests.rs`, `../skills/loom-plan-writer/references/v2-contracts.md`, `../README.md`.

## Steps

1. **Feedback in the signal.** In `generate_contract_signal`, read `crate::orchestrator::adjudication::feedback::read_feedback(work_dir, &stage.id)` with NO gate and, when a notice is present, append `## Adjudicator Feedback (from the refused freeze)` with the notice, placed before `## Your Job`. Do not copy the stage signal's `has_disputes` gate (`signals/generate.rs:344-354`): it sums only the old tallies, and `freeze_refusal_disputes` does not exist until W2's wave, so a copied gate would never show the notice. `has_disputes` stays as it is, without the new counter, so the contract-phase notice never leaks into the implementer's signal. `feedback.md` is cleared only by the adjudication apply (`adjudication/apply.rs:214`); nothing in the signal clears it.
2. **Rules.** Rewrite `append_rules` items 7-8 and add two items, keeping the scaffold wording stage 1 added:
   - "Finish with `loom stage contracts freeze <id>`, then stop. If it refuses, fix what it names and run it again. If you believe the stage's contract setup (its contracts or harness) or a format criterion makes the freeze impossible, record why with `loom memory note \"freeze-dispute: <evidence>\"`, run the freeze anyway, and stop: every refused freeze goes to an adjudicator, who reads your note and decides."
   - "If the Adjudicator Feedback section asks questions, answer each with `loom memory note \"freeze-dispute: answer <n>: <evidence>\"`, then run the freeze and stop; that is your follow-up. Do not commit."
   - "You cannot change the plan, and neither can the operator's answer to a question: do not ask the operator to amend contracts, harness or criteria, or to run commands for you. A refused freeze reaches the adjudicator on its own. Ask the operator only what only a person can decide."
   - "If the work needs something only a person can provide (a credential, a host install), run `loom stage block <id> \"<what and why>\"`."
   `AskUserQuestion` stays available: a question the writer does ask genuinely needs a person, and the existing `ask-user-pre.sh` behaviour is unchanged. Never tell the writer to run a dispute command: it has none.
3. **Docs.** `v2-contracts.md` Section 3 sequence step 2 (about L73): a refusal no longer parks the stage `WaitingForInput`; loom files it as a `freeze-refusal` dispute (at once for a refusal the daemon made, after five quiet minutes with no running subagent for one the writer's CLI recorded) and an adjudicator decides (reject sends a fresh writer with instructions; accept amends contracts, harness or replaces a format criterion with a cited reason; needs-more-evidence is answered through `loom memory note` and a re-freeze; the fourth refusal goes to human review). Section 5 "Disputes" bullet (about L129): add `freeze-refusal` (filed by loom, never by an agent). `../README.md`: the refused-freeze text (about L345 "or when a v2 contract freeze is refused" and about L854) and the dispute-kinds text (about L922, "four more dispute kinds", becomes five), plus `harness` in the `loom stage amend` field list (about L924). Locate lines with `rg -n` first.

## Tests

`contract_tests.rs`: the signal carries the feedback section when a notice exists (with a default tally, no dispute counted) and not otherwise; the rules name freeze-and-stop, the memory-note route for a setup dispute and for evidence questions, and no dispute command.

Your one check, at the end: `cargo test --lib orchestrator::signals::contract::`. Do not run clippy or `--all-targets`. If the build fails in a file you do not own, report it and stop.
