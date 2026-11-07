# Planner Follow Ups Delivery
>
> Slips

>

## A component that owns a live region was mounted a second time

**What happened**: the kickoff head reused `chat/state-line.tsx` `StateLine` for its state words. `StateLine` carries its own `sr-only` `role=status` region, so the kickoff page had two status regions and `kickoff-route.test`'s `getByRole('status')` found both.
**Why**: a component that owns a live region is a panel-level singleton; the page already mounted one.
**Prevention**: copy the words (six strings) instead of mounting a second region on a page that already has one.
**Fix**: `new/kickoff-head.tsx` draws the words itself with the same `chat-state-row` and `chat-state-words` classes.

## A test constant typed `as const` passed vitest and failed `tsc -b`

**What happened**: a fix worker typed a chat item test constant `as const` (a readonly `attachments: []`). Vitest accepted it and the worker reported green; `bun run build` then failed with TS2322.
**Why**: workers cannot run `tsc` (the verify guard) and vitest does not type-check.
**Prevention**: brief web test workers to annotate fixture items with the schema type (`const x: Item = {...}`), and run the build after any worker that edits tests.
**Fix**: `state-line.test.tsx` says `: Item`.

## A file-name classifier test used a name the generator never writes

**What happened**: `spine-model.ts` tested `PLAN_FILE` (`/^(?:[A-Z_]+-)?PLAN-/`) before `REVIEW_FILE`, so the generator's own review name (`REVIEW-<plan id>`) still fell in the Plans group. The worker's test used only a `REVIEW-DONE-` name, which `PLAN_FILE` happens not to match.
**Why**: the optional uppercase prefix in `PLAN_FILE` also matches `REVIEW-`.
**Prevention**: a test for a file-name classifier uses the exact name the generator writes (`loom/src/fs/plan_review/mod.rs` `REVIEW-{plan_id}.md`).
**Fix**: `REVIEW_FILE` is tested first, with a row for the generator's exact name.

## A fix round rewrote a call another stage's wiring check names

**What happened**: a fix commit in integration-verify rewrote `generate.rs`'s call as a bare `append_product_sections(` behind a `pub(super)` re-export. That broke follow-ups-core's literal wiring pattern `product_sections::append_product_sections(` and failed the aggregated wiring check at `loom stage complete`.
**Why**: the fix round ran `loom check integration-verify`, which does not re-run completed stages' wiring checks; only `loom stage complete` does.
**Prevention**: after editing a file another stage's wiring names, search the tree for every wiring pattern of the plan before completing. See [pinned literals and wiring](pinned-literals-ledgers-and-wiring.md).
**Fix**: `generate.rs` calls `product_sections::append_product_sections(` again; the re-export stays for `recovery.rs`.

## A container query was put on its own container

**What happened**: the statistics cards set `container-type` and the grid on one element (`.sp-cards`), so the `@container` rule that sets two columns never applied.
**Why**: a container query cannot style its own container.
**Prevention**: the grid goes on a child of the container. A screenshot at 1440px caught it; DOM tests did not.
**Fix**: the grid sits on a child of `.sp-cards` (`summaries/summary-panel.css`).

## A CSS reset lost to a :last-child rule and the mockup's two frames disagreed

**What happened**: on the `/new` foot, `[data-loom-plan] .kickoff-field__foot > :last-child { margin-inline-start: auto }` (0,3,0) beat the reset `.kickoff-field__reason + * { margin-inline-start: 0 }` (0,2,0), so both auto margins centred the reason mid-foot. Separately, the mockup `alt-a-threshold.html` drew glyphs on the starter pills in its wide state 1 frame and none in its narrow frame and state 3.
**Why**: `:last-child` weighs as a class. The mockup's frames are not one spec, so a worker copies whichever frame it reads.
**Prevention**: a reset names the same pseudo-classes as the rule it cancels. When mockup frames disagree, the brief settles the choice (here: no glyphs) and names the frame that wins.
**Fix**: the reset names `:last-child` too (`new/threshold.css`); starters are words alone.

## A screenshot driver cloned into the worktree root

**What happened**: a screenshot subagent whose first setup call was blocked ran `git clone` in the worktree root and left `clone/.git` there. `rm -rf clone` fails with `Read-only file system` because the session sandbox denies writes to any `.git` path, and `git status` does not list it.
**Why**: the clone ran in the current directory after the `cd` to the scratch path had failed.
**Prevention**: brief drivers to `cd` into an absolute scratch path in the same command as the clone (`cd "$S/x" && git clone ...`), so a failed `cd` stops the clone.
**Fix**: the operator removes `<worktree>/clone` outside the sandbox.

