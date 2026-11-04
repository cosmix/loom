# Planner review fixes: shared context

Every worker reads this file first, then its own brief. The plan is `doc/plans/PLAN-planner-review-fixes.md` (`doc/plans/IN_PROGRESS-PLAN-planner-review-fixes.md` once `loom run` has started). Its YAML is authoritative for gates and file ownership.

## The operator's review (2026-11-04), as numbered issues

1. **Standalone plans on Home.** The disclosure opens on a badly laid out list that shows about 24 plans. "Add plan" and "Move plan" look bad. Move plan lists every plan, takes one, and moves it to a feature. The operator wants a table with multi-select, so several plans move to a feature at once. Redesign from scratch.
2. **Add plan / New work.** Add plan opens "What do you want to build?". It is not clear whether you are starting a plan or a feature.
3. **Vision first.** Opening a feature lands on the product overview. It must land on the vision. The vision must have its own two-level summary that reads in thirty seconds to a minute: a few paragraphs.
4. **Sources key.** Coloured icons with a number and a dot sit above a product spec page. It is not clear what they mean or which document is which without hovering.
5. **Summary levels.** In some cases a first click on a summary sentence shows nearly the same text again. Only a second click shows longer text.
6. **Summary to source.** Following a link to the source opens the file view. The sidebar loses all connection to the feature, and the only way back is a small Summary button at the top of the file.
7. **Chat panel.**
   - The collapsed rail's icon is not inviting.
   - "About section 4" at the top of the panel is noise.
   - Each Add to chat selection adds a chip, and every chip reads "Summary · section_04".
   - Chat is poorly integrated and needs rethinking: how it renders, how it is presented and how it works.
8. **Attachments.** The paperclip, on New work and in chat, attaches only existing documents. A user needs to attach a screenshot, a sketch, an image or a transcript. The operator decided: **build it now**.
9. **Kickoff conversation.** Typing the first message changes the whole screen. "New work · Show" appears twice. Show reveals internal agent instructions. "Skill loom-plan:kickoff" and Bash rows show prominently. The type is too small.
10. **Taking shape panel.**
    - Its styling is poor.
    - Clicking "Make this a feature instead" shows that message twice.
    - The agent's name for the work cannot be edited.
    - Open questions show as a JSON object with `problem`, `decisions` and `questions` keys.
11. **Drafts.**
    - Leaving the kickoff loses it. It should appear as an unfinished item on Home, named "Untitled" with the date, or with the agent's name once it has one.
    - The activity popover says "waiting for you · loom plan: kickoff" but offers no way back.
    - New work reopens the unfinished kickoff. It must start a new one.
12. **Changed since you looked.** The modal dialog is so narrow that the diff is hard to read. It should be a proper, wide page.
13. **Limits and statistics.** The Claude and Codex limits are buried in the activity popover. Show them in a footer as the dashboard does, saying when a provider's limits are unavailable. The token statistics (today's fresh tokens, cache reads, output) do not belong in the popover.
14. **Share.** Remove the SSH tunnel line. Keep the URL.
15. **Header.**
    - The feature dropdown also holds Home and New work, which are not needed there.
    - The header should look as polished as the dashboard's.
    - The space between the logo and the header's bottom border is too tight.
    - The switcher should look better.

## Evidence (captured 2026-11-04 from the operator's kickoff session `529ae179…`)

The first user record of the kickoff is ONE wrapped item holding the guidance and the typed text:

```text
"\n\n<pasted_content id=\"f509\">\nloom plan: Kickoff.\nDiscuss the work and confirm its size and name before writing anything under doc/.\nLoad the skill loom-plan:kickoff, then begin by telling me what you read and what you plan to ask.\n\ni would like to add support for microvm test environment\n</pasted_content id=\"f509\">\n"
```

