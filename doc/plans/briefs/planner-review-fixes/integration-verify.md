# Integration verify: the 15 review issues, checked in the running app

Read `common.md` first. You are the last gate, and passing tests do not prove the operator's complaint is gone. Check each issue in the running app.

## Order

Your session starts in `loom/`. "From the worktree root" means `cd "$(git rev-parse --show-toplevel)"` first.

1. **Build.** From the worktree root: `(cd web && bun run build) && (cd loom && cargo build)`. The binary embeds `web/dist`.
2. **Diagnostic gate and preflight.**
   - Run `loom check integration-verify`. Fix every failure through an engineer of the cheapest capable tier, rebuild after each fix (step 1), and re-check only the failed criteria with `--criterion <n>`.
   - Then run the browser preflight: from the worktree root, `scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/preflight" /`. If it fails, file `loom stage block integration-verify "<preflight output>"`.
3. **Journeys.** `scripts/plan-journeys.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/journeys"`, without `--strict`. List every PENDING id in the walk.
4. **Screens.** `scripts/plan-screens.sh --worktree-doc --themes=ledger,aubergine loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens"` over:
   - `/`
   - `/new`
   - `/f/feat-planner/vision`
   - `/f/feat-planner/product`
   - `/f/feat-planner/product/section_05`
   - `/f/feat-planner/source/product/feat-planner/section_05.md`
   - `/f/feat-planner/engineering`
   - `/changes`

   States the script cannot reach (an open chat with tool rows and chips, the kickoff conversation, the open activity popover, the switcher popover, the move dialog with two plans selected, the Settings Summaries section, a source page inside a feature with the chat's "Viewing" chip) go through a throwaway Playwright driver under `$LOOM_SCRATCH_DIR`, run in one Bash call with its own server start (plan Environment inventory). Where a session cannot start, stub `WebSocket` in `addInitScript` and replay frames built from `web/src/plan/api/fixtures/chat-snapshot.json`, `kickoff.json` and the `kickoff_drafts` fixture.
5. **Issue walk.** Judge each item yourself, against the shots and the code. For each of 1-15 write one line, "fixed", "not fixed" or "could not check here", with the evidence (shot path, test name or `file:line`). Record it with `loom memory note "issue-walk #N: <verdict>: <evidence>"`, citing the gate the plan's "Issue to gate map" names. An item you cannot show fixed is a defect to fix now: a visual defect goes to a fresh opus/medium visual worker under common.md's loop; anything else goes to the cheapest capable engineer.
6. **Final build and gate.** After the last fix, rebuild (step 1) and run the full `loom check integration-verify` once more. That run is the acceptance.
7. **Review.** After the final gate, spawn parallel `loom-code-reviewer` subagents:
   - (a) **Upload security.** The `/api/attachments` gate (415 and 413 before the body is read); magic-byte and UTF-8 checks; that the filename never reaches a path; the id grammar in `resolve`; the sandbox read-only grant on `<runtime>/attachments` and no write grant; the 30-day cleanup; that the client never sends a path; `<Untrusted>` on every server reason.
   - (b) **Drafts and transcript.** Draft file caps and atomic writes; session-id validation before a path; `live` on crash recovery; the rename override; the continuation opening's sanitizing; the two-item opening match and the `isMeta` drop; the doubled-send cause and its test.
   - (c) **Summaries.** The vision id through store, evaluate, seal, concept page, delta and key; the overview two-member rule and its exemption; prompts that agree with the seal.
   - (d) **Dead code.** `VisionPage`'s old raw-source branch (it stays as the route element that offers "start vision" or renders the summary), the dialog code of changed-since (`focusReadingRegion`, `onCloseAutoFocus`), `CHIPS_SHOWN`, the share tunnel, the switcher's Home and New work entries, the context line's old top placement. The Home tab stays (frozen `spine.contracts.test.tsx`).

   Fix or dispute every finding. The review gate refuses completion when the tree changed after its round, so a fix after a round needs step 6 again and another round.
8. **Commit.** The code fixes first, then `web/dist` last as `chore(web): rebuild the planner bundle`.

## Issue walk evidence to look for

| # | Evidence |
| --- | --- |
| 1 | Table, multi-select and Move of two plans in one message (contract `standalone-plans-move-together`); shots at 1440px and 390px. |
| 2 | /new shows the Feature or Standalone plan control. Add plan preselects Standalone plan (contract `add-plan-preselects-standalone`). |
| 3 | `/f/feat-planner` lands on `/vision` (contract `feature-opens-on-vision`). The vision page is a summary of at most 8 sentences. The stored vision summary exists only after an operator rebuild (operator check). |
| 4 | Key chips show titles and the word "changed" (contract `key-chip-shows-its-title`). |
| 5 | Contract `overview-cites-two-member-sentences`. Stored overviews change on the next rebuild (operator check). |
| 6 | Contract `source-keeps-the-spine`; journey J2; shot of the source route with the spine; the chat still shows "Viewing <file>" on that route. |
| 7 | Rail shot. Context chip in the composer. Contract `selection-chips-differ`. Add to chat opens the panel. |
| 8 | Contracts `upload-stores-by-content-and-refuses-a-lie` and `file-attachment-is-sent-by-id`. A real image read by a session is an operator check. |
| 9 | Contract `opening-splits-guidance-from-typed-text`, T1's `a_meta_skill_body_with_list_content_is_skipped`, and C1's Skill-row drop. Fold line shot. Type sizes. |
| 10 | Contracts `kickoff-fence-hidden` and `name-is-editable-while-taking-shape`. The test "one click on a shape choice sends one message", or T1's Rust regression test. |
| 11 | Contracts `drafts-survive-a-restart`, `new-work-starts-fresh-beside-an-open-draft` and `home-lists-unfinished-drafts`. The waiting row links. |
| 12 | Contract `changes-is-a-page`. Shot of `/changes` at 1440px. |
| 13 | Contracts `footer-shows-limits-and-unavailable`, `footer-hides-the-note-when-limits-exist` and `popover-has-no-limits-or-token-totals`. Settings shows the Summaries section. |
| 14 | Contract `share-shows-no-tunnel`. |
| 15 | Header shots against the dashboard's. The switcher lists features only. |

Record the operator checks below with `loom memory note "issue-walk open: …"`.

## What the operator checks after the run

Report these as open, never as passed:

- Rebuild summaries. The vision summary appears, and no overview sentence opens to a single member sentence.
- Attach a screenshot in a real chat. The session reads the image.
- Start a kickoff, leave, return from Home's Unfinished shelf, rename it, set it aside, continue it.
- Outside a stage, run `LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --manifest-path loom/Cargo.toml --test plan_session_confinement` so U1's probe `an_attachment_is_readable_and_not_writable` runs in a real sandbox.
