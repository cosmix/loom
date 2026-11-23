# init-and-plan-validation / w3-plan-text: metadata fence scanner, amendment fence width, bare plan path, memory entry trim

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/planner-backend.md`, sections CR-44, CR-46 (narrowed form), CR-75 (sub-claim a only).

Unit rules (restated): you run no `git` command, touch no `.loom/` path and run no tests. Your only proof is `cargo check --all-targets` from `loom/`. The orchestrator runs the tests you write. You answer from the base tree and cannot see another worker's edits; the other codex unit (w2) owns `src/plan/schema/**` and never touches your files.

## Findings covered
- CR-44: `extract_yaml_metadata_with_ranges` closes the YAML block at the first run of backticks anywhere, including inside a YAML string such as `description: "wrap code in ``` fences"`; `splice_metadata_yaml` can write a plan whose new body holds a line that closes the fence.
- CR-46 (narrowed): `apply_amendment` fails on a bare relative plan path (`plan.md`) because `checked_plan_path` canonicalizes the empty parent; `prepare_content` reads the plan unbounded. The lock identity claim is refuted; do not touch locking.
- CR-75(a): `EntryBuilder::build` trims `content`, `MemoryEntry::validate` compares exactly, so `loom memory note "text\n"` is refused with "does not round-trip through the journal".

## Files
Own (write), all under `loom/`:
- `src/plan/parser/extraction.rs`
- `src/plan/parser/fence.rs` (new)
- `src/plan/parser/mod.rs`
- `src/plan/amendment.rs`
- `src/plan/amendment/apply.rs`
- `src/plan/amendment/fence_width.rs` (new)
- `src/plan/amendment/fence_tests.rs` (new)
- `src/plan/amendment/relative_path_tests.rs` (new)
- `src/plan/amendment/retry_tests.rs` (visibility of two helpers only)
- `src/fs/memory/types.rs`
- `src/fs/memory/tests/parser_escaping.rs` (declared by `src/fs/memory/parser.rs`; add tests only, no registration needed)

Read only: `src/fs/safe_read.rs` (`read_to_string_bounded`, `OverLimit`), `src/planning/discover.rs` (`MAX_DOC_BYTES`), `src/plan/amendment_catch_up.rs` (second caller of `splice_metadata_yaml`; signature stays).
Frozen (contract tests, never edit): `src/plan/parser/fence_contract_tests.rs`.

Size limits that bind this unit (`loom/maintainability-baseline.txt`): `extraction.rs` is 410 lines (baseline row) and `extract_yaml_metadata_with_ranges` is 51 lines (baseline row); neither may grow, so the new scanning code goes in `fence.rs` and the edit in `extraction.rs` must shrink the function. `src/plan/amendment.rs` is 293 lines: keep `splice_metadata_yaml` at most 50 lines by putting the widening in `fence_width.rs`.

## Changes

### 1. Closing-fence scanner (CR-44, `fence.rs` and `extraction.rs`)
New file `src/plan/parser/fence.rs`, module-private in `parser` (add `mod fence;` in `parser/mod.rs`), with:

```rust
/// Backtick count when `line` (no terminator) can close a fence: at most three
/// leading spaces, a run of three or more backticks, then only whitespace.
fn closing_run(line: &str) -> Option<usize>

/// Byte offset in `section` of the start of the first line, after the line holding `from`,
/// that closes a fence of `fence_len` backticks (a closing run of at least `fence_len`).
pub(super) fn find_closing_fence(section: &str, from: usize, fence_len: usize) -> Option<usize>

/// Fence length that holds `body` without any of its lines closing the fence:
/// the larger of `minimum` and one more than the longest closing-capable run in `body`.
pub(crate) fn fence_len_for(body: &str, minimum: usize) -> usize
```

Rules (CommonMark): `closing_run` counts leading spaces (more than three gives `None`), counts the backtick run (fewer than three gives `None`), and requires the rest of the line to be empty after `trim()` (so a trailing `\r` is fine). `find_closing_fence` starts at the first byte after the next `\n` at or after `from` (the opening line, `yaml` and its terminator, never closes), walks `split_inclusive('\n')` keeping a running byte offset, strips the `\n` before calling `closing_run`, and returns the offset of the line start (not of the first backtick) when the run is at least `fence_len`. `fence_len_for` walks `body.lines()`. Both share `closing_run`.

In `extraction.rs`, `extract_yaml_metadata_with_ranges`: replace the `closing_fence` string and the `.find(&closing_fence)` (the four lines after the comment `Find the closing fence with the same number of backticks`) by `super::fence::find_closing_fence(metadata_section, yaml_content_start_in_section, fence_len)` with the same `ok_or_else` error `No closing {fence} for YAML block` (build the message with `"`".repeat(fence_len)` inline). `yaml_body_end_in_section` becomes the returned offset directly. Byte ranges, CRLF handling and the `yaml` trim stay as they are. In `parser/mod.rs` add `pub(crate) use fence::fence_len_for;` for the amendment path. Do not change `find_yaml_fence` (the opening scan). Count lines: `extraction.rs` at most 410 and the function at most 51.

### 2. Fence width on splice (CR-44, `fence_width.rs`, `amendment.rs`)
New file `src/plan/amendment/fence_width.rs` (add `mod fence_width;` in `amendment.rs`):

```rust
pub(super) fn widen_fences(
    prefix: &str,
    fence_start: usize,
    fence_len: usize,
    suffix: &str,
    wanted: usize,
) -> (String, String)
```

`prefix` is the original text up to and including the `yaml` hint (the `prefix` local in `splice_metadata_yaml`); the opening backtick run sits at `fence_start..fence_start + fence_len`. Return the prefix with that run replaced by `wanted` backticks, and the suffix (`original[body_end..]`, which starts at the closing fence line) with its closing run (after at most three leading spaces) replaced by `wanted` backticks when it is shorter than `wanted`; a longer or equal closing run stays.
In `splice_metadata_yaml` (`amendment.rs`, about line 242): after computing `fence_len`, `body_start`, `prefix`, `suffix`, compute `let wanted = crate::plan::parser::fence_len_for(new_yaml_body, fence_len);`; when `wanted > fence_len` replace `prefix` and `suffix` with the widened strings from `widen_fences(prefix, fence_range.start, fence_len, suffix, wanted)`; otherwise leave both borrowed text untouched. The function returns `String`, so widen, never refuse. Do not forbid backticks in YAML. The caller `amendment_catch_up.rs` needs no change.

### 3. Bare relative plan path and bounded read (CR-46, `apply.rs`)
- Add `fn plan_parent(plan_path: &Path) -> &Path` returning `plan_path.parent()` when it is `Some` and not empty, else `Path::new(".")`.
- `checked_plan_path` (about line 36): use `plan_parent(&plan_path).canonicalize()?` in place of `plan_path.parent().context("plan_path has no parent")?.canonicalize()?`.
- `prepare_content` (about line 56): replace `fs::read_to_string(plan_path)?` by `crate::fs::safe_read::read_to_string_bounded(plan_parent(plan_path), Path::new(plan_path.file_name().context("plan_path has no file name")?), crate::planning::discover::MAX_DOC_BYTES)?` (open `read_to_string_bounded` in `src/fs/safe_read.rs` first and match its parameter types; `plan/inputs.rs` already imports `MAX_DOC_BYTES` the same way). A plan that is a symlink or over the limit is now refused with the typed `OverLimit` error.
- The lock call `lock_plan(&plan_path)` needs no change.

### 4. Memory entry trim (CR-75a, `types.rs`)
In `MemoryEntry::new` store `content.trim().to_string()` (the parameter stays `String`); in the private `with_context_value` store `Some(context.trim().to_string())`. This makes the stored value equal to what `EntryBuilder::build` and the parser produce (`parser.rs` trims context too). Do not touch `validate`, `EntryBuilder::build` or the persistence limits. Callers (`commands/memory/handlers/record.rs`, `handoff/session_content.rs`, `commands/hook/review_harvest.rs`, `orchestrator/adjudication/prompt/freeze_refusal.rs`, `commands/memory/handlers/pending_groups.rs`, `MemoryEntry::receipt`) all go through `new`/`with_context`; none changes.

## Interfaces shared with other workers
None. `crate::plan::parser::fence_len_for(body: &str, minimum: usize) -> usize` is used only by your own `amendment.rs`.

## Tests (write them; the orchestrator runs them)
1. `src/plan/parser/fence.rs` own `#[cfg(test)] mod tests`: `closing_run` cases (three, four, three-with-indent-three accepted; indent four, two backticks, trailing text, `\r` accepted); `find_closing_fence` on a section whose YAML holds `description: "wrap code in ``` fences"` before the real closing line returns the real line; four-backtick fence ignores a three-backtick line; CRLF section; no closer returns `None`; `fence_len_for("a\n```\nb\n", 3) == 4` and `fence_len_for("x: 1\n", 3) == 3`.
2. `src/plan/parser/extraction.rs` is at its baseline: do not add tests there. Put the end-to-end case in `fence.rs` tests: `extract_yaml_metadata` of a full document whose YAML string holds an inline triple backtick returns the whole body (fails today: cut at the inline run).
3. `src/plan/amendment/fence_tests.rs` (new; register `#[cfg(test)] mod fence_tests;` in `amendment.rs`; `use super::*;`): `splice_widens_the_fence_when_the_body_holds_a_closing_line` (original is a minimal plan with a three-backtick fence; `new_yaml_body` is a string with a line of exactly three backticks at column 0; the spliced text contains a four-backtick opening and closing and `extract_yaml_metadata_with_ranges(&spliced).unwrap().yaml == new_yaml_body.trim()`; fails today: the extraction stops at the body's line); `splice_keeps_the_fence_when_nothing_closes_it` (the spliced text equals what the function produced before: opening and closing still three backticks).
4. `src/plan/amendment/relative_path_tests.rs` (new; register `#[cfg(test)] mod relative_path_tests;`): make `fixture` and `request` in `retry_tests.rs` `pub(super)` (visibility only, no other edit) and `use super::retry_tests::{fixture, request};`. `#[serial]` (`serial_test::serial`): `apply_amendment_accepts_a_bare_relative_plan_path`: set the working directory to the fixture's temp directory (restore on drop), call `apply_amendment(Path::new("plan.md"), &work, request("a", AmendmentPatch::Delete { index: 0 }))`, assert `Ok` and that `plan.md` no longer lists criterion `one` for stage `a` (see `assert_policy` in `retry_tests.rs`). Fails today with a `No such file` error from `canonicalize`. Second test `apply_amendment_refuses_an_oversized_plan`: grow the fixture's plan with `File::set_len(MAX_DOC_BYTES as u64 + 1)` and assert the error `downcast_ref::<OverLimit>()` is `Some`.
5. `src/fs/memory/tests/parser_escaping.rs` (its imports already include `append_entry`, `read_journal`, `MemoryEntry`, `MemoryEntryType`, `TempDir`): `a_trailing_newline_in_the_content_is_stored_trimmed`: `MemoryEntry::new(MemoryEntryType::Note, "text\n".to_string())` then `append_entry(temp.path(), STAGE, &entry).unwrap()` and `read_journal` returns content `text`; second case `with_context` with a context ending in a newline stores it trimmed. Fails today: `append_entry` refuses with "does not round-trip". Check the other tests of that file for assertions on untrimmed content before editing; report any.
Existing tests to keep green: every `test_extract_*` in `extraction.rs`, `src/plan/tests/amendment*.rs`, `src/plan/amendment/tests.rs`, `retry_tests.rs`, `retry_recovery_tests.rs`, `tests/plan_amendment_integration.rs`.

Fixture sweep (report only; you own none of these files): `rg -n '\}```' loom/src loom/tests --glob '*.rs'` lists test fixtures that glue a closing fence right after an interpolated value (`{yaml}```\n`, `{stages}```\n`). The new scanner needs the closing fence at the start of its line, so an interpolated value that does not end in a newline would now fail to parse. Read each hit and name in your report every one whose interpolated text can lack a trailing newline.

Contract test this unit turns green (by id): `inline-triple-backtick-keeps-metadata-body`.

## Proof
From `loom/`: `cargo check --all-targets`. Then `wc -l src/plan/parser/extraction.rs src/plan/amendment.rs src/plan/amendment/apply.rs` and state the counts in your report (`extraction.rs` at most 410). You run no tests.

## Traps
- The closing line may be indented up to three spaces; the returned offset is the line start, so the suffix keeps that indentation and the old column-zero behavior is unchanged.
- `find_yaml_fence` keeps its position-blind opening scan; changing it is out of scope and would break `test_find_yaml_fence_*`.
- The four-backtick extraction tests contain a three-backtick line with trailing text (` ```inner `); it must stay inside the body.
- Widen only when `wanted > fence_len`; rewriting an unchanged fence changes bytes the doc comment of `ExtractedMetadata` promises to preserve.
- `fence_len_for` must scan the same predicate as the scanner (`closing_run`), otherwise a body line the scanner treats as a closer is missed.
- Trimming belongs in `new` and `with_context` only; trimming inside `validate` would hide a real round-trip fault.
