# cli-and-accounting / w3-usage-and-misc: bounded transcript reads for `loom usage`, anchored stage filter

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/security.md`, section CR-16; `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md`, section CR-58 (sub-claim c only).

You run no `git` command, touch no path under `.loom/`, and run no tests; your proof is `cargo check --all-targets` from `loom/`. You answer from the base tree and cannot see other workers' edits; no other worker edits a file you own. Use `loom map --outline <file>` and `rg` to find the symbols named below.

## Findings covered
- CR-16: the `loom usage` Claude and Codex transcript scanners read lines with `BufRead::lines()` (no cap, one multi-GB line grows memory without bound) and open with `File::open` (a FIFO in the Codex sessions tree blocks `open` forever).
- CR-58(c): `filter_stages` keeps a file when its project slug merely ends with `-worktrees-<stage>`, so another repository's worktree with the same stage id is counted in `--plan` totals.

## Files
Own (write):
- `loom/src/commands/usage/bounded_lines.rs` (new)
- `loom/src/commands/usage/bounded_lines_tests.rs` (new)
- `loom/src/commands/usage/transcript.rs`
- `loom/src/commands/usage/codex_provider.rs`
- `loom/src/commands/usage/discovery.rs`
- `loom/src/commands/usage/discovery_tests.rs`
- `loom/src/commands/usage/mod.rs` (one line: `mod bounded_lines;`, alphabetical among the other `mod` lines)

Read only: `loom/src/tokens/tail.rs` (`next_line`, `MAX_LINE_BYTES`), `loom/src/commands/usage/parent_spawns.rs` (`index_parent`, the bounded pattern), `loom/src/provider_data/codex_discovery.rs` (collects FIFOs; do not edit).
Frozen (never edit): `loom/src/commands/usage/bounded_read_contract_tests.rs` (new, written before your work).

## Changes

### 1. `bounded_lines.rs` (new, about 70 lines)
Two `pub(super)` functions and a test module declaration (`#[cfg(test)] #[path = "bounded_lines_tests.rs"] mod tests;`).
- `open_regular(path: &Path) -> io::Result<File>`: `OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path)?` (use `std::os::unix::fs::OpenOptionsExt`), then `file.metadata()?`; if `!is_file()` return `Err(io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"))`. `O_NONBLOCK` makes opening a FIFO return at once; the fstat then refuses it. Symlinks are still followed: discovery keeps file symlinks on purpose (`provider_data/codex_discovery_tests.rs` asserts it) and the data only feeds counts.
- `for_each_line(file: File, mut on_line: impl FnMut(usize, Option<&str>)) -> io::Result<()>`: wrap in `BufReader`, loop `crate::tokens::tail::next_line(&mut reader, &mut line, crate::tokens::tail::MAX_LINE_BYTES)?`. For `Some((_, cut))`: call `on_line(ordinal, None)` when `cut` or the bytes are not valid UTF-8 (`std::str::from_utf8`), else `on_line(ordinal, Some(text))` with one trailing `\r` removed (as `BufRead::lines` removes `\r\n`). `ordinal` starts at 0 and counts every line delivered, cut or not, exactly like `.lines().enumerate()` did. For `None` (end of file or an unterminated last line): if `line` is non-empty and shorter than `MAX_LINE_BYTES`, deliver it the same way as a final line (`lines()` delivered an unterminated last line; a transcript being appended to must not lose it); then stop. A final line that reached the cap is delivered as `None`. Return `Ok(())` at the end.

### 2. `transcript.rs`
- `parse`: `let handle = bounded_lines::open_regular(&file.path).with_context(|| format!("Failed to read transcript {}", file.path.display()))?;` then `let mut scan = scan_lines(handle, range).with_context(|| same message)?;`.
- `scan_lines(file: File, range) -> io::Result<Scan>`: builds `Scan::default()` and calls `bounded_lines::for_each_line(file, |ordinal, line| { ... })` with the existing loop body (decode, `note_unplaced_row` on failure, `first_user_entry`, `capture_project_path`, `add_row`). Remove the `BufRead, BufReader` imports.
- `decoded_line(line: Option<&str>, diagnostics) -> Option<Value>`: `None` or a parse failure increments `diagnostics.malformed_rows` and returns `None`, as the old `Err` arms did.

### 3. `codex_provider.rs`
- `parse_file`: `let file = bounded_lines::open_regular(path)?;` and `bounded_lines::for_each_line(file, |ordinal, line| { ... })?;` with the existing body: a `None` line or a JSON parse failure increments `diagnostics.malformed_rows` and sets `state.chronology_uncertain = true`; otherwise the existing `update_context` / `direct_row` / `fallback_row` calls (they push to `rows`). Delete `json_line` if nothing else uses it; remove the `BufRead, BufReader, File` imports that become unused.
- A FIFO or other non-regular file makes `parse_file` return `Err`, which `normalize_unclassified` already counts in `diagnostics.unreadable_files` (the `Err(_)` arm). No change there.

