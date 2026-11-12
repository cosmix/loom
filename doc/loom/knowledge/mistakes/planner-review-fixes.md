# Planner Review Fixes Mistakes
>
>

Slips from the planner review-fixes plan (kickoff, chat, drafts, uploads, vision summary). Current behaviour is in [Planner Uploads and Kickoff Drafts](../architecture/planner-uploads-and-drafts.md) and [Planner Web Shell](../architecture/planner-web-shell.md).

## A Structured Block and a Composite Opening Were Never Replayed Through the Transcript

**What happened:** the operator's first real kickoff showed the assistant's `loom-kickoff` JSON fence as a code block in the transcript, and the opening hid the typed sentence behind Show next to the generated guidance. Every unit test passed before the plan: the Taking shape panel, the kickoff parser and the transcript each had green tests.

**Why:** the first user record of a kickoff is one `<pasted_content>` item holding the guidance and the typed text, joined by `format!("{generated_opening}\n\n{text}")` in `guidance/provider.rs`; the mapper treated it as one collapsed origin item. The fence is instructions for the server (`chat/kickoff.rs` parses it), and nothing hid it from the markdown renderer. Tests built synthetic one-item openings and well-formed assistant text, so no test ever put a recorded kickoff through the mapper and the transcript together.

**Prevention:** whenever an opening, a delivered prompt or a structured block in assistant text changes, replay a captured kickoff transcript (the real record shapes, the `isMeta` skill-body record after a Skill tool result included) through the mapper and render the transcript, and read what a user sees. A new block the server parses needs its own "never rendered" test beside its parser.

**Fix:** the bridge sends a kickoff opening with typed text as a collapsed guidance item and a visible user item (the `opening_typed` file), which one recorded prompt replaces (`transcript/opening.rs`); `withoutKickoffFence` drops the fence, streaming included; Skill tool rows and `isMeta` records never render.

## A Sent Item and Its Record Arrived in the Opposite Order

**What happened:** "Make this a feature instead" showed twice. Two causes: in the bridge, the `UserPromptSubmit` hook confirms delivery (the sent item `s-N` becomes Delivered) before the plain human record arrives, and `take_server_item` matched only an item that was not yet Delivered, so the record added `u-<uuid>` beside it; in the browser, the Size control and the proposal each dispatched on one click.

**Why:** the stage assumed the record came first. The web contract could not catch the browser half either: `proposal.test`'s `beforeEach` mocks `crypto.randomUUID` to one value, so a doubled send failed inside `registerPending` and hid itself.

**Prevention:** reproduce a doubled row in Rust against the real event order (hook, then record) before changing the browser; a regression test for a double dispatch needs distinct generated ids.

**Fix:** `transcript/opening.rs` matches a Delivered sent item too; `useProposalActions` guards on a shared pending atom.

## A Fix Worker Satisfied a Text Wiring Check With a Comment

**What happened:** twice in integration-verify a fix worker, told to remove a duplicate, deleted a call and left its name only in a comment: `featureSourceHref(` in `concept/passage.tsx` (wiring of the reading stage) and `uploadAttachment(` in `new/document-starter.tsx` (wiring of the chat stage). The plan's `rg` wiring checks passed on the comment while the wiring they guard was gone. The main agent reverted both and left the reviewer suggestion pending.

**Why:** a wiring check is a text match; a brief that says "remove the redundant branch" does not say that the call must stay in code.

**Prevention:** a brief that touches a wired file names the wiring check and the call that must survive. After a fix round, read the match of each affected check (`rg -n` shows whether it sits in code or in a comment); never trust the exit code alone.

## A Chat Test Sent a Message and Asked for a Final Write Without Finishing the Turn

**What happened:** `a_continued_draft_carries_its_name_size_and_attachments` failed about 1 run in 10 with "timed out waiting for the closing message".

**Why:** `rename_kickoff` sends a server-written message through `bridge.send`. When the driver tick pastes it before `set_aside`'s end, the message waits in `flow.delivering` for a `UserPromptSubmit` the fake host never sends, so `closing_due` (`bridge/flow.rs`) stays false until `delivery_timeout` (10 s), longer than the fixture's `final_write_bound` (5 s), and the end tears down without a closing paste. When the end wins the race, `return_unsent` hands the text back and the closing goes out.

