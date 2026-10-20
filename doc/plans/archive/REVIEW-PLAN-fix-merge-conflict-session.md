# Code Review: Fix Merge Conflict Session Lifecycle

**Plan:** PLAN-fix-merge-conflict-session | **Generated:** 2026-04-16 07:07 UTC

## Summary

This plan fixes a merge conflict session lifecycle bug where the original Claude Code session continues running in parallel with a merge resolver, wasting tokens while trapped by the commit-guard hook. It implements four coordinated changes: making `loom stage complete` bail on conflict detection, allowing session exit in the hook, actively killing the stale process before spawning a resolver, and documenting the resolver's inherited stage responsibilities. The result is clean session exit, reduced token waste, and clear handoff semantics between original and resolution sessions.

## Changes by Stage

### Fix Merge Conflict Session Lifecycle (fix-merge-session-lifecycle)

**Status:** completed  
**Purpose:** Fix the merge conflict session lifecycle so the original session exits
cleanly and the merge resolver inherits stage responsibilities.

Use parallel subagents and skills to maximize performance.

CONTEXT: When loom stage complete encounters a merge conflict, the original
Claude Code session continues running while a resolver is spawned. The original
is trapped by commit-guard.sh (blocks MergeConflict exit) and wastes tokens.
Four coordinated changes fix this.

