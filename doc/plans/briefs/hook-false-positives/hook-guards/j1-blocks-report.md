# Codex Job J1: `loom hook blocks` Report

Lane: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 1560000 ms. Wave 1, in parallel with worker F and J2; it needs no file another job writes.

Do not run git. Do not read or write anything under `.loom/` or `.work/`. Run no `cargo` command: J2 edits the same crate at the same time, so a build here can hit its half-applied edits; the orchestrator compiles and runs your tests after wave 1.

## Files you own

- `loom/src/commands/hook/blocks.rs` (new)
- `loom/src/commands/hook/tests_blocks.rs` (new)
- `loom/src/commands/hook/mod.rs`
- `loom/src/cli/types_ops.rs`
- `loom/src/cli/dispatch.rs`

Write `blocks.rs` and `tests_blocks.rs` first, then make the three small edits to `mod.rs`, `types_ops.rs` and `dispatch.rs` last, together.

## Entry points (find each by symbol; line numbers drift)

- `enum HookCommands` in `loom/src/cli/types_ops.rs` (177-240 at 104ec387): 11 variants; `UserPrompt`, `ReconcileGraph` and `PreCompact` are visible, the rest carry `hide`.
- `fn dispatch_hook` in `loom/src/cli/dispatch.rs` (121-135): arms in the `hook::x::x(` style.
- `loom/src/commands/hook/mod.rs`: `pub mod` lines 7-19, test modules at 21-22.
- Reusable pieces: `super::target::non_empty_env` (`pub(crate)`, `loom/src/commands/hook/target.rs:96`), `WorkDir::new` and `WorkDir::root()` (`loom/src/fs/work_dir.rs:127-135, 374`). `WorkDir::new` never fails: with no workspace it returns `<base>/.loom/work` whether or not it exists, and it treats a hint as the state root only when the hint ends in `.loom/work` or `.work` or holds a `config.toml` (`loom/src/fs/work_dir/discovery.rs:41-46`); otherwise it walks up into the enclosing checkout (`work_dir.rs:153-167`). `reviewer_model.rs`'s `resolve` is private and not reusable.
- `loom/src/commands/usage/time_range.rs:67`: the repo's checked `Duration::try_hours(..).context(..)` pattern (chrono 0.4.45).

## What to build

1. Add a visible variant to `HookCommands`, with no `hide`:

   ```rust
   /// Summarize the guard blocks recorded by loom's hooks under
   /// `$LOOM_WORK_DIR/heartbeat/<stage>.blocks.jsonl`.
   Blocks {
       /// Only count blocks recorded in the last N hours
       #[arg(long, default_value_t = 168)]
       since_hours: u64,
       /// Group the blocks by guard and rule instead of listing recent ones
       #[arg(long)]
       by_rule: bool,
   },
   ```

