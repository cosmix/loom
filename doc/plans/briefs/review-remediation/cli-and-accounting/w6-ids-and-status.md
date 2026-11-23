# cli-and-accounting / w6-ids-and-status: sanitised stage events, one stage-id validator

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/security.md`, section CR-54; `doc/plans/briefs/review-remediation/validation/verification.md`, section CR-38 (narrowed).

You run no `git` command, touch no path under `.loom/`, and run no tests; your proof is `cargo check --all-targets` from `loom/`. You answer from the base tree and cannot see other workers' edits; no other worker edits a file you own. Use `loom map --outline <file>` and `rg` to find the symbols named below.

## Findings covered
- CR-54: `StatusData.activity` (the last stage events) is built from `stage_events::read_recent` with no sanitising. A session writes the `reason` (via `loom stage block`) and a hostile stage id would be rendered by the dashboard as it is.
- CR-38 (narrowed): `valid_stage_path_component` (used by the memory journal path check and the status collector) and the private `valid_stage_id` in `orchestrator/tick.rs` accept ids that `validate_id` rejects (a dot, an `NN-` prefix, reserved names). Plans enforce `validate_id` on every stage id, so no real stage id differs; the fix removes the second spelling.

## Files
Own (write):
- `loom/src/validation.rs`
- `loom/src/commands/memory/handlers/work_dir.rs`
- `loom/src/status/data/sanitize.rs`
- `loom/src/status/data/sanitize_stage_event_tests.rs` (new)
- `loom/src/status/data/collector_snapshot.rs`
- `loom/src/orchestrator/tick.rs`

Read only: `loom/src/fs/stage_events.rs` (`StageEvent`: `at`, `stage_id`, `from`, `to`, `session: Option<String>`, `session_type`, `reason: Option<String>`), `loom/src/stage_cache/mod.rs`, `loom/src/native_worker/ids.rs`.
Frozen (never edit): `loom/src/status/data/activity_sanitize_contract_tests.rs` (new, written before your work).
Not yours: `loom/src/stage_cache/mod.rs` (`StageCache::resolve` has its own weaker check; it is outside this stage) and `native_worker::ids::validate_stage_id` (already ends in `validate_id`; leave it).

## Changes

### 1. CR-38: one validator (`validation.rs`, `tick.rs`)
- `validation.rs`: `valid_stage_path_component(id: &str) -> bool` becomes `validate_id(id).is_ok()` (keep the name and `pub`; the three callers are `commands/memory/handlers/work_dir.rs`, `status/data/sanitize.rs` (re-export as `valid_stage_id`) and the tick below). Update its doc comment: it is `validate_id` as a predicate (at most 128 bytes, ASCII alphanumerics, `-` and `_`, no reserved name, no `NN-` prefix), so it also forbids `.`, `/`, `\` and `..`.
- `tick.rs`: delete `fn valid_stage_id` and `const MAX_STAGE_ID_BYTES` (with its doc comment). In `parse_busy` use `.filter(|id| crate::validation::validate_id(id).is_ok())`; in the `debug_assert!` of `record_busy` use `crate::validation::validate_id(&busy.stage_id).is_ok()`. Update the format doc at the top of the file only if it names the character set (it does not today).
- `work_dir.rs` (`validate_stage_id`): no code change; it calls `valid_stage_path_component`. Keep its error text.

### 2. CR-54: sanitise stage events (`sanitize.rs`, `collector_snapshot.rs`)
- `sanitize.rs`: add `pub(super) fn sanitized_stage_event(mut event: StageEvent) -> Option<StageEvent>` (import `crate::fs::stage_events::StageEvent`): return `None` when `!valid_stage_id(&event.stage_id)` (the id is a path component elsewhere and the dashboard prints it; a valid id contains no control or bidi character, so it needs no further flattening); otherwise `event.session.iter_mut().for_each(flatten);` `bound_close_reason(&mut event.reason);` and `Some(event)`. `flatten` and `bound_close_reason` already exist in the file. Add a doc comment naming the trust boundary: the event log is written by sessions.
- Declare the new test file at the end of `sanitize.rs`, next to the other two: `#[cfg(test)] #[path = "sanitize_stage_event_tests.rs"] mod stage_event_tests;`.
- `collector_snapshot.rs`: in `collect_status_data`, `activity: crate::fs::stage_events::read_recent(..).into_iter().filter_map(sanitized_stage_event).collect()`, with `use crate::status::data::sanitize::sanitized_stage_event;` at the top of the file (the module is private to `status::data` and visible to its descendants).

