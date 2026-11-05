# Planner polish: shared context

Read this first, then your own brief. The plan YAML (`doc/plans/PLAN-planner-polish.md`) is authoritative for gates and file ownership.

## The operator's review (2026-11-05), as numbered issues

1. The footer stacks the Claude and Codex limits one above the other. They must sit side by side, exactly as in the dashboard footer.
2. "Pressure test" in the spine only shows whether plans were pressure tested. Show that as a badge beside each plan instead, and drop the row.
3. After Stop in the chat, the transcript shows two lines, "Stopped" and "Interrupted". One is enough.
4. The chat looks unstyled:
   - the Send button and the Stop button are badly placed (Stop sits on its own row under Send, on the opposite side);
   - the "Waiting for you" state at the top;
   - the line "user authoring · 0 watching · 52.2k tokens", which wraps.
5. The chat's ellipsis menu shows End chat inside an odd container.
6. Summaries show no middle level: from the top-level summary, opening a sentence jumps straight to text that links to the file.
7. The metaplan has no expandable summary, only a river of text.
8. The chat sidebar must always show, unless the viewport is too narrow.
9. Every progress indicator uses the activity roundel from aurora-ui.
10. Sidebar text (the chat and the spine) is too small. Make it one step larger.
11. The Graphite theme's accent is too subtle. It needs a brighter accent that pairs well with graphite hues.
12. The summary statistics in the Settings dialog look poor.
13. Remove the Claude and Codex limit meters from the Settings dialog. Nobody asked for them there.
14. Update the help dialog and the documentation to match.
15. Adding references to a chat (select text, Add to chat) gives a messy composer. Each chip shows a book icon tile, a truncated quote with a lone opening quote mark and a ×, and its source on a second line. With "Now viewing section_03 ×" above them, three differently aligned rows stack inside the composer box, above the placeholder. The whole chat sidebar needs a revamp by a fable designer that uses `frontend-design:frontend-design`.

## Operator decisions (asked 2026-11-05; do not reopen)

- **Issue 6.** A feature opens on its product overview again, the page whose sentences open to member-summary sentences (two levels). Document summary pages (vision, sections, plans, metaplans) keep two levels: summary, then source. The stale stored engineering overview (12 of 28 sentences cite a single member sentence) is fixed by an operator rebuild after the run.
- **Issue 7.** Each metaplan gets its own document summary, and the metaplan route renders it like any other summary page.
- **Issue 2.** The Pressure page (`/f/<feature>/pressure`) is removed together with its spine row. Each plan row in the spine's Plans group carries the pressure record. A test still starts from the plan page's Pressure test button, the spine's next action, and the pressure dialog.
- **Lanes.** Claude only.
- **Issue 15.** The whole chat sidebar is redesigned by a fable worker with `frontend-design:frontend-design`: head, run line, transcript, notices, tool rows, context chip, reference and file chips, composer and its actions, and the menu. It absorbs issues 4, 5, 8's layout and the chat half of 10.
- **Models.** Sonnet and haiku wherever they can do the work. Fable only for the visual design, loaded with `frontend-design:frontend-design`: V1 (issues 1, 2's look, 9, the spine half of 10, 11) and V3 (the chat sidebar: 4, 5, 8's layout, the chat half of 10, 15). Opus at effort medium only for the Settings statistics (12, with 13), also with `frontend-design:frontend-design`. Opus or fable otherwise only after a cheaper tier failed.

## Plan decisions (settled by the plan author from the evidence)

- **D1, the Stop notice.** The browser's Stop sends `interrupt`. `Session::interrupt` (`loom/src/plan_session/bridge/commands.rs:316-338`) adds a "Stopped" server notice on the first press, through `TranscriptMapper::add_server_item` (`plan_session/transcript/mod.rs:196`). Claude Code then writes "[Request interrupted by user", and `interrupt()` (`plan_session/transcript/content.rs:121-143`) pushes a second notice, "Interrupted", into the same mapper's turn. Fix: `interrupt()` skips the notice push when the turn's last item is a `Notice` of kind `Stopped`. It still pushes the `RecordMark::Interrupted` mark, marks pending tools `NotFinished` and closes the turn. An interrupt typed in the tmux pane, with no Stopped notice before it, keeps its "Interrupted" line.
- **D2, metaplan summary ids.** Each metaplan file gets a document summary with id `<feature>/metaplan/<stem>`. `<stem>` is the file stem (`index` for `doc/plans/<feature>/index.md`, `metaplan-02` for `doc/plans/<feature>/metaplan-02.md`) and matches `[a-z0-9][a-z0-9-]*`. The summary file is `doc/loom/summaries/<feature>/metaplan/<stem>.md`.
  - The summary is a document summary. `sentence_limit` returns `MAX_DOCUMENT_SENTENCES` for it, and its sentences cite source passages of the metaplan only.
  - It is a member of neither overview. Like the vision summary, it sits in no `Scope` list. The engineering overview keeps citing the metaplans' raw passages and the plan summaries, so the overview seal rules (`seal/links.rs`, "every member and every metaplan") do not change.
  - Only a metaplan of a feature (`DocKind::Metaplan { feature: Some(_) }`) gets one. A misplaced top-level `doc/plans/index.md` gets none.
  - The concept page key equals the id. `PageKey::Summary` gains the scope `Metaplan` in `plan_server/concept/page_key.rs`, and the web `ConceptScope` gains `"metaplan"`.