**Prevention:** a chat test that sends (or renames) and then asks for a final write runs the pasted message's turn (`UserPromptSubmit`, `Stop`) to Idle first. `tests_dispatch/drafts.rs` has the `rename_turn` helper.

## Tooling Slips

- **A scripted splice duplicated a chunk.** A script located a range with `str.index` of a doc-comment text that also occurred earlier in `use-activities.ts`, and a block was duplicated. Slice with unique anchors or use the Edit tool, and re-read after a scripted splice.
- **Parallel vitest runs blamed the wrong worker.** Verification workers ran `vitest` while the home worker was mid-edit, so a `useNavigate` failure in the activity tests looked like theirs. Judge failures only from a full `src/plan` run after every worker returned.
- **A test used a feature name with a space.** `isFeatureName` rejects spaces (`new/feature-name.ts`), so a `/f/<feature>/…` test with an encoded space never reaches a document.
- **Page text was written for one state and shown in all.** The key lede said "Sources: the 12 sections this summary draws on" on a document summary page, a never-built page listed all 12 key documents, and the app bar read "Planning · new feature" on every `/new`. The same lesson as [Page Text Was Accepted Without Reading It Against Live State](planner-redesign.md#page-text-was-accepted-without-reading-it-against-live-state): list the states a string appears in (document or overview, built or never built, empty form or live kickoff) in the brief and read the string in each.

## A Route Test Sent the Id Form the Web Client Never Sends

**What happened:** `/api/built` answered `notes: []` in every real server, so the Built marker dialog never rendered. The web sent `section=product/<f>/section_NN.md` (`web/src/plan/review/api.ts`), `built_inputs` looked it up with `tree.by_id` on the id form without `.md` (`plan_server/review/routes.rs`), and `route_tests.rs` sent the id form. The same functional browser run found three more defects no test caught: `useSelectedDiff` dropped a diff when `project_key` went undefined to defined mid-load; the composer chip named the viewed document while Send went out with `attachments: []` on a fresh load; every `/api` GET waited about 10 s on a debug build.

**Why:** tests built their own input instead of the client's, and no stage ran the web against the real server before integration-verify.

**Prevention:** a route test sends the query exactly as the web client builds it (build it with the client function or pin both against one fixture). Integration-verify runs a functional pass in a real browser; a harness against `target/debug/loom` first waits for one `/api` answer ([the timing](../conventions/plan-web.md#visual-stages-and-the-screenshot-script)).

**Fix:** the route takes the document-relative path; the tests use that form.

## A Background Hand-Back Reopened Activity After the Closing Stop

**What happened:** the first close-gate draft for the agent end (`bridge/driver.rs`) let a background hand-back reopen main activity after the final `Stop`, so the chat never closed without another `Stop`.

**Why:** peer hand-backs produce `SelfStartedTurn` marks, and the draft traced only background-table updates, not transcript activity marks.

**Prevention:** trace transcript activity marks as well as table updates whenever a state check reads activity.

**Fix:** after a finished closing turn, self-started background records count as table evidence; real prompts and assistant records still set activity ([the gate](../architecture/planning-session-bridge.md#interrupt-end-teardown)).

## A Bulk Route Held the Project Read Guard Across Saves

**What happened:** the first `settle_all_route` draft held `req.ctx.project()` while calling `save_document`, which reads the project again.

**Why:** snapshot ownership was assumed instead of read from `PlanContext::project`.

**Prevention:** collect candidate paths and hashes, drop the guard, then call the shared save path (the watcher may also be waiting to replace the project between documents).

**Fix:** `settle_all.rs` snapshots path and hash pairs inside a block before the loop.

## Removing a Page Removed Tests of Behaviour It Shared

**What happened:** removing the engineering overview, a worker deleted two document-key behaviour tests together with the overview fixtures, and three later review fixes removed assertions about orphaned behaviour (non-product `PageKey::Overview`, `overviewKey` scope, a false reload promise). Integration-verify has no INTEGRITY line, so each removed assertion raised a TI-edit event.

**Why:** the tests were treated as overview-only without separating shared behaviour.

**Prevention:** before deleting fixtures, list what each test asserts and keep the shared behaviour on the surviving route. Where an assertion tests behaviour a review finding removed and an equal or stronger check replaces it, dispute the TI-edit event (`loom stage dispute-integrity`) instead of reverting, since reverting restores the dead code the reviewers flagged.

**Fix:** four TI-edit events disputed in integration-verify; re-review confirmed each replacement.

## Spine Head Tests Pinned an Amber Verdict With No Trace

**What happened:** `spine-model.test.ts` ("flags written but never settled ... without a trace") and `spine.test.tsx` ("lists the four groups", "reads a shown unit once") pinned amber "Not all settled" with no trace loaded, the defect the architecture review raised, while rows already refused to claim Draft before the trace loads (`spine-model.ts`).

**Why:** the tests copied an old shape in which the head read the checklist alone.

**Prevention:** a state that depends on loaded data is tested with the data loaded and, separately, with it absent.

**Fix:** those tests keep their `expect` lines and load a trace with no entries for the documents.

## A New Test Module Pushed a Source File Past 400 Lines

**What happened:** the integration-verify fix worker added `/api/built` tests to `plan_server/review/route_tests.rs` and took it to 423 lines; `loom/tests/maintainability.rs` fails any source file over 400 lines without a baseline entry.

**Why:** the brief named the limit but the worker checked only production files.

**Prevention:** after adding tests run `wc -l` on every touched file, test files included, and split into a sibling test module before 360 lines. A scoped file-size audit applies the limit to files the change owns, not to a long document it only edits a paragraph of.

**Fix:** the tests moved to `built_route_tests.rs`.

## `loom check` Run in the Background or While Workers Edit

**What happened:** `loom check integration-verify` run with `run_in_background` never reached the daemon (the relay hook reads stdout of a foreground Bash call only) and the next foreground call reported `refused`. Separately, each `loom check` while workers were editing queued a new host job (new tree fingerprint, same HEAD id).

**Prevention:** run `loom check` in the foreground and rerun the same command only to wait; submit it only when no worker is writing. `--criterion` takes the 0-based index from the criterion's `[criterion <n>]` label, not the 1-based number in the signal's task list.

## A Plan `reachable` Entry Named a Route Handler Held as a Value

**What happened:** the entry `settle_all_route from routes` could not pass: `RouteSpec.handler` holds the fn as a value and the Rust extractor emits call edges only (`context/extract/rust.rs`), so the reverse impact of every route handler is empty.

**Prevention:** write `reachable` entries only for named calls; the rule and its fix are in [A `reachable` Check Fails When the Call Is a Value](verification-v2-delivery.md#a-reachable-check-fails-when-the-call-is-a-value-or-goes-through-a-re-export).

## A Gate Contract Fixture Needs the Offer's Real Threshold

**What happened:** the scenario text of the gate contract named two metaplan entries; with two the checklist answered `NotOffered` for `draft-all-plans` whatever the gate did (`checklist/offers.rs` offers it only when two or more plans are missing), so the fixture names three (`PLAN-f-01..03`, only 01 written).

**Prevention:** read the offer's own condition before writing a fixture for a contract that asserts it.

## Workers Share One Scratchpad Directory

**What happened:** a design worker wrote `contrast.mjs` to the session scratchpad, a sibling worker had written a file of the same name there, and a later `cat >>` appended a block to the sibling's script (restored with Edit).

**Prevention:** prefix scratch files with the worker id or use a per-worker subdirectory, and never append to a scratch file without checking it is still yours. In zsh an unquoted variable is not word-split (`F="a b"; tool $F` passes one argument): pass file paths literally or use an array. Run repository-relative edit scripts from the worktree root.

## Chat called crypto.randomUUID, which LAN HTTP does not provide

**What happened:** the planner chat failed with a `crypto.randomUUID` error whenever it was opened over `http://<lan-ip>`; every chat command and kickoff document add threw.
**Why:** `commands.ts` and `empty-kickoff.tsx` assumed a secure context. Tests run in a secure context and mock `randomUUID`, so no test saw the insecure case.
**Prevention:** use `newId()` for browser ids; check any new browser API against secure-context requirements, since LAN binds are plain HTTP.
**Fix:** `web/src/plan/id.ts` `newId()` with a `getRandomValues` v4 fallback, tested with `randomUUID` removed.
