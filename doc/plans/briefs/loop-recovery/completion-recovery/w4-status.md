# W4 — Status DTO, CLI/TUI, and web rendering

Lane: Codex `gpt-5.6-sol`, effort `xhigh`. Starts after W1; parallel with W3. W4 exclusively owns `loom/src/daemon/wire_tests.rs`; W2 must leave it untouched. Do not run git.

## Owned files

- `loom/src/commands/status/data/mod.rs`
- `loom/src/commands/status/data/collector.rs`
- `loom/src/commands/status/data/collector_tests.rs`
- `loom/src/commands/status/data/collector_activity_tests.rs`
- `loom/src/commands/status/data/sanitize.rs`
- `loom/src/commands/status/render/activity.rs`
- `loom/src/commands/status/render/attention_model.rs`
- `loom/src/commands/status/render/attention_model_tests.rs`
- `loom/src/commands/status/render/attention.rs`
- `loom/src/commands/status/render/attention_tests.rs`
- `loom/src/commands/status/render/compact.rs`
- `loom/src/commands/status/render/graph_tests.rs`
- `loom/src/commands/status/ui/tui/state.rs`
- `loom/src/commands/status/ui/tui/ledger/cells.rs`
- `loom/src/commands/status/ui/tui/ledger/rows.rs`
- `loom/src/commands/status/ui/tui/ledger/tests.rs`
- `loom/src/commands/status/ui/tui/ledger/tests_alignment.rs`
- `loom/src/commands/status/web/model_tests_stages.rs`
- `loom/src/daemon/wire_tests.rs`
- `web/src/api/schema.ts`
- `web/src/api/schema.test.ts`
- `web/src/lib/format.ts`
- `web/src/lib/format.test.ts`
- `web/src/lib/graph.ts`
- `web/src/lib/graph.test.ts`
- `web/src/components/stage-sections.tsx`
- `web/src/components/stage-sections.test.tsx`
- `web/src/components/stage-heading.tsx`
- `web/src/components/ledger-row.tsx`
- `web/src/components/graph/stage-node.tsx`
- `web/src/components/stage-modal.test.tsx`

## DTO and collection

Add optional `outgoing_session_exit_reason` and `completion_blocker` summary fields to Rust `StageSummary`. The summary contains a short fingerprint, bounded summary/evidence, first/last observation, repeat count, commit, and next action; do not send the full command environment or unchecked handoff prose.

`build_stage_summary` reads W1's checkpoint only for the exact `stage.session`. Show an exit reason only for a terminal outgoing session; never attach an old predecessor's reason to a live current session. A repeated blocker is actionable when identity/session/commit still match even if W3 has not parked it yet. Sanitize every persisted string before render using the existing untrusted-value boundary.

Update every Rust `StageSummary {}` literal in the owned status tree and `daemon/wire_tests.rs`; these are compiler-caught. The semantic hazards are silent: wrong-session lookup, formatter precedence, attention selection, TUI truncation/alignment, graph footer height, and TypeScript fallback branches.

## User-visible meaning

Keep the workflow status unchanged. For the first verified boundary failure, render `completion pending: <summary>` ahead of generic Executing/stale activity. For repeated evidence/NeedsHumanReview, render `completion blocked: <summary>`. Session detail renders `stalled`, `context ceiling`, `criteria blocked`, etc. independently of terminal `ContextExhausted`/Completed status.

CLI compact and attention output must expose the blocker and next action. TUI ledger/activity cells must show the same priority without breaking wide-glyph alignment. Add fingerprint and pending-to-blocked tracking to the activity log; deduplicate unchanged polls and test that transition.

Extend strict `web/src/api/schema.ts` with exact enums/objects. `web/src/lib/format.ts::activityText` owns precedence. `StateLine`, `LedgerRow`, and graph `Footer` already consume it; `stageSections` supplies detailed evidence. Update `hasFooter`/node-height behavior if the new activity makes a footer appear. Tests must cover overview graph, ledger, and modal through those real consumers, not formatter-only assertions.

## Lifecycle display cases

- first attempt, writer live: Executing + `completion pending`;
- repeated attempt while W3 resolves writer: Executing + `completion blocked`, no claim that it is safe to reassign;
- confirmed parked: NeedsHumanReview + blocker/next action;
- unknown writer: blocker plus explicit ownership uncertainty;
- session/commit changed: old checkpoint hidden from current activity;
- true ceiling and low-context stall: distinct exit reason labels with measured context shown separately;
- completed stage: blocker absent from activity, historical reason only where terminal session detail is intentionally shown.

## Regressions and proof

Rust wire test serializes a representative pending and parked summary. The TypeScript strict schema parses both and rejects malformed enums. CLI/TUI tests assert priority and alignment. Component tests render graph footer, ledger row, stage heading/modal detail and next action. Include untrusted control characters/long evidence to prove sanitization and bounds.

Optional single scoped check: `cargo test --manifest-path loom/Cargo.toml --lib commands::status::`. The stage orchestrator runs `bun run --cwd web check` and `bun run --cwd web build` as the canonical web gate.

Done means a user sees the first failure immediately, can distinguish pending/blocked/unknown-writer states, and never sees a stall mislabeled as context exhaustion.