## Interfaces shared with other workers
None.

## Tests (write them; the orchestrator runs them)
Existing tests whose assertions change (CR-38): these two accepted `stage-1.a_b`, which is now refused.
- `commands/memory/handlers/work_dir.rs`, `stage_ids_require_safe_path_components`: `("stage-1.a_b", true)` becomes `("stage-1.a_b", false)`; add `("stage-1_a-b", true)`, `(".hidden", false)`, `("01-foo", false)`, `("con", false)`.
- `status/data/sanitize.rs`, `stage_ids_that_could_escape_a_directory_are_rejected`: `assert!(valid_stage_id("stage-1.a_b"))` becomes `assert!(!valid_stage_id("stage-1.a_b"))`; add `assert!(valid_stage_id("stage-1_a-b"))` and `assert!(!valid_stage_id(".hidden"))`, `"01-foo"`, `"con"`.
- `orchestrator/tick.rs`, `test_record_busy_roundtrips_the_busy_line` uses `busy_for("stage-a.1_x")`; change the id to `"stage-a-1_x"` (a dot would now trip the `debug_assert`). In `test_bad_busy_lines_read_as_not_busy` add the lines `busy provisioning {now} a.b`, `busy provisioning {now} 01-foo`, `busy provisioning {now} con`.
- `validation.rs` tests module: `valid_stage_path_component_follows_validate_id`: for `"stage-1"`, `"a_b"` true; for `"stage-1.a_b"`, `".hidden"`, `"01-foo"`, `"con"`, `""`, `"a/b"`, `".."` false.
New tests for CR-54 in `sanitize_stage_event_tests.rs` (`use super::*;` plus `chrono::Utc`, `StageStatus`):
- `a_stage_event_reason_and_session_are_neutralised`: an event with `stage_id: "stage-1"`, `to: StageStatus::Blocked`, `reason: Some("\u{202E}x\u{1b}[2J".into())`, `session: Some("s\u{1b}[31m1".into())`; `sanitized_stage_event` returns `Some` with neither `reason` nor `session` containing `\u{202E}` or `\u{1b}`. Today the function does not exist and the collector passes the event through untouched.
- `a_stage_event_with_a_control_character_in_its_id_is_dropped`: `stage_id: "stage-\u{1b}[2J"` gives `None`; `stage_id: "a.b"` gives `None`; `"stage-1"` gives `Some`.
- Contract to turn green: `status::data::activity_sanitize_contract_tests::a_stage_event_with_a_control_character_in_its_id_is_not_reported`.

## Proof
From `loom/`: `cargo check --all-targets`. Nothing else.

## Traps
- The validation section for CR-38 says "none in the tree" for tick callers of a dotted id; the roundtrip test above is one. Search with `rg 'busy_for\(|record_busy'` before finishing.
- `sanitize.rs` is 321 lines; keep new code in it under 20 lines and put the tests in the new file.
- `collector.rs` imports `sanitize` items on one line; do not edit it, `collector_snapshot.rs` takes the new function by full path.
- Dropping an event, rather than blanking its id, is deliberate: an event for an id the plan could never contain is not a real stage event.
- `StageCache::resolve` is outside your files; do not touch it even though the validation section lists it.