- The provider builds it in `loom/src/plan_server/guidance/provider.rs` as `format!("{generated_opening}\n\n{text}")`. The guidance comes from `planning/guidance/opening.rs` (`kickoff_opening`). The user's own sentence therefore sits behind Show (issue 9).
- The Skill tool's result is followed by a user record with `"isMeta": true` and a `sourceToolUseID`. Its content is a list holding one text part, "Base directory for this skill: …/plugin/skills/kickoff\n\n# Start new work…", which is the skill body. `plan_session/transcript/records.rs:193` already skips `isMeta` records, so the prominent "Skill loom-plan:kickoff" line is the Skill tool row (issue 9).
- The assistant ends each reply with a fenced `loom-kickoff` JSON block. The blocks are well formed: `questions` is an array of strings. The browser renders that fence as a code block inside the assistant's prose, so the operator saw the JSON in the transcript (issue 10). `plan_server/chat/kickoff.rs` parses the block, and nothing hides it from the transcript.
- "Make this a feature instead." is one plain user record (no wrapper). The doubled row therefore comes from the browser or from the sent item not being replaced (`plan_session/transcript/mod.rs::take_server_item`). Reproduce it before fixing it.
- No session, kickoff shape or draft outlives the session: `KickoffTracker` is in memory (`plan_server/chat/kickoff.rs`), and the pre-start draft is React state (`new/empty-kickoff.tsx`, pinned by "keeps the draft in memory only").

## Operator decisions (settled; do not reopen)

- Attachments beyond repository documents (images, sketches, PDFs, text transcripts) are built in this plan.
- Claude lanes only. Haiku and sonnet do most of the work. Opus at effort medium does every visual design, and invokes `frontend-design:frontend-design` first. Fable is used only after opus has failed twice on the same item, through `loom-advisor` (read-only diagnosis) first.

## Plan decisions (settled by the plan author from the evidence)

