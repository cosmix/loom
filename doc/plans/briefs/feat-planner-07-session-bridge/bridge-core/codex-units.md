# Codex units of stage bridge-core

Each unit is forwarded through one `loom-codex-forwarder` subagent in the FOREGROUND with
`--model gpt-6.1-terra --effort high` and an explicit Bash timeout of 600000 ms; the forwarded text
is the unit block below, verbatim. Every unit reads `doc/plans/briefs/feat-planner-07-session-bridge/common.md`
first: it pins every type and rule these units implement.

Rules for every unit: write only the files named; run at most `cargo build` or
`cargo test --no-run` from `loom/` as a compile check; never run git; keep files under 360 lines
and functions under 45; double literal braces inside `format!`; CLI placeholders in rustdoc go in
backticks. When a file named under Reads was written earlier in this stage, read the file itself:
`loom map` answers from the base layer and does not show it. The orchestrator runs the proof
command and `git status --short` after each unit.

## C1 attachment types (wave 1)

- Owns: `loom/src/plan_session/attachments/mod.rs` (seeded with a purpose line).
- Entry points: `Attachment`, `AttachmentInput`, `AttachmentKind`, `LockedTag`, `AttachmentError`
  (`Path(String)`, `NotADocument`, `QuoteTooLarge { bytes: usize }`, `RestsOn(String)`,
  `SentenceId`, each with a `Display` message), `MAX_ATTACHMENTS`, `MAX_QUOTE_BYTES`, `QUOTE_CHARS`,
  exactly as common.md "Attachments" (derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`,
  serde `snake_case`). The seed already declared `mod format; mod parse; mod resolve;
  #[cfg(test)] mod tests;` here and created those four files (W1 owns them): keep those lines, add
  `pub use format::format_message; pub use parse::parse_message; pub use resolve::resolve;` only
  once W1 has written the functions (the orchestrator adds the three re-exports after W1).
- Steps: 1. the types; 2. compile.
- Done: `cargo build` passes.

## C2 chat model types (wave 2)

- Owns: `loom/src/plan_session/model/mod.rs` (seeded).
- Reads: `loom/src/plan_session/attachments/mod.rs` (C1, this stage: the `Attachment` type).
- Entry points: every type of common.md "The chat model" (`SessionView`, `Failure`, `FailureKind`,
  `TokenCounts`, `ChangedFile`, `Turn`, `Item`, `Delivery`, `Origin` with `Offer, Start, Closing`,
  `ToolStatus`, `PermissionStatus`, `DenyReason`, `NoticeKind`, `QueuedMessage`, `EndedChat`,
  `ChatSnapshot`, `ChatDelta`) and `SessionState`, `Activity` from common.md "State machine". The
  seed declared `#[cfg(test)] mod tests;` and created `model/tests.rs` (C13 owns it): keep that line.
- Steps: 1. the types with one-line doc comments; 2. serde attributes exactly as pinned
  (`Item` tagged `type`, `ChatDelta` tagged `op`, enums `snake_case`, `TokenCounts` `Default`);
  3. compile.
- Done: `cargo build` passes.

## C3 spool (wave 2)

- Owns: `loom/src/plan_session/spool.rs` (seeded), `loom/src/plan_session/spool_tests.rs`
  (declared from `spool.rs` with `#[cfg(test)] #[path = "spool_tests.rs"] mod tests;`).
- Reads: `loom/src/tokens/tail.rs` (`read_lines`, `stat`, `Line`; the seed widened them to
  `pub(crate)`), `loom/src/fs/safe_fs.rs` (`safe_append`).
- Entry points: `SPOOL_EVENTS`, `SpoolEvent`, `append_event`, `SpoolReader::{new, read_new}`,
  `pub(crate) fn read_payload(stdin: impl Read) -> Option<Value>`, exactly as common.md "Spool".
- Steps: 1. the hook-side append (line form of common.md, 64 KiB per-string cut,
  `safe_append`); 2. the reader (whole lines only, reset the offset to 0 on a shrunk file or a new inode and set
  `SpoolReader::was_reset(&self) -> bool` for that call, unparsable lines skipped with
  `tracing::warn!`); the reader does not de-duplicate: the bridge does; 3. tests: four threads append at once and four
  whole lines result; a 1 MiB + 1 payload is refused by `read_payload`; a half-written line is not
  read until its newline arrives.
- Done: `cargo test --lib plan_session::spool` passes.

## C4 (moved to Claude)

The input sanitising and send checks are the pane-delivery boundary and belong to W2 (opus);
there is no codex unit C4.

## (former C4 text, for reference only; W2 implements it)

- Owns: `loom/src/plan_session/input.rs` (seeded).
- Entry points: `sanitize`, `check_send`, `check_delivered`, `InputRefusal` with `Display`, as
  common.md "Input".
- Steps: 1. the three functions; 2. a `#[cfg(test)] mod tests` at the bottom: sanitize strips
  `\x1b[201~`, `\u{9b}`, `\x7f`, `\x00`, `\x07`, keeps newline, tab and non-ASCII letters, turns
  CRLF into LF and drops a lone CR; check_send on `/clear`, leading-space `/exit`, `/compact`,
  `/compact keep`, `/compactx`, and `path /tmp`; check_delivered at 100,000 and 100,001 bytes;
  3. compile and run.
- Done: `cargo test --lib plan_session::input` passes.

## C5 tool summaries and patches (wave 2)

- Owns: `loom/src/plan_session/transcript/tools.rs`, `loom/src/plan_session/transcript/tests_tools.rs`
  (declared from `tools.rs` with `#[cfg(test)] #[path = "tests_tools.rs"] mod tests;`).
- Entry points: `pub(crate) fn summary(tool: &str, input: &Value, project_root: &Path) -> String`,
  `pub(crate) fn patch_counts(structured_patch: &Value) -> (u32, u32)`,
  `pub(crate) fn unified_patch(structured_patch: &Value) -> String`,
  `pub(crate) fn result_text(content: &Value) -> (String, bool /* truncated */)`.
- Steps: 1. the summary table of common.md "Transcript mapping" (200-char cut on a char boundary,
  root-relative paths); 2. the patch and result helpers (64 KiB cap on a char boundary); 3. one
  test per tool row and per helper, using the `structuredPatch` shape quoted in common.md.
- Done: `cargo test --lib plan_session::transcript::tools` passes. The mapper (W3, Claude) calls
  these four functions by these names.

## C6 chat wire enums (wave 3)

- Owns: `loom/src/plan_server/wire/chat.rs` (the seed declared `pub mod chat;` and
  `#[cfg(test)] mod tests_chat;` in `plan_server/wire/mod.rs` and created both files).
- Reads: `loom/src/plan_session/model/mod.rs` (C2), `loom/src/plan_session/attachments/mod.rs` (C1).
- Entry points: `ChatFrame`, `CommandResult`, `ChatCommand`, exactly as common.md (wire section).
- Steps: 1. the enums; 2. compile.
- Done: `cargo build` passes.

## C7-C10 wire fixtures (wave 3, one forward per file)

- C7 owns `web/src/plan/api/fixtures/chat-snapshot.json`; C8 `chat-deltas.json`; C9
  `chat-commands.json`; C10 `chat-frames.json`.
- Reads: common.md "The chat model" and the wire section (the serde form is pinned there; the Rust
  types may not exist yet in your view of the tree).
- Steps: 1. write the JSON so every enum value the file can hold appears at least once (C7: a
  snapshot with a closed turn holding one item of every type and every tool, permission and
  delivery status, an open turn, a queue, a previous chat; C8: an array with one delta per `op`;
  C9: one command per `command`; C10: one frame per `frame`); 2. realistic values (uuid ids, RFC
  3339 times ending in Z, pretty JSON text in tool `input`, unified diff text in `patch`,
  repo-relative `doc/...` paths); 3. validate with `python3 -m json.tool <file>`.
- Done: the orchestrator formats the four files with `node_modules/.bin/oxfmt` from `web/` (codex does
  not format web files) and runs `cargo test --lib plan_server::wire::tests_chat` after C12.

## C11 usage (wave 3)

- Owns: `loom/src/plan_session/transcript/usage.rs`.
- Reads: `loom/src/plan_session/model/mod.rs` (C2: `TokenCounts`), `loom/src/provider_data/claude.rs`
  (`parse`, `invalid_usage`, both `pub(crate)`).
- Entry points: `pub(crate) struct MessageUsage`, `pub(crate) fn record(&mut self, message_id: &str, usage: &Value)`
  (the last record per message id wins; an invalid usage is skipped), `pub(crate) fn total(&self) -> TokenCounts`.
- Steps: 1. the type; 2. inline tests: three records of one id count once with the last values,
  two ids add up, an invalid usage is ignored; 3. compile and run.
- Done: `cargo test --lib plan_session::transcript::usage` passes.

## C12 wire parity tests (wave 4)

- Owns: `loom/src/plan_server/wire/tests_chat.rs`.
- Reads: `loom/src/plan_server/wire/tests.rs` (plan 2, the pattern), C6 and C7-C10 (this stage).
- Steps: 1. per fixture `<name>_fixture_matches_serde_output` (`include_str!` the fixture,
  deserialize, serialize, compare `serde_json::Value`s) and `<name>_fixture_deserializes`;
  2. per enum a test that every variant appears in the fixtures; 3. confirm the `include_str!`
  relative path from this file.
- Done: `cargo test --lib plan_server::wire::tests_chat` passes.

## C13 model tests (wave 4)

- Owns: `loom/src/plan_session/model/tests.rs`.
- Steps: 1. serde round trips for each type with a hand-built value; 2. `Item` tag values and
  `ChatDelta` op values as strings; 3. `TokenCounts::default()` is all zero.
- Done: `cargo test --lib plan_session::model` passes.