- **D3, metaplan route.** `/f/<feature>/metaplan` renders the first metaplan's summary page through `ConceptRoute`, and `/f/<feature>/metaplan/<file>` renders the summary of the metaplan whose `metaplanFile(path)` equals `<file>` (the matching that `MetaplanPage` does today). The summary heading's Source link opens `/f/<feature>/source/plans/<feature>/<file>` (`featureSourceHref`). A metaplan whose summary is not built yet shows the existing "not built yet" page state with Rebuild, never the raw text.
- **D4, landing.** `landingPath(checklist)` (`web/src/plan/feature/landing.ts`) sends a feature with a product spec (`has_product`) to `product`, a feature with only metaplans to `engineering`, and anything else to `vision`, which offers to start one.
- **D5, pressure record on plan rows.** The spine model's plan child (`planChildren`, `feature/spine-model.ts:122-134`) carries the plan row's `pressure_date`. `ChildRow` (`feature/spine-row.tsx`) renders a badge `span.plan-spine__badge` after the state, reading "Pressure-tested <date>" or "Not pressure-tested", the same words as `pressureChip` (`files/heading-parts.tsx:86-96`). Its accessible text is part of the row link's name. A plan whose state is `retired` (`planning/slug.rs` `PlanState`) shows no badge; `written`, `running` and `done` do.
- **D6, Pressure group removed.** `GroupId` loses `"pressure"`. `spineGroups` draws five groups. The `pressure` route, `feature/pressure-page.tsx` and the `page === "pressure"` branch of `pageDrawsPrimary` go. The optional-pressure mark override (`markOf`, `spine-model.ts:163-170`) goes with the group. `pressure_test` offers are drawn by the plan page (`planPageDraws`) when the page is that plan, otherwise by the spine's next action.
- **D7, chat always shown.** No collapse. `panelCollapsedAtom`, `CollapsedRail`, the Collapse chat button and every `store.set(panelCollapsedAtom, false)` call go. The panel shows on every route that mounts one today (`hasChatPanel`: not `/` and not `/new`). `DESKTOP` (`chat/offer-flow.ts:17`) becomes `(min-width: 1280px)`, so the reading column keeps about 40rem beside the 18rem spine and the 21.25rem panel. Below it the panel is the sheet behind the header's Chat button, as today. The panel stays resizable (20-40rem).
- **D8, Stop placement.** Stop sits in `.chat-composer-actions`, immediately before the Send slot and at Send's size. It shows only while a turn runs, as today (`StopControl`). Send stays enabled while a turn runs, because a message sent then is queued.
- **D9, the chat menu.** The menu stays a `<details>` (frozen `chat/chat-fixes.contracts.test.tsx` uses `closest("details")`). Its pop takes the header popovers' surface (`header/header.css` `.plan-overflow`), and End chat is a full-width menu row, not a bordered button. Escape and a click outside close it.
- **D10, the roundel.** One roundel: `BusyRoundel` from `@/aurora-ui/feedback/BusyRoundel` (the one `chat/tool-row.tsx:28` uses), inside an `aria-hidden` wrapper, so the caller's own status text is the only announcement.
  - `ActivityGlyph` (`ui/activity-glyphs.tsx`) draws it for every kind that means work in progress (`starting`, `summaries`, `running`, `drafting`, `pressure`, `codex`) unless `paused`.
  - `permission` and `waiting` keep their glyphs: they mean "your turn", not progress.
  - `LoadState state="loading"` draws the roundel with its title in place of the skeleton, and keeps `role="status"` and its `aria-label`.
  - A new `BusyText` (`web/src/plan/ui/busy-text.tsx`, props `{ children: ReactNode; className?: string }`) puts the roundel before a text-only busy message. Each caller keeps its own role.
  - The pressure strip keeps its determinate bar (it carries the count).