## A frozen contract named an id that a plan's truth check bans

**What happened**: the plan's `after_stage` truth check ("no trace of the Compare spec or round offers", `rg refine-spec loom/tests`) matched the frozen contract `loom/tests/follow_ups_contracts.rs`, which names `refine-spec` as a banned id. `after_stage` checks are not disputable through `dispute-criteria`.
**Why**: the contract and the check were written separately and both name the same literal.
**Prevention**: a plan that bans a literal everywhere builds that literal in its contracts by concatenation, or limits the check's path.
**Fix**: the contract was disputed and now builds the literal by concatenation.

## The stage text's scaffold order disagreed with rustfmt

**What happened**: the stage text told the implementer to place `#[cfg(test)] mod follow_up_contract_tests;` and `mod levels_contract_tests;` after the existing `mod tests;` lines. `cargo fmt` (`reorder_modules`) sorts `mod` items, so the format gate required the other order.
**Why**: the brief was written without running the formatter over its own scaffold lines.
**Prevention**: keep the formatter's order and say so in the memory; a brief that names a `mod` position states it as "sorted".
**Fix**: the scaffold lines sit where `cargo fmt` puts them.

## A socket test waited on a frame that never came

**What happened:** a trace-server test looped on `support::read_until` for a `trace_changed` frame that never arrived and hung.
**Why:** the planning server keeps sending other frames (summaries ticks, presence), so the 10 s read timeout never fires.
**Prevention:** wait for one specific WebSocket frame with an explicit `Instant` deadline over `socket.read()`, as `plan_server/trace/contract_tests.rs` `trace_changed_follows_a_heading_edit` does.
**Fix:** the test reads against its own deadline.

## Summary pages posted the summary store's hash to the settle route

**What happened:** the summary-page heading posted `ConceptPage.file_hash` to `/api/settle`; the route compares the source document's `normalized_hash`, so every settle from a summary page answered 409.
**Why:** the frozen web contract used one `HASH` constant for both, which hid the difference.
**Prevention:** the settle hash always comes from the opened source document (`useKeptDoc(path).doc.hash`); a contract fixture gives the summary hash and the document hash different values.
**Fix:** `concept/page-heading.tsx` posts the source document's hash.

## The Coverage page counted the section skill's own heading as uncovered

**What happened:** the Coverage page marked `## Out of scope for this section` as an uncovered heading on every feat-planner section, and journey J6 counted it 12 times.
**Why:** `claims.rs` excluded the anchors `open-questions` and `out-of-scope`, but `plan-plugin/skills/section/SKILL.md` mandates the heading `Out of scope for this section` (anchor `out-of-scope-for-this-section`).
**Prevention:** when a rule excludes a heading by anchor, `rg` the skills and templates that write headings for the anchor they actually produce.
**Fix:** `UNCOVERABLE` in `planning/trace/build/claims.rs` lists the three anchors.

## Plan this and Fold into entry stayed disabled until every plan was done

**What happened:** the Findings page gated both actions on the checklist's `plan_follow_ups` offer, which the checklist emits only at `ReadyAllDone`; the server accepts the start for any feature with a metaplan. While an entry's plan was unrun both buttons were disabled, and once all plans were done the Fold picker was empty.
**Why:** the page reused an offer whose availability follows the feature's run state, not the finding's.
**Prevention:** a control that acts on an item takes its availability from the action's server rule (`guidance/start.rs` `follow_up_start`), not from a checklist offer built for a different screen; the walk drives the control in each run state.
**Fix:** `followUpsOffer` (`trace/findings-model.ts`) uses the checklist offer when present and otherwise builds one from the trace for any feature with a metaplan.

## Two CSS rules that jsdom cannot see

**What happened:** the maps stylesheet `@import`ed its container-width steps at the top, so they cascaded before the base rules and lost on equal specificity: the focus panel never docked and the columns never stacked at 390px. A `@container` query on `.maps-finding` matched nothing because the element declaring `container-type` is never matched by its own query.
**Why:** an `@import` must precede every other rule, so an imported sheet always cascades first; a container query selects descendants of the container.
**Prevention:** the sheet holding the overrides is the page's entry and imports the base sheet; `container-type` sits on the parent and the child is queried ([rules](../conventions/plan-web.md#planning-trace-page-rules)). Check width steps in a real browser at 390px.
**Fix:** `maps.css` holds the steps and imports `maps-columns.css`; the list declares the container.

