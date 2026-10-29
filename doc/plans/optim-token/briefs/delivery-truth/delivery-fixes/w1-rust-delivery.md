# W1 rust-delivery: remove the receipt module and CLI surface, update the worker-brief tests, correct the delivery docs

Read `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md` first. Load `Skill(skill="loom-skills", args="loom-rust")`. All paths below are relative to `loom/` unless they start with `loom-hooks/` or `doc/`.

Two codex units run beside you: C1 rewrites `src/commands/hook/worker_brief.rs` and C2 edits `src/commands/hook/pre_compact.rs` with `src/commands/hook/tests_pre_compact.rs`. Never write those three files. W2 and W3 own the shell hooks and the integration tests; the contract files are frozen.

## Read before editing

- `src/cli/types_ops.rs` (`HookCommands::WorkerBrief`, `HookCommands::ReadReceipt`) and `dispatch_hook` in `src/cli/dispatch.rs`.
- `src/commands/hook/mod.rs` (18 lines) and `src/context/mod.rs` (the `pub mod read_receipts;` line).
- `src/commands/hook/tests_worker_brief.rs` and `src/commands/hook/worker_brief/test_support.rs` in full.
- `src/context/delivery.rs` (doc on `worker_delivery_dir`) and `src/context/delivery/session.rs` (module doc, doc on `worker_recipient_id`).
- `common.md` section 4: the signatures C1 leaves in `worker_brief.rs`, which your tests and your dispatch arm call.

## 1. Delete the read-receipt module and its CLI surface

Before deleting, run `rg -n 'read_receipt|read-receipt|receipt_eligible|ReadReceipt|epoch_token_for_session|rotate_epoch_for_session' src tests` and check the list against the consumers below. At `6fee4ecd` the only consumers outside the deleted files are `src/context/mod.rs`, `src/commands/hook/mod.rs`, `src/cli/types_ops.rs`, `src/cli/dispatch.rs` (yours), `src/commands/hook/pre_compact.rs` and `tests_pre_compact.rs` (C2), `tests/integration/hooks_read_guard.rs` and `hooks_read_guard_receipts.rs` (W2), and the fixture string `"Terra read-receipts"` in `src/plan/schema/structural_checks/worker_table_tests.rs`, which is plan-verify test data and stays. Report any other consumer.

1. Delete `src/context/read_receipts.rs` and the whole directory `src/context/read_receipts/` (`source.rs`, `storage.rs`, `transcript.rs`, `tests.rs`, `tests_extra.rs`). Delete `src/commands/hook/read_receipt.rs`. Use `rm`, never `git rm`.
2. `src/context/mod.rs`: delete the line `pub mod read_receipts;`.
3. `src/commands/hook/mod.rs`: delete `pub mod read_receipt;`. After the module list add `#[cfg(test)]` and `mod tests_cli_surface;` on two lines.
4. `src/cli/types_ops.rs`: delete the `ReadReceipt { prepare, check, complete }` variant together with its doc comment (`/// Maintain the tool-read receipt state machine.`) and its `#[command(hide = true, group(...))]` attribute. Change `WorkerBrief { bind_agent, agent_type, transcript }` to the unit variant `WorkerBrief,`, keeping `#[command(hide = true)]`, with the doc comment `/// Retrieve a scoped brief for a typed worker.`
5. `src/cli/dispatch.rs`, function `dispatch_hook` only: delete the `HookCommands::ReadReceipt { prepare, check, .. } => { ... }` arm and replace the `WorkerBrief` arm with `HookCommands::WorkerBrief => hook::worker_brief::worker_brief(),`. The ledger row `function src/cli/dispatch.rs dispatch 73` pins the function `dispatch`; leave it byte for byte.
6. New file `src/commands/hook/tests_cli_surface.rs` with a one-line module doc and one test, `removed_hook_flags_and_subcommands_do_not_parse`, using `use crate::cli::Cli;` (re-exported at `src/cli/mod.rs:15`) and `use clap::Parser;`: `Cli::try_parse_from(["loom", "hook", "read-receipt", "--check"])` is an error, `Cli::try_parse_from(["loom", "hook", "worker-brief", "--bind-agent", "agent-1"])` is an error, and `Cli::try_parse_from(["loom", "hook", "worker-brief"])` parses. The first two fail at `6fee4ecd`, where both forms parse.

## 2. Test support and worker-brief unit tests

These files call C1's new items through `use super::*;`. Write against the signatures in `common.md` section 4; C1 normally finishes before you reach your check.

