# check-routes W4: judging and applying a dispute over any check list

Tier: opus. Wave 2, after W0. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `tests/check_routes_contracts.rs`.

## Why

A criterion dispute's verdict is parsed by `orchestrator/adjudication/verdict.rs`
(`classify_and_validate`, `validate_accept`, `admits`), normalised by `plan_patch.rs`, applied
by `apply_accept.rs` through `apply.rs::amend_plan`, and briefed by `prompt.rs::build`
(`criterion.rs`, `criterion_entry.rs`). Each knows four lists: acceptance, wiring, wiring tests
and the live-test-lane plan's `live` (a `Live` dispute amends `live` only, and its
`criterion_entry` briefing quotes the entry and its receipt outcome). The new lists and the
owner need the whole path; every `Live` arm stays as it is.

## Edits (you own these files)

1. `src/orchestrator/adjudication/verdict.rs`
   - `admits(disputed, field)`: a dispute over any list may amend that list only; an acceptance
     dispute may also amend `wiring` (unchanged). `admitted_fields` spells each.
   - The accept re-prompt text names the disputed list's own spelling.
2. `src/orchestrator/adjudication/plan_patch.rs`
   - `normalize` reads every field spelling, kebab and snake (`reachable`, `artifacts`,
     `dead-code-check`/`dead_code_check`, `regression-test`/`regression_test`,
     `after-stage`/`after_stage`); its error text lists them.
   - `build_amendment_requests(target_stage: &str, plan_patch, dispute_key: &str)`: the
     request's `stage_id` is `target_stage`; patch `k` carries `dispute_id =
     "<dispute_key>.<k>"`. The caller passes `dispute_key = "<dispute id>"` for an own-list
     dispute and `"<filer stage>/<dispute id>"` for an owner dispute (an owner's own dispute
     `n` keys `"n.k"`, so the two never collide; W2's `apply_amendment` matches exactly).
3. `src/orchestrator/adjudication/apply_accept.rs`
   - `apply_accept` takes the dispute's `owner: Option<&str>`; with an owner, every request
     amends the owner and the notice reads `Dispute <n> accepted: stage <owner> <field>
     <changes> ...`. `field_name` covers every `AmendmentField`.
   - The filer is re-queued (`notify_and_requeue`) exactly as today; the owner's record is
     changed only by `apply_amendment`.
4. `src/orchestrator/adjudication/apply_kinds.rs`: read the owner from the request
   (`dispute_kind`) and pass it to `apply_accept`.
5. `src/orchestrator/adjudication/apply.rs`: `disputed_entry` names every list ("reachable
   check", "artifact", "dead-code check", "regression test", "after-stage check"), and with an
   owner appends `of stage '<owner>'`.
6. `src/orchestrator/adjudication/verdict_freeze_refusal.rs` and `apply_freeze_refusal.rs`:
   the exhaustive matches gain the new variants in the arm that refuses a field a
   freeze-refusal verdict may not amend (read the arm for `Wiring | WiringTests`).
7. `src/orchestrator/adjudication/prompt.rs`: the `DisputeKind::Criterion` arm routes the five
   new fields to `goal_entry::build(&input, plan_path, field, criterion_indices, owner)` and
   wiring/wiring-tests disputes with an owner to `criterion_entry` with the owner.
8. New `src/orchestrator/adjudication/prompt/goal_entry.rs` (declare it where `prompt.rs`
   declares `criterion_entry`). Reuse `criterion.rs`'s shared helpers (`push_dispute_header`,
   `push_plan_and_listing`, `verdict_rules`, `schema_json`, `PATCHES_RULE`,
   `run_before_judging`) exactly as `criterion_entry.rs` does. Per field:
   - reachable: quote `symbol`, `from`, `min_confidence`; observe with `loom map --find-all
     <symbol>` and `loom map --impact <symbol>` from the execution site; say that a call
     through a trait object, a stored closure, a function pointer or a registration by string
     is a path the graph may not bind; an accept that DELETES an entry must cite a test that
     executes the path (a frozen contract, a test function or a wiring test) by file and name,
     otherwise replace the entry with one the graph resolves (for example `from` an
     intermediate function the walk does reach) or reject. A `could not be evaluated` gap is a
     loom graph failure: judge whether the unit is wired by reading the code.
   - artifacts: quote the path; observe with `test -f` and reading the file; the stub rule
     (`verify/goal_backward/artifacts.rs`) is what failed.
   - dead-code-check: quote command, fail and ignore patterns; observe by running the command.
   - regression-test: quote file and `must_contain`; observe by reading the file.
   - after-stage: quote the truth check; observe by running it.
   - With an owner, quote the OWNER's stage block and list (load it with
     `crate::verify::transitions::load_stage(owner, work_dir)`), state that the verdict amends
     stage `<owner>`, and that integration-verify re-runs the check on the merged tree.
   Keep each builder under 50 lines; the file under 400 (split `goal_entry_observe.rs` if
   needed).
9. `src/orchestrator/adjudication/prompt/criterion_entry.rs`: accept an owner for wiring
   disputes (quote the owner's entries).
10. Tests in files you own: `tests_criterion_field.rs`, `verdict_tests.rs`, `plan_patch_tests.rs`,
    `apply_accept_tests.rs`, `record_tests.rs`, `prompt/tests_kinds.rs`, `prompt/tests.rs`.
    `prompt/tests_golden.rs` pins the existing briefings: they must not change for the four
    existing fields (`live` included) with no owner. Add a golden test for a reachable owner
    briefing.

## Traps

- The verdict never chooses the amended stage: the request's owner does. A `plan_patch` with a
  `stage` key is ignored (and the re-prompt says so if it is present).
- Apply is idempotent per patch: an interrupted apply re-runs `apply_amendment`, which returns
  the recorded row for the same exact dispute id.
- The judge's working directory is the filer's worktree (`session.rs::judge_cwd`): for
  integration-verify that is the merged tree, the right place to observe a predecessor's check.
- `verdict.rs` is 385 lines plus the live plan's `Live` arm, and gate-waivers adds to it next:
  keep it at or under 395 (`wc -l`), moving a helper into a `#[path]` child module of
  `verdict.rs` if it would not fit (`adjudication/mod.rs` belongs to spawn-recovery, which runs
  beside this stage).

## Done

Your one check after the stage compiles: `cargo test --lib orchestrator::adjudication::`.
