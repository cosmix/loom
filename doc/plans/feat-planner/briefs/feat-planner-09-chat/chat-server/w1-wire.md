# chat-server W1: wire model, suggestions, open questions

Dispatch: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 600000 ms. The orchestrator partitions this territory into ordered file-sized forwards before dispatch to fit the 540 s deadline; workers never delegate. All test/proof commands below are for the orchestrator; workers run compile-only checks, no git, and never write under `.loom/`.

Lane note: codex units own `wire/review.rs` (C1), `review/suggest.rs` (C2) and
`review/questions.rs` (C3), each with its test file; the sections below on them are the
reference their briefs build on. C1 is in the tree when you start.

Working dir `loom/`. You own `src/plan_server/wire/tests_review.rs`, the two
`Review` variants in plan 2's `src/plan_server/wire/message.rs` (add-only), and the fixtures `../web/src/plan/api/fixtures/review-{restore,compare,
suggestion}.json`. The stage description's PUBLIC SURFACE is authoritative for `suggest.rs` and
`questions.rs`. Do `wire/review.rs` first and report when it compiles. The frozen contracts are
`tests/plan_review_contracts.rs` and `tests/plan_review_contracts/**`: never edit them.

## wire/review.rs

Serde (snake_case; `#[serde(tag = "kind")]` on enums) for every route body:

- `RestoreRequest { session: String, turn: String, paths: Vec<String>, force: bool }`;
  `RestoreResponse { files: Vec<RestoreFile> }`, `RestoreFile { path, result }` where `result`
  is `restored` | `refused { reason }` | `changed { current_text, current_hash,
  turn_start_text }` | `invalid { errors }`.
- `HashesResponse { head: Option<String>, docs: BTreeMap<String, String> }` (doc path to
  `normalized_hash`, every document file mode shows).
- `RefResponse { commit: String }`; `CompareResponse { commit, available: bool, docs:
  Vec<ChangedDocWire { path, change: "added"|"changed"|"removed" }> }`; `AtCommitResponse {
  path, text: Option<String> }`.
- `BuiltResponse { notes: Vec<BuiltNoteWire> }` mirroring `built::BuiltNote`.
- `OpenQuestionsMessage { counts: BTreeMap<String, usize> }` (the HTTP body of
  `/api/open-questions`; on the WebSocket the same counts travel only as the `review` kind
  `open_questions`, never as a top-level `type`).
- `TurnSectionsResponse { turn, files: Vec<{ path, anchors: Vec<String> }> }`.
- `ImplementedByResponse { sections: BTreeMap<u32, Vec<PlanLink>> }` reusing plan 5's
  `wire::concept::PlanLink`, built from `Project.implemented_by`.
- WS: `ClientMessage::Review(ReviewCommand)` with `ReviewCommand` tagged `kind`: `suggest {
  passage, note }`, `suggestion_answer { id, action: attach|dismiss }`; `PlanMessage::Review(
  ReviewMessage)` with `kind`: `suggestion`, `suggestion_status` (states `sent` with `holder`
  display name and `away: bool`, `attached`, `dismissed`, `dropped` with the reason "authoring
  changed hands"), `open_questions { counts }`,
  `refused { reason }`. The outer enums are plan 2's, tagged `type`; the inner tag is `kind` so
  the two never collide. `SuggestionBox`'s `Delivery.message` values are these serialised.

Fixtures by hand, oxfmt-clean, one realistic case each (a restore with all four results, a
compare with one of each change, a suggestion and its status). `wire/tests_review.rs` follows
plan 2's `wire/tests.rs` pattern: `include_str!`, deserialize, serialize back, compare as
`serde_json::Value`.

## suggest.rs

As the PUBLIC SURFACE states. Ids: `sg-<n>` from a counter in the box (an id is not a secret: an
answer is accepted only from the holder). Keep insertion order. Also `pub fn drop_for_holder_change(&mut self) -> Vec<Delivery>` that
clears everything and returns one `suggestion_status` `dropped` delivery per dropped suggestion,
addressed to its sender, when authoring passes to another browser or frees (the route calls it from
plan 2's holder-change notification), and `pending_for(&self) -> Vec<Value>` so a holder that
reconnects gets the pending cards again. Unit tests for both.

## questions.rs

`open_question_counts(project)`: for every `ProjectDoc` of a product section, metaplan or plan
whose `sections` hold a heading titled `Open questions`, its `open_questions` count (0 kept).
Unit test on a `Project::load` of a TempDir tree.

## Required test cases (coverage audit; named tests)

`suggest.rs`: "drop_for_holder_change clears pending", "pending_for returns pending cards in
order", "answer by a non-holder refused", "unknown id refused", "bad path refused".
`questions.rs`: "zero count kept for empty heading", "documents without the heading omitted".
