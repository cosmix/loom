# Code Review: Fix Container Backend UX Bugs

**Plan:** IN_PROGRESS-PLAN-fix-container-backend-ux | **Generated:** 2026-05-11 18:32 UTC

## Summary

This plan fixes five container backend UX bugs: unreliable container lookup in `logs`/`shell` commands, missing shell completions, no way to list containers, and undocumented deletion lifecycle. It implements a new `loom container list` command, refactors session resolution to reliably find the right container even when multiple sessions exist for a stage, wires shell completions for container subcommands and stage IDs, clears stale container identity from session files after removal, and documents the complete lifecycle of container creation and removal across all removal paths.

## Changes by Stage

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final integration verification — runs AFTER container-ux-fixes is merged.

Use parallel subagents and skills to maximize performance.

CRITICAL: This stage must verify FUNCTIONAL INTEGRATION, not just tests
passing. Code that compiles and passes tests but is never reachable
is USELESS. The new `loom container list` MUST be invocable; the new
completion entries MUST surface from `loom complete`.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST — before any verification):
0a. Read the plan file from doc/plans/ (check .work/config.toml source_path).
0b. Read ALL stage memories: loom memory show --all.
0c. Read doc/loom/knowledge/architecture.md (Container Backend section),
    entry-points.md (Container Subcommand Implementations), and
    mistakes.md.

⛔ ZERO TOLERANCE for warnings/errors. Fix ALL — nothing is "pre-existing".

BUILD & TEST:

1. cargo build (debug and release)
2. cargo test (full suite)
3. cargo clippy -- -D warnings
4. cargo fmt --check
5. Check for regressions in existing container tests:
     cargo test -p loom container

CODE REVIEW (MANDATORY — spawn parallel loom-code-reviewer subagents):
6. Security review subagent:
     - All new shell-outs (docker/podman/container ps and inspect)
       use Command::new(...).args(&[...]) (no string interpolation).
     - Container names are not user-controlled (derived from stage IDs
       which are validated). Confirm.
     - No new credential paths or socket exposures.
7. Architecture review subagent:
     - resolve_session_for_stage helper shared between logs.rs and
       shell.rs (or honestly duplicated — no half-extracted helper).
     - `list`, `logs`, and `shell` PREFER persisted `session.runtime`
       via Runtime::from_binary and fall back to detect_runtime("auto")
       ONLY when the session was written before that field existed.
       NOT detect-first (would target the wrong runtime if the
       operator switched Docker ↔ Podman after spawn).
     - clear_container_identity called in the FOUR in-scope removal
       sites: completion_handler, merge_handler (stale-session
       reap), event_handler::on_needs_handoff, and stop.
       sessions::kill_single_session is intentionally EXCLUDED
       because it deletes the session file outright — DO NOT flag
       its absence. handle_budget_exceeded is also excluded (it
       does not remove the container).
     - Lifecycle doc-comment in container/mod.rs lists every
       removal trigger.
8. Test coverage review subagent:
     - Unit test for clear_container_identity (in session/tests/identity.rs).
     - Unit tests for the pure pick_container_session selector with
       in-memory Vec<Session> fixtures (no Docker required).
     - Unit tests for render_table / render_jsonl in list.rs.
     - Tests for completion routing (subcommands include logs/list;
       shell and logs return stage IDs; `--tail` returns empty).
     - One JSON round-trip test for `list --json` output shape.
9. Fix every issue surfaced by reviewers. Reviewers are read-only —
   the fixes are applied by loom-software-engineer agents OR by
   direct edit if simple.

FUNCTIONAL VERIFICATION (the wiring check):
10. `./target/debug/loom container --help` lists `list` and `logs`
    among its subcommands. (clap exits 0 here only if both are wired.)
11. `./target/debug/loom container list --help` exits 0 and documents
    `--all` and `--json` (the subcommand IS wired).
12. `./target/debug/loom container logs --help` exits 0 and documents
    `--follow` and `--tail`.
