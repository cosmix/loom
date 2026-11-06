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
