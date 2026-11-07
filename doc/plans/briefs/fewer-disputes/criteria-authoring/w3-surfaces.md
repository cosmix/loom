# criteria-authoring W3: what the writer, the implementer and the adjudicator read

Tier: sonnet. Wave 2, with W1a, W1b and W2. Read `common.md` first. Frozen contracts:
`tests/writer_choices_contracts.rs` (never edit it).

## Why

Once the contract writer names the test, the plan's `ContractSpec.test` is a hint, and the facts live in two
places: the daemon's saved choices before the freeze and the freeze record after it. Every surface that prints a
contract or builds its run command reads them through W0/W1b's helpers, never the bare plan spec.

## Files you own

`src/orchestrator/signals/contract.rs`, `src/orchestrator/signals/contract/rules.rs` (new),
`src/orchestrator/signals/contract/choices_tests.rs` (new), `src/orchestrator/signals/v2_section.rs`,
`src/orchestrator/signals/v2_section_effective_tests.rs` (new),
`src/orchestrator/signals/format/sections/goal_backward_section.rs`,
`src/orchestrator/adjudication/apply_contract.rs`, `src/orchestrator/adjudication/apply_contract_choice_tests.rs` (new),
`src/orchestrator/adjudication/apply_freeze_refusal.rs`, `src/orchestrator/adjudication/prompt/contract.rs`,
`src/orchestrator/adjudication/prompt/contract_choice_tests.rs` (new),
`src/orchestrator/adjudication/prompt/freeze_refusal_evidence.rs`,
`src/orchestrator/adjudication/verdict_freeze_refusal.rs`, `src/commands/stage/contracts/show.rs`,
`src/orchestrator/signals/contract_tests.rs` (W0b's former row: you wrap its `test` values in `Some(..)` and update the
two assertions below that pin the text you rewrite).

## Edits

1. Writer signal (`signals/contract.rs`, 388 lines today: move `append_rules` and its text into
   `contract/rules.rs`, declared `mod rules;` inside `contract.rs`, so neither file passes 400).
   - Build `contracts` from `choices::lenient_contracts(work_dir, stage)` (the plan's specs overlaid with the
     choices saved by earlier freezes of this stage). A spec still without a test: the Test cell reads
     `(you choose)`; its run line reads `` `<id>`: name its test with `--test <id>=<identifier>`; loom builds
     the command from it ``; no `resolve_run` call for it.
   - After the table add a short "What you choose" paragraph: the plan fixes `runner` for <ids> and `cwd` for
     <ids> (list them, or `none`); for every other contract you may pass `--runner <id>=<adapter>` (a known
     adapter) and `--cwd <id>=<dir>` (an ancestor of the file, inside the working directory, holding the runner's
     root marker as committed at the stage base); the daemon refuses a directory whose marker you added yourself.
   - Rules: step 6 also says the freeze runs the stage's lint and search checks over what you wrote (run them
     first). Step 7's command becomes `loom stage contracts freeze {stage_id} --test <id>=<identifier> ...` with
     one `--test` per contract that shows `(you choose)` or whose hint you replace, plus any `--runner` or
     `--cwd`; a later freeze keeps the flags it does not repeat. The freeze now waits for the host run as `loom
     check` does: step 7 says to run the same command again while `Host job:` reads `requested` or `pending`;
     `passed` means loom froze the contracts and ends the session (then stop); `failed` names what to fix, and
     when it names a file you wrote (a formatter, lint or search check) you fix that file and run the freeze
     again, with the same flags; the wait ends with a line saying whether the refusal was returned to you
     (`n of 3`) or filed; use this wording: a static refusal is returned to you; it is filed as a
     `freeze-refusal` dispute on the third static refusal in the session, or when you stay quiet 5 minutes after
     a refusal, and any other refusal is filed at once. Replace "then stop" and "If it refuses, fix what it names and run it again" accordingly and keep
     the `freeze-dispute` note sentences. Keep every other rule's wording.
2. Implementer signal (`signals/v2_section.rs`): `append_frozen_contracts` loads the record
   (`load_freeze(..)?`, the `is_some` check already reads it) and renders each contract through
   `store::effective_contract(&record, contract)`; the Test cell is the effective test.
