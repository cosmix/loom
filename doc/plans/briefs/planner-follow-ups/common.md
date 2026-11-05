# Planner follow-ups: shared context

Read this first, then your own brief. The plan (`doc/plans/PLAN-planner-follow-ups.md`) holds the requirements R1-R13 in its prose; its YAML is authoritative for gates and file ownership.

## What the user asked for (2026-11-05)

Part 1, feature follow-ups:

- R1-R2. Drop "Compare spec with build" (`Offer::RefineSpec`) and the `loom-plan:retro` skill. The user never asked for them.
- R3. One metaplan per feature, `doc/plans/<f>/index.md`. Metaplan rounds (`doc/plans/<f>/metaplan-<nn>.md`) go. "New metaplan round" becomes "Plan follow-ups": a metaplan session that appends entries under `## Follow-ups` in the feature's `index.md`. The existing checklist then offers "Write plan <slug>" for each new entry.
- R4. Integration-verify reads the product sections the plan's `Implements:` line names, fixes every gap between spec and build, and may correct the spec when the build is right. The run review gains `## Open Findings`.
- R5. A finished feature's landing view shows a Built state: done plans, spec-check outcomes, candidate follow-ups from the open findings, and free text.

Part 2, the user's polish review:

- R7. Spine: the done glyph and the pressure-tested glyph light up green.
- R8. Summary sentences that bring in the same passage (a table, a section) merge into one highlighted fragment and open once. Plan summaries get two levels: initial summary, expanded summary, then source passage with a link.
- R9. Themes: Ledger (light, amber accent), Walnut (dark, orange accent, the default dark), Graphite (dark, orange accent). Aubergine and Pacific go.
- R10. Settings: pictures for the reading sizes, the preview as a framed specimen, a redesigned statistics block.
- R11. Fold chevrons without squares; a redesigned Add to chat control.
- R12. `/new` follows the approved mockup `doc/plans/mockups/new/alt-a-threshold.html`, with a follow-up kind and a feature picker.
- R13. The chat follows the approved mockup `doc/plans/mockups/chat/alt-d-hearth.html`. New chat starts no session until the user sends. No ⋯ menu, no idle "Waiting for you". Ending still writes the decisions, through a designed ending that shows no internal prompt or token counts.

## User decisions (do not reopen)

- Integration-verify may edit the product spec when the build is right and the spec is wrong.
- One metaplan per feature; follow-ups append to `index.md`; no rounds.
- The run reviews' open findings are the default suggested follow-ups.
- Plan summaries have two levels before the source. All adjacent summary content that brings in the same passage is one highlighted fragment.
- Ledger keeps amber; Walnut and Graphite use orange. Walnut replaces Aubergine and Pacific.
- Chat: Hearth (alternative D). The busy roundel is an activity signal, never an avatar: at most one per pane, on what is working now, none when nothing works. The mockup's per-message roundels and 42px avatar column are rejected.
- `/new`: Threshold (alternative A). After Send the doodle is removed: the mockup's shrunken corner doodle that animates on hover is not built.
- Ending a chat writes settled decisions into the documents only when the user sent at least one message.
- Lanes: Claude only. Fable for the visual work, always loading `frontend-design:frontend-design` first; sonnet and haiku for behaviour; opus for the summary generator and the integration-verify spec check.

## Mockups are specifications

`doc/plans/mockups/chat/alt-d-hearth.html` and `doc/plans/mockups/new/alt-a-threshold.html` are the visual specifications for R13 and R12. Open them in a browser (serve the folder over loopback; `file://` is blocked for Playwright) and match them state by state, except where the decisions above override them. Their CSS is a reference: build with the planner's tokens and the real aurora-ui components (`web/src/aurora-ui/feedback/BusyRoundel`, `HazardPanel`, `HazardRail`, `hazard.css`), never by pasting the mockup's CSS. The mockups' JetBrains Mono stands in for the planner's mono face.

## Design rules

