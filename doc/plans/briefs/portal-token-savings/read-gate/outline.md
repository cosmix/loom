# Markdown outline

The stage description defines the behaviour. The frozen contracts `fenced_headings_are_not_outlined` and `symlinked_markdown_gets_no_rows` in `loom/tests/read_gate_contracts.rs` must pass. Never edit them.

## Why not a source-graph extractor

An extractor would change cache identity and the coverage figures, would miss untracked files, and would still pay graph load (1.4 s warm, about 9 s cold, against the hook's 2 s bound). A direct read has none of these costs.

## New module

Create `loom/src/map/views/markdown_outline.rs` and declare it with `pub mod markdown_outline;` in `loom/src/map/views/mod.rs`, because the wiring check needs the literal `markdown_outline::` in `map.rs`. `map.rs` is 383 lines: its wiring gets at most 17 lines including imports, and the logic stays in the new module:

```rust
pub fn is_markdown_path(arg: &str) -> bool;
pub fn render(project_root: &Path, arg: &str) -> String;
pub fn to_json(project_root: &Path, arg: &str) -> serde_json::Value;
```

## Parsing

- Use `crate::fs::knowledge::splice::{atx_heading, fence_mask, section_end}`, as `planning/sections.rs` does. Do not write another scanner. Headings are column-zero, matching the chunker and `replace-section`. Strip a trailing run of `#` from the title yourself, since `atx_heading` keeps it. Setext headings are out of scope, with a comment saying so.
- Range: the range ends at `section_end`.
- Scope: the enclosing headings' titles joined with `::`, the separator `format_node_line` uses.
- Signature: the heading's source text, trimmed.

## Rendering

- Reuse the row format. Extract the `{:<11} {:<11} {:<30} {}` format from `outline.rs::format_node_line` into a shared helper, and call it from both, with the kind column `heading`. `_read_discipline.sh` parses this format through its `L<a>-L<b>` regex. Each row carries the two-space prefix `render_outline` adds at its call site, not inside `format_node_line`: `_read_discipline.sh`'s regex requires leading whitespace, and without it every Markdown denial says no outline.
- Pass scope and signature through `super::json::safe`, and cap the signature with `crate::utils::truncate_for_display` at `SIGNATURE_MAX_LEN`, as `outline.rs` does. Heading text is repository-controlled and reaches agents.
- End with `coverage: markdown headings`.
- Print at most 200 rows, in source order. Past that, print one line `... <n> more headings` that does not match the row regex.

## Reading

- Use `project_relative` from `views/mod.rs`, then `crate::fs::safe_read::read_bounded(project_root, Path::new(&rel), 4 * 1024 * 1024)`, and decode with `String::from_utf8_lossy`.
- `project_relative` canonicalizes, which follows a symlink before the read. Check the path as given: if the argument resolved to something outside the root, or the file at `<root>/<as-given relative path>` is a symlink, treat it as refused. `read_bounded` must receive the uncanonicalized relative path to refuse a symlink inside the root. The contract `symlinked_markdown_gets_no_rows` drives this.
- The hook always passes an absolute path, and `project_root` is `.` when the cwd is the workspace. Check the final component with `symlink_metadata` on the argument as given, and take `rel` from `project_relative`; never strip an uncanonicalized root.
- A refused or oversized file prints the header line and `coverage: markdown unavailable - <reason>`, with no rows. Pass the reason through `safe`.

## JSON

`to_json` mirrors `outline_json`: `path`, `match` (`MatchMode::Id.as_str()`), `coverage` with `status`, and `symbols` with `id` as `<rel>#L<start>`, `kind` as `heading`, `scope`, `line_start`, `line_end` and `signature`. Print `{"schema": MAP_SCHEMA, "views": {"outline": to_json(..)}}`; `snapshot` and `coverage` are absent because no graph loaded.

## Wiring

In `loom/src/commands/map.rs::execute_graph_views`, after `project_root` resolves and before `load_graph`, serve the outline directly when `args.outline` is a Markdown path, `requested_views(args) == 1` and `!args.census`. Honour `--json` and print with the same `Output` handling. Do not change the graph path for any other file. Check `--timings` still behaves, and that `loom-hooks/_read_discipline.sh`'s `_loom_outline_covered_rows` needs no change.

`loom/src/commands/tests_map.rs` and `loom/src/map/views/tests.rs` may pin `lexical-only` for a Markdown outline: `outline_of_a_lexical_only_file_states_status_and_detail` in `views/tests.rs` calls `render_outline` directly, so it should be unaffected. Confirm this. Put non-contract tests in `loom/src/map/views/markdown_outline_tests.rs`: nested levels, closing a longer fence, an empty file, 201 headings.

## Check

Run `cargo test --manifest-path loom/Cargo.toml --lib map::` once. Report the result to the orchestrator.