- **D11, type step.** Every text size in the spine and the chat panel moves one step up the scale: `--type-fine` to `--type-ui`, `--type-ui` to `--type-body`, `--type-body` to `--type-title`. The hard-coded assistant prose size goes from 15px to 16px. The 13px floor holds. The kickoff transcript inherits the chat sizes. `ui/DESIGN.md` "Type" and "Page rhythm" state the new sizes.
- **D12, Graphite accent.** Only the Graphite theme block (`web/src/aurora-ui/shared/styles/tokens.css`, `.dark[data-dark-theme="gray"]`) changes: `--primary`, `--primary-foreground` and `--ring`. The new `--primary` has an oklch chroma of at least 0.12. `--primary-foreground` on `--primary` reaches 4.5:1 contrast. Ledger and Aubergine do not change. The token is shared, so the dashboard's Graphite theme changes too, which is intended. Operator override in integration-verify (2026-11-05): Pacific's `--primary`, `--ring`, `--sidebar-primary` and `--sidebar-ring` change too, to the violet `oklch(0.75 0.15 305)`, with `--primary-foreground` unchanged; the dashboard's Pacific theme changes with them.
- **D13, footer.** `.plan-footer` takes the dashboard footer's layout (`web/src/routes/shell.tsx:61-85`): a grid whose first track is `minmax(0, 1fr)` and holds `QuotaMeters`, so the meters' container queries measure a real width. Today the meters are a flex item with `container-type: inline-size` (`components/quota-meters.css:1-4`), which gives them no intrinsic width and selects the stacked layouts. `components/quota-meters.*` do not change.
- **D14, Settings.** `SummaryPanel` no longer renders `QuotaMeters` (it is mounted only by Settings' `SummariesGroup`). The limits live in the footer alone. The `SummariesGroup` hint stops naming provider limits.

## Design rules

- `web/src/plan/ui/DESIGN.md` is the design record. Read it before touching layout, type or colour. Theme tokens only, no colour literal outside `tokens.css`. One surface level. Status is a glyph and a word. 390px must not scroll sideways. Every surface must read in Ledger (light), Aubergine, Pacific and Graphite (dark).
- `frontend-design:frontend-design` is loaded first by every visual worker. Its general advice on fonts, palettes and textures does not override DESIGN.md. The brief's fixed targets and copy win over the skill.
- Deliberate exceptions go into DESIGN.md "Surfaces and borders". Its one owner in this plan is V1. Another worker reports each one as `exception: <surface>: <rule>: <why>`. V3 reports its chat type sizes and chat surface rules the same way, and the main agent passes them to V1 if V1 is still running, else writes them into DESIGN.md itself.

## Visual work and the refinement loop

- Subagents cannot build the bundle or the binary (`loom-hooks/subagent-verify-guard.sh`; the server embeds `web/dist`). The stage's main agent builds and shoots: `scripts/plan-screens.sh --worktree-doc --themes=ledger,graphite loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens" <routes>`, at 1440x900 and 390x844. It adds 1280px and 1180px shots of a feature page from a throwaway Playwright driver, to check the D7 breakpoint. States the script cannot reach use the same driver (plan Environment inventory).
- A visual miss goes to ONE fresh fable visual worker for the area it belongs to (V1's or V3's files) (`loom-senior-software-engineer`, model fable) that loads `frontend-design:frontend-design` first, with the shot paths and the miss list. After a second miss on one item, `loom-advisor` diagnoses first, then one more fable round. An item still missing is recorded with `loom memory note "visual-miss: …"` for integration-verify.

## Frozen contracts that bind this plan

Never edit a `*.contracts.test.tsx`, a Rust `*contract_tests.rs` or a `loom/tests/*_contracts.rs` file (or a module under `loom/tests/*_contracts/`), unless the table below names the edit and your brief gives it to you. These hold, and their accessible names survive every redesign:

- `chat/chat-fixes.contracts.test.tsx`: the chat menu is a `<details>` labelled "Chat menu" holding a button "End chat". The composer has a visible border (`.chat-composer-box` in `chat-composer.css` keeps `border: 1px solid var(--…)`).
- `chat/chat.contracts.test.tsx`: the fold label "Used 3 tools · edited 2 files", folded rows in the `<details>` DOM, the attachment cap, draft survival, and the changed-since button.
- `chat/context.contracts.test.tsx`: the group "Viewing <name>", the placeholder holding the name, and the button "Dismiss context".
- `shell-footer.contracts.test.tsx`: the footer shows Claude meters, the "Codex limits unavailable" note when Codex has no data, and no note when both report.
- `header/popover-stats.contracts.test.tsx`: the popover has no meters and no token totals.
- `concept/concept.contracts.test.tsx`, `concept/review-fixes.contracts.test.tsx`, `reading.contracts.test.tsx`: key chips, runs and passages.
- `feature/spine.contracts.test.tsx`: the Vision row links `/f/<f>/vision`, no screen draws one offer twice, and the header has no rail or view tabs.
- `loom/tests/planning_summary_contracts.rs` and the other summary contract targets: unchanged apart from the id lists and counts the new metaplan ids add (M1 below).

### Planned edits of frozen tests

Each is the only change its file gets. The stage's main agent names every one in its `dispute-integrity` filing after the commit.

| File | Test | Edit | Owner | Why |
| --- | --- | --- | --- | --- |
| `web/src/plan/feature/spine.contracts.test.tsx` | `SPINE_ROWS` | drop `/\bPressure test\b/` | W1 | D6, operator decision |
| `web/src/plan/feature/spine.contracts.test.tsx` | "a feature opens its vision" | delete the test (its successor is the contract `feature-opens-on-product-overview`) | W1 | D4, operator decision |
| `web/src/plan/feature/review-fixes.contracts.test.tsx` | "a feature with a written spec opens on its vision" | delete the test (same successor) | W1 | D4 |
| `web/src/plan/feature/pages.contracts.test.tsx` | "the Pressure page lists every plan with its record state", with `pressureChecklist` and the helpers only it uses | delete (successor: the contract `plan-rows-carry-the-pressure-record`) | W1 | D6 |
| `web/src/plan/summaries/contracts.test.tsx` | "panel shows running rebuild, last cost, totals and quota" | drop the three quota `getByRole("img", …)` assertions and add `expect(screen.queryByRole("img", { name: /window .*% used/ })).toBeNull()`; every other assertion stays | V2 | D14, issue 13 |
| Rust summary contract targets (`loom/tests/planning_summary_contracts.rs`, `planning_summary_cli_contracts.rs`, `summary_engine_contracts.rs`, `plan_concept_contracts.rs`, `plan_summary_rebuild_contracts.rs`) | any assertion that lists or counts expected summary ids over a fixture that has a metaplan | add the metaplan ids and update the counts, nothing else | M1 | D2 |

## Code and test rules (from `doc/loom/knowledge/conventions/plan-web.md`)

- Server and model strings render through `<Untrusted>`. Storage keys go through `storageKey`.
- jest-dom is not installed. A route-level test goes through `renderAt` (`concept/concept-route-support.tsx`). Chat frames are seeded through the `FakeSocket` seam. Stub `matchMedia` for wide layouts. Read CSS through `@/...css?raw`. Contract tests are top-level `it()` calls named exactly as the contract's `test`.
- Help scenes are pinned by selector (`help/chat-scene.test.tsx` requires each pin target to match exactly one element). A worker that moves a pinned element updates the scene's selector and its test.
- `chat.css` holds no `box-shadow`, `opacity` or `--tone-` (`panel-a11y.test.tsx`). New chat CSS goes in `chat-surface.css`, `chat-composer.css` or `chat-transcript.css`.
- Limits: 400 lines per file, 50 per function. Split before crossing either. Near the limit at base: `chat/chat-surface.css` 375, `chat/panel.tsx` 373, `chat/chat-transcript.css` 365, `chat/composer.tsx` 357, `loom/src/plan_session/bridge/commands.rs` 392, `summaries/contracts.test.tsx` 390. `cargo test --test maintainability` enforces the Rust limit against `loom/maintainability-baseline.txt`, a ratchet file: never raise it.
- `cargo test` takes ONE test-name filter. Run several modules as `cargo test --manifest-path loom/Cargo.toml --lib -- a:: b::`.
- When a test pins behaviour this plan changes on purpose, update it in the same change and name it in your report.
- A change to a shared component breaks tests outside your directory. Before you finish, `rg` for importers and for every class, name and text you change, and report each hit outside your files.

## Worker rules

- Subagents write code and tests and report. They run no git command and no `bun run build`, `cargo build` or `tsc` (the verify guard blocks them). They run only the focused test command their brief names, once, at the end.
- Record a mistake, a decision or a surprise with `loom memory note "…"` or `loom memory decision "…"` as it happens. Never use Claude Code auto-memory.
- No mention of Claude, Claude Code or any AI system in code, comments or docs.
- Report: files changed, tests run with their result, every pinned test you changed and why, and anything you could not settle.