1. `worker_brief/test_support.rs`: rename `pending_count` to `record_count`; it counts the `*.json` entries in `fixture.worker_dir()`. Delete `Fixture::transcript`, the `root` field of `Fixture` (its declaration and the `root,` initializer; `write_overlay(&root)` keeps using the local `root`), and the helper `marker`: after the bind tests go nothing reads them (`marker` is used only by `transcript_with` and `marker_before_a_truncated_multibyte_character_binds`, `Fixture::root` only by `Fixture::transcript`), and dead code fails `cargo clippy --all-targets -- -D warnings`. Keep `Fixture`, `payload`, `pack`, `item`, `STAGE`, `PLAN`, `PARENT` and the `#[path = "worker_brief/test_support.rs"] mod support;` line in the test file unchanged: plan 4 builds a test on them.
2. `tests_worker_brief.rs`:
   - Delete `transcript_with` and the tests `one_nonce_binds_once_to_the_first_actual_child`, `marker_before_a_truncated_multibyte_character_binds`, `two_pending_nonces_are_isolated_when_one_child_starts`, `missing_or_ambiguous_transcript_marker_binds_nothing` and `forwarder_binding_credits_only_the_claude_forwarder`.
   - Replace `task_and_owned_path_emit_one_nonce_and_only_a_pending_record` with `issued_brief_files_one_record_load_deliveries_returns`: keep its assertions on the nonce, the scoped path and the single marker line; then assert that `load_deliveries(&fixture.config.work_dir, PLAN, STAGE)` returns exactly one record, whose `recipient_id` equals `worker_recipient_id(STAGE, PARENT, &envelope.nonce)` and whose `delivered` list is not empty, and that `record_count(&fixture) == 1`. Import `worker_recipient_id` from `crate::context::delivery`.
   - Convert `changed_epoch_or_content_reopens_a_bound_child_pack` to `changed_epoch_or_content_reopens_an_issued_worker_pack`: the same three packs filed with `store_record(&fixture.config, NONCE_A, &first)`, `NONCE_B` and `NONCE_C`, no transcripts, no bind, the same three `delivered_in_epoch` assertions.
   - `invalid_payload_and_empty_selection_write_nothing`, `oversized_envelope_fails_closed_without_a_pending_record` (rename it `oversized_envelope_fails_closed_without_a_record`) and `unmet_required_material_is_rendered_without_plan_or_navigation_doctrine` use `record_count`.
   - Add `worker_query_leads_with_the_worker_task`: a `Stage` with id `q-stage` and name `Distinct stage name`; `worker_query_text(&stage, "worker task", "worker prompt")` contains `worker task`, then `worker prompt`, then `Distinct stage name`, in that order (compare `find` offsets).
   - Add `capped_query_still_issues_a_scoped_brief`, the pre-merge proof that the cap leaves issuance working on populated inputs: `let mut fixture = Fixture::new(true);` set `fixture.config.stage.description = Some("stage words ".repeat(4_000));` (48 KB); prompt = the prompt of `issued_brief_files_one_record_load_deliveries_returns` (owned path `src/scoped.rs`) followed by `"\n"` and `"filler words ".repeat(300)` (about 3.9 KB, so the joined query exceeds `WORKER_QUERY_MAX_BYTES` and the stage text is cut); `issue(&payload(&prompt), &fixture.config)` returns `Some`; the brief contains `` `src/scoped.rs` `` and exactly one marker line; `record_count(&fixture) == 1`; `load_deliveries(&fixture.config.work_dir, PLAN, STAGE)` holds one record with a non-empty `delivered` list. At `6fee4ecd` the record half fails (only a `.pending` file is written).
   - Remove the imports the deletions orphan: `delivered_to_session` and `std::path::PathBuf`.

3. `src/context/tests/delivery/session.rs`: `worker_and_child_prompt_recipients_cannot_share_a_namespace` and `nonce_address_and_same_named_child_record_use_isolated_directories` still describe the bind model (a local named `pending`, a "bound" record for "actual-child"). Rename the local `pending` to `worker` and the fixture `bound`/`"bound"` to `issued`/`"issued"`, and rename the second test `nonce_address_and_same_named_prompt_record_use_isolated_directories`. Change no assertion's meaning. The reviewers flag comments and names that describe removed code.

## 3. Delivery doc comments

- `src/context/delivery.rs`, the doc on `worker_delivery_dir` ("Pending and nonce-addressed bound worker receipts, isolated from the ordinary `<recipient>.json` namespace."): `/// Worker-brief delivery records, filed at issue under their nonce-derived key and kept apart from the prompt and spawn records.`
- `src/context/delivery/session.rs`, module doc: the sentence "[`delivered_to_session`] is the read side that respects that key while still honouring the one deliberate exception — a resumed stage session must still see its own spawn brief as delivered." becomes: [`delivered_to_session`] respects that key and also credits the asking stage session's own spawn record for the session's whole life, compaction included (see `commands/hook/pre_compact.rs`). Keep the intra-doc links that resolve today.
- Same file, doc on `worker_recipient_id`: "Filesystem-safe identity for one not-yet-bound worker brief." becomes "Filesystem-safe identity for one issued worker brief: the record's file name and its recipient id."

## Your one check

From `loom/`, once, at the end: `cargo test --lib commands::hook::`. It compiles every unit test in the crate and runs the hook module's tests. If it fails to compile only in `worker_brief.rs`, `pre_compact.rs` or `tests_pre_compact.rs` (the codex units' files), report the error and stop. The contracts in `tests/worker_brief_contracts.rs` run in the verifier round.

## Report

The files you changed and deleted, the consumer list from section 1, the final line counts of `tests_worker_brief.rs` and `test_support.rs`, and anything that did not match this brief.