13. With a fixture .work/sessions/*.md containing a Running container
    session AND a Completed one for the same stage_id, prove the
    new resolution picks the Running one. This is a unit test
    already added in container-ux-fixes; re-run and confirm pass.
14. Dynamic completion sanity check. The `Complete` subcommand takes
    args in the order: <shell> <cwd> <cmdline> <current_word> <prev_word>.
    Run:
      ./target/debug/loom complete bash . "loom container " "" "container"
    and confirm output includes both "logs" and "list".
15. Stage-ID completion path. From a worktree (or any cwd with a
    populated .work/stages/), run:
      ./target/debug/loom complete bash . "loom container shell " "" "shell"
    and confirm the output contains stage IDs from the .work/stages/
    fixture.

NOTE: Knowledge distillation and the canonical documentation update
are the job of the knowledge-distill stage that runs AFTER this one.
Record your findings to loom memory so knowledge-distill can curate.

#### Files Changed

No changes recorded.

#### Key Decisions

- Do NOT block integration-verify on the list.rs/logs.rs helper duplication. *(Findings are Important (not Critical). The acceptance criteria and goal-backward verification pass; security, runtime preference, and clear_container_identity wiring are all correct. The duplication is a refactor candidate, not an integration defect. Recorded for the knowledge-distill stage and a future cleanup plan.)*

#### Notes

- Security review: container UX fixes are clean. All shell-outs use Command::new(binary).args(&[...]) with whitelisted runtime binaries from Runtime::binary(); container_name flows from session.tracking_key derived from validated stage IDs (validation.rs alphanumeric+_-); no new credentials/sockets/mounts. JSON via serde_json::json\! escapes safely. One Low note: eprintln\! warnings leak full .work/sessions/*.md paths (local-only CLI, acceptable).
- Test coverage finding: list --json schema uses keys 'stage', 'container', 'runtime', 'status', 'session_id' — NOT 'stage_id', 'container_name', 'state'. Tests assert the actual keys. If consumers expect stage_id/container_name shape, schema review needed; otherwise document the chosen schema.
- Architecture review: resolve_session_for_logs and resolve_session_for_shell in logs.rs:131-228 are near-duplicates differing only by require_running arg and the Exited-state branch. A single resolve_session_for_stage(sessions_dir, stage_id, mode: ResolveMode) would remove ~50 lines.
- Architecture review: list.rs::build_rows reimplements load_sessions + runtime-resolution logic that exists as crate-visible helpers in logs.rs (load_sessions, pick_container_session). Not used. Counts as a 'half-extracted helper' — flagged Important by reviewer. Detection: rg 'session.runtime.as_deref' surfaces 3 sites in logs.rs/list.rs that should call one helper.

### Knowledge Distillation (knowledge-distill)

**Status:** executing  
**Purpose:** Knowledge distillation — curate all stage memories into permanent
knowledge and update user-facing documentation.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST):

1. Read the plan file from doc/plans/ (check .work/config.toml source_path).
2. Read ALL stage memories: loom memory show --all.
3. Read doc/loom/knowledge/*.md to understand current state.

MEMORY CURATION (use loom knowledge update — NOT Write/Edit):
4. Synthesize all stage memories into actionable knowledge.
5. Likely mistake-rule additions (loom knowledge update mistakes "..."):
     - "Session files outlive their containers — filter by status ==
       Running AND verify via `<runtime> inspect` before exec'ing
       into a named container."
     - "First-match iteration over .work/sessions/*.md is unsafe when
       a stage has multiple session files (retries). Sort by
       last_active or filter by status before picking one."
     - "Every long-lived resource handle on Session needs both a
       setter and a clearer (set_container_identity /
       clear_container_identity); callers must invoke the clearer
       after the resource is gone, otherwise stale session files
       poison lookups."
6. Patterns to consider for loom knowledge update patterns "...":
     - "Container CLI commands (logs, shell, list) should query the
       runtime directly for State.Status before trusting the
       session file — defense in depth against stale records."
     - "Lifecycle documentation belongs in the module that owns the
       lifecycle (orchestrator/terminal/container/mod.rs //!
       block), not scattered across callers."
7. Architecture updates (loom knowledge update architecture "..."):
     - Add `loom container list` to the Container Subcommand
       Implementations table in entry-points.md.
     - Add `Session::clear_container_identity` to relevant
       architecture lists (the symmetry rule for runtime identity
       fields).

KNOWLEDGE REVIEW:
8. Run `loom review` and remove stale entries.
9. Confirm architecture.md references `loom container list` and
   the clear_container_identity symmetry.

DOCUMENTATION UPDATE (MANDATORY):
10. Update README.md sections that describe the container backend
    to list `loom container list` and `loom container logs` and
    point to the lifecycle section in the module docs. Only
    update affected sections — do NOT rewrite the whole README.
11. Fix the stale README syntax at README.md:177:
      loom container shell [--stage <stage-id>]
    The CLI takes a POSITIONAL `<stage-id>`, not `--stage`. Change to:
      loom container shell <stage-id>
    and likewise add a `loom container logs <stage-id>` synopsis.
    (Finding 12.)
12. Update CONTRIBUTING.md only if it currently documents the
    container subcommand surface.
13. If no user-facing behavior surfaces in README/CONTRIBUTING
    currently, record the reason in memory and skip — do NOT
    invent new sections.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- found: build_rows in list.rs reimplements session loading + runtime detection logic that exists in logs.rs (load_sessions, pick_container_session). The duplication is a known refactor candidate (not a blocker). Document in concerns or patterns as tech debt.
- found: loom container list is fully implemented (list.rs). JSON output keys are: stage, container, runtime, status, session_id — NOT stage_id/container_name/state. Integration-verify confirmed tests assert these exact keys. Document this schema in entry-points.
- found: README.md:177 has stale syntax 'loom container shell [--stage <stage-id>]'. The actual CLI (cli/types.rs:334-338) uses a positional stage_id argument, not --stage flag. Same issue: logs synopsis at README:178 is correct (positional). Fix: change to 'loom container shell <stage-id>'.
- found: clear_container_identity already implemented in models/session/methods.rs:135-139. set_container_identity is its counterpart at :122-127. Both clear container_name AND runtime fields. The symmetry is in place — knowledge update needed to document it.

## Open Questions

No open questions.
