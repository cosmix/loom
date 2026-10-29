# C2 pre-compact (codex unit): drop the receipt epoch from `loom hook pre-compact`

Read `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md` sections 1 and 4 first. All paths below are relative to `loom/`.

## Files

- Write: `src/commands/hook/pre_compact.rs` and its test file `src/commands/hook/tests_pre_compact.rs`. Nothing else.
- Read only: `src/commands/hook/target.rs` (`HookTarget::from_environment`), `src/context/delivery/session.rs` (`discard_session_delivery`).
- Never run git. Never read or write a path under `.loom/`. Do not run cargo: W1 deletes `src/context/read_receipts*` from the same crate while you work. The stage orchestrator compiles and tests after every worker returns.

## Steps

1. **`pre_compact.rs` code.** `use crate::context::{delivery, read_receipts};` becomes `use crate::context::delivery;`. In `reset_for_payload`, delete the line `read_receipts::rotate_epoch_for_session(&session_id);`. Nothing else in the code changes.
2. **`pre_compact.rs` module doc.** Replace the first two paragraphs, from the line starting `` //! `loom hook pre-compact` `` down to the line ending `record, both describe context windows this compaction never touched.`, with exactly:

   ```rust
   //! `loom hook pre-compact` — reopen delivery suppression for a compacted session.
   //!
   //! Two hooks run this delegate. `loom-hooks/pre-compact.sh` runs it on every
   //! PreCompact event of a stage session, before its block-then-allow handoff.
   //! `loom-hooks/knowledge-orient.sh` runs it for an ordinary session on the
   //! SessionStart event whose `source` is `compact`, which fires once after a
   //! compaction. No PreCompact hook is registered outside stage worktrees
   //! (`fs::permissions::hooks::config::build`).
   //!
   //! Suppression in [`crate::context::delivery`] is scoped to a recipient and an
   //! epoch: while a session's context window is intact, skipping units it
   //! already read is correct. A compaction may drop what the session was given,
   //! so the delegate deletes exactly one record, the compacting session's own
   //! prompt-hook record. A live sibling session's record stays.
   //!
   //! A stage session's spawn record also stays, so a compacted stage session
   //! still counts its spawn brief as delivered. Stage sessions have not
   //! compacted in practice, and `loom-hooks/session-start.sh` tells a compacted
   //! one to re-read its signal.
   ```

   Keep the paragraph that starts `//! Same fail-open contract as every hook delegate` and everything after it unchanged.
3. **`tests_pre_compact.rs`.** Delete the line `use crate::context::read_receipts;` and the whole test `pre_compact_rotates_only_the_payload_sessions_receipt_epoch` (its `#[test]` and `#[serial]` attributes included). Every other test stays as it is.

## Constraints the graph does not show

- `HookTarget::from_environment` resolves `LOOM_WORK_DIR`, else the process working directory. The payload's `cwd` is not read, and this unit does not change that.
- The module doc is checked by `RUSTDOCFLAGS="-D warnings" cargo doc`: the one intra-doc link, [`crate::context::delivery`], resolves today; add no other bracketed link.

## Done when

From `loom/`:

- `rg -n 'read_receipts|receipt' src/commands/hook/pre_compact.rs src/commands/hook/tests_pre_compact.rs` prints nothing.
- `rg -c '^fn |^pub fn ' src/commands/hook/pre_compact.rs` still counts 4 (`pre_compact`, `pre_compact_from`, `reset_for_payload`, `parse_session_id`).

Report what you changed and any step that did not match the file.