- **D1 Vision summary.** The vision document `doc/product/<f>/index.md` gets a document summary with id `<f>/product/vision`, stored at `doc/loom/summaries/<f>/product/vision.md`, with at most 8 sentences in at most 3 paragraphs. The 8-sentence cap is enforced by `format.rs` and stated by the rebuild brief through one `format::sentence_limit(id)`; the 3-paragraph limit is a prompt rule only (no parser counts paragraphs). It is built and rebuilt like a section summary. The vision id is in neither overview's members nor in `Scope.outside`. A feature with no vision keeps its "start vision" offer on `/f/<f>/vision`. The product overview keeps citing the vision's sections directly, so overview sealing is unchanged. `/f/<f>/vision` shows that summary in concept mode; Source opens the full vision.
- **D2 Landing.** `/f/<f>` opens `vision` whenever a vision exists, and `engineering` for a feature with only a metaplan.
- **D3 Overview levels.** An overview sentence that cites member sentences cites at least two of them. The seal rejects a sentence citing exactly one member sentence, unless that member's summary holds only one sentence. The overview prompt says the same. Stored overviews change on their next rebuild.
- **D4 Source stays in the feature.** A document that belongs to a feature (its vision, sections, metaplan or plans) opens its source at `/f/<f>/source/<repo path without doc/>`, inside `FeatureLayout`, so the spine stays and marks the document. Source buttons and passage chips on feature pages link there. `/files/*` stays the file browser. The heading's Summary/Source switch is a clear two-state control. The chat's viewed document maps `/f/<f>/source/<rel>` exactly as it maps `/files/<rel>` (`chat/viewed-document.ts`, kickoff stage), so "Viewing <name>" and the attachment on send survive the move.
- **D5 Key chips** show the document's short title, visibly, beside its glyph and number. "Changed" is written as a word, not a dot.
- **D6 Transcript.** One opening record becomes a collapsed guidance item plus a visible user item holding the typed text and attachments, each shown once. Records marked `isMeta` and Skill tool rows never render. Every run of tool rows folds into one quiet line while the turn runs and after it ends; errors and permission cards stay visible. The line keeps today's label form, "Used <n> tools · edited <m> files" (frozen `chat.contracts.test.tsx`), and the folded rows stay in the `<details>` DOM (frozen "chat expanded tool row"). `loom-kickoff` fences never render.
- **D7 Context** moves from the panel top into the composer, as a quiet "About <name>" chip that can be dismissed.
- **D8 Selection chips** read as the quoted start of the selection (about 48 characters, then an ellipsis), with the source as secondary text. Add to chat opens the panel, or the sheet on narrow screens.
- **D9 Uploads.** `POST /api/attachments` accepts a raw body:
  - Role: `HolderOrTake`. Attaching happens before Start on a fresh New work page, when nobody holds authoring yet; `Holder` would refuse that first upload with `NoHolder`. Another browser, read-only mode and `stopping` are still refused.
  - Media types: `image/png`, `image/jpeg`, `image/gif`, `image/webp`, `application/pdf`, `text/plain`, `text/markdown`.
  - Size: at most 10 MiB.
  - Checks: magic bytes for images and PDF, UTF-8 for text.
  - Storage: `<runtime>/attachments/<first 16 hex of sha256>.<ext>`, created 0700 through `safe_fs`. The display name comes from the `name` query parameter (`POST /api/attachments?name=<percent-encoded>`, read with `QueryParams::get`); it is sanitized and never used in a path. No `RequestHead` field is added.
  - Store cap: the directory holds at most 256 MiB; an upload past it is refused with 507 and a one-line reason.
  - Response: `{id, name, media_type, bytes}`.
  - A new attachment kind `file {id, name}` resolves server-side, from id to absolute path. An unknown id is refused, and the client never sends a path.
  - The delivered block names the absolute path, media type, size and name, and the session reads the file with Read. The transcript chip carries `{id, name, media_type, bytes}`, never the absolute path. The planning session sandbox gains a read-only grant on `<runtime>/attachments`, created before every launch by `ensure_runtime_grants`.
  - Files older than 30 days are removed by `uploads::install(ctx)`, one line in `routes::install_all`, so it runs at every server start. Re-uploading a stored file refreshes its `stored_at` (`uploads::touch`), and so does every write of a kickoff draft that references the file (D10), so an unfinished draft's attachments live as long as the draft.
- **D10 Drafts.**
  - Each kickoff persists `<runtime>/kickoffs/<session id>.json` holding: `session`, `started_at`, `updated_at`, `messages` (the user's typed texts, at most 40 of 4 KiB each), `shape` (last agent `KickoffShape`, without the override), `attachments` (the start command's attachment inputs, at most 10, `[]` when absent on read), `name_override`, `target` (the confirmed document, set by `confirm_kickoff`, else null) and `live`.
  - The file is written at start, on every shape change, on every user send, on rename and on confirm.
  - `live` in every view is computed when read: the stored flag AND the bridge's current chat session is this one. After a restart every draft therefore reads `live: false`. On `BridgeEvent::Ended` the file is rewritten with `live: false`.
  - A draft whose `target` exists under the project root (any lifecycle prefix of a plan file included) has landed: the store deletes it on its next read or write and never lists it. A discard deletes it too.
  - Ended drafts older than 30 days are removed at server start. A file that fails to parse or exceeds 256 KiB is skipped with a warning and left in place.
  - `GET /api/kickoffs` returns `{drafts: [KickoffDraftView]}`, and a `kickoff_drafts` socket message carries the same list. `KickoffDraftView {session, name, size, problem, started_at, updated_at, live}`.
