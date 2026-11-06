# Planner Polish Mistakes
>
> Polish

## The Footer Meters Stacked Because a Container-Query Component Sat in a Flex Item

**What happened:** the planner footer showed the Claude and Codex limit meters stacked, where the dashboard footer shows them side by side.

**Why:** `QuotaMeters` (`web/src/components/quota-meters.css`) sets `container-type: inline-size` and switches layout by container query (stacked under 680px). The dashboard places it in a `minmax(0, 1fr)` grid track (`web/src/routes/shell.tsx`); the planner made it a flex item of `.plan-footer`, where an inline-size container has no intrinsic width and so measures 0 and selects the stacked layout.

**Prevention:** place a container-query component in a sized grid track (`minmax(0, 1fr)`) or give it a `flex-basis`; never leave it as an unsized flex item. Copy the host layout of the screen that already renders it.

**Fix:** `.plan-footer` is a grid with a `minmax(0, 1fr)` track for the meters (`web/src/plan/ui/design.css`); `quota-meters.*` did not change.

## A Layout Fix Was Judged From a Stubbed Payload

**What happened:** an integration-verify walk marked the footer fix done from a screenshot built on a stubbed quota payload; the operator still saw stacked meters in the real app. The operator's installed binary also embedded the old committed `web/dist`, so part of the report came from the stale bundle ([the embed](../architecture/web-dashboard.md)).

**Why:** the stub's shape (two windows per provider, no notes) was invented by the driver, so the container width it produced was not the real one. A merged implementation branch shows the old UI until integration-verify commits the rebuilt bundle.

**Prevention:** judge a width-dependent layout fix with the real payload shape (fixtures or the live server) at the widths the operator uses, and read the container-query thresholds in the CSS. When an operator reports a fix as absent, check which `web/dist` their binary embeds before debugging the code.

## An Every-X-Uses-Y Issue Was Judged Fixed From One Surface

**What happened:** issue 9 (the roundel stands for every running or rebuilding glyph) was marked fixed from the activity popover shot while `PLAN_STATE_GLYPHS.running`, `SUMMARY_STATE_GLYPHS.rebuilding` and the compact summary trigger still drew a rotating `RefreshCw`. Five existing tests pinned an SVG or `.lucide-refresh-cw` for those states and had to be widened to `svg, .plan-roundel-glyph`.

**Why:** the walk checked the one surface the brief named instead of every implementation of the glyph.

**Prevention:** for an "every X uses Y" issue, `rg` every alternative (`RefreshCw`, `Loader`, spin keyframes) across `web/src/plan` before calling it fixed, and list the tests that pin the old shape in the worker's brief.

**Fix:** state glyphs route through `RoundelGlyph`; `rg RefreshCw web/src` finds nothing ([the glyph](../architecture/planner-web-shell.md#roundel-and-busy-text)).

## The Stop Notice Appeared Twice

**What happened:** pressing Stop in a chat drew "Stopped" and then "Interrupted" as two ruled breaks.

**Why:** two producers announced one event. `Session::interrupt` (`loom/src/plan_session/bridge/commands.rs`) adds the Stopped notice, then Claude Code's interrupt record makes `interrupt()` (`plan_session/transcript/content.rs`) push Interrupted into the same turn. A first fix that checked only the turn's last item missed the race where assistant text arrived between the two.

**Prevention:** when the server announces an action, the record that echoes it must not announce it again. Check the notice since the last user item, not the last item.

**Fix:** `interrupt()` adds nothing when the turn holds a Stopped notice after its last user item; `tests_server.rs` and `tests.rs` pin both the race and the later-user-item case. The fix pushed `interrupt()` past the 50-line limit, so a Rust worker's single check for an edited function includes `cargo test --test maintainability`.

## Spine Plan Rows Lost the Plan Name to Their Badges

**What happened:** plan rows in the 18rem spine showed the state and pressure words ("Done", "Not pressure-tested") and the slug was cut to "fe…". A second layout (words on a muted second line) still lost the name in the operator's view.

**Why:** the badges carried words beside their icons; a slug runs to about thirty characters and the words to about 16rem at 13px.

**Prevention:** a spine row's state and pressure record are icons only, named for assistive technology; the slug keeps the row's width. Judge row fixes at the real spine width with real slugs.

**Fix:** `spine-row.tsx` draws glyphs with the word as `.sr-only` and the wrapper's `title`; a note with no glyph keeps its word ([the row](../architecture/planner-feature-screens.md#spine)).

## Worker and Merge Slips

- A fix worker read files under the main-repo path instead of the worktree and saw stale code. Briefs give absolute worktree paths only and say never to read the main checkout.
- A new `const mark` in a `tree-entry.test.tsx` `it.each` body collided with the existing `mark` and vitest failed with PARSE_ERROR. Search the enclosing block for a name before declaring it.
- A journey step (`plan-journeys-reading.mjs`, J1) failed twice after the landing moved to `product`: at 390px `openRow` does not wait for React to commit a route, so a row was judged against the previous page, and the old landing had hidden the race. Seed `prev` with the landing's text.
- A merge that changed nothing under `web/` or `loom/` still moved the review fingerprint, because moving the merge base changed the stage diff for four `web/dist` assets and `DESIGN.md`; a fresh review round was needed.
- `loom check integration-verify` run with `run_in_background` was refused with "the request never reached the daemon" ([the relay](plan-server-delivery.md)). Run it in the foreground.

## A Long Chat Scrolled the Whole Page Through a Hidden Span

**What happened:** with a long chat, the dock panel, the kickoff page and the mobile sheet all scrolled as a whole instead of only the transcript (html scrollHeight 23435 against 900 at 1440x900).
**Why:** the working turn's sr-only "Working" span is `position: absolute`, and `.chat-transcript` was static, so the span's containing block was Transcript's relative root outside the scroller. It sat at its static offset at the end of the chat, unclipped. Visual passes checked layout in jsdom or on short transcripts, where nothing overflows.
**Prevention:** a scroller holding positioned content is itself positioned ([plan-web conventions](../conventions/plan-web.md)); check chat layout with a long transcript in a real browser.
**Fix:** `position: relative` on `.chat-transcript`, pinned by `chat/transcript-scroll.test.ts`.
