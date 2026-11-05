# B1: no session without a message, the closing write only after a user message (R13, server)

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first. Wave 1, beside N1 and T1 (web). You own the Rust side, the spec text that the Rust tests pin, the session skills that quote the closing message, and the one shared web fixture the Rust parity test reads.

Line numbers below are from the tree before follow-ups-core ran; it edits `opening.rs`, `start.rs`, `section_09.md` and `planning_guidance_contracts.rs` before you. Anchor every edit by symbol and snippet, never by line.

## Settled decisions

- `AuthoringStart::NoStep` goes everywhere. A chat that the user starts by typing is the existing `AuthoringStart::Message { paths }` start (`plan_server/guidance/provider.rs` `authoring_start`, the `(None, Some(text))` arm). No new start kind.
- `Origin::Start` goes: its only producer is the `(None, None)` arm of `plan_server/chat/dispatch.rs` `launch` (`opening_origin`). After this change `launch` has two arms: `Some(_)` offer gives `Some(Origin::Offer)`, a message gives `None`.
- A start command with no offer and no typed content (`text` None and no attachments) is refused in `dispatch.rs` `start()` right after `let typed = ...`, before `draft_cmd::writable` and `take_authoring`: `return Err(NO_MESSAGE.to_owned())` with `const NO_MESSAGE: &str = "a chat starts with your first message";`. `provider.rs` `authoring_start`'s `(None, None)` arm becomes `anyhow::bail!("a chat starts with a message or an offer")` (defence for direct callers).
- The closing write runs only when the user wrote: the session holds a user item with `origin: None` whose `delivery` is not `Undelivered` (a typed send, the typed part of an offer opening, or a message start's opening; `bridge/opening.rs` `opening_delivery` and `flow.rs` `delivery` add these as server `sent` items at paste time, so they are in `self.mapper.turns()` before any transcript record). Decide it in `Session::plan_end` (`bridge/commands.rs`): for `SessionKind::Authoring`, `Closing::NotSent` when `self.flow.recovered.is_some() || self.user_wrote()`, else `mode = EndMode::Teardown` and `Closing::Skip` (the same pair `flow.rs` `closing_delivery` sets when a closing message cannot be sent). A recovered session keeps its closing write: `recovery.rs` `resume` sets `flow.recovered` before it calls `plan_end`, and its mapper is still empty then, so without that clause every session recovered after a restart would lose its closing write.
- `user_wrote(&self) -> bool` lives on `Session` in `bridge/session.rs` (290 lines): iterate `self.mapper.turns()` items, match `Item::User { origin: None, delivery, .. }` with `*delivery != Delivery::Undelivered`. `commands.rs` is 392 lines: if your edit takes it past 400, move `plan_end` into `ending.rs` (327 lines).
- `CLOSING_MESSAGE` (`planning/guidance/mod.rs`) asks for a machine-readable tail that the chat's ended card reads (T1 parses it; it is never shown). Its new text, exactly:

```text
loom plan is ending this session; nobody will answer further questions.
1. Write into the documents every decision this conversation settled that is not in them yet.
2. Add every point still open as an item under "## Open questions" in the document it concerns,
   with the options and your recommendation.
3. Finish the turn-end duty for what you changed.
Then reply "Closing write done." followed by a fenced block whose info string is loom-closing,
holding one JSON object {"decisions": [...], "questions": [...]}: one plain sentence for each
decision this conversation settled and for each point it left open. Then stop.
```

  Keep it a `concat!` of lines with `\n`, as now.

## Edits

1. `planning/guidance/start.rs`: delete the `NoStep` variant, its `label()` arm (Message keeps "New chat") and its `step_class` arm.
2. `planning/guidance/opening.rs`: delete the `AuthoringStart::NoStep =>` opening arm ("loom plan: no step chosen. ...") and the `Some(AuthoringStart::NoStep) => "chat"` arm of `display_name`.
3. `planning/guidance/brief.rs` `starting_point`: delete both `matches!(start, AuthoringStart::NoStep)` branches (the step is always the label's, and the skill line is always read).
4. `planning/guidance/mod.rs`: `CLOSING_MESSAGE` as above. `planning/guidance/text/common_brief.md` "## The closing message": the quoted block becomes the new text, and the sentence above it says that with nothing to write the reply is `Closing write done.` with empty lists, at once.
5. `plan_server/guidance/provider.rs`: the `(None, None)` arm as above.
6. `plan_server/chat/dispatch.rs`: the refusal in `start()`; `launch`'s `opening_origin` match loses its `(None, None)` arm (a two-arm match on `&offer`).
7. `plan_session/model/mod.rs`: delete `Origin::Start`.
8. `plan_session/bridge/commands.rs` `plan_end` and `bridge/session.rs` `user_wrote` as above.
9. Spec, to the new truth:
   - `doc/product/feat-planner/section_09.md`: the paragraph "A chat the user starts with no offer (an empty chat box) gets the common brief with `Step: none chosen` ..." becomes: New chat starts no session; the user's first message starts it, with the common brief of a message start (`Step: any, Refine`, skill `loom-plan:refine`) and that message as the opening. The fenced closing message under "Recording as you go, and the closing write" becomes the new text, and the paragraph after it says the closing message is delivered only when the user sent at least one message; a chat with none ends without a model turn. Fix every other "no step" mention in the section (around lines 481 and 528) to match.
   - `doc/product/feat-planner/section_07.md` "Interrupt and end", **End**: the first bullet becomes "the user starts a new chat or opens another offer from the authoring browser"; add that a session whose user sent no message is torn down at once without the closing message.
10. `plan-plugin/skills/section/SKILL.md` "Close the session", `brainstorming/SKILL.md` (around line 155) and `vision/SKILL.md` (around line 87): where each says reply `Closing write done.` and stop, say reply as the closing message asks (`Closing write done.` and its `loom-closing` block), then stop.
11. Web parity for the removed variant: `web/src/plan/api/schema-chat.ts` `originSchema` is `z.enum(["offer", "closing"])`; `web/src/plan/api/schema-chat.test.ts` line 78 lists `["offer", "closing"]`; `web/src/plan/api/fixtures/chat-snapshot.json` item `u-...011` (`"origin": "start"`, around line 110) gets `"origin": null`. `loom/src/plan_server/wire/tests_chat.rs` `origin_variants_in_fixtures` lists `[Offer, Closing]`.

## Tests to update (all yours)

- `planning/guidance/tests.rs`: the `common_brief()` helper builds with `AuthoringStart::Message { paths: Vec::new() }`; `closing_message_matches_product_spec_with_continuation_indent` reads the new block's line range in `section_09.md` (find it after your edit: the lines of the fenced closing message).
- `planning/guidance/tests/starting.rs`: `starting_point_without_documents_has_no_skill_and_reports_absence` asserts the message start's starting point instead (`Step: any, Refine.`, `Skill: loom-plan:refine.`, then the no-feature line); rename it to what it now checks. The other tests there pass `Message { paths: Vec::new() }`.
- `planning/guidance/tests/blocks.rs` (`kinds()` and the restarted-block test), `start/tests/labels.rs` (`message_and_no_step_labels_are_new_chat`: Message only, renamed), `start/tests/classification.rs` (drop the NoStep entry, rename), `opening/tests.rs` (delete `no_step`), `opening/tests/names.rs` (drop the NoStep row).
- `plan_server/chat/tests.rs` `start()` carries `text: Some("Plan the next step.".into())` (no line growth: this file is at 400). `tests_dispatch.rs` `start_without_offer_or_text_takes_authoring_and_marks_the_opening_as_start` becomes a typed start whose opening item has `origin: None` and whose guidance call is `(None, Some(<text>), None)` (read `record_guidance` in `tests_dispatch/recording.rs` for the tuple order). `tests_dispatch/start.rs` line ~146 and every other `Origin::Start`: `Origin::Offer`. Run `rg -n "Origin::Start|NoStep|text: None" loom/src/plan_server` and fix each hit that started a chat with neither.
- `plan_session/bridge/harness.rs` `request()`: `opening_origin = None` (a test session's opening is the user's own first message, so every existing final-write test keeps its closing write). `tests_opening.rs` (~164): `let origin = None;`. `plan_session/transcript/tests_server.rs` (39, 50) and the frozen `transcript/paste_contract_tests.rs` (62, 72): `Origin::Offer`. `plan_session/model/tests.rs` (~287): delete the `(Origin::Start, "start")` row.
- Add to `bridge/tests_end.rs`: a typed-message session (harness default) ended at idle pastes the closing message (the positive half of the contract below).
- Frozen, planned: `loom/tests/planning_guidance_contracts.rs` (three `AuthoringStart::NoStep` uses become `AuthoringStart::Message { paths: Vec::new() }`), `loom/tests/plan_session_bridge_runtime_contracts/support.rs` (~279, `opening_origin: None`; drop the import if unused), `loom/tests/plan_session_bridge_e2e.rs` (~251, `None`).

## Contracts (written before you start; make them pass, do not edit them)

- `loom/src/plan_session/bridge/closing_contract_tests.rs`: `an_offer_chat_the_user_never_wrote_in_ends_without_a_closing_message`.
- `loom/src/plan_server/chat/start_contract_tests.rs`: `a_start_with_no_offer_and_no_text_launches_nothing`.

## Done when

`rg -n 'NoStep|Origin::Start|no step chosen|none chosen' loom plan-plugin doc/product` prints nothing; `commands.rs`, `session.rs`, `dispatch.rs` and `plan_server/chat/tests.rs` stay at or under 400 lines and every function you touch under 50.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::guidance:: plan_server::chat:: plan_server::guidance:: plan_server::wire:: plan_session::` from the worktree root. The integration targets and the maintainability check run in the stage gate.

## Report

Files changed; the test result; every test you deleted, renamed or whose assertion changed, by file and name (the main agent files one `dispute-integrity` for them); the final `section_09.md` line range of the closing block.