- `web/src/plan/ui/DESIGN.md` is the design record. Read it before touching layout, type or colour. Theme tokens only, no colour literal outside `tokens.css`. Every surface must read in Ledger, Walnut and Graphite. 390px must not scroll sideways.
- `frontend-design:frontend-design` is loaded first by every visual worker. Its general advice does not override DESIGN.md, the mockups or the brief's fixed targets.
- Visual requests are proven by screenshots, not only by DOM tests (`doc/loom/knowledge/mistakes/planner-visual-feedback-shortfall.md`). A visual stage's main agent shoots the changed screens in Ledger and every dark theme and compares them with the brief's targets and the mockups before the final review round.

## Visual work and the refinement loop

- Subagents cannot build the bundle or the binary (`loom-hooks/subagent-verify-guard.sh`; the server embeds `web/dist`). The stage's main agent builds and shoots with `scripts/plan-screens.sh --worktree-doc --themes=<ids> loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens" <routes>` (1440x900 and 390x844). States the script cannot reach use a throwaway Playwright driver under `$LOOM_SCRATCH_DIR` (plan Environment inventory).
- A visual miss goes to ONE fresh fable worker for the area it belongs to (`loom-senior-software-engineer`, model fable), which loads `frontend-design:frontend-design` first, with the shot paths and the miss list. After a second miss on one item, `loom-advisor` diagnoses first, then one more fable round. An item still missing is recorded with `loom memory note "visual-miss: ..."` for integration-verify.

## Code and test rules (from `doc/loom/knowledge/conventions/plan-web.md`)

- Server and model strings render through `<Untrusted>`. Storage keys go through `storageKey`.
- jest-dom is not installed. A route-level test goes through `renderAt` (`concept/concept-route-support.tsx`). Chat frames are seeded through the `FakeSocket` seam. Stub `matchMedia` for wide layouts. Read CSS through `@/...css?raw`. Contract tests are top-level `it()` calls named exactly as the contract's `test`.
- Help scenes are pinned by selector (`help/chat-scene.test.tsx` requires each pin target to match exactly one element). A worker that moves a pinned element updates the scene's selector and its test.
- Limits: 400 lines per file, 50 per function. Split before crossing either. `cargo test --test maintainability` enforces the Rust limit against `loom/maintainability-baseline.txt`, a ratchet file: never raise it.
- `cargo test` takes ONE test-name filter. Run several modules as `cargo test --manifest-path loom/Cargo.toml --lib -- a:: b::`.
- No backwards compatibility or migration code: the project is unreleased.
- When a test pins behaviour this plan changes on purpose, update it in the same change and name it in your report.
- A change to a shared component breaks tests outside your directory. Before you finish, `rg` for importers and for every class, name and text you change, and report each hit outside your files.

## Worker rules

- Subagents write code and tests and report. They run no git command and no `bun run build`, `cargo build` or `tsc` (the verify guard blocks them). They run only the focused test command their brief names, once, at the end.
- Record a mistake, a decision or a surprise with `loom memory note "..."` or `loom memory decision "..."` as it happens. Never use Claude Code auto-memory.
- No mention of Claude, Claude Code or any AI system in code, comments or docs.
- Report: files changed, tests run with their result, every pinned test you changed and why, and anything you could not settle.

## Frozen contracts that bind this plan

Never edit a `*.contracts.test.tsx`, a Rust `*contract_tests.rs` or a `loom/tests/*_contracts.rs` file (or a module under `loom/tests/*_contracts/`), unless the planned-edit tables below name the edit and your brief gives it to you.

### follow-ups-core

Every `*.contracts.test.tsx`, `loom/tests/*_contracts.rs` (and their `#[path]` modules) and `*contract_tests.rs` file stays unedited except for the rows below. The ones this stage's changes reach, and which keep holding unedited: `loom/tests/planning_summary_contracts.rs` (`demo/metaplan/index` is still the metaplan id), `loom/tests/plan_review_contracts.rs` (its `review_doc` fixture keeps the `**Generated:**` line `plan_server::review::built::parse_generated` reads, and `fs::plan_review` keeps writing it), `loom/src/plan_server/concept/metaplan_tests.rs` is not a contract file and is unaffected (`index` stem). The stage's own new contracts: `loom/tests/follow_ups_contracts.rs`, `loom/tests/spec_check_contracts.rs`, `web/src/plan/chat/follow-ups.contracts.test.tsx`.

### planner-visual

