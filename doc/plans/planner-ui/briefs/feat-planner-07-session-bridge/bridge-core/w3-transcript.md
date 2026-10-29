# W3 (bridge-core): transcript mapper

Read `doc/plans/briefs/feat-planner-07-session-bridge/common.md` first ("Transcript mapping" with the record
shapes captured from real transcripts, "The chat model"). You run in wave 2, beside W2, after W1
returned (`plan_session::model` and `plan_session::attachments::parse_message` exist).

## Files you own (write only these)

- `loom/src/plan_session/transcript/mod.rs` (`Mapper` and its public API)
- `loom/src/plan_session/transcript/records.rs` (record classification: which records map to what)
- `loom/src/plan_session/transcript/tests.rs`

Codex units C5 (`transcript/tools.rs` + `tests_tools.rs`) and C11 (`transcript/usage.rs`) write the
helpers of steps 2 and 3 below under the names `codex-units.md` pins; call them, do not edit them.
You run in wave 3 (C5 has landed; C11 lands in the same wave: rebuild when it does).
- `loom/tests/fixtures/plan_session/transcripts/*.jsonl` (recorded-transcript fixtures)

## API (what the bridge calls; contracts are written against it)

```rust
pub struct Mapper { .. }
impl Mapper {
    pub fn new(project_root: PathBuf) -> Self;
    pub fn expect_opening(&mut self, origin: Option<Origin>);   // the next human user record's origin
    pub fn expect_closing(&mut self, text: String);             // a human record with this sanitised text is Origin::Closing
    /// Opens turn `t<n>` at a UserPromptSubmit, or adopts the turn a human record already opened
    /// implicitly (the record arrived first); returns its id. Never opens two turns for one prompt.
    pub fn begin_turn(&mut self, at: DateTime<Utc>) -> (String, Vec<ChatDelta>);
    pub fn end_turn(&mut self) -> Vec<ChatDelta>;               // closes the open turn (Stop)
    pub fn ingest_main(&mut self, line: &str) -> Ingest;
    pub fn ingest_subagent(&mut self, agent_id: &str, spawned_by: Option<&str> /* tool_use_id */, line: &str) -> Ingest;
    pub fn ingest_tokens_only(&mut self, line: &str) -> Vec<ChatDelta>;   // usage into the open turn, no item (plan 11's teammates)
    pub fn add_server_item(&mut self, item: Item) -> Vec<ChatDelta>;     // permission cards, "sent" user items, notices
    pub fn replace_server_item(&mut self, item: Item) -> Vec<ChatDelta>; // by id, in whichever turn holds it
    pub fn turns(&self) -> &[Turn];
    pub fn open_turn(&self) -> Option<&str>;
    pub fn last_main_record_at(&self) -> Option<DateTime<Utc>>;
    pub fn interrupted_since(&self, at: DateTime<Utc>) -> bool;          // an interrupt record after `at`
    pub fn changed_by_tool(&self, turn_id: &str) -> Vec<String>;         // repo-relative paths
}
```

```rust
pub struct Ingest { pub deltas: Vec<ChatDelta>, pub marks: Vec<RecordMark> }
pub enum RecordMark {           // what the bridge feeds the state machine, in record order
    Main { at: DateTime<Utc> },  // any main assistant/tool record
    Interrupted { at: DateTime<Utc> },
    AgentsKilled { at: DateTime<Utc> },     // system subtype agents_killed
    SelfStartedTurn { at: DateTime<Utc> },  // <task-notification> / origin task-notification user record
    CompactBoundary { at: DateTime<Utc> },
    HumanUser { at: DateTime<Utc>, text: String },   // sanitised user text, for delivery matching
    SubagentFinal { agent_id: String, at: DateTime<Utc> }, // a subagent's final assistant record (stop_reason end_turn)
}
```