- **D11 New work.**
  - `/new` always opens the empty form. When a kickoff is live, the form also shows "<name or Untitled> is still open", with "Continue" and "Set it aside". Set it aside ends that session and keeps the draft.
  - `/new?session=<id>` opens a draft. A live draft shows its conversation. An ended draft shows its problem, decisions, questions and the user's messages, with "Continue this draft", which starts a new kickoff whose opening carries them. The continuation carries the whole draft: the effective name (as the new draft's `name_override`), the size (`small` for a plan), the complete shape and the attachments that still resolve; one that no longer resolves is named in the opening as no longer available. The replacement draft is written before the old one is deleted, and a refused or failed start leaves the old draft as it was.
  - Home lists drafts as "Unfinished", titled by name, else "Untitled · <date>".
  - The activity popover's waiting row links to its session's screen.
- **D12 Rename.** A `rename_kickoff {name}` command (Holder, `is_feature_name`) stores `name_override` beside the tracker's current view, never inside it, so later agent blocks are still compared raw. The override wins over the agent's name in every `KickoffView` and draft view, and the server tells the agent the new name in one user message. The Name row in Taking shape is editable until the work is confirmed: a rename before any shape exists is refused ("Nothing to rename yet"), and a rename after `confirm_kickoff` armed the write target is refused ("The name is fixed once the work is confirmed"); the Name row hides Rename once `target` is set.
- **D13 Kind of work on /new** is a visible two-option control, "Feature" or "Standalone plan". Add plan preselects Standalone plan.
- **D14 Changed since** is a routed page at `/changes`, full width. The popover row stays a `<button>` named "Changed since you looked · N" that navigates to `/changes` with `state.from` (frozen `chat.contracts.test.tsx` finds it by role button) and closes the popover without returning focus to the pill.
- **D15 Footer.** `PlanShell` draws a footer with the dashboard's `QuotaMeters` (`@/components/quota-meters`). A provider with no data reads "<Provider> limits unavailable". The activity popover draws no quota and no token totals: its summary row renders the exported `RebuildActivity` (queue, failures, start error) instead of `SummaryPanel`. `SummaryPanel` stays whole, quota meters included (frozen `summaries/contracts.test.tsx` asserts the totals and the claude/codex meters), and moves to a Summaries section in Settings.
- **D16 Share** drops `tunnel` from the `ShareLink` wire type and the dialog. The plain-HTTP warning for a remote bind stays.
- **D17 Header.**
  - The switcher lists features only (issue 15 names its Home and New work entries).
  - The logo is a home link, keeping its name "Loom planner home".
  - The Home and Files tabs stay: frozen `feature/spine.contracts.test.tsx` "the header has no rail or view tabs" asserts links named exactly "Home" and "Files".
  - The spacing follows the dashboard header (`web/src/components/header.tsx`): 16px block padding, with the logo at 32/40/56px (frozen contract `header/activity-fixes.contracts.test.tsx`).
  - The dashboard's card-to-background header wash is allowed as a deliberate DESIGN.md exception.

## Design rules

- `web/src/plan/ui/DESIGN.md` is the design record. Read it before touching layout, type or colour. Theme tokens only. No colour literal, no invented `color-mix`. One surface level. Status is a glyph and a word. 390px must not scroll sideways. Every surface must read in Ledger (light) and Aubergine (dark).
- **Design direction (frontend-design, set for this plan).**
  - Tone: editorial precision, a reading instrument. Serif reading prose (`--font-reading`), quiet Inter chrome, mono for paths.
  - Alignment: one centre line per control row, one text column, one gap value per relationship from the `--space-*` scale.
  - Motion: only to explain state, and none under `prefers-reduced-motion`.
  - The skill's general advice on fonts, palettes and textures does not apply; DESIGN.md wins.
- **Type floor:** no transcript, table or panel text below 13px. The reading prose stays at its DESIGN.md size.
- **Deliberate exceptions** go into DESIGN.md's Surfaces section through its one owner per stage: V1 in the reading stage, V3 in the kickoff stage (carrying the chat stage's exceptions from V2's report), V4 in the home stage (carrying H1's and V5's). A visual worker that does not own DESIGN.md reports each one as `exception: <surface>: <rule>: <why>`. The header wash (D17) is V4's.

