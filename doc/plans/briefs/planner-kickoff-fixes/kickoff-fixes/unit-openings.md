# Unit openings

Implementer: codex, gpt-6.2-sol, effort xhigh. Wave: 1.

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` in full first (the
settled decisions, the wire contract and, for codex units, "Rules for codex units"). Then read
section 1 of `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/server.md`.

## Files you own (write only these)

- `loom/src/planning/guidance/opening.rs`
- `loom/src/planning/guidance/continuation.rs`
- `loom/src/planning/guidance/start.rs`
- `loom/src/planning/guidance/tests/drafting.rs`
- `loom/src/planning/guidance/opening/tests/drafting.rs`
- `loom/src/planning/guidance/start/tests/lookup.rs`
- `plan-plugin/skills/kickoff/SKILL.md`

## Entry points

`kickoff_opening` in `opening.rs`; the session title in `start.rs`; `DraftContinuation` in `continuation.rs`.

## Done

Both openings read `loom plan: Kickoff, feature.` and `loom plan: Kickoff, standalone plan.`,
the feature session title is `New feature`, `DraftContinuation.target` adds the pinned line,
the skill names the size, the tests assert the new strings, and `cargo check --all-targets`
shows no error in your files. Orchestrator proof: `cargo test --lib planning::guidance::`.

## Traps

`DraftContinuation` gains a field: its constructor in `chat/draft_cmd.rs` is owned by `draft-commands` and breaks until wave 2. In SKILL.md change only the sentence named; keep every other line.