| File | Test | What it requires of this stage |
| --- | --- | --- |
| `web/src/plan/ui/polish.contracts.test.tsx` | the Graphite accent is a bright colour | Graphite's block keeps a selector containing `[data-dark-theme="gray"]` and an OKLCH `--primary` with chroma at least 0.12 (orange, 0.15, passes) |
| `web/src/plan/summaries/contracts.test.tsx` | panel shows running rebuild, last cost, totals and quota | `Rebuilding 1 of 3`, the running ids and the failure text; the last rebuild's `fresh` / `cache reads` / `output` labels before their figures in one smallest element; `unknown` and no standalone `0` when output is absent; `today`, `7 day`, `12,400`, `74,600` in the panel text; no limit meters |
| `web/src/plan/summaries/contracts.test.tsx` | panel shows the read-only reason instead of controls | no enabled `/rebuild/i` button on a read-only panel (Retry is named Retry and hidden while read-only) |
| `web/src/plan/settings/polish.contracts.test.tsx` | Settings shows the summary statistics and no provider limits | region "Token totals" in the dialog; no limit meter |
| `web/src/plan/header/popover-stats.contracts.test.tsx` | the activity popover shows no limits and no token totals | `RebuildActivity` (shared with the popover) draws no "Token totals" region |
| `web/src/plan/files/files.contracts.test.tsx` | (AddToChat with a registered secondary, no author rights) | the secondary renders alone; no "Add to chat" button |
| `web/src/plan/concept/reading-fixes.contracts.test.tsx` | an open block draws a 2px edge in its document hue | `.concept-passages` keeps `border-inline-start: 2px solid var(--run-colour)`; no `--run-edge` start border (V1 edits only the cap rules of `concept-runs.css`) |
| `web/src/plan/feature/spine.contracts.test.tsx`, `web/src/plan/feature/polish.contracts.test.tsx` | spine rows and plan-row pressure record | link names and texts unchanged ("Pressure-tested 2026-10-20", "Not pressure-tested"); `data-state` is an added attribute only |

### summary-fragments

- Rust: `loom/tests/planning_summary_contracts.rs`, `planning_summary_cli_contracts.rs`, `planning_summary_session_cli.rs`, `plan_concept_contracts.rs` (with `plan_concept_contracts/support.rs`), `summary_engine_contracts.rs`, `plan_summary_rebuild_contracts.rs`, `planning_guidance_contracts.rs` (reads `agents/summary.md` frontmatter: unchanged), and the lib contracts `loom/src/planning/summary/seal/two_member_contract_tests.rs` (green unchanged: the exemption now counts citable sentences, identical for section members), `seal/restate_contract_tests.rs`, `store/metaplan_contract_tests.rs` and `store/vision_contract_tests.rs` (untouched).
- Web: `web/src/plan/concept/concept.contracts.test.tsx` (its section-summary test queries the merged "Each stage names the files" fragment by the leading regex it already uses), `concept.contracts-runs.test.tsx`, `reading.contracts.test.tsx`, `reading-fixes.contracts.test.tsx` (Open all opens fragments; the overview's nested members cite different anchors and stay separate), `review-fixes.contracts.test.tsx`, the kits `concept.contracts-kit.tsx` and `reading-contracts-kit.tsx`, and `web/src/plan/feature/polish.contracts.test.tsx`, `spine.contracts.test.tsx`, `review-fixes.contracts.test.tsx`, `web/src/plan/chat/chat.contracts.test.tsx` (checked: no change needed; single-sentence fragments keep today's markup).

### chat-hearth