## Visual work and the refinement loop

- Visual workers are `loom-senior-software-engineer` spawned with `effort: medium`. Their first action is to invoke `frontend-design:frontend-design` through the Skill tool, before reading code.
- Subagents cannot build the bundle or the binary (`loom-hooks/subagent-verify-guard.sh`; the server embeds `web/dist`). So the stage's main agent builds and shoots: `scripts/plan-screens.sh --worktree-doc --themes=ledger,aubergine loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens" <routes>`, at 1440px and 390px. For states the script cannot reach, it uses a throwaway Playwright driver (plan Environment inventory).
- A miss goes to ONE fresh opus/medium visual worker with the shot paths and the miss list. A second miss on one item gets one more round. After a third miss, the main agent spawns `loom-advisor` (fable) with the shots, both attempts and their evidence, then one fresh opus worker on its diagnosis. An item still missing after that is recorded with `loom memory note "visual-miss: …"` for integration-verify.

## Frozen contracts that bind this plan

Never edit a `*.contracts.test.tsx` or Rust `*contract_tests.rs` / `tests/*_contracts.rs` file (or a module under a `tests/*_contracts/` directory) unless the list "Planned edits of frozen tests" below names the edit and your brief gives it to you. Those named below hold, and their accessible names must survive every redesign:

- `feature/spine.contracts.test.tsx`: the Vision row links `/f/<f>/vision`; no screen draws one offer twice; the header has no rail or view tabs, and holds links named exactly "Home" and "Files".
- `home/features.contracts.test.tsx`: the region "Standalone plans" holds a `details` that starts closed, whose summary contains "<n> standalone plans", and whose plan links (`/files/plans/…`) sit inside the details and outside the summary. The feature card shows the slug.
- `chat/context.contracts.test.tsx`: a group labelled "Viewing <name>" follows the route, and the composer placeholder contains the name. Sending attaches the viewed document. A dismissed context is not attached.
- `chat/chat-fixes.contracts.test.tsx`: an opening shows its label and hides the prompt until Show. The composer has a visible border.
- `chat/chat.contracts.test.tsx`: three consecutive tool rows fold after an ended turn into the label "Used 3 tools · edited 2 files", with an error row kept outside; an expanded tool row shows its detail (folded rows stay in the `<details>` DOM); an eleventh attachment is refused with a note; the cap disables Send; the draft and chips survive a remount; "chat changed since you looked…" opens the activity pill, finds a BUTTON named /Changed since you looked/, clicks it and finds the changed documents.
- `concept/concept.contracts.test.tsx` and `reading.contracts.test.tsx`: each key chip is a link whose accessible name holds the document title; four chips, then a "… n more" button; `data-dashed` on an out-of-date chip.
- `header/activity-fixes.contracts.test.tsx`: the logo sizes; a running plan appears once in the popover.
- `review/changed-since-fixes.contracts.test.tsx`: it goes through `changed-since-test-kit.tsx` (`openList`, row buttons `Show diff for <path>`, groups "Before" and "After").
- `summaries/contracts.test.tsx`: the `SummaryPanel` export renders the token totals and the claude 5h, claude 7d and codex 7d quota meters; `SummaryStatus` shows "Rebuilding" for a running rebuild.
- `plan_session/transcript/paste_contract_tests.rs`: a pasted record with an attachment block parses into its text and one `Attachment`.

### Planned edits of frozen tests

Each is the only change its file gets. The stage's main agent names every one in its `dispute-integrity` filing after the commit.

