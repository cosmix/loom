# git-preservation / w6-stash-surface: remove the stash outcome from every merge result, record and status surface

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/git.md`, section G5 (option (a): "the `stash` field of `MergeResult::Success`, `note_merge_stash` consumers").

This worker exists because w1 deletes the stash-and-reapply landing, so no merge can stash the operator's changes any more. Everything that carried, recorded or displayed a stash outcome is orphaned and goes. Several of these files sit outside the stage's original territory; the plan widens the territory for exactly the files listed below.

## Findings covered
- G5 (tracked half, consumers): `StashReapply`, `MergeResult::Success.stash`, `MergeDisposition::Landed.stash`, the stage record's `merge_stash` and `merge_unrestored_stashes`, the status `stash_warning`, the `STASH NOT RESTORED` attention entry, the TUI `stash!` cell and the web "stash not restored" row.

## Files
Own (write), relative to `loom/`:
- `src/orchestrator/core/merge_handler/landing.rs`; `src/orchestrator/core/merge_handler/landing_tests.rs`; `src/orchestrator/core/merge_handler/auto_merge_outcome.rs`
- `src/commands/stage/merge.rs`; `src/commands/stage/merge/landing.rs`
- `src/commands/stage/progressive_complete.rs`; `src/commands/stage/progressive_complete/report.rs`
- `src/stage_service/merge_outcome.rs`; `src/stage_service/merge_outcome_tests.rs`
- `src/orchestrator/auto_merge.rs`; `src/orchestrator/progressive_merge/mod.rs`; `src/orchestrator/progressive_merge/execution.rs`
- `src/orchestrator/core/merge_lifecycle_e2e_tests_blocked.rs`
- `src/models/stage/merge_block.rs`; `src/models/stage/merge_block/tests.rs`; `src/models/stage/types.rs` (one doc line)
- `src/status/data/merge_note.rs`; `src/status/data/mod.rs`; `src/status/data/collector.rs`; `src/status/data/sanitize.rs`
- `src/status/attention_model.rs`; `src/status/attention_model_tests.rs`
- `src/commands/status/web/model_tests_stages.rs`; `src/commands/status/ui/tui/ledger/cells.rs`
- `../web/src/lib/format.ts`; `../web/src/lib/format.test.ts`; `../web/src/api/schema.ts`; `../web/src/components/stage-sections.tsx`; `../web/src/components/stage-now.tsx`

Read only: `src/git/merge/mod.rs` and `src/git/merge/tree.rs` (w1 changes them), `src/orchestrator/core/merge_handler/blocked_retry.rs` (w1).
Frozen: none of yours.

## Changes
Delete every use of `StashReapply`; it no longer exists after w1. Make no other change in these files.

1. Merge results. `MergeDisposition::Landed` (`stage_service/merge_outcome.rs`), `AutoMergeResult::Success` (`orchestrator/auto_merge.rs`) and `ProgressiveMergeResult::Success` (`orchestrator/progressive_merge/mod.rs`) lose their `stash` field and its doc; every constructor and pattern drops it (`From` impls in `merge_outcome.rs` and `auto_merge.rs`, `attempt_auto_merge` in `auto_merge.rs`, `progressive_merge/execution.rs`, the two tests in `progressive_merge/mod.rs`, the three literals in `merge_outcome_tests.rs`). Patterns that listed `stash,` become `..` only where other fields are omitted.
2. Reporting. Delete `report_stash` and `Orchestrator::note_merge_stash` from `merge_handler/landing.rs`, and their calls in `land_stage_merge` (`landing.rs`) and `apply_auto_merge_outcome` (`auto_merge_outcome.rs`). Delete `note_merge_stash` from `commands/stage/progressive_complete/report.rs` (with its test `note_merge_stash_records_the_outcome_and_ignores_none`) and its re-export in `progressive_complete.rs`; drop its calls in `progressive_complete.rs`, `commands/stage/merge/landing.rs` and `commands/stage/merge.rs` (`MergeResult::Success { .. } =>`). Update the module doc of `report.rs` (it reports a blocked or held merge).
3. Stage record (`models/stage/merge_block.rs`). Delete `MergeRecord::stash` (`merge_stash`) and `MergeRecord::unrestored` (`merge_unrestored_stashes`), `Stage::record_merge_stash`, and the `StashNotRestored` branch in `Stage::block_merge`. Rewrite the `MergeRecord` doc (`merge_block` stays a top-level key). `Stage` flattens `MergeRecord` and does not deny unknown fields, so an old stage file with `merge_stash:` still loads and drops the key; add no reader and no migration. In `src/models/stage/types.rs` rewrite the one doc line above `pub merge: MergeRecord` to `/// Why the merge was not advanced; see [`MergeRecord`].` (the file is on the maintainability baseline: the line count must not change).
4. Status. Delete `stash_warning` and `AUTOSTASH_REF_PREFIX` from `status/data/merge_note.rs` (keep `merge_block_text`; rewrite the module doc to the block text only) with its tests `the_stash_warning_lasts_while_the_backup_ref_exists`, `a_restored_stash_or_a_foreign_ref_gives_no_warning`, `every_unrestored_backup_that_still_exists_is_warned`, `a_restored_note_does_not_join_the_warning` and helpers `repo_work_dir`, `stage_with_stash`, `stage_with_stashes` if nothing else uses them. Delete `StageSummary::stash_warning` (`status/data/mod.rs`), its fill in `collector.rs`, its flatten in `sanitize.rs`, the `stash_warning: None` literal in `commands/status/web/model_tests_stages.rs`, the `stash!` arm of `merge_cell` (`cells.rs`), and in `attention_model.rs` the first branch of `attention_entry` plus `stash_entry`. In `attention_model_tests.rs` delete `stash_warning_outranks_every_other_state` and `stash_warning_lists_every_backup_ref`.
5. Tests that pinned the stash landing. In `merge_handler/landing_tests.rs` delete `a_landing_that_stashed_the_operators_changes_records_the_outcome` and `note_merge_stash_persists_an_unrestored_stash_and_ignores_none` and the `StashReapply` import. In `models/stage/merge_block/tests.rs` delete `record_merge_stash_keeps_the_outcome`, `block_merge_stash_not_restored_records_the_unrestored_stash`, helper `stash`, `a_restored_stash_note_keeps_an_unrestored_one`, `an_unrestored_stash_note_replaces_any_note`, `every_unrestored_backup_ref_is_kept_once`; in `leaving_merge_blocked_by_transition_clears_the_block_and_keeps_the_stash` remove the stash setup and assertions and rename it `leaving_merge_blocked_by_transition_clears_the_block`; in `merge_record_keeps_top_level_keys_and_round_trips` remove the `merge_stash`/`merge_unrestored_stashes` lines and fields. In `orchestrator/core/merge_lifecycle_e2e_tests_blocked.rs` replace `an_operator_edit_that_reapplies_cleanly_survives_the_landing` with `an_operator_edit_on_a_path_the_merge_touches_blocks_the_landing`: same setup; `try_auto_merge(ID)` leaves the stage `MergeBlocked` with `merge.block` `Some(MergeBlock::UncommittedOverlap { .. })` naming `multi.txt`, `text_of(root, "multi.txt")` is the operator's text, `main_tip(root)` unchanged.
6. Web (`../web/src`). Delete `stash_warning` from `api/schema.ts`; the `stash_warning` branch of `mergeText` in `lib/format.ts` and its case in `lib/format.test.ts`; the `stashWarning` hint, the `Boolean(stage.stash_warning)` term and the `stash not restored` row in `components/stage-sections.tsx`. In `components/stage-now.tsx` change the comment's example list to drop "unrestored stash". Leave `attention-panel.test.tsx` and `stage-report.test.tsx` alone: their fixtures test generic attention rendering.