- `web/src/plan/chat/polish.contracts.test.tsx`: both tests, unedited. Its `vi.mock` factories fix the names the panel, head and composer may import from `./start-chat` (`NewChatButton`, `StartingBrief`), `./end-chat` (`EndChatMenuItem`, `useCanEndChat`), `./run-header` (`RunHeader`), `./transcript` (`Transcript`), `./attention` (`useAttention`) and `@/plan/review/changed-since` (`ChangedSinceControl`). "Stop sits in the composer's action row" needs Stop inside `.chat-composer-actions`.
- `web/src/plan/chat/chat.contracts.test.tsx`: unedited. The fold label "Used 3 tools · edited 2 files", the group "Turn 1 changes" with one Diff link and Restore all, the permission region "Permission: Bash" with buttons whose text is exactly "Allow" and "Deny" and "Waiting for Dana to decide", a Stop button matching `/^stop/i`, a Send button, the "Message" textbox, `+N more` chips.
- `web/src/plan/chat/context.contracts.test.tsx`: unedited. "Suggested prompts" list, `Viewing <name>`, Dismiss context; on a concluded chat exactly one button matches `/new chat/i`.
- `web/src/plan/chat/chat-fixes.contracts.test.tsx` "the composer has a visible border": `.chat-composer-box` keeps `border: 1px solid var(--…)`.
- `web/src/plan/chat/repair.contracts.test.tsx`, `review-fixes.contracts.test.tsx`: unedited (one permission card; `withoutKickoffFence` keeps hiding the kickoff block).
- `loom/tests/plan_session_bridge_runtime_contracts.rs`: unedited; it keeps passing because its shared request (support.rs, planned edit) opens with the user's own message.
- `loom/src/planning/guidance/tests.rs` `closing_message_matches_product_spec_with_continuation_indent` (not a contract file, pinned spec equality): kept, with its line range moved to the new block.

### built-and-new

Must pass unedited:

- `web/src/plan/new/review-fixes.contracts.test.tsx`: "New work shows an empty form beside an open draft" (textbox `/What do you want to build/`, "<name> is still open", link Continue), "the Name row renames the work" (Taking shape in the live kickoff), "Add plan opens New work with Standalone plan chosen" (radios `/Standalone plan/` and `/Feature/`, route state `{ small: true }`).
- `web/src/plan/feature/spine.contracts.test.tsx`, `feature/review-fixes.contracts.test.tsx`, the rest of `feature/polish.contracts.test.tsx` and `feature/pages.contracts.test.tsx`.
- `web/src/plan/home/features.contracts.test.tsx`, `home/drafts.contracts.test.tsx`, `home/review-fixes.contracts.test.tsx`.
- `web/src/plan/chat/*.contracts.test.tsx` (the offer handler gains an optional second argument only).
- `loom/tests/planning_guidance_contracts.rs` (offer lookup refusals), `loom/tests/plan_review_contracts.rs` (built notes from feature-home reviews, `REVIEW-IN_PROGRESS-` names), `loom/tests/plan_redesign_contracts.rs` (`checklist_has_no_title_and_six_steps`, `plan_rows_carry_pressure_date`), `loom/tests/plan_feature_card_contracts.rs`.

## Planned edits of frozen tests

Each is the only change its file gets for that test. The stage's main agent names every one in its `dispute-integrity` filing after the commit.

### follow-ups-core

