# O2: two-level summaries "seem to be gone" (#18)

Tier: opus (`loom-senior-software-engineer`). Read `../common.md` first. Knowledge: `doc/loom/knowledge/architecture/plan-server.md` (sections "Page route and page model", "Open-state rule", "Reading column, focus and keys") and `architecture/planner-web-shell.md` ("Reading column").

## What is known

- The data still has three levels. `doc/loom/summaries/overview/feat-planner/product.md` holds 46 citations of member sentences (`#s-xxxxxxxx`). `overview/feat-planner/engineering.md` holds 43 and `overview/optim-token/engineering.md` holds 14. `loom plan summary status` reports every summary current. No commit since 2026-10-01 removed level code (`git log` on `loom/src/plan_server/concept`, `web/src/plan/concept`, `loom/src/planning/summary`).
- Page model: `loom/src/plan_server/concept/body.rs` `citations()` (~L116-133) builds `Citation::Sentence` and drops one when `summary_source` or `doc_link` fails for it (the cited document is gone), which collapses the middle level; `overview_level` (~L153-182) consumes the citations.
- Web: `web/src/plan/concept/column-model.ts` `runFor` gives level 0 to an overview sentence and level 1 to a cited sentence. A level-1 sentence opens passages at level 2. `cited-pages.ts` prefetches the cited summary pages. An opened run reads "Loading" until every cited page arrives (`page-state.ts`, `loadConceptPages` commits through `Promise.all`).
- A document-summary page has only two levels: its sentences open straight to the source passage. Only an overview opens through all three.
- Item #1 (fixed in another stage) makes many overview sentences open to a near-copy of themselves. That alone can read as "one level".

## Task

1. Reproduce in a real browser. The main agent builds the bundle and the binary before spawning you; you may run the built `loom/target/debug/loom` and `node`, but not `cargo build` or `bun run build`. Start a server on a scratch clone the way `scripts/plan-screens.sh` does (its server-start block, about lines 57-80) and drive it with a throwaway Playwright script written under `$LOOM_SCRATCH_DIR`, never in the repository:
   - loopback works only inside one Bash call, so the server start, the wait for its `http://...?token=` line and the `node` run of your script are ONE Bash call;
   - your script has no `node_modules` beside it, so import `playwright-core` by absolute path: `import { chromium } from "<worktree>/web/node_modules/playwright-core/index.mjs";`;
   - launch with `chromium.launch({ args: ["--no-sandbox"] })`, as `web/scripts/plan-screens.mjs` does.
   Do this on `/f/feat-planner/product` and `/f/feat-planner/engineering`:
   - Open the overview sentence at `overview/feat-planner/product.md:55` and at least three others whose member sentence differs from them.
   - Then open the nested member sentence.
   - Screenshot each step.
   - Repeat after a reload and after "Open all".
   - Check that the nested level appears, that it differs visually from the source passage level, and that it does not stay at "Loading".
2. Find the cause of every case where the middle level is missing, unreachable, or indistinguishable. Check at least these candidates:
   - a dropped `Citation::Sentence`;
   - a cited page that never loads, or an id mismatch between the overview link and the member page's sentence ids;
   - Open all opening every level at once so the levels flatten;
   - a run style that draws a level-1 run exactly like a level-2 passage.
3. If you find a defect, fix it at its owner and add a regression test that fails on the code at the stage's base. Name the wrong behaviour it fails on.
4. If every overview sentence opens to its member sentence and then to source, and the levels are told apart, change nothing. Report the screenshots as evidence that #18 is the #1 restatement effect.
5. Record the outcome either way with `loom memory note "o2-#18: <reproduced: cause | is #1>: <routes, shot paths>"`. Integration-verify cites that memory in its #18 line.

## Files you own

- `web/src/plan/concept/column-model.ts`, `column-model.test.ts`, `cited-pages.ts`, `page-state.ts`, `run.tsx`, `reading-column.tsx`, `reading-column-runs.test.tsx`, `open-state.ts`, `open-state.test.ts`.
- `loom/src/plan_server/concept/**`.

`concept-runs.css`, `fold-handle.tsx`, `page-heading.tsx`, `open-all.ts`, `document-key.tsx`, `concept-page.css` and `page-header-extras.tsx` belong to V1, who works in parallel with you. If the fix needs one of those (for example a level-1 run styled exactly like a level-2 passage in `concept-runs.css`), stop and report the exact change; the main agent sends it to V1's refinement round.

## Proof

Run once at the end: `node_modules/.bin/vitest run src/plan/concept` from `web/`. If you changed Rust, also run `cargo test --lib plan_server::concept::` and `cargo test --test plan_concept_contracts` from `loom/`. Report the result lines, the screenshots' paths, and the cause.
