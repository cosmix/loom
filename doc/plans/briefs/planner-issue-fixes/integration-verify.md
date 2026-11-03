# Integration verify: the 18 issues, checked in the running app

Read `common.md` first. You are the last gate. Tests passing does not mean the operator's complaint is gone. Check each issue in the running app.

## Order

Your session starts in `loom/` (the stage's `working_dir`). "From the worktree root" below means `cd "$(git rev-parse --show-toplevel)"` first; acceptance criteria run from `loom/` and reach the web package with `cd ../web`.

1. **Build.** From the worktree root: `(cd web && bun run build) && (cd loom && cargo build)`. The binary embeds `web/dist` at compile time. Building first means every later gate inspects the new bundle, never the restored one the web stages left.
2. **Diagnostic gate and preflight.** Run `loom check integration-verify`. It covers the full Rust suite, clippy, fmt, rustdoc with `-D warnings`, the web typecheck, lint, format, the full vitest run, the bundle rules and the dist freshness check, which passes now that step 1 rebuilt `web/dist`. This run is diagnostic: fix every failure through an engineer agent of the cheapest capable tier, rebuild (step 1) after each fix, and re-check only the failed criteria with `--criterion <n>`. Then run the browser preflight from the plan's Environment inventory (`scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/preflight" /` from the worktree root). If it fails, file `loom stage block integration-verify "<preflight output>"`: the screens and the visual walk lines cannot pass without a browser.
3. **Journeys.** `scripts/plan-journeys.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/journeys"`, WITHOUT `--strict`: `--strict` turns PENDING into a failure, and a journey that needs tmux prints PENDING in a stage sandbox. A FAIL still exits non-zero. Report every PENDING id in the issue walk, never pass them silently.
4. **Screens.** `scripts/plan-screens.sh --worktree-doc --themes=ledger,aubergine loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens"` over these routes:
   - `/`
   - `/f/feat-planner/product`
   - `/f/feat-planner/product/section_05`
   - `/f/feat-planner/engineering`
   - `/f/feat-planner/run`
   - `/f/optim-token/engineering`
   Open runs, the activity popover, the chat menu and the changed-since dialog with a throwaway Playwright driver under `$LOOM_SCRATCH_DIR`, run in one Bash call with its own server start. Import `playwright-core` by absolute path (`<worktree>/web/node_modules/playwright-core/index.mjs`) and launch with `args: ["--no-sandbox"]`. No repository script feeds chat frames: where a sandbox cannot start a session, stub `WebSocket` in `addInitScript` and replay frames from `web/src/plan/api/fixtures/chat-snapshot.json`.
5. **Issue walk.** Judge each item yourself, against the shots and the code. This is the verification the operator assigned to opus: do not delegate the judgement. For each of 1-18, write one line: fixed / not fixed / could not check here, with the evidence (shot path, test name or `file:line`), and record it with `loom memory note "issue-walk #N: <verdict>: <evidence>"`. The plan's "Issue to gate map" names each item's executable gate; cite it. An item you cannot show fixed is a defect to fix now, through a fresh engineer, with the evidence in the brief. A visual defect goes to a fresh `loom-senior-software-engineer` spawned with `effort: medium`. It invokes `frontend-design:frontend-design` first, is given the shot paths, and its result is shot again (common.md, "Visual work"). Concretely:
   - 1: `cargo test` output names the two restate contracts. Note that stored overviews change only on the next rebuild (operator check).
   - 2: the contract "an open block draws a 2px edge in its document hue"; caps on a muted chip and the edge on an open block, in both themes.
   - 3: a live chat's menu holds End chat only.
   - 4 and 5: the paste-wrapper contracts pass, and an opening shows its label only.
   - 6: the strip contract passes; the popover row reads "M of N stages merged" from the flag count.
   - 7: the collapsed rail shows the chat icon when idle; `chat-state-glyph.tsx` no longer names `BusyRoundel`.
   - 8: Open all turns into Fold all and back.
   - 9: the key takes at most two lines; status and Rebuild sit on one line.
   - 10: the header controls share one height and centre line, at 1440px and 390px.
   - 11: the contract "the planner logo matches the dashboard's sizes"; the logo is 56px at 1440px, as on the dashboard.
   - 12: rotating arrows for a running plan, and no play icon left: `rg -n '\bPlay\b|lucide-play|"play"' web/src/plan` prints nothing.
   - 13 and 14: one run row, controls-only footer, at 1440px and 390px; a running summary rebuild also shows once, and its row still opens the queue and errors; a stopped run keeps one dashboard line and lights no activity (V2's two named tests).
   - 15: the dialog's lede, the contracts "a viewed document's next diff starts from what was seen" and "seen text outlives a missing checkpoint commit", and O1's two named tests.
   - 16: B2's three named regression tests (`a_pressed_rebuild_reaches_the_spawner_within_one_tick`, `a_pressed_session_is_costed_as_button`, `a_press_during_a_running_session_joins_the_next_start`) in the `cargo test` output, and the `b2-#16:` memories with their fails-at-base lines and timeline.
   - 17: the contract "the composer has a visible border"; a visible bordered box in both themes.
   - 18: the `o2-#18:` memory with O2's evidence or fix.
   Record the three operator checks below with `loom memory note "issue-walk open: ..."`.
6. **Final build and gate.** Once the last fix has landed, rebuild `web/dist` and the binary (step 1), then run the full `loom check integration-verify` once more. This run is the stage's acceptance and inspects the exact bytes you commit. A fix after it needs another rebuild and full check.
7. **Review.** After the final gate, spawn parallel `loom-code-reviewer` subagents:
   - (a) the Rust changes: the paste unwrap's edge cases, the untrusted id and the transcript cap; the scheduler's start path and the settled attribution (any pressed or approved id makes the session `button`); the seal check;
   - (b) web security and state: `<Untrusted>` on server strings, seen-text storage through `storageKey`, wrapped storage access, caps and eviction, the storage-full banner, the reset generation guarding stale diff responses;
   - (c) dead code: `RunStrip`, the pulse CSS, `BusyRoundel` in the chat, the removed menu item, the now-unreached end-then-start branch of `beginChat`.
   Fix or dispute every finding. Weigh every pending reviewer suggestion. The review gate refuses completion when the tree changed after the round it records (`verify/review/gate.rs`, `covers_review`; only `*.md` prose is exempt), so a fix after a round needs step 6 again and another round.
8. **Commit.** Commit the code fixes, then `web/dist` last as `chore(web): rebuild the planner bundle`.

Record every discovery with `loom memory`, including stale knowledge (`loom memory note "stale-knowledge: ..."`). Expected stale sections: `architecture/planner-web-shell.md` ("Header and dock": the activity pill and the RunStrip row; "Chat home and the chat panel": the quartered roundel and the New chat menu item; the composer surface), `architecture/summary-store.md` (the overview model default), and `doc/product/feat-planner/section_10.md`'s changed-since passage if O1 did not rewrite it.

## What the operator checks after the run

Report these as open, never as passed:

- A real New chat in the operator's Claude Code shows one opening and no markup.
- A real Rebuild press starts within seconds.
- The next overview rebuild shows no restated sentence.