### 3a. CR-16 callers you do not change
`provider_data/codex_discovery.rs` still lists FIFOs; that is intended, the open refuses them.

### 4. CR-58(c): anchored stage filter (`discovery.rs`)
- `filter_stages(files: Vec<DiscoveredFile>, stages: &HashSet<String>, repo_slug: Option<&str>) -> Vec<DiscoveredFile>`: keep a file when `stage_from_project_slug(&file.project_slug, repo_slug).is_some_and(|stage| stages.contains(stage))`. `stage_from_project_slug` (same file) already anchors on `repo_slug` when it is `Some` and applies the charset rule.
- In `discover`, compute the slug before `filter_stages` runs and pass it: add `fn report_repo_slug(options: &DiscoveryOptions) -> Result<Option<String>>`: `None` when `options.all` (a report over every project has no single repository); `Some(project_slug(&absolute(project)?))` when `options.project` is set; otherwise `Some(project_slug(&repository_root()?))`. These are the same three sources `selected_slugs` already uses (`absolute`, `repository_root` and `project_slug` exist in the file). Call it once in `discover` and pass `repo_slug.as_deref()`.
- Do not add a field to `DiscoveryOptions` (it is built in `mod.rs` and in several test files).

## Interfaces shared with other workers
None.

## Tests (write them; the orchestrator runs them)
- `bounded_lines_tests.rs` (child module of `bounded_lines`; uses `tempfile::tempdir`, `std::fs`, `serde_json::json`):
  - `a_fifo_is_refused_without_blocking`: `mkfifo` a path with `libc::mkfifo` (pattern: `commands/usage/spawn_join_tests.rs`, `requested_model_ledger_refuses_oversize_fifo_and_parent_symlink`), call `open_regular` on a spawned thread and `recv_timeout(5 s)` on a channel; assert `Err` and that it returned. Today a plain `File::open` blocks.
  - `lines_are_delivered_with_ordinals_and_an_unterminated_last_line`: bytes `a\r\nb\n\xff\nlast` yields `(0, Some("a"))`, `(1, Some("b"))`, `(2, None)` (invalid UTF-8), `(3, Some("last"))`.
  - `an_overlong_line_is_cut_and_the_next_row_still_reads`: a line of `MAX_LINE_BYTES + 10` bytes then a short line; callback sees `(0, None)` then `(1, Some(short))`.
  - `transcript_parse_counts_an_overlong_valid_row_as_malformed`: a valid assistant row (`{"type":"assistant","timestamp":"2026-09-12T20:00:00Z","message":{"id":"m","model":"x","content":[],"usage":{"input_tokens":1}},"pad":"<MAX_LINE_BYTES of x>"}`) followed by a normal row; `super::super::transcript::parse` (with a `DiscoveredFile` as `transcript_tests.rs::discovered` builds it and a `TimeRange { since: 2026-09-01, until: None }`) gives `diagnostics.malformed_rows == 1` and the normal row present. Today the padded row parses as a request.
  - `codex_normalize_counts_a_fifo_as_unreadable_and_an_overlong_row_as_malformed`: `super::super::codex_provider::normalize(&[fifo_path], &range)` returns with `diagnostics.unreadable_files == 1`; a rollout file whose first line is a valid padded row gives `diagnostics.malformed_rows == 1`.
- `discovery_tests.rs`: update `stage_filter_matches_only_worktree_suffix` to the new call `filter_stages(files, &stages, Some("-repo"))` (assertions unchanged) and add `stage_filter_ignores_another_repositorys_worktree`: files `-repo--worktrees-core`, `-other--worktrees-core`; stages `{"core"}`, repo slug `Some("-repo")`; only the first survives. Fails today (both survive).
- Contract to turn green: `commands::usage::bounded_read_contract_tests::a_fifo_and_an_overlong_line_do_not_stall_or_grow_the_scan`.

## Proof
From `loom/`: `cargo check --all-targets`. Nothing else.

## Traps
- `next_line` returns `None` for an unterminated last line and leaves its bytes in `line`; the old `.lines()` delivered that line. Dropping it loses the last request of a transcript being appended to.
- `O_NONBLOCK` is set on the descriptor you keep reading from; that is harmless for a regular file.
- A cut line is not an I/O error. Count it as malformed and continue with the next line; do not return `Err`.
- `ordinal` feeds `claude_usage::parse` and `request(.., ordinal)`; keep it the line index including malformed lines.
- Keep following file symlinks: only the type check (`is_file` on the opened descriptor) is new.
