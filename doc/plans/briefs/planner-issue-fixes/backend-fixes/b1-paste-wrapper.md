# B1: unwrap Claude Code's pasted-content wrapper (#4, #5)

Tier: opus (`loom-senior-software-engineer`). Read `../common.md` first.

## Root cause (measured from real transcripts)

Claude Code 2.1.316 records a pasted prompt wrapped in a tag pair whose id repeats on the closing tag. This is the exact JSON string of a real New chat opening (transcript `~/.claude/projects/-home-dkaponis-src-loom/cf2fa1b6-fb1d-4b7e-bf36-8c0270cf672e.jsonl`, first user record):

```text
"\n\n<pasted_content id=\"f3a0\">\nloom plan: no step chosen.\nAsk me what I want to work on.\nFeature: feat-planner.\nChecklist: Vision=Done, ProductSpec=Done, Metaplan=Done, Plans=Done, PressureTest=Done, Run=Active.\n</pasted_content id=\"f3a0\">\n"
```

A message with attachments arrives the same way (transcript `af7fa405-fbc7-4d4e-b62e-3c5a16b4a4d6.jsonl`):

```text
<pasted_content id="733e">
What are the main decisions here?

<loom-attachments>
[1] source doc/product/feat-planner/index.md
> # Loom Plan — Product Vision
...
</loom-attachments>
</pasted_content id="733e">
```

Older records in the same version arrive unwrapped (`0a52b299-a6be-4cf9-9bbb-503b2f3cffdc.jsonl`). Both forms must work.

Consequences in the tree:

- `loom/src/plan_session/attachments/parse.rs::parse_message` finds the `<loom-attachments>` line, but the line after `</loom-attachments>` is the closing wrapper tag. `parse_block` therefore returns `None`, and the fallback `_ => (delivered.to_owned(), Vec::new())` returns the whole message, wrapper and block included. The transcript shows the markup (#5).
- `transcript/mod.rs::take_server_item` compares `sanitize(text) == clean`. The wrapped text never equals the sent item's text, so the server's `s-<n>` item is never removed and the delivered record adds a second user item. The opening shows twice (#4). The closing message's `take_origin` match (`self.closing.as_deref() == Some(clean)`) fails the same way, so the closing message renders as raw user text with the wrapper.
- `bridge/flow.rs::confirm_prompt` and `confirm_text` match late deliveries through `parse_message(prompt)`, so they inherit the defect.

## Task

1. Add one function that strips the wrapper, used at the one seam every reader of delivered text passes through. That seam is `parse_message` (`attachments/parse.rs`, 146 lines): put the unwrap inside it, or in a new module under `attachments/` that it calls.
   - The transcript mapper's human path (`transcript/mod.rs`, the function that calls `cap_chars`, then `parse_message`, `sanitize`, `take_origin`, `take_server_item` and the HumanUser mark) and `bridge/flow.rs::confirm_prompt` both go through `parse_message`, so one unwrap there covers #4, #5, the closing match and the `UserPromptSubmit` matching. An unwrap only in `records.rs` (`Kind::Human`) would miss `confirm_prompt`.
   - The other `parse_message` callers (`flow.rs` server-side text, `commands.rs`, `plan_server/guidance/provider.rs`) carry unwrapped server text and must stay unchanged; the rule below leaves non-wrapped text alone.
   - Do not grow `transcript/mod.rs` (389 lines) or `transcript/records.rs` (388 lines): both are near the 400-line limit.
   - `cap_chars(delivered, USER_TEXT_CHARS)` (100,000 chars) runs before `parse_message` in the mapper. A delivery near `MAX_DELIVERED_BYTES` plus the wrapper loses its closing tag to the cap, and the markup returns. Unwrap before the cap in that path, or prove with a test at the cap that the result carries no wrapper.
   - Verify the seam with `rg -n 'parse_message|take_origin|take_server_item|match_text|late_confirm|cap_chars' loom/src/plan_session`.
2. The unwrap rule: after trimming surrounding blank lines, a message that is exactly one `<pasted_content id="X">` line, a body, and a `</pasted_content id="X">` line with the same id yields the body. Anything else is left alone. That covers an id that does not match, a missing close, text outside the pair, and a typed message that merely mentions the tag. Never strip a wrapper from the middle of typed text. The id is untrusted: accept only `[A-Za-z0-9_-]{1,32}`.
3. Keep `WRAPPER_TAGS` (`records.rs`, about line 28) as it is. Those records are skipped entirely; a pasted user prompt is not.

## Contracts (frozen before you start; make them pass)

`loom/src/plan_session/transcript/paste_contract_tests.rs` (test path `plan_session::transcript::paste_contract_tests::<fn>`):

- `wrapped_message_parses_typed_text_and_attachments`: `parse_message` on the wrapped attachment message returns the typed text and one attachment.
- `wrapped_opening_replaces_the_sent_item`: `Mapper::add_server_item` with a sent opening, then `ingest_main` on the wrapped record of the same text, leaves one user item, not two.
- `wrapped_closing_keeps_its_origin`: after `Mapper::expect_closing(text)`, the wrapped closing record maps to `Origin::Closing`.

## Your own tests

Add focused tests beside the existing ones (`attachments/tests.rs`, `transcript/tests_server.rs`). Cover: an unwrapped message is unchanged, a mismatched id stays whole, an id with `"` or `>` stays whole, typed text that quotes the tag is unchanged, and `confirm_prompt` confirms a wrapped late delivery.

## Files you own

`loom/src/plan_session/attachments/**`, `loom/src/plan_session/transcript/**` (except the frozen contract file), `loom/src/plan_session/bridge/flow.rs`, `loom/src/plan_session/bridge/tests_queue.rs`.

## Proof

`cargo test --lib plan_session::` and `cargo test --test plan_session_bridge_core_contracts` once at the end. Report both result lines. The contract file is frozen: never edit `transcript/paste_contract_tests.rs`.
