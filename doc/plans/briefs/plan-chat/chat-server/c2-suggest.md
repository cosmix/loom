# chat-server C2: suggestion box

Codex unit (gpt-6.1-terra, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/review/suggest.rs`; `src/plan_server/review/suggest_tests.rs`
- May read: `doc/plans/PLAN-plan-chat.md (or its IN_PROGRESS- spelling) stage chat-server PUBLIC SURFACE bullet suggest.rs (authoritative)`; `src/plan_server/wire/review.rs (unit C1)`; `the plan 1 function planning::paths::check_relpath_syntax`

## Entry points

BrowserRef, SuggestedPassage, SuggestionBox { new, submit, answer, pending, drop_for_holder_change, pending_for }, SuggestionAction, Delivery, SuggestRefusal.

## Steps

1. Implement exactly the PUBLIC SURFACE rules: NoHolder, SenderIsHolder, NoteTooLong (note.chars().count() > 1000), TooMany on the 21st, BadPath via check_relpath_syntax on the doc-relative form of the path; ids sg-<n> from a counter; insertion order kept.
2. Delivery messages are serde_json values of PlanMessage Review shape: {"type":"review","kind":"suggestion","id","from","passage","note"} carrying no browser id; answer only by the holder, removes the entry, delivers suggestion_status attached or dismissed to the sender id; drop_for_holder_change returns one dropped delivery per suggestion.
3. In suggest_tests.rs (declared by the orchestrator in mod.rs as a cfg(test) module; start it with use super::suggest::*;) write tests drop_for_holder_change_clears_pending, pending_for_returns_cards_in_order, answer_by_non_holder_refused, unknown_id_refused, bad_path_refused.

## Frozen contracts this unit must pass

suggestion-reaches-holder-only, suggestion-refused-without-holder, note-limit-counts-characters
and twenty-first-suggestion-refused, all in tests/plan_review_contracts.rs (never edit that file).
This file is security-relevant (holder-only routing, no browser id in any message), so it is also
named in the integration-verify security review.

## Done

The module and its five tests compile; the frozen contracts in tests/plan_review_contracts.rs pass when the orchestrator runs them.

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::review::suggest_tests:: && cargo test --test plan_review_contracts`

## Traps

The passage path arrives repo-relative (doc/...); strip doc/ with plan_server::wire::paths::to_doc_rel before check_relpath_syntax. Never put BrowserRef.id into any message.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