| File | Test | Edit | Owner | Why |
| --- | --- | --- | --- | --- |
| `web/src/plan/feature/spine.contracts.test.tsx` | "finished feature opens the product overview" | replaced by `it("a feature opens its vision", …)`: same body, expected pathname `/f/atlas/vision`, content check reads the vision page's text | R3 | D2, operator decision |
| `web/src/plan/feature/spine.contracts.test.tsx` | "the Vision row opens the vision document" | keeps the href and pathname assertions; `stubServer` answers concept key `atlas/product/vision` with a summary page, and `findByText(VISION_BODY)` / `fetchedDocs` become a check for that page's first sentence | R3 | D1: the vision route is a summary page |
| `web/src/plan/concept/concept.contracts.test.tsx` | "concept opening a summary sentence shows its source passages as inset blocks" | only `startsWith("/files/product/demo/section_01.md")` becomes `startsWith("/f/demo/source/product/demo/section_01.md")` | R3 | D4 |
| `loom/src/plan_session/transcript/paste_contract_tests.rs` | the expected `Attachment` literal | add the line `file: None,` | U1 | D9: `Attachment` gains `file` |
| `loom/tests/plan_session_bridge_core_contracts/records.rs` | `attachment()` | add the line `file: None,` | U1 | D9 |
| `loom/tests/plan_session_sandbox_contracts.rs` | new test only | add `guard_allows_reads_of_attachments_and_denies_writes` beside `guard_denies_reads_of_the_runtime_lock_and_allows_drafts`; nothing else changes | U1 | D9 sandbox grant |
- `plan-shell.contracts.test.tsx`: "plan schema parses every wire fixture", and "share popover fetches the link on every open".
- Journeys (`web/scripts/plan-journeys*.mjs`) use: "Loom planner home", "Start a conversation", the textbox /What do you want to build/, `aside[aria-label="Chat"]` textarea and Send, `[aria-label^="Viewing "]` and the button "Dismiss context" (J4, which goes PENDING, not red, when they vanish), `button.concept-run-cap`, `.concept-passage`, `.concept-mark[data-dashed]`, and a popover's `data-slot="popover-content"`. A stage that changes a journey's path owns that journey script (plan YAML).

## Code and test rules (from `doc/loom/knowledge/conventions/plan-web.md`)

- Server and model strings render through `<Untrusted>`. Storage keys go through `storageKey`. A new `PlanMessage` variant needs both schema arms in `web/src/plan/api/schema.ts`, plus a wire fixture.
- jest-dom is not installed. A route-level test goes through `renderAt` (`concept/concept-route-support.tsx`). Chat frames are seeded through the `FakeSocket` seam. Stub `matchMedia` for wide layouts. Read CSS through `@/...css?raw`. Contract tests are top-level `it()` calls named exactly as the contract's `test`.
- Limits: 400 lines per file, 50 per function. Split before crossing either. Files already near the limit at base: `plan_server/chat/dispatch.rs` 392, `plan_session/transcript/mod.rs` 391, `transcript/records.rs` 388, `plan_server/chat/mod.rs` 380, `chat/chips.tsx` 367, `chat/model.ts` 358, `web/scripts/plan-journeys.mjs` 407 (over: R3 splits it). `cargo test --test maintainability` enforces the Rust limit against `loom/maintainability-baseline.txt`, which is a ratchet file: never raise it.
- `cargo test` accepts ONE test-name filter. Run several modules as `cargo test --manifest-path loom/Cargo.toml --lib -- a:: b::` (filters after `--`).
- When a test pins behaviour this plan changes on purpose, update it in the same change and name it in your report. The main agent files one `dispute-integrity` after the commit.
- A change to a shared component breaks tests outside your directory. Before you finish, `rg` for importers and for every class, name and text you change, and report each hit outside your files.

## Worker rules

- Subagents write code and tests and report. They run no git command and no `bun run build`, `cargo build` or `tsc` (the verify guard blocks them). They run only the focused test command their brief names, once, at the end.
- Record a mistake, a decision or a surprise with `loom memory note "…"` or `loom memory decision "…"` as it happens. Never use Claude Code auto-memory.
- No mention of Claude, Claude Code or any AI system in code, comments or docs.
- Report: files changed, tests run with their result, every pinned test you changed and why, and anything you could not settle.