## Chip text at reduced opacity failed contrast

**What happened:** the edge chips' settle word in `--muted-foreground` under a group opacity of 55% measured 2.2 to 3.0:1; at 100% opacity the Ledger theme's value was 4.9:1.
**Why:** contrast was judged on the raw tokens, not on the composited colour.
**Prevention:** compute contrast on `alpha*top + (1-alpha)*page` in sRGB; a text run inside a dimmed pill keeps `--foreground` and the dot carries the tone.
**Fix:** resting opacity 0.7 (the Ledger label reads 5.65:1) and the chip's settle word uses the foreground; recorded in `ui/DESIGN.md`.

## A fix worker rewrote an existing assertion line

**What happened:** a fix worker implementing an optional reviewer suggestion rewrote an existing `expect` line in `findings-model.test.ts`; completion raised test-integrity event `TI-edit` and the suggestion was reverted.
**Why:** the brief said "never weaken an assertion" but not that an existing assertion line stays byte-identical.
**Prevention:** briefs say existing assertion lines stay byte-identical; a change that needs one edited is dropped and reported, not disputed. A formatter reflow of a merged line (`spine-model.test.ts`, one 101-character line split over five) also raises `TI-edit`; revert it only when the gate allows, else dispute it.
**Fix:** the suggestion was dropped.

## Test files grew past 400 lines and the size check missed them

**What happened:** earlier stages grew `concept/page-heading.test.tsx` (389 to 433 lines), `chat/turn-footer.test.tsx` (395 to 403) and `summaries/summary-status.test.tsx` (420 to 423) past the 400-line web rule.
**Why:** each stage's `wc -l` check named only files in its own list, and test files and files edited by another stage fell outside it.
**Prevention:** the last step's size check runs over every web file the stage touched, tests included, compared with the plan base.
**Fix:** the tests were split into sibling files.

## Editing a file while the code reviewer ran skipped its round

**What happened:** a stage's main agent edited a doc comment while `loom-code-reviewer` ran, twice; the round was dropped (`review_harvest_failed.worktree_changed`).
**Why:** `loom check` ran beside the review and its failures were fixed in the tree mid-review.
**Prevention:** run `loom check` to green before spawning the reviewer, and touch no file while a reviewer runs (the same cause as [editing while `loom check` ran](live-state-pollution.md#editing-while-loom-check-ran-and-a-backgrounded-check-that-never-relayed)).
**Fix:** the reviewer was spawned again on the settled tree.

## A sibling branch merged in deleted a file a finished stage had wired to

**What happened:** integration-verify failed the aggregated wiring check because `trace-server`'s wiring entry named the Built page's wire module, deleted by the Built-page removal that reached the plan through the feat-planner-ui merge after `trace-server` completed. Integration-verify could not dispute it: `dispute-criteria` indexes only the disputing stage's own wiring list, and a self-service request must own the stage.
**Why:** `verify/completion/mod.rs` re-runs every completed stage's wiring on the merged tree with no dispute path from integration-verify.
**Prevention:** after a plan-level merge of a sibling branch that deletes or moves files, `fd` each completed stage's wiring source before integration-verify starts. The repair is an operator amend of the owning stage's `wiring` field (delete the entry by index); see also [the third shape](pinned-literals-ledgers-and-wiring.md).
**Fix:** the operator removed the entry.

## Small slips that recurred in this plan

- A probe file was written to a path under `.worktrees/scratch/` instead of the session scratchpad; temp files go only under the scratchpad directory.
- A zsh variable holding several file paths does not word-split, so `oxlint` and `oxfmt` received one bogus path and reported no files; pass literal paths to the lint and format tools.
- A subagent brief that forbids `git add` and `git rm` still got `git mv` for a test-file rename; the rename staged. Rename with plain `mv` in subagent work ([the same slip](source-graph-delivery.md#a-worker-ran-git-mv-despite-the-subagent-git-restriction); a hook proposal is in [concerns](../concerns/planning-document-model-followups.md#planning-trace-limits)).
- A `loom check integration-verify` run with `run_in_background` was never relayed and the next call reported `the request never reached the daemon` (a recurrence of [the backgrounded check](live-state-pollution.md#editing-while-loom-check-ran-and-a-backgrounded-check-that-never-relayed); the hook proposal is in [planning-guidance-followups](../concerns/planning-guidance-followups.md)).
- Fact reports on a branch with uncommitted work say working tree versus HEAD: on `feat-planner-ui` about 94 web files were uncommitted while HEAD still had `NextAction` in `spine.tsx`.
