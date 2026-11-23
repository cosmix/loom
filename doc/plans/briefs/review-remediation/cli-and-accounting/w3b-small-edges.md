# cli-and-accounting / w3b-small-edges: quota suffix underflow, dead graph display, bounded session read

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md`, section CR-77 (the items named below only).

You run no `git` command, touch no path under `.loom/`, and run no tests; your proof is `cargo check --all-targets` from `loom/`. You answer from the base tree and cannot see other workers' edits; no other worker edits a file you own. Use `loom map --outline <file>` and `rg` to find the symbols named below.

## Findings covered
- CR-77 `append_suffixes`: with an empty text and less room than `SUFFIX_GAP`, `room -= SUFFIX_GAP + needed` underflows (a panic in debug builds).
- CR-77 `build_graph_display`: dead outside its own tests; `show()` uses `build_tree_display`. Removing it also leaves `status_priority` without a caller.
- CR-77 `provider_data/sessions.rs`: `load_session_from_file` reads a session file with an unbounded `fs::read_to_string` that follows symlinks and blocks on a FIFO.

## Files
Own (write):
- `loom/src/commands/status/ui/tui/ledger/quota.rs`
- `loom/src/commands/status/ui/tui/ledger/quota_tests.rs`
- `loom/src/commands/graph/display.rs` (delete the file)
- `loom/src/commands/graph/mod.rs`
- `loom/src/commands/graph/indicators.rs`
- `loom/src/commands/graph/tests.rs`
- `loom/src/provider_data/sessions.rs`
- `loom/src/provider_data/sessions_tests.rs` (new)

Read only: `loom/src/fs/safe_read.rs` (`read_to_string_bounded`), `loom/src/fs/session_files/` (`save_session`), `loom/src/fs/work_dir.rs` (`sessions_dir`).
Not yours: `loom/maintainability-baseline.txt`. Removing the tests below shrinks `graph/tests.rs` below its recorded 425 lines; the orchestrator deletes that baseline row. Do not edit the file.

## Changes

### 1. `append_suffixes` (`ledger/quota.rs`)
In the loop, replace the test `needed <= available` with `needed + SUFFIX_GAP <= room`. The branch body (`room -= SUFFIX_GAP + needed; shown.push(text)`) stays. Keep `let available = room.saturating_sub(SUFFIX_GAP);` for the `else if available >= SUFFIX_MIN_CELLS` truncation branch. Both conditions agree whenever `room >= SUFFIX_GAP`; the new one cannot underflow when it is smaller.

### 2. Remove `build_graph_display` (`commands/graph/`)
- Delete `display.rs` (91 lines).
- `mod.rs`: remove `mod display;`, `pub use display::build_graph_display;`, the `display` bullet in the module doc comment, and `status_priority` from the line `pub use indicators::{status_indicator, status_priority};`. Keep `pub use levels::compute_stage_levels;` (`tree.rs` uses `levels::compute_stage_levels` through `super`).
- `indicators.rs`: delete `status_priority` (its only caller was `display.rs`; `rg status_priority loom` finds no other use) and change the module doc to say it provides the status indicator only.
- `tests.rs`: remove `use super::display::build_graph_display;` and the eight tests `test_build_graph_display_empty`, `..._single_stage`, `..._linear_chain`, `..._diamond_pattern`, `..._shows_all_deps`, `..._multiple_roots`, `..._all_statuses`, `..._status_sorting`. Keep `strip_ansi`, `create_test_stage`, `test_status_indicator`, `test_compute_stage_levels` and every `test_tree_*` test. Remove an import or helper only when nothing left uses it (`cargo check --all-targets` warns).

### 3. Bounded session read (`provider_data/sessions.rs`)
- Add `const MAX_SESSION_FILE_BYTES: usize = 1024 * 1024;` (the limit the other session readers use: `fs/session_files/enumerate.rs`, `stage_service/ownership.rs`).
- `load_session_from_file` takes the sessions directory and the file name: `fn load_session_from_file(sessions_dir: &Path, name: &OsStr) -> Result<Session>`, reading with `crate::fs::safe_read::read_to_string_bounded(sessions_dir, Path::new(name), MAX_SESSION_FILE_BYTES)` and keeping the context `format!("Failed to read session file: {}", sessions_dir.join(name).display())`. The call in `load_all_sessions` passes `&sessions_dir` and `&entry.file_name()`; keep the `.md` extension test on `path` and the existing `eprintln!` warning on `Err`. Replace `use std::fs;` with only what is still used (`fs::read_dir` stays).
- Add `#[cfg(test)] #[path = "sessions_tests.rs"] mod tests;` at the end of the file.

## Interfaces shared with other workers
None.

## Tests (write them; the orchestrator runs them)
- `ledger/quota_tests.rs`: `an_empty_suffix_with_no_room_does_not_underflow`: `let mut spans = vec![Span::raw("x".repeat(10))]; append_suffixes(&mut spans, vec![String::new()], 10);` asserts no panic and `spans.len() == 1`. Panics in a debug build today (`room` is 0, `needed` is 0). `Span` is already imported by the file's parent (`use super::*;`).
- `provider_data/sessions_tests.rs` (`use super::*;`, `tempfile::tempdir`, `crate::fs::work_dir::WorkDir`):
  - `an_oversize_session_file_is_skipped`: `WorkDir::new(root)`, `initialize()`, write a valid session with `crate::fs::session_files::save_session(&Session::new(), work.root())`; write a second valid session file under `work.sessions_dir()` by copying the first's text to `big.md` and appending `"\n"` plus `MAX_SESSION_FILE_BYTES + 1` bytes of `x` (the parser reads only the front matter, so it would load today). `load_all_sessions(&work)` returns exactly one session. Today it returns two.
  - `a_fifo_named_like_a_session_does_not_block_the_load`: `libc::mkfifo` of `<sessions_dir>/pipe.md` (pattern: `commands/usage/spawn_join_tests.rs`), run `load_all_sessions` on a thread and `recv_timeout(5 s)`; assert it returned and the valid session loaded. Today it blocks.
- Existing tests removed with the dead function: the eight `test_build_graph_display_*` tests listed above, because the function under test is deleted.

## Proof
From `loom/`: `cargo check --all-targets`. Nothing else.

## Traps
- Do not delete `tree.rs`'s use of `levels::compute_stage_levels` or the `colors` module: `plan/verify/human.rs` imports `commands::graph::colors::stage_color`.
- `tests.rs` is over 400 lines today (baseline 425) and only shrinks; it must not grow.
- `read_to_string_bounded` takes a root directory and a relative path; passing the full file path as the root would fail every read.
- `Session::new()` is the constructor the status contract tests use (`crate::models::session::Session`).