SUBAGENT FILE ASSIGNMENTS:

  Subagent 1 — Progressive Complete (loom-software-engineer):
    Files Owned: loom/src/commands/stage/progressive_complete.rs
    Files Read-Only: loom/src/commands/stage/complete.rs

    Task: In complete_with_merge(), change the combined match arm at lines 192-195
    from returning Ok(false) to bail!() for each variant separately.

    Step 1: Add bail to the anyhow import at line 6:
      Change: use anyhow::{Context, Result};
      To:     use anyhow::{bail, Context, Result};

    Step 2: Replace lines 192-195 (the combined arm):
      Old:
        MergeOutcome::Conflict | MergeOutcome::Blocked => {
            // Stage already saved in conflict/blocked state
            Ok(false)
        }
      New (two separate arms):
        MergeOutcome::Conflict => {
            bail!(
                "Merge conflict detected for stage '{}'.\n\
                 A resolution session has been spawned to handle the merge.\n\
                 Your work is committed on the stage branch -- this session should exit now.\n\
                 Do NOT attempt to resolve the merge conflict yourself.",
                stage.id
            );
        }
        MergeOutcome::Blocked => {
            bail!(
                "Merge blocked for stage '{}'.\n\
                 The stage has been marked MergeBlocked.\n\
                 This session should exit now. Fix the issue and run: loom stage retry {}",
                stage.id,
                stage.id
            );
        }

    Why safe: The only caller (complete.rs:623) uses ? to propagate errors.
    The bool return was already discarded. attempt_progressive_merge() callers
    in verify.rs and human_review.rs are unaffected -- they call the inner function.

    Do NOT modify the existing test module at lines 199-330.

  Subagent 2 — Commit Guard Hook (loom-software-engineer):
    Files Owned: hooks/commit-guard.sh
    Files Read-Only: (none)

    Task: Allow session exit when stage is in MergeConflict status.

    Step 1: Update comment at line 420:
      Old: # BLOCK: merge-conflict (needs manual resolution)
      New: # ALLOW: merge-conflict (handled by resolution session)

    Step 2: Replace lines 429-433 (the MergeConflict case in the status check):
      Old:
        merge-conflict | MergeConflict)
            stage_incomplete=1
            issues+=("Stage '$STAGE_ID' has MERGE CONFLICT that needs resolution")
            debug_log "Stage has merge conflict - will block"
            ;;
      New:
        merge-conflict | MergeConflict)
            debug_log "Stage has merge conflict - resolution session handles this, allowing stop"
            ;;

    Step 3: Remove the dead merge-conflict branch in the error message builder.
      Replace lines 532-536 (including the fi on line 536):
        Old:
          if [[ "$blocking_status" == "merge-conflict" ]] || [[ "$blocking_status" == "MergeConflict" ]]; then
              message+="\n\n${step_num}. Stage has a MERGE CONFLICT. Resolve it manually:\n   - Check the conflicting files\n   - Resolve conflicts and commit\n   - Run: loom stage complete $STAGE_ID"
          else
              message+="\n\n${step_num}. Stage is still in '$blocking_status' status. After committing, run:\n   loom stage complete $STAGE_ID"
          fi
        New (remove if/else, keep only the general message):
          message+="\n\n${step_num}. Stage is still in '$blocking_status' status. After committing, run:\n   loom stage complete $STAGE_ID"

    The hook is embedded via include_str! in loom/src/fs/permissions/constants.rs:8.
    No change needed there -- cargo build re-reads the file at compile time.

  Subagent 3 — Merge Handler (loom-software-engineer):
    Files Owned: loom/src/orchestrator/core/merge_handler.rs
    Files Read-Only: loom/src/orchestrator/terminal/mod.rs (for kill_session trait)

    Task: Actively kill the original session process when spawning merge resolver.

    In spawn_merge_resolution_sessions(), replace lines 611-617:
      Old:
        // Either a stale Stage session or a dead Merge session -- clean up
        let stale_session_id = session.id.clone();
        self.active_sessions.remove(&stage_id);
        // Remove the old signal file so it doesn't block respawning
        if let Err(e) = remove_signal(&stale_session_id, &self.config.work_dir) {
            eprintln!("Warning: Failed to remove stale signal for session '{stale_session_id}': {e}");
        }
      New:
        // Either a stale Stage session or a dead Merge session -- clean up
        let stale_session = self.active_sessions.remove(&stage_id).unwrap();
        let stale_session_id = stale_session.id.clone();
        // Kill the original session to prevent zombie processes.
        // When loom stage complete detected a merge conflict, the Stage session
        // may still be running -- actively terminate it.
        if let Err(e) = self.backend.kill_session(&stale_session) {
            tracing::debug!(
                session_id = %stale_session_id,
                error = %e,
                "Failed to kill stale session (may already be dead)"
            );
        }
        // Remove the old signal file so it doesn't block respawning
        if let Err(e) = remove_signal(&stale_session_id, &self.config.work_dir) {
            eprintln!("Warning: Failed to remove stale signal for session '{stale_session_id}': {e}");
        }

    The .unwrap() is safe: we are inside if self.active_sessions.contains_key(&stage_id)
    at line 602. kill_session is idempotent -- killing a dead process is a no-op.
    Use tracing::debug! (not eprintln!) because failure to kill a dead session is expected.

    BORROW CHECKER NOTE: The variable `session` (from get() at line 603) holds an
    immutable borrow of self.active_sessions. In the new code, `session` is last used
    at line 604/606 (the type/liveness check), so NLL releases the borrow before
    the remove() call at line 612. If the compiler complains, restructure by extracting
    the type check result into a bool before the cleanup block.

    Do NOT modify any other function in the file. Do NOT modify the test module.

  Subagent 4 — Merge Signal (loom-software-engineer):
    Files Owned: loom/src/orchestrator/signals/merge.rs
    Files Read-Only: (none)

    Task: Add "Inherited Responsibilities" section to merge signal content.

    In format_merge_signal_content(), add a new section after line 116 (after the
    closing of the "## Important" section, before the function returns):

      content.push_str("## Inherited Responsibilities\n\n");
      content.push_str(
          "This resolution session now **owns** this stage. \
           The original execution session has exited.\n\n",
      );
      content.push_str(
          "- The original stage's acceptance criteria already passed before the merge conflict\n",
      );
      content.push_str("- Do NOT re-run the original stage's tasks or acceptance criteria\n");
      content.push_str(&format!(
          "- After resolving: `loom stage merge {} --resolved` marks the stage Completed \
           and triggers dependents\n",
          stage.id
      ));
      content.push_str(
          "- The orchestrator handles plan completion when all stages finish\n",
      );
      content.push_str(
          "- If this session exits without resolving, the orchestrator will spawn a new resolver\n\n",
      );

    Parser-safe: parse_merge_signal_content only reads Target and Conflicting Files
    sections. The new section creates an ignored map entry. Do NOT modify any other
    function in the file.

NO FILE OVERLAP between subagents confirmed.

MEMORY RECORDING (use loom memory ONLY):
⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/)
⛔ NEVER use loom knowledge update (reserved for knowledge-bootstrap/integration-verify)

- MISTAKE: loom memory note "mistake: ..."
- DECISION: loom memory decision "..." --context "..."
- SURPRISE: loom memory note "found: ..."

#### Files Changed

No changes recorded.

#### Key Decisions

- changed MergeOutcome::Conflict|Blocked combined arm to bail\!() per variant *(return Ok(false) left the original session running; bail\!() propagates via ? in complete.rs:623 and forces the session to exit with a clear message)*

#### Notes

- found: merge_handler.rs used active_sessions.get() then .remove() -- NLL releases the immutable borrow before remove() since session is last used at the is_alive check. kill_session(&stale_session) works because stale_session is owned after remove()
- found: commit-guard.sh MergeConflict case was blocking session exit, preventing the original session from exiting cleanly when merge conflict was detected -- resolution session was supposed to handle it but original session was trapped