| File | Test | Edit | Owner | Why |
| --- | --- | --- | --- | --- |
| `loom/tests/planning_guidance_contracts.rs` | `SKILLS` constant (used by `authoring_plugin_carries_skills_and_agents_only`) | drop `"retro"`, array length 10 to 9 | R1 | the retro skill is deleted (R2) |
| `loom/tests/planning_guidance_contracts.rs` | `compare_spec_with_build_keeps_retro_guidance` | delete the test | R1 | it pins the removed offer (R2) |
| `loom/tests/plan_redesign_contracts.rs` | `finished_feature_has_no_primary` | ids `["refine-spec", "new-metaplan-round"]` become `["plan-follow-ups"]` | R1 | the finished feature's sole secondary is Plan follow-ups (R3) |
| `loom/tests/planning_checklist_contracts.rs` | `checklist_state_rows` | `ReadyAllDone` secondary `["plan-follow-ups"]` | R2 | same (R3) |
| `loom/tests/planning_checklist_contracts.rs` | `metaplan_entry_any_spelling` | drop `round-2-metaplan.md`; its two entries move into `index.md` under `## Follow-ups` (5 and 6); `paths == [META]`; primary `WritePlan { metaplan: META, slug: "next" }`; slug list and line-9 problem assertions kept | R2 | one metaplan per feature (R3) |
| `loom/tests/planning_checklist_contracts/features.rs` | `complete_documents_have_no_draft_all_offer` | offers `["start-run"]` | R2 | `Ready` no longer offers Compare spec (R2) |
| `loom/tests/planning_documents_contracts/kinds.rs` | `plans_kinds_and_ids_in_standalone_and_feature_homes` | only `index.md` is a metaplan; `round-2-metaplan.md` asserts `DocKind::Other` | R2 | one metaplan per feature (R3) |
| `loom/tests/planning_documents_contracts/features.rs` | `misplaced_documents_need_features_and_offer_moves_without_a_fallback_feature` | drop `doc/plans/round-2-metaplan.md` from the misplaced list; assert it is in neither `needs-feature` nor `moves` | R2 | a top-level `*metaplan*.md` is `Other` (R3) |
| `loom/src/planning/summary/store/metaplan_contract_tests.rs` | `a_feature_metaplan_has_a_document_summary` | one metaplan id (`alpha/metaplan/index`); `summary_id_for(metaplan-02.md)` is `None`; `alpha/metaplan/metaplan-02` joins the rejected ids; module doc line | R2 | one metaplan id per feature (R3) |
| `web/src/plan/feature/pages.contracts.test.tsx` | `a finished feature's Run page holds the compare offer once` | renamed `a finished feature's Run page lists every plan and holds no follow-up offer`: secondary `[plan_follow_ups]`, no button containing `Plan follow-ups`, its handler never runs, both plan-row assertions kept | W1 | the Run page loses its follow-ups (R2, R5) |
| `web/src/plan/feature/polish.contracts.test.tsx` | fixture `finishedChecklist` (used by `a feature with a written spec opens on its product overview` and siblings) | secondary `[offer("plan_follow_ups", "Plan follow-ups")]` | W1 | the removed kinds no longer parse (R2, R3) |
| `web/src/plan/feature/spine.contracts.test.tsx` | fixture at ~138-147 (finished checklist) | same | W1 | same |
| `web/src/plan/feature/review-fixes.contracts.test.tsx` | fixture at ~58-67 | same | W1 | same |

The deletion of `plan-plugin/skills/retro/` and the retro and round tests lowers the Rust test-declaration and assertion counts (`TI-decl-rust`, `TI-assert-rust`); the main agent names them in the one `dispute-integrity` after the commit.

### planner-visual

None. Every frozen contract above holds with the briefs' settled names (Graphite keeps the id `gray`; the last rebuild keeps its labelled fresh / cache reads / output element; `RebuildActivity` keeps its regions and heading).

| File | Test | Edit | Owner | Why |
| --- | --- | --- | --- | --- |
| (none) | | | | |

Non-frozen tests the workers change on purpose (one `dispute-integrity` after the commit): `web/src/components/theme-picker.test.tsx`, `theme-picker-palette.test.tsx` (V1); `web/src/plan/ui/pacific-accent.test.tsx` deleted (V1); `web/src/plan/settings/settings-dialog.test.tsx`, `planner-swatch.test.tsx`, `web/src/plan/summaries/summary-panel.test.tsx` (V2).

### summary-fragments

| File | Test | Edit | Owner | Why |
| --- | --- | --- | --- | --- |
| `loom/tests/planning_summary_support/fixtures.rs` | `DRAFT_X` (used by `plan_fixture`, `plan_x`, `git_plan_x` and the CLI contracts) | `"# X\n\nX is planned. [d](#s-a1edcae5)\n\nX does x. [§](../../../plans/demo/PLAN-x.md#plan-x)\n"` | S1 | the seal refuses a one-level plan draft |
| `loom/tests/planning_summary_contracts.rs` | `plan_summary_cites_stage_anchor` | draft `"# X\n\nX opens with alpha. [d](#s-63eb2337)\n\nAlpha runs first. [§](../../../plans/demo/PLAN-x.md#stage:alpha)\n"`; `want` gains `("X opens with alpha.", "current")` first | S1 | same; the opening sentence is a second evaluated sentence |
| `loom/tests/planning_summary_contracts.rs` | `deleted_source_deletes_summary` | `y` = `"# Y\n\nY is planned. [d](#s-946cc169)\n\nY does y. [§](../../../plans/demo/PLAN-y.md#plan-y)\n"` | S1 | same |
| `loom/tests/plan_concept_contracts/support.rs` | `plan_draft` (used by `implements_not_stated`, `implements_broken_name_kept`, `unlisted_plans_listed`) | an opening line `Plan {slug} is outlined. [d](#{sid(&plan_sentence(slug))})`, a blank line, then the existing line | S1 | same; the overview in `unlisted_plans_listed` keeps citing `plan_sentence`, a detail sentence, under the single-citable-sentence exemption |