2. Dispatch: `HookCommands::Blocks { since_hours, by_rule } => hook::blocks::blocks(since_hours, by_rule),`
3. `mod.rs`: add `pub mod blocks;` in alphabetical order, and `#[cfg(test)] mod tests_blocks;` beside the other test modules.
4. `blocks.rs`:
   - `pub fn blocks(since_hours: u64, by_rule: bool) -> anyhow::Result<()>` reads the directory the hooks write: when `non_empty_env("LOOM_WORK_DIR")` is set, `PathBuf::from(it).join("heartbeat")` VERBATIM (the hooks append to `$LOOM_WORK_DIR/heartbeat/` as given, so resolving the variable through `WorkDir` would read a different directory for a scratch work dir); otherwise `WorkDir::new(".")?.root().join("heartbeat")`. It prints `reading <dir>` as the first line, then `render(&summarize(&dir, chrono::Utc::now(), since_hours)?, by_rule)`. A missing directory prints `no ledger directory at <dir>` instead of the render, and returns `Ok(())`.
   - `pub(crate) struct BlockRecord { ts: DateTime<Utc>, stage: String, session: String, role: String, guard: String, rule: String, tool: String, subject: String }`, deserialized with serde from one ledger line. The line format is pinned below.
   - `pub(crate) struct Summary { records: Vec<BlockRecord>, malformed: usize }`.
   - `pub(crate) fn summarize(heartbeat_dir: &Path, now: DateTime<Utc>, since_hours: u64) -> anyhow::Result<Summary>`:
     - The cutoff is `now.checked_sub_signed(chrono::Duration::try_hours(i64::try_from(since_hours).unwrap_or(i64::MAX)).context(..)?)`; a window that cannot be built is an `anyhow` error, never a panic.
     - A missing directory gives an empty summary.
     - It reads every regular file whose name ends in `.blocks.jsonl`, skipping symlinks (`symlink_metadata`).
     - It parses each line: a line that fails to parse adds 1 to `malformed`, and a record older than the cutoff is dropped.
     - Records come back sorted newest first.
     - It reads at most the last 4 MiB of each file (seek to `len - 4 MiB` when larger and discard the first, partial line), and at most 50,000 lines, the newest kept. A line longer than 4,096 bytes counts as malformed without being parsed. The hooks stop appending at 4 MiB, so these bounds only matter for a ledger something else wrote.
   - `pub(crate) fn render(summary: &Summary, by_rule: bool) -> String`:
     - No records: `no hook blocks recorded in the window`, plus `(<n> malformed lines skipped)` when `malformed > 0`.
     - `by_rule == false`: the 20 newest records, one per line, `<ts RFC3339> <stage> <role> <guard>/<rule> <subject truncated to 120 chars>`. Truncate on a char boundary.
     - `by_rule == true`: one line per `guard/rule`, sorted by count descending and then by name, `<count> <guard>/<rule> main=<n> subagent=<n>`. Under each line, up to 3 indented sample subjects, newest first, truncated the same way.
     - A trailing `<n> malformed lines skipped` line when `malformed > 0`.
     - Terminal safety: every printed field (stage, role, guard, rule, subject) comes from a file any process can write, and JSON decoding restores newlines and escape bytes. Replace each `char::is_control` character with `?` before truncating, so every record prints on exactly one line with no ESC byte.
5. `tests_blocks.rs` covers:
   - Two files across two stages give one sorted list.
   - A record older than the window is dropped.
   - `since_hours = u64::MAX` returns without panicking (an error or an empty window, never a panic).
   - A malformed line is counted.
   - A symlinked `.blocks.jsonl` is ignored.
   - `render` with `by_rule` orders by count and splits roles: two `worktree-isolation/path-traversal` records (one main, one subagent) and one other render `2 worktree-isolation/path-traversal main=1 subagent=1` first.
   - A 200-char multibyte subject truncates without panic.
   - A subject holding `\n` and `\u001b[31m` renders on one line with no `\x1b` byte, in both modes.
   - A 5,000-byte line counts as malformed; a file just over 4 MiB still yields its newest records.
   - `crate::cli::Cli::try_parse_from(["loom", "hook", "blocks", "--by-rule", "--since-hours", "24"])` parses.

   Use `tempfile::TempDir` (a normal dependency).

## Pinned ledger line (written by the hooks; do not change)

```json
{"ts":"2026-11-07T12:00:00Z","stage":"s","session":"x","role":"main","guard":"worktree-isolation","rule":"path-traversal","tool":"Bash","subject":"cat ../../x"}
```

## Rules quoted from the loom-rust skill and this repo

- Errors use `anyhow` with `.with_context(...)` at IO boundaries.
- No `unwrap()` outside tests.
- Files stay under 400 lines and functions under 50; `loom/tests/maintainability.rs` fails on any new violation. Leave a 10% margin, because `cargo fmt` can grow a file. `dispatch.rs` is 352 lines, `types_ops.rs` 262, `mod.rs` 22.
- Literal braces inside `format!` are doubled.
- A CLI placeholder in a rustdoc comment goes in backticks (`RUSTDOCFLAGS='-D warnings' cargo doc` runs in CI).

## Done when

The files are written. The orchestrator then runs `cargo check --all-targets`, `cargo test --manifest-path loom/Cargo.toml --lib commands::hook::tests_blocks` and `git status --short`.
