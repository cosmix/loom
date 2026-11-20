# Web W2: stage dialog Tokens section and footer time left (Claude, sonnet)

Read `doc/plans/briefs/token-flow/common.md`, then `doc/loom/knowledge/conventions/web-dashboard-typography.md`, then the approved mockup <https://claude.ai/artifact/BFDRMiWwtsv653hqsMPvED> (Mockup B and the A1 footer). Wave 1 has already landed `api/schema.ts`, `lib/tokens.ts` (`fresh`, `compactTokens`, `cacheHitPercent`, `stageFlow`, `agentLabel`, `quotaLeftText`) and `components/sparkline.tsx` (`HeatStrip`). Use them and do not edit those files. The dialog has no tabs: its existing heading, description, status line, DEPENDS ON / FEEDS and section cards all stay. Paths are under `web/src/`.

## Files you own

- `components/stage-tokens.tsx` (new), `components/stage-tokens.test.tsx` (new)
- `components/stage-sections.tsx`, `components/stage-sections.test.tsx`
- `components/stage-heading.tsx`
- `components/stage-modal.test.tsx`
- `components/quota-meters.tsx`, `components/quota-meters.test.tsx`
- `routes/shell.tsx` (the footer)

Read-only and frozen: `src/token-flow.contracts.test.tsx`.

## Changes

1. **Tokens section** (`stage-tokens.tsx`): `StageTokens({ stageId })` reads `snapshotAtom`, finds `stageFlow`, and renders nothing without one. Otherwise it renders a section in the `ReviewNotes` style (`<section className="flex flex-col gap-2.5 rounded-lg border border-hairline bg-card p-4">` with an eyebrow h2 "tokens"):
   - **Header row:** "30 s cells" plus a five-step intensity ramp drawn with `HeatStrip`.
   - **Four chips:**
     - fresh rate "{compactTokens}/m", labelled "fresh, 5 min avg";
     - fresh total, labelled "fresh so far";
     - cache hit %, labelled "cache hit";
     - "{active} of {agents}", labelled "agents live".
   - **One lane per agent**, in flow order:
     - label from `agentLabel`, with a small "codex" marker for kind codex;
     - a sub-line "{id} · {model} · live|idle";
     - a `HeatStrip` of the agent series against the stage maximum, trimmed of the leading buckets that are zero for every agent, keeping at least 20 cells; an agent series shorter than the stage's (an idle agent's can be empty, common.md section 3) is left-padded with zeros to the stage series length first;
     - the rate "{x}/m", or "—" when inactive;
     - the fresh total.

   Each lane is an element with `data-testid="token-lane"`. Live/idle, the "agents live" count and the rate dash read `AgentFlow.active` verbatim; never derive them from `series` (common.md section 4).
2. **Mount** it in `stage-sections.tsx` `StageSectionGrid` after the `stageSections` cards and immediately before `FailureSection`, wrapped exactly like `FailureSection` (`cn("md:col-span-2", wide && "xl:col-span-3")`). `ReviewNotes` (and, with the operator's work committed, `AcceptanceList`) render first in the grid, so "before ReviewNotes" is not a slot. Both consumers get it: the modal and `routes/stage.tsx`.
3. **Status line rate** (`stage-heading.tsx` `StateLine`): when the stage has a flow and is executing, append "{rate}/m fresh" in mono after the time.
4. **Footer time left** (`quota-meters.tsx`):
   - `QuotaMeters` receives only `snapshot: QuotaSnapshot` (`status.quota`), not the whole status. Its only caller is `Footer` in `routes/shell.tsx`, which passes `snapshot={quota}`. Add a `quotaLeft: readonly QuotaLeft[]` prop to `QuotaMeters`, and pass `status.tokens?.quota_left ?? []` from `Footer`.
   - In `QuotaGauge` for the five-hour window, when `quotaLeft` has an entry for that provider and `quotaLeftText` returns text, render it after the reset text in its tone class.
   - Render it whatever the age of the quota reading, even when the reset text says "now": the route-level footer clocks real time (`useNow`) against a fixture dated 2026-09-04 (`quota-meters.test.tsx` notes this), and the footer contracts do not fake timers.
   - Its title is "At the last 30 min average"; it hides below 900px wide, like the reset text. The gauge is `role="img"`, so assistive technology does not read its child text: add the time-left text to the gauge's `aria-label` too.
   - Nothing changes for providers without an entry, so Codex's 5h dash stays as it is.
5. **Tests:**
   - three lanes for the fixture's server stage, one with the codex marker;
   - no section for a stage without a flow;
   - the footer shows "~2h07m left" for the fixture;
   - "lasts to reset" when the fixture is modified so `lasts_to_reset` is true and `minutes_left` is null;
   - nothing when `quota_left` is empty.

## Check

From `web/`: `bun run typecheck`, `bun run lint`, `bun run format` then `bun run format:check`, and `bun run test`.
