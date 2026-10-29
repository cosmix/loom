# C1 worker-brief (codex unit): cap the worker query, file the record at issue, delete the bind path

Read `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md` sections 2 to 4 first. All paths below are relative to `loom/`.

## Files

- Write: `src/commands/hook/worker_brief.rs`. Nothing else.
- Read only: `src/commands/hook/tests_worker_brief.rs` and `src/commands/hook/worker_brief/test_support.rs` (W1 edits both while you work), `src/context/delivery.rs` (`worker_delivery_dir`, `DeliveryRecord::from_pack`), `src/context/delivery/session.rs` (`worker_recipient_id`), `src/orchestrator/signals/retrieval.rs` (`StageQuery::build_stage_query_text`), and the frozen contract file `tests/worker_brief_contracts.rs`.
- Never run git. Never read or write a path under `.loom/`. Do not run cargo: W1 deletes modules from the same crate while you work, so a build shows errors that are not yours. The stage orchestrator compiles and tests after every worker returns.

## Steps

1. **Cap the worker query.** Above `issue`, add `pub const WORKER_QUERY_MAX_BYTES: usize = 2_048;` with a doc comment: retrieval time grows with query length (about 1.7 s at 2,048 bytes; 9.6 s for a recorded loom spawn whose query carried an 11 KB stage description, against the spawn guard's former 3-second bound); `spawn-guard.sh` bounds this delegate at 5 seconds; revisit the cap once retrieval cost stops growing with query length. Below it add `pub fn worker_query_text(stage: &Stage, description: &str, prompt: &str) -> String`: join `description`, `prompt` and `StageQuery::build_stage_query_text(stage)` with `"\n"`, in that order; when the result is longer than `WORKER_QUERY_MAX_BYTES`, truncate it at the largest index at or below the cap for which `str::is_char_boundary` holds. Never slice `&text[..WORKER_QUERY_MAX_BYTES]`: it panics inside a multi-byte character. Doc comment: the worker's own task leads because the stage text already reaches the stage session through its signal's brief. In `retrieve_pack`, replace the five lines that build `text` (the `build_stage_query_text` call, two `push('\n')`, two `push_str`) with `let text = worker_query_text(&config.stage, &payload.tool_input.description, &payload.tool_input.prompt);`. Leave `declared_paths` in `issue` reading the whole prompt: declared paths feed `query.dependency_paths`, not the text.
2. **File the record at issue.** Rename `store_pending` to `store_record`, keeping the signature `fn store_record(config: &Config, nonce: &str, pack: &ContextPack) -> Result<Option<()>>`. It computes `dir` and `key` as today, builds `bound = dir.join(format!("{key}.json"))` and `DeliveryRecord::from_pack(key, pack)`, and under `locked_dir_update` returns `Ok(None)` when `bound` exists, otherwise writes it with `atomic_write_locked` and returns `Ok(Some(()))`. The `pending` path and the `.pending` suffix are gone. Doc comment: the brief rides the spawn's own `updatedInput`, so the record is filed when the delegate issues the brief; a delegate killed between this write and its stdout leaves a record for a brief the spawn never carried, which is accepted because a delivery record is an optimisation (`context::delivery` module doc). In `issue`, call `store_record` where it called `store_pending`.
3. **Delete the bind path.** Delete `bind_pending`, `bind_nonce`, `read_transcript`, `transcript_nonce`, `collect_value_markers` and the constant `MAX_TRANSCRIPT_BYTES`. Keep `marker_nonce`: `render_brief` calls it. `worker_brief` becomes `pub fn worker_brief() -> Result<()>` with the body `emit_normal(); Ok(())` and the doc comment `/// Print one worker-brief envelope for the spawn payload on stdin, or \`{}\`.` The module doc becomes `//! Scoped retrieval briefs for typed Task/Agent workers, each filed as a delivery record when it is issued.` Remove the imports the deletions orphan, and only those: `MAX_ID_LENGTH` (keep `validate_id`),`use std::fs;`,`use std::collections::BTreeSet;`,`use serde_json::Value;` and `Context` from `anyhow` (keep `Result`).`Path` and `PathBuf`stay (`bullet_path` and `HookPayload` use them).

## Constraints the graph does not show

- W1 changes the caller in `src/cli/dispatch.rs` to `HookCommands::WorkerBrief => hook::worker_brief::worker_brief(),` and W1's tests call `store_record(&fixture.config, NONCE_A, &pack)` and `worker_query_text(&stage, "worker task", "worker prompt")` through `use super::*;`. Keep these three signatures exactly as written above.
- The frozen contracts in `tests/worker_brief_contracts.rs` call `loom::commands::hook::worker_brief::{worker_query_text, WORKER_QUERY_MAX_BYTES}` from outside the crate, so both stay `pub`.
- Leave the three `None` returns in `issue` (empty pack without skills, oversized envelope, failed store) as they are. The replay in `common.md` section 2 measured a 12.1 KB envelope against `max_payload_bytes` 16,384, so no trimming is added.
- Keep `#[cfg(test)] #[path = "tests_worker_brief.rs"] mod tests;` at the end of the file.
- Name `StageQuery::build_stage_query_text` in doc comments in backticks only, never as a bracketed intra-doc link: it is `pub(crate)` (`src/orchestrator/signals/retrieval.rs`), and `RUSTDOCFLAGS='-D warnings' cargo doc` fails on `rustdoc::private_intra_doc_links` from a `pub` item.
- The doc comment on `worker_query_text` also states the cost of the order: when description plus prompt reach 2,048 bytes the stage text is cut entirely. A recorded loom spawn prompt is 3,580 bytes, so on loom's own stages the query is the worker's description and prompt; the stage text reaches the stage session through its signal.
- `cargo fmt` runs after you and can lengthen lines; end the file at 360 lines or fewer (it is 400 now). Inside `format!` strings, literal braces are doubled.

## Done when

From `loom/`, each of these holds:

- `rg -n 'bind_pending|bind_nonce|read_transcript|transcript_nonce|collect_value_markers|MAX_TRANSCRIPT_BYTES|store_pending|\.pending' src/commands/hook/worker_brief.rs` prints nothing.
- `rg -c 'worker_query_text|store_record|WORKER_QUERY_MAX_BYTES' src/commands/hook/worker_brief.rs` counts at least 5.
- `wc -l < src/commands/hook/worker_brief.rs` prints 360 or less.

Report the final line count and any step that did not match the file.
