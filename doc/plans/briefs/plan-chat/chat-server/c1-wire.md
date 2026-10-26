# chat-server C1: wire types for review routes

Codex unit (gpt-6.1-terra, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/wire/review.rs`; `src/plan_server/wire/review_tests.rs`
- May read: `doc/plans/PLAN-plan-chat.md (or its IN_PROGRESS- spelling) stage chat-server PUBLIC SURFACE`; `doc/plans/briefs/plan-chat/chat-server/w1-wire.md section wire/review.rs (the full list of bodies and messages)`; `src/plan_server/wire/concept.rs (plan 5: PlanLink)`

## Entry points

Types: RestoreRequest, RestoreResponse, RestoreFile, HashesResponse, RefResponse, CompareResponse, ChangedDocWire, AtCommitResponse, BuiltResponse, BuiltNoteWire, OpenQuestionsMessage, TurnSectionsResponse, ImplementedByResponse, ReviewCommand (tag kind: suggest, suggestion_answer), ReviewMessage (tag kind: suggestion, suggestion_status, open_questions, refused).

## Steps

1. Write every type listed in the w1-wire brief section wire/review.rs, serde rename_all snake_case, tag kind on ReviewCommand, ReviewMessage, RestoreFile result and ChangedDocWire change.
2. suggestion_status carries state sent | attached | dismissed | dropped, with holder and away for sent and reason for dropped.
3. Every path field gets a rustdoc line: repo-relative, starts with doc/. In review_tests.rs (declared by the orchestrator in wire/mod.rs as a cfg(test) module; start it with use super::review::*;) write tests every_review_command_kind_round_trips, every_review_message_kind_round_trips, suggestion_status_states_serialize_snake_case, asserting the exact JSON kind strings.

## Done

The file compiles with the module line already in wire/mod.rs.

Proof (the orchestrator runs it after you): `cargo test --lib plan_server::wire::review_tests::`

## Traps

Do not add the PlanMessage or ClientMessage variants: another worker owns message.rs. The inner tag is kind because the outer enums use type.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
