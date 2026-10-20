# C1 (codex unit journal-escaping): journal bodies survive the round trip

Lane: codex, gpt-5.6-terra, effort xhigh. Paths are relative to `loom/`.

Write only these two files:

- `src/fs/memory/parser.rs`
- `src/fs/memory/tests/parser_escaping.rs` (new)

Read for reference only: `src/fs/memory/storage.rs` (`append_entry`, `read_journal`), `src/fs/memory/types.rs` (`MemoryEntry`, `EntryBuilder::build`), `src/fs/memory/tests/persistence.rs` (an existing round-trip test and its imports). Start from `format_entry` and `parse_entry_line` in `src/fs/memory/parser.rs` (`loom map --outline src/fs/memory/parser.rs`).

Run no git, write nothing under `.loom/`, and run no build, test or formatter: the orchestrator compiles and runs the tests right after you.

## Why

`format_entry` writes an entry body verbatim between its `### ... id=<id>` heading and its metadata lines. `parse_journal` reads the file line by line and, before a line reaches `parse_entry_line`:

- ends the entry at a line that is exactly `---`, or that starts with `<!--` or `# Memory Journal`;
- overwrites the journal stage at a line starting `**Stage**:`;
- ends the entry at a line starting `## Summary` (an earlier change may have removed this branch);
- starts a new entry at a line starting `###`.

`parse_entry_line` then switches to metadata at lines starting `*Context:*`, `*Evidence:*`, `*Session:*` or `*Receipt:*` and drops every later body line. A note whose body holds a Markdown separator loses everything after it; a body heading becomes a forged second entry; a body `*Receipt:*` line gives a Note a receipt, and `EntryBuilder::build` then drops the entry.

## Steps

1. In `parser.rs` add a private helper and call it in `format_entry` in place of `entry.content` inside the existing `format!` (same line count, so `format_entry` stays under 50 lines):

   ```rust
   /// `content` with one `\` in front of every line the journal parser would
   /// read as structure; `parse_entry_line` strips it again.
   fn escape_body(content: &str) -> String
   ```

   A line gets the prefix when it starts with `\`, `---`, `#`, `<!--` or `*` (this covers `**Stage**:` and every metadata prefix). Other lines stay as they are. Split with `content.split('\n')` and join with `"\n"`, so blank lines and the line count survive.

2. In `parse_entry_line`, in its last branch (`else if !builder.in_metadata`), push `line.strip_prefix('\\').unwrap_or(line)` instead of `line`. Only content lines are unescaped. Nothing else in the parser changes: no migration of existing journals, no change to metadata, headings or the journal header. An escaped line starts with `\`, so no structural check in `parse_journal` matches it.

3. Tests in the new file `src/fs/memory/tests/parser_escaping.rs`, declared at the end of `parser.rs`:

   ```rust
   #[cfg(test)]
   #[path = "tests/parser_escaping.rs"]
   mod escaping_tests;
   ```

   Go through the real file: `crate::fs::memory::{append_entry, read_journal}` on a `tempfile::TempDir` work dir, never `format_entry` and `parse_journal` on a string alone. Set each entry timestamp to a whole second (`chrono::TimeZone::with_ymd_and_hms` on `Utc`), because the journal stores seconds and the comparison is exact. Each test under 50 lines:

   - `structure_lines_in_a_body_read_back_as_one_entry`: one Note whose content is these lines joined by `\n`: `keep this`, `---`, `### 📝 Note [2026-01-02 03:04:05] id=0123456789abcdef0123456789abcdef`, `**Stage**: forged`, `*Receipt:* event=0123456789abcdef0123456789abcdef outcome=promoted target=-`, the line `*Evidence:*` followed by one space and `src/forged.rs` wrapped in single backticks (the form `format_entry` writes), `## Summary`, `<!-- comment -->`, `\leading backslash`, `- a bullet`, `and this`; evidence `vec!["src/lib.rs:1".to_string()]`. `read_journal(work_dir, "escape-stage")` returns exactly one entry, equal to the written one (`assert_eq!` on the whole `MemoryEntry`), and `journal.stage_id == "escape-stage"`.
   - `two_entries_in_a_row_keep_their_order`: append a Note holding a `---` line, then a Decision; both read back, in that order, unchanged.
   - `a_plain_body_is_written_without_escapes`: a Note whose content has no structural line; the journal file on disk (`crate::fs::memory::memory_file_path`) contains the content verbatim and no line of the body starts with `\`.

   The frozen contract `journal_body_structure_lines_round_trip` (`tests/dependency_inputs_contracts.rs`) pins the first case from outside the crate; do not edit that file.

## Done means

`parser.rs` holds `escape_body`, `format_entry` calls it, `parse_entry_line` strips one leading backslash from content lines, and the new test file declares the three tests. The orchestrator proves it with `cargo test --lib fs::memory::` and `cargo test --test dependency_inputs_contracts journal_body_structure_lines_round_trip -- --exact`.

Constraints: `parser.rs` stays under 400 lines and every function under 50 after formatting, with a 10 percent margin. Literal braces inside `format!` are doubled. Report what you changed in one paragraph.