## Interfaces shared with other workers
Uses from w1: `MergeResult::Success { files_changed: u32, insertions: u32, deletions: u32 }` (no `stash`); `MergeBlock` without `StashNotRestored` and `StashResetRefused`; `StashReapply` gone from `crate::git` and `crate::git::merge`. w1 stops reading `stage.merge.unrestored` in `blocked_retry.rs`; you delete the field.

## Tests
No new behaviour, so no new test. The rewritten e2e test above fails today (today the landing stashes and succeeds) and passes once w1 lands. Report every test you deleted or changed, with the reason "pinned the deleted stash landing".

## Proof
From `loom/`: `cargo build --all-targets`; `cargo test --lib models::stage::`, `cargo test --lib status::`, `cargo test --lib stage_service::`, `cargo test --lib orchestrator::core::merge_handler::`, `cargo test --lib orchestrator::core::merge_lifecycle_e2e_tests::`. From `web/`: `node_modules/.bin/oxfmt` and `node_modules/.bin/oxlint --deny-warnings` on the changed files, then `node_modules/.bin/vitest run src/lib/format.test.ts`.

## Traps
- `rg -n 'stash' src` afterwards should find no merge-stash consumer; operator-facing remedies that tell the operator to "commit, stash or move" their changes (`MergeBlock::UncommittedOverlap` text, web run dialog) stay.
- `merge_lifecycle_e2e_tests_blocked.rs` also runs `git stash push` as the operator clearing a block; those lines stay.
- `models/stage/types.rs` and `methods.rs` are on the maintainability baseline; you should not need them. If you do, report it.
