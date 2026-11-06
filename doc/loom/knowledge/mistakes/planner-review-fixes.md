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