3. Stage signal (`goal_backward_section.rs`): when `stage.consumer_deferred` is `Some`, add a subsection
   `### Consumer deferred` after the reachable one: `This stage declares that no consumer of its new units
   exists in this plan: <reason>. It waives only the leaf wiring rule; reachable checks, the unwired-file
   detection and the definition-site rule still apply.` The reason goes through `strip_control`.
4. `contracts show` (`commands/stage/contracts/show.rs`): each line gains `test=<test>` (`unrecorded` when the
   entry has none).
5. Adjudication, post-freeze (`prompt/contract.rs`): `contract_run` and `spec_block` use
   `store::effective_contract(record, contract)` when a record exists, else the lenient overlay
   (`choices::lenient_contracts` equivalent for one spec: find the stage and work directory on
   `KindPromptInput`); print `test` and add a `writer's test` line when the effective test differs from the
   plan's hint.
6. Adjudication, re-freeze (`apply_contract.rs`): `refrozen_contracts` builds `RefrozenContract { .., test }`.
   Rule: a contract the verdict's `contracts` patch touched (replace or insert) takes its amended spec's `test`
   when that is `Some`, else the record's; every other contract keeps the record's test
   (`store::effective_contract(record, contract).test`). Find how the function learns which contracts the
   verdict amended (the patches are in the dispute's verdict) and pass that set in.
7. Adjudication, pre-freeze (`apply_freeze_refusal.rs`): after an accepted verdict applies `contracts` patches,
   drop the saved choice of every contract id a patch touched (`load_choices`, remove the ids, `save_choices`),
   so the amended spec governs until the next writer states its flags again.
8. Evidence and verdict checks: `prompt/freeze_refusal_evidence.rs` `plan_section` lists contracts from the lenient
   overlay (test `(none yet)` when absent) and its criteria filter and heading use `is_pure_check` (`Format and
   lint criteria`); `verdict_freeze_refusal.rs` `replace_format_check` uses `is_pure_check` for the current and the
   replacement criterion and its messages say `pure check`. Keep each function's line count (the second file is at
   396 lines).

## Traps

- `resolve_run` now fails for a spec without a test; every caller you touch handles that as text, not a panic.
- The writer's signal is generated at session spawn, before the writer chooses anything, but a replacement writer
  after a refusal sees the saved choices: that is why the lenient overlay is read there.
- Pinned text: `signals/contract_tests.rs` (yours) and `signals/tests_doctrine*.rs` pin phrases of the writer
  signal. The rule-7 rewrite changes exactly two assertions in `contract_tests.rs` (lines 126-127 today):
  `7. Finish with `loom stage contracts freeze s1`` becomes the new step-7 opening (the command now continues
  with ` --test <id>=<identifier> ...` inside the backticks) and `then stop. If it refuses, fix what it names`
  becomes a phrase of the new step 7 (for example `fix what it names and run it again`). Change those two
  lines only, keep every other pinned phrase verbatim, and list both in your report (the orchestrator files them
  in the `dispute-integrity`). Do not edit `tests_doctrine*.rs`: report any pin there that must change, with the
  exact old and new text.
- Reasons, tests and choice values are plan or agent text: render them through `table_cell` / `single_line`
  helpers the neighbouring code uses.

## Tests (new files, each wired with `#[cfg(test)] #[path = ".."] mod` from its parent)

- `contract/choices_tests.rs`: a spec with a hint shows the hint; a spec without one shows `(you choose)`; a saved
  choice shows as the test; the "What you choose" paragraph lists planner-fixed runner and cwd; rule 7 names the
  flags.
- `v2_section_effective_tests.rs`: the implementer table shows the record's test over the plan hint and fills a
  missing one.
- `goal_backward_section.rs` inline: the consumer-deferred subsection appears with its reason, and not when `None`.
- `apply_contract_choice_tests.rs`: a re-freeze keeps the record's test for an untouched contract, takes the amended
  spec's test for the touched one, and keeps the record's when the amendment has no test.
- `prompt/contract_choice_tests.rs`: the spec block prints the effective test and the writer's test line.
- `show.rs` inline: `test=` appears.

## Done

Your one check once the crate compiles: `cargo test --lib orchestrator::signals::`.
