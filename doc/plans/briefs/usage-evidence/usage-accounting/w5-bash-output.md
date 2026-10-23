# W5 (codex unit): Bash output bytes by command

Tier: codex terra (`loom-codex-forwarder`, `--model gpt-5.6-terra --effort xhigh`). One file plus its test. Paths are relative to `loom/`.

## Owns

- `src/commands/usage/sections/tools.rs`
- `src/commands/usage/sections/tools_tests.rs` (new)

Read: `src/commands/usage/transcript_types.rs` (`Transcript.entries`, `Entry::Assistant(Box<Request>)`, `Entry::User(UserEntry)` with `tool_use_id: Option<String>` and `text: String`, `ToolUse { id, name, input }`), `src/commands/usage/sections/fmt.rs` (`heading`, `row`, `format_u64`).

## What to add

`ToolShape` (`tools.rs` lines 8-16) reports request shapes only. Add output volume per Bash command:

```rust
#[derive(Debug, serde::Serialize, Default)]
pub struct BashOutput {
    pub calls: usize,
    pub output_bytes: u64,
    pub by_command: Vec<CommandOutput>,
}

#[derive(Debug, serde::Serialize)]
pub struct CommandOutput {
    pub command: String,
    pub calls: usize,
    pub output_bytes: u64,
    pub max_bytes: u64,
}
```

and `pub bash_output: BashOutput` as the last field of `ToolShape`, filled in `build`.

## Steps

1. `fn add_entries(totals: &mut BTreeMap<String, (usize, u64, u64)>, entries: &[Entry])`: map each `Bash` tool use id of the assistant entries to a command key; then for each `Entry::User` whose `tool_use_id` is in that map, add `text.len()` (UTF-8 bytes) to that key: one call, its bytes, and the running maximum. `build` calls it once per transcript with `&transcript.entries`, then derives `BashOutput`.
2. `fn command_key(command: &str) -> String`: split on whitespace; drop leading `NAME=value` words (NAME matches `[A-Za-z_][A-Za-z0-9_]*`); drop a leading `cd <dir> &&` pair of words; the program is the file name of the next word (text after the last `/`). When the program is one of `cargo`, `git`, `loom`, `bun`, `bunx`, `npm`, `uv`, `rg`, `fd`, `jq` and a next word exists that does not start with `-`, the key is `"<program> <next word>"`; otherwise the program alone. An empty command gives `"(empty)"`.
3. `by_command` keeps the 20 keys with the most output bytes, ties broken by key ascending. `calls` and `output_bytes` on `BashOutput` cover every Bash result, not only the top 20.
4. `render` adds a heading `Bash output by command` after the existing rows, then rows `bash results` and `bash output bytes`, then one line per key: `<key>: <calls> calls, <bytes> bytes, max <max>`.

## Tests (`tools_tests.rs`, declared at the end of `tools.rs` with `#[cfg(test)] #[path = "tools_tests.rs"] mod tests;`)

Test `command_key` directly and `add_entries` over hand-built `Vec<Entry>` values (`Entry::Assistant(Box::new(Request { .. }))` with every `Request` field set, as `src/commands/usage/sections/lifecycle_tests.rs` builds requests; `Entry::User(UserEntry { .. })`). Do not build `Transcript` literals: another worker adds a field to `Transcript` in the same wave. Cases: `cargo test --lib x` and `cargo test` share the key `cargo test`; `RUST_LOG=debug cargo build` keys as `cargo build`; `cd loom && cargo fmt` keys as `cargo fmt`; `/usr/bin/rg -n foo` keys as `rg`; a result whose tool use is not Bash adds nothing; 25 distinct keys keep 20 in `by_command` while `calls` counts all 25.

## Done means

Your two files compile against the interfaces pinned in common.md. Run no build, test or formatter: the Claude workers build the same crate in this worktree at the same time, so a cargo command can wait on their build lock past the 540-second forward deadline, and the orchestrator compiles right after you. If you meet a file you do not own that does not fit the pinned interfaces, report it and do not touch that file. The orchestrator runs `cargo test --lib commands::usage::sections::tools::` after you return. `tools.rs` under 250 lines and every function under 50 lines after `cargo fmt`. No git, no path under `.loom/`.
