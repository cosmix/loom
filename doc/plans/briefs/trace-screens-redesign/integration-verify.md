# Integration verification: planning trace UI

Read `design.md` first. Work in this order.

1. **Build.** From the worktree root: `(cd web && bun run build) && (cd loom && cargo build)`.
2. **Diagnostic check.** Run `loom check integration-verify` and read every failure before fixing.
3. **Browser preflight.** Run `scripts/plan-screens.sh --browser=firefox loom/target/debug/loom
   "$LOOM_SCRATCH_DIR/preflight" /`. If Firefox cannot launch, rerun with Chromium and record the
   gap with `loom memory note`. If both fail, run `loom stage block integration-verify
   "<preflight output>"`.
4. **Screen walk.** Shoot every route V1 and V2 shot (see `trace-ui/v-look.md`) in Firefox at 1440
   and 390, in Ledger and Walnut. Check each shot against the matching approved screenshot in
   `doc/plans/mockups/trace-screens-redesign/` and design.md, and record one `loom memory note
   "req-walk <screen>: done|not done: <evidence>"` per screen. Check:
   - **Spine:**
     - one glyph per row
     - an amber glyph on a changed document
     - the roundel on a running plan
     - short slugs
     - no Coverage or Findings rows
     - the head sentence and its button
   - **Heading:**
     - the title in Inter
     - the scope standfirst in the reading face
     - chips on one line
     - Edit · Discuss · settle
     - the settle menu
     - the confirm popover, and the bottom sheet at 390
   - **Plan page:** the pressure card, What it implements, Briefs.
   - **Opened passage:** the Covered by line.
   - **Turn footer:** "Now out of step with what you settled".
   - **Coverage:**
     - the tiles filter
     - aligned columns
     - every row expands
     - tiles 2×2 at 390
   - **Findings:**
     - the run groups
     - cards
     - the fold picker
     - the Dismiss reason gate
     - decided lines
     - Review the feature
   - **Everywhere:** no title or heading in a serif. Check computed `font-family` with the
     Playwright driver on each screen's h1, h2 and h3.
5. **Live settle walk.** In a scratch clone under `$LOOM_SCRATCH_DIR`, with a throwaway Playwright
   driver against the built `loom plan`:
   - Mark settled on a section.
   - Edit it: the spine row turns amber, and the head names it.
   - Use See what changed.
   - Settle a section holding `(My call.)`: the popover quotes it.
6. **Fixes.** Rebuild `web/dist` and the binary after each fix. Visual fixes go to a fresh
   `loom-senior-software-engineer` (opus) that invokes `frontend-design:frontend-design` first;
   other fixes go to the cheapest capable tier.
7. **Final check.** The full `loom check integration-verify` against the rebuilt bytes is the
   acceptance.
8. **Final review round.** Run parallel `loom-code-reviewer` subagents on:
   - (a) web state and models: settle menu keyboard access, focus return, the confirm resend, and
     text rendering of untrusted strings
   - (b) the pages' accessibility: tile `aria-pressed`, row `aria-expanded`, arrow keys, and names
   - (c) CSS against design.md "Look" and DESIGN.md

   Fix or dispute every finding.
9. **Commits.** Code fixes first, then `web/dist` last as `chore(web)`.