`ingest_*` returns the deltas to publish (`ItemAdded`, `ItemReplaced`, `ItemRemoved`,
`TurnAdded`, `TurnUpdated`). A record before any turn is open opens one implicitly (a session's
opening prompt). A human user record that matches a server item `s-<n>` of the open turn (by
`input::sanitize`d text, user text only, attachments excluded) produces `ItemRemoved` of the server
item and `ItemAdded` of the transcript item with `delivery: delivered`.

## Steps

1. `records.rs`: parse each line with `serde_json`; classify per common.md's table. The interrupt
   test is "a text block or a string content starting with `[Request interrupted by user`"
   (covers `[Request interrupted by user for tool use]`). Human = not `isMeta`, `origin` absent or
   `origin.kind == "human"`, and the text does not start with a wrapper tag.
2. `usage.rs`: per `message.id`, keep the usage of the last record seen; turn tokens = the sum
   over the turn's message ids (main and subagent). Parse with
   `crate::provider_data::claude::parse(&usage)` (`provider_data/claude.rs:13`); skip a usage for
   which `provider_data::claude::invalid_usage` is true.
3. `tools.rs`: summaries per common.md; `added`/`removed` from `structuredPatch[*].lines`; the
   unified patch text is `@@ -a,b +c,d @@` hunks of those lines; result text from the
   `tool_result` content (a string, or the concatenated `text` blocks of an array), capped at
   64 KiB on a char boundary with `result_truncated`.
4. `mod.rs`: the `Mapper`. `ingest_tokens_only` adds a record's usage (last record per message id
   wins, as for any message) to the open turn and returns only a `TurnUpdated` delta. Marks per
   the `RecordMark` list above; an interrupt record also closes the open turn (`end_turn` runs
   inside the mapper, and a later `end_turn` call for the same turn returns nothing). Keyed de-duplication by `uuid` across main and subagent records. A
   thinking item's `duration_ms` is filled (`ItemReplaced`) from the next record's timestamp.
   Subagent records: tool items with `subagent: Some(agent_id)`; the spawning tool item's
   `nested_tools` increments (`ItemReplaced`); subagent text and thinking blocks produce nothing.
   A `filePath` result adds the path (relative to `project_root` when inside it) to the turn's
   by-tool set. At an interrupt every `pending` tool of the turn becomes `not_finished`.
5. Fixtures (`loom/tests/fixtures/plan_session/transcripts/`): build them from common.md's
   captured shapes (keep every key those shapes show, with neutral values): `simple-turn.jsonl`
   (opening prompt, thinking, text, `turn_duration`), `tools.jsonl` (Read, Bash, Edit with
   `structuredPatch`, an `is_error` result), `streamed.jsonl` (one `message.id` across thinking,
   text and tool_use records, each with usage), `interrupt.jsonl`, `compaction.jsonl`, `api-error.jsonl` (a `system` record with `level: "error"` and one with an
   `apiError` field: each a notice of kind `api_error` with its first line),
   `noise.jsonl` (every skipped type listed in common.md, `isMeta` records, a task-notification, a
   `<local-command-caveat>` and `<bash-input>` user record, a repeated uuid),
   `attachments.jsonl` (a user record whose text carries a `<loom-attachments>` block), and a
   subagent pair `subagent-main.jsonl` + `subagent-agent.jsonl` + `subagent-agent.meta.json`.
   If the stage host has real transcripts you may read to check a shape (`~/.claude/projects/`), do
   not copy their content into fixtures: they hold the operator's work.
6. Tests: each fixture driven through the mapper with the exact expected items, ids, statuses and
   tokens; ingesting a fixture twice yields no new deltas; a partial last line is never handed to
   the mapper (that is the reader's job: assert `ingest_main` on an invalid JSON line returns
   nothing and does not panic).

## Proof

`cargo test --lib plan_session::transcript::` and `cargo clippy --all-targets -- -D warnings` from
`loom/`. Report anything surprising (and `loom memory note` it).