### Integration Verification (integration-verify)

**Status:** executing  
**Purpose:** Final integration verification for merge conflict session lifecycle fix.

Use parallel subagents and skills to maximize performance.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST):
0a. Read the plan file: doc/plans/PLAN-fix-merge-conflict-session.md
    (or check .work/config.toml for source_path)
0b. Read ALL stage memories: loom memory show --all
0c. Read doc/loom/knowledge/mistakes.md for context on prior merge issues

⛔ ZERO TOLERANCE: ALL compiler warnings, linter errors, test failures must be
FIXED (not suppressed). Nothing is "pre-existing" or "too trivial."

BUILD & TEST:

1. cd loom && cargo test (full suite)
2. cd loom && cargo clippy -- -D warnings
3. cd loom && cargo build
4. cd loom && cargo fmt --check

CODE REVIEW (MANDATORY):
5. Spawn PARALLEL loom-code-reviewer subagents:

- Review loom/src/commands/stage/progressive_complete.rs:
     Verify bail! arms are correct, error messages are clear, no regressions
     in the test module (lines 199-330)
- Review loom/src/orchestrator/core/merge_handler.rs:
     Verify kill_session call is safe (inside contains_key guard),
     no phantom-merge regressions (the file has extensive phantom-merge prevention)
- Review hooks/commit-guard.sh:
     Verify MergeConflict now allows exit, no other blocking statuses affected,
     dead code removed cleanly
- Review loom/src/orchestrator/signals/merge.rs:
     Verify new section doesn't break parse_merge_signal_content parser
1. Fix ALL issues found by reviewers

FUNCTIONAL VERIFICATION (MANDATORY):
7. Verify the 4 changes are wired correctly:
   a. progressive_complete.rs: rg for "bail!" in the MergeOutcome match arms
   b. commit-guard.sh: Verify MergeConflict case does NOT set stage_incomplete
   c. merge_handler.rs: rg for "kill_session" in spawn_merge_resolution_sessions
   d. merge.rs: rg for "Inherited Responsibilities" in format_merge_signal_content
8. Verify detection.rs still treats MergeConflict session exits as normal
   (not crash) -- read loom/src/orchestrator/monitor/detection.rs:146-162
9. Verify no other callers of complete_with_merge are broken:
   rg "complete_with_merge" in loom/src/

KNOWLEDGE DISTILLATION (MANDATORY):
10. Record own discoveries to loom memory FIRST
11. Read all stage memories: loom memory show --all
12. Curate the merge conflict session lifecycle fix as a mistake entry:
    loom knowledge update mistakes with:
    - What happened: original session continued running after merge conflict
    - Why: Ok(false) return + blocking Stop hook + no process kill
    - Prevention: bail! on merge conflict, allow exit, kill stale sessions
    - Fix: 4-part coordinated change
13. Update patterns.md if the "Merge Anti-Respawn Pattern" section
    (in doc/loom/knowledge/patterns.md) needs updating to reflect the
    new kill behavior

DOCUMENTATION UPDATE:
14. Update doc/loom/knowledge/patterns.md "Merge Recovery Flow" section
    if it references the old behavior
15. Check if architecture.md needs a note about the session lifecycle change

#### Files Changed

No changes recorded.

#### Key Decisions

- kept bail\!() message wording as-is for MergeOutcome::Conflict *(Reviewer noted the message says 'A resolution session has been spawned' which may not always be true (spawning can fail). However, changing to a weaker message would reduce agent compliance with the exit instruction. The spawn failure is already logged separately. The priority is ensuring the session exits.)*

#### Notes

- found: progressive_complete.rs NoBranch arm (line 66-71) does not call try_mark_merge_blocked() or save_stage() unlike the Err arm. This is asymmetric but pre-existing. Also ProgressiveMergeResult::is_success() in progressive_merge/mod.rs still classifies NoBranch as success, inconsistent with the fix treating it as Blocked.
- found: merge_handler.rs BranchMissing path in handle_merge_session_completed (line 97-103) calls finalize_merge_resolution which sets merged=true WITHOUT git ancestry verification. This is a pre-existing phantom-merge risk per the project invariant in mistakes.md. Not introduced by this plan but should be tracked as a concern.
- found: commit-guard.sh case statement was missing explicit needs-human-review|NeedsHumanReview case -- fell through to wildcard catch-all (correct behavior but inconsistent with other statuses). Fixed by adding explicit case.

## Open Questions

No open questions.
