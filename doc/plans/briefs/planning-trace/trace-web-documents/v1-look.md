# V1: the look of settle state, stubs, hints, chips and impact (trace-web-documents, wave 2)

Invoke `frontend-design:frontend-design` first. Read `../common.md` "The browser",
`web/src/plan/ui/DESIGN.md` in full, and W1's report. You own `web/src/plan/trace/trace.css` and
`web/src/plan/ui/DESIGN.md`; markup changes in W1's files go back to the orchestrator as a list.

## What to design

- **Settle state on spine rows.** A small mark beside the existing state glyph: draft is quiet
  (muted outline), settled is the done tone, revised/stub_changed/upstream_changed share one
  attention tone (amber, as DESIGN.md's warnings). The mono slug must keep its full width at
  18rem (the spine mistake in `mistakes/planner-polish.md`): no words beside the glyphs.
- **Frontier hints.** Up to five short link lines under the next-step sentence, lighter than the
  sentence, one line each, ellipsis at the column edge.
- **Document heading.** The settle state as a pill beside the Summary/Source switch; Mark settled
  as a secondary button; the unresolved confirmation as the planner's existing dialog style.
- **Stub panel.** Above the body, quieter than the body text, reading face at a smaller size,
  with its link and the "stub changed" note in the attention tone.
- **Edge chips.** Quiet at rest (they must not compete with the reading measure): small mono
  chips after the sentence, visible on hover, focus and when the sentence is opened. Finding
  counts as a chip with the count.
- **Impact rows.** Under each changed file in the turn footer, indented, one row per downstream
  document with its settle mark.

Add DESIGN.md entries for the settle tones, chips and hints under the existing headings
(`## Status`, `## Page rhythm`); never restate rules already there.

## Proof

Build (`cd web && bun run build`, `cargo build --manifest-path loom/Cargo.toml`), start the
`loom plan` server against the scratch fixture repository (stage description and the plan's
Environment inventory; the fixture is DEMO from `loom/tests/planning_trace_contracts.rs` with a
settled section, a revised plan, a findings ledger and a summary store copied from the server's
concept fixtures if available) and shoot with a throwaway Playwright driver at 1440x900 and
390x844, in Ledger and Walnut: the spine, a settled and a revised section summary page, a plan
page with chips, and a chat turn footer (feed it with the stubbed socket frames the planner shot
scripts use). Check each against this brief, fix, re-shoot. Report the shot paths. Restore
`web/dist` when done.