The ids are `sentence_id` values computed at authoring time (`printf '%s' '<text>' | sha256sum`, first 8 hex digits): "X does x." `a1edcae5`, "Alpha runs first." `63eb2337`, "Y does y." `946cc169`. Each new opening text was checked against `restate::restates` (Dice under 0.75 against its detail sentence). Pinned non-contract test, also reported in the integrity dispute: `web/src/plan/concept/fold-caps.test.tsx` "both caps of each opened run are named Fold and tabbable in reading order" queries the merged button by `new RegExp("^" + secondSentence)` instead of the exact name (W1).

### chat-hearth

| File | Test | Edit | Owner | Why |
| --- | --- | --- | --- | --- |
| `web/src/plan/chat/chat-fixes.contracts.test.tsx` | a live chat's menu offers End chat and not New chat | Delete the test and the imports only it used (`PanelHead`; `within` if unused) | N1 | R13 removes the menu; `live-chat-has-no-menu` replaces it |
| `web/src/plan/chat/chat-fixes.contracts.test.tsx` | an opening shows its label and hides the prompt until Show | `origin: "start"` becomes `origin: "offer"` in the `item(...)` call; assertions unchanged | N1 | `Origin::Start` and `"start"` leave the wire with `NoStep` |
| `loom/src/plan_session/transcript/paste_contract_tests.rs` | wrapped_opening_replaces_the_sent_item | `Origin::Start` becomes `Origin::Offer` (struct field and `let delivered`); assertion unchanged | B1 | the variant is removed |
| `loom/tests/planning_guidance_contracts.rs` | authoring_plugin_carries_skills_and_agents_only, brainstorming_copy_keeps_mit_notice, render_refuses_symlinked_plugin_dir | `AuthoringStart::NoStep` becomes `AuthoringStart::Message { paths: Vec::new() }` | B1 | the variant is removed |
| `loom/tests/plan_session_bridge_runtime_contracts/support.rs` | every runtime contract (shared request builder) | `opening_origin: Some(Origin::Start)` becomes `None`; unused import removed | B1 | the variant is removed; a `None` opening is the user's message, so the closing write still runs |

Non-frozen tests the stage deletes or re-asserts (one `dispute-integrity` after the commit): `chat/panel-head-menu.test.tsx` (deleted), the run-line tests in `chat/state-line-header.test.tsx`, `chat/rebuilt-summary-extras.test.tsx` (deleted), the "Thought for" cases in `chat/items.test.tsx` and `chat/transcript.test.tsx`, "closing origin labelled Closing message from loom plan" in `transcript.test.tsx`, the "fresh tokens" case in `turn-footer.test.tsx`, "a user message is set apart by its rule and face, not shrink-wrapped into a bubble" in `items.test.tsx`, `opening/tests.rs` `no_step`, `tests/starting.rs` `starting_point_without_documents_has_no_skill_and_reports_absence`, `tests_dispatch.rs` `start_without_offer_or_text_takes_authoring_and_marks_the_opening_as_start`, `model/tests.rs`'s `Origin::Start` row, and whatever the workers report.

### built-and-new

| File | Test | Edit | Owner | Why |
| --- | --- | --- | --- | --- |
| `web/src/plan/feature/polish.contracts.test.tsx` | a feature with a written spec opens on its product overview | `renderRoutes("/f/atlas", finishedChecklist())` becomes `renderRoutes("/f/atlas", pressureChecklist())` | S2 | An all-done feature now lands on `/f/atlas/built` (R5); `pressureChecklist()` has two written plans and the product spec, so the test keeps its subject. |
| `web/src/plan/feature/pages.contracts.test.tsx` | the test that expects a follow-up offer button on the Run page ("a finished feature's Run page holds the compare offer once" before follow-ups-core), if follow-ups-core kept it | delete it with the helpers and `registerOfferAction` entries only it used | S2 | R5 removes the Run page's follow-up section; the offer moves to the Built page. |
