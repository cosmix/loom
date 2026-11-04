# T1 — one opening shown once, no internal records, no doubled sends (issues 9, 10)

Tier: sonnet (`loom-software-engineer`); escalate to opus only after a failed attempt. Read `common.md` first, in particular "Evidence" and D6.

## 1. Split the opening (issue 9)

The kickoff opening is one delivery: `guidance + "\n\n" + typed text` (`plan_server/guidance/provider.rs`, the offer arm that joins `guidance.opening_message` and the first message), plus any `<loom-attachments>` block. Today one origin item holds it all, so the user's own text sits behind Show.

- At delivery, the bridge creates the sent items for an opening that carries typed text as TWO server items: an origin item (`origin: Offer`) holding the guidance, and a plain user item (`origin: None`) holding the typed text and attachments (`plan_session/bridge/flow.rs` `Session::delivery`, `bridge/start.rs` `Session::configure`, which calls `expect_opening`).
  - A guidance-only opening (New chat, pressure, a move) stays one origin item. The frozen web contract "an opening shows its label and hides the prompt until Show" holds.
  - The server knows the typed part: `plan_server/guidance/provider.rs` joins it as `format!("{}\n\n{text}", guidance.opening_message)` for an offer with a first message. Never re-split by searching the text.
  - Carry it through the session directory, not through a struct: add `SessionDir::opening_typed()` (`plan_session/kind.rs`, beside `brief()` and the other path helpers) naming `<dir>/opening-typed.txt`. Inside its guidance closure the provider writes the typed text there (only when an offer opening carries typed text). `Session::configure` reads it, and the flow builds the two sent items from it. Do NOT add a field to `LaunchGuidance`, `StartRequest` or `AuthoringStart`: their struct literals sit in about 14 files, three of them frozen, and one (`plan_server/summaries/spawner.rs`) in the parallel reading stage.
  - `Opening` and `Delivering` (`bridge/session.rs`) carry every sent item id of the delivery, not one `item_id`. A failed paste, a timeout and a late confirm (`flow.rs` `late_confirm`, `bridge/opening.rs` `set_delivery`) update all of them, so the typed item is never left showing as sending.
- `plan_session/transcript/mod.rs` `take_server_item` and `Mapper::human`: a delivered record whose unwrapped, sanitized text equals the sanitized texts of two consecutive sent items joined by `"\n\n"` replaces both. The result is the delivered origin item plus the delivered user item, and no sent item is left behind. Keep the existing single-item match.
  - `take_origin` hands the expected opening origin to the first human record, and `Mapper::human` stamps it on the item it builds. In the two-item case only the guidance item takes `Offer`; the typed item keeps `origin: None`. The two delivered items get different ids (`u-{uuid}` is per record today: suffix the second).
  - `transcript/mod.rs` is 391 lines at base. Put the two-item match in a new `transcript/opening.rs` and keep `mod.rs` under 400.
- Before changing the match, feed the captured opening record (`common.md` "Evidence") through `records.rs` `user` (the `kind` decision). Confirm that `WRAPPER_TAGS` does not classify the `<pasted_content>`-wrapped human record as `Skipped`; a skipped record would leave the sent item standing. Record what you find with `loom memory note "t1-opening: …"`.

## 2. Internal records (issue 9)

`plan_session/transcript/records.rs:193` already maps a user record with `isMeta: true` to `Kind::Skipped`, so the "Skill loom-plan:kickoff" line the operator saw is the Skill tool row, which C1 drops in the browser. Add one regression test, `a_meta_skill_body_with_list_content_is_skipped`: it feeds a record whose content is a LIST holding one text part, in the shape of the operator's record (`common.md` "Evidence"), and pins the skip. If the test fails, fix the skip.

## 3. The doubled send (issue 10)

"Make this a feature instead." is one plain user record, yet it showed twice.

1. Reproduce it before changing anything.
   - Mapper: `add_server_item` a Sent user item with that text, then `ingest_main` the plain record. Does exactly one item remain?
   - Bridge: does `Session::send` (`bridge/commands.rs` `Session::send`) create both a queued `q-N` and a sent `s-N` for one send while the agent is busy?
2. Record what you found with `loom memory note "t1-dup: <cause or 'not reproduced in Rust'>"`.
3. If the Rust side is clean, report that to the main agent: C1 owns the browser side (`new/proposal.tsx` `useProposalActions` has a per-hook `inFlight` ref, and `ShapeSize` and `Proposal` use separate instances).

## Tests

- The frozen stage contract `opening-splits-guidance-from-typed-text` (the mapper half).
- The provider half, named `a_kickoff_offer_with_typed_text_writes_opening_typed`, in `plan_server/guidance/` tests: the real provider's guidance closure, given a kickoff offer with a first message (and one attachment input), writes exactly the typed text to `SessionDir::opening_typed()`; a guidance-only offer (New chat) writes no file. A plan `after_stage` checks the name.
- The bridge half, named `a_kickoff_opening_with_typed_text_creates_two_sent_items`, in `bridge/tests_opening.rs`: a kickoff start through `Session::configure` and the flow, with `opening-typed.txt` written by `SessionDir::opening_typed()` as the provider writes it and one attachment on the start, yields two Sent items before delivery (guidance with `Offer`; typed text with `None`, carrying the attachment), and both become Delivered after it. The provider and the bridge meet only at that one path function, so the two halves together cover the production path. A plan `after_stage` checks the name.
- `a_failed_opening_delivery_marks_both_items`: the same start whose paste fails (and, in a second case, times out) leaves neither item Sent; both carry the failure. Also grepped.
- The meta-record test from section 2.
- Your regression test for the doubled send, named `a_sent_message_is_one_item_after_delivery`. It must fail at the stage base if the cause is in Rust. If it passes at base, say so in your report.
- Expected breakage: `plan_session/transcript/tests*.rs` and `bridge/tests_*.rs` tests that pin a single opening item for a kickoff with typed text. Update only the non-frozen ones and list them. `paste_contract_tests.rs` stays green and is not yours: U1 adds its one `file: None,` line, and `transcript/tests_server.rs` is U1's too. Put your own new transcript tests in a new file (for example `transcript/tests_opening.rs`).
- `tests/plan_session_bridge_runtime_contracts.rs` (frozen) must stay green: the session-directory seam leaves every launch struct as it is.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- plan_session:: plan_server::guidance::` (one command; filters after `--`). The integration targets run in the main agent's stage gate.
