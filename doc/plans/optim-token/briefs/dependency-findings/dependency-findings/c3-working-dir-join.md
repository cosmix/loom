# C3 (codex unit working-dir-join): a stage path made repository-relative

Lane: codex, gpt-5.6-terra, effort xhigh. Paths are relative to `loom/`.

Write only these three files:

- `src/context/rank_source/paths.rs`
- `src/context/rank_source/paths_tests.rs` (new)
- `src/context/rank_source.rs`: one line only, the re-export below

Start from `normalize_dependency_path` and `names_dependency_path` in `src/context/rank_source/paths.rs` (`loom map --outline src/context/rank_source/paths.rs`).

Run no git, write nothing under `.loom/`, and run no build, test or formatter: the orchestrator compiles and runs the tests right after you.

## Why

Source-graph node paths are repository-relative (`loom/src/...`). A stage declares its `files` and `artifacts` relative to its `working_dir` (`src/...` with `working_dir: loom`), so the exact comparison in `names_dependency_path` never matches them. A memory entry evidence path is relative to its producer stage `working_dir` too. Both need one lexical join.

## Steps

1. Add beside `normalize_dependency_path` in `paths.rs` (exact signature; later workers call it):

   ```rust
   /// `path` joined to the project-relative directory `working_dir`, with `.` and
   /// `..` resolved lexically and the result normalized like
   /// [`normalize_dependency_path`]. `None` for an empty or absolute `path`, or
   /// when `..` climbs above the project root. Glob characters pass through.
   pub fn join_working_dir(working_dir: Option<&str>, path: &str) -> Option<String>
   ```

   - Normalize `path` with `normalize_dependency_path` first (it trims, turns `\` into `/` and drops leading `./`). Empty, or starting with `/`: `None`.
   - A `working_dir` of `None`, `""` or `"."` adds nothing; otherwise its segments (normalized the same way, trailing `/` ignored) come first.
   - Walk all segments: skip `""` and `"."`; `".."` pops one segment, and popping from an empty list returns `None`; push anything else. An empty result is `None`. Join with `/`.
   - No filesystem access, no `canonicalize`.

2. Tests in the new file `src/context/rank_source/paths_tests.rs`, declared at the end of `paths.rs`:

   ```rust
   #[cfg(test)]
   #[path = "paths_tests.rs"]
   mod tests;
   ```

   One test function per line below, each asserting `join_working_dir` with `use super::*;`:

   - `(Some("loom"), "src/a.rs")` gives `Some("loom/src/a.rs")`;
   - `(None, "./src/a.rs")` and `(Some("."), "src/a.rs")` give `Some("src/a.rs")`;
   - `(Some("loom"), "../web/x.ts")` gives `Some("web/x.ts")`;
   - `(Some("loom"), "../../x")` gives `None`;
   - `(None, "/abs/x")` and `(Some("loom"), "")` give `None`;
   - `(Some("loom"), "src/**/*.rs")` gives `Some("loom/src/**/*.rs")`;
   - `(Some("loom/"), "src\\a.rs")` gives `Some("loom/src/a.rs")`.

3. In `src/context/rank_source.rs` widen the existing line `pub use paths::normalize_dependency_path;` to `pub use paths::{join_working_dir, normalize_dependency_path};`. Same line count; the file is 378 lines and gains none.

## Done means

`join_working_dir` exists with that signature and is reachable as `crate::context::rank_source::join_working_dir`; the test file holds the cases above. The orchestrator proves it with `cargo test --lib context::rank_source::paths::`.

Constraints: every file under 400 lines and every function under 50 after formatting. Report what you changed in one paragraph.
