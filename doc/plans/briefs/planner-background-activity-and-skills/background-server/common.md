# background-server: shared brief

Every worker of stage `background-server` reads this file in full, then its own brief. This file settles the interfaces between the workers; the plan settles the behaviour.

## Where things are

- **The plan.** `doc/plans/IN_PROGRESS-PLAN-planner-background-activity-and-skills.md` once `loom run` has started (`PLAN-…` before). Read "A. Background activity": "What exists", "What Claude Code 2.1.321 does", Decisions A-D1 to A-D4 and Steps A2 to A5. The Decisions are authoritative for behaviour. Where this file and the plan differ on a name or a signature, this file wins.
- **The evidence.** `loom/tests/fixtures/plan_session/probe-notes.md` (section "P1. Background tasks") and the `p1-*` files under `loom/tests/fixtures/plan_session/transcripts/` and `spool/`. They are read-only. Copy record shapes from them, never from memory. A field the fixtures lack: stop and report it; the main agent blocks the stage.
- **Fixture ids are redacted.** Session and agent ids appear as `<SESSION-1>` and `<AGENT-1>`, paths as `<HOME>`, `<BASE>`, `<CLAUDE-TMP>`. A notification body therefore reads `<task-id><AGENT-1></task-id>`: parse up to the closing tag, never up to the next `<`. `is_safe_id` (`loom/src/models/forward_receipt.rs:33`) refuses `<AGENT-1>`, so a test that feeds a fixture through the bridge or the token pass first replaces `<AGENT-n>` with a safe id such as `a<n>` and `<SESSION-1>` with the harness session id.
- **Paths.** The stage's `working_dir` is `loom/`. Every command in a brief runs from the worktree's `loom/` unless it starts with `cd ../web`.

## Rules for every worker

- Write only the files your brief's "Files owned" lists. Another worker's file is read-only to you; if you need it changed, stop and report.
- These contract files are frozen and read-only to every worker (the contract session wrote them before you started): `src/plan_session/transcript/notification_contract_tests.rs`, `src/plan_session/bridge/background_contract_tests.rs`, `src/tokens/planner_contract_tests.rs`, `src/plan_server/chat/background/contract_tests.rs`. `loom stage contracts show background-server` prints them. Make them pass; never edit them. The `mod` lines that declare them are yours to keep.
- Compile and format only the files you wrote (`cargo check`, `cargo build`, `rustfmt --edition 2021 <files>`; no `cargo fmt`, no linter). Run the tests of your own module once your code is ready; never the whole suite. Your brief names the command.
- Size caps: 400 lines per file, 50 per function, counted after `rustfmt`. Near the cap today: `bridge/mod.rs` 372, `driver.rs` 367, `registry.rs` 364, `flow.rs` 376, `transcript/records.rs` 388, `plan_server/chat/mod.rs` 395, `wire/tests_chat.rs` 397, `restore.tsx` 383, `schema-chat.ts` 347. New code goes in the new modules. No row of `loom/maintainability-baseline.txt` may be added or raised.
- Untrusted text: every string from a transcript, a meta file, a spool payload or a `Stop` list is untrusted. Cap strings you store (the mapper's `cap_chars`, `records.rs:287`) and flatten them before they leave the server (below).
- MEMORY: record mistakes, decisions and surprises at once with `loom memory note "..."` / `loom memory decision "..." --context "..."`, attaching `--evidence file:line` where you have it. Never Claude Code auto-memory. Never `loom knowledge`.
- Report: files changed, the tests you ran and their result, assumptions, anything unresolved. Do not run git.

## Interfaces between the workers

### W1 provides `crate::agent_state` (A3)

```rust
// src/agent_state/mod.rs
pub mod background_id;
pub mod progress;
pub mod stall;
pub use background_id::background_task_id;
pub use progress::{AgentProgress, SubagentState, DEFAULT_DONE_DEBOUNCE_SECS};
pub use stall::stall_reason;

// progress.rs
pub enum SubagentState { Done, Failed, Cancelled, ToolWait, Generating, Unknown, ForwardWait, ForwardFailed, ForwardUnknown } // moved unchanged
pub const DEFAULT_DONE_DEBOUNCE_SECS: u64 = 180; // moved unchanged
#[derive(Debug, Clone, Default)]
pub struct AgentProgress { /* last: Option<Value>, last_at: Option<DateTime<Utc>>, assistant_entries: usize, last_tool: Option<String> */ }
impl AgentProgress {
    pub fn observe(&mut self, entry: &Value);
    pub fn state(&self, now: DateTime<Utc>, debounce_secs: u64) -> SubagentState;
    pub fn idle_secs(&self, now: DateTime<Utc>) -> Option<i64>;
    pub fn last(&self) -> Option<&Value>;
    pub fn last_at(&self) -> Option<DateTime<Utc>>;
    pub fn assistant_entries(&self) -> usize;
    pub fn last_tool(&self) -> Option<&str>;
}

// stall.rs
pub fn stall_reason(state: SubagentState, idle_secs: i64, last_tool: Option<&str>, budget_secs: i64) -> Option<String>;

// background_id.rs
pub fn background_task_id(tool_use_result: &Value) -> Option<String>;
```

### W2 provides the planner token pass (A2)

```rust
// src/tokens/planner.rs, re-exported from src/tokens/mod.rs
pub struct PlannerGroup { pub id: String, pub main: PathBuf, pub subagents: Vec<(String, PathBuf, Option<String>)> }
pub fn planner_pass(tokens_dir: &Path, groups: &[PlannerGroup], now: i64) -> anyhow::Result<Vec<(String, Vec<AgentFlow>)>>;
pub fn spawn_planner_tokens(
    tokens_dir: PathBuf,
    groups: Box<dyn Fn() -> Vec<PlannerGroup> + Send>,
    sink: Box<dyn Fn(Vec<(String, Vec<AgentFlow>)>) + Send>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<()>>;
```

`spawn_planner_tokens` names its thread `planner-tokens`, runs a pass at once and then every `TRACKER_INTERVAL_SECS` (5 s) until `stop` is set, calls `groups()` at the start of each pass and `sink(flows)` after each pass that succeeded, and logs a failed pass only when its message differs from the last one logged (the tracker's `log_change`).

### W3 provides the bridge table (A4)

```rust
// src/plan_session/transcript/notification.rs (pub mod notification in transcript/mod.rs)
pub struct TaskNotification { pub task_id: String, pub tool_use_id: Option<String>, pub status: NotificationStatus, pub summary: Option<String>, pub interim: bool }
pub enum NotificationStatus { Completed, Failed, Killed, Other(String) }
pub fn parse_task_notification(text: &str) -> Option<TaskNotification>;

// src/plan_session/bridge/background.rs (pub mod background in bridge/mod.rs)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackgroundTask {
    pub id: String,
    pub kind: TaskKind,
    pub label: Option<String>,
    pub agent_type: Option<String>,
    pub tool_use_id: Option<String>,
    pub state: TaskState,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub steps: Option<u64>,
    pub last_tool: Option<String>,
    pub report_preview: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind { Agent, Shell }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState { Running, ToolWait, Stalled, Done, Failed, Stopped }
impl TaskState { pub fn is_ended(self) -> bool } // Done, Failed, Stopped

// src/plan_session/bridge/mod.rs
BridgeEvent::Background { id: String, tasks: Vec<background::BackgroundTask> } // after every change of the session's table, tasks in started_at order, then id

// src/plan_session/bridge/agents.rs, re-exported from bridge::registry (pub mod registry)
pub struct AgentMeta { pub tool_use_id: Option<String>, pub agent_type: Option<String>, pub description: Option<String>, pub finished: bool, pub teammate: Option<String> }
pub fn read_agent_meta(subagents_dir: &Path, agent_id: &str) -> Option<AgentMeta>; // size-bounded, no-follow, as AgentFeed::classify reads it today
```

`BackgroundTasks` itself (the table type, its inputs and `blocking()`) is W3's to shape; `tests/plan_session_bridge_core_contracts.rs` uses it as W3's brief says.

### W4 provides the wire (A5 Rust)

`PlanMessage::Background { session: String, tasks: Vec<BackgroundTaskView> }`, serde tag `"background"`, with `BackgroundTaskView` in `src/plan_server/wire/background.rs`: every `BackgroundTask` field with the same name and serde form, plus `model: Option<String>`, `total: Option<u64>`, `tokens_per_min: Option<u64>`, `active: Option<bool>`. `total` is `TokenUsage::fresh(&flow.total)` and `tokens_per_min` is `TokenUsage::fresh(&flow.rate)` of the `AgentFlow` whose `id` equals the task id; the dashboard shows the same two figures (`web/src/components/stage-token-lanes.tsx:77,80`). All four are `null` for a task with no flow row (a shell, or an agent the pass has not read yet). `started_at` and `last_activity_at` serialize as RFC 3339 strings (chrono's serde). Every string field passes through `crate::context::untrusted::inline_safe` (`loom/src/context/untrusted.rs:178`), the function `status/data/sanitize.rs` `flatten` and `sanitize_token_flow.rs` use.

The fixture `web/src/plan/api/fixtures/background.json` is one such frame, written by W4 from the Rust serializer and round-tripped in Rust with `include_str!`. Its `session` is `"00000000-0000-4000-8000-000000000001"`, the session id of `web/src/plan/api/fixtures/chat-snapshot.json`. It holds five tasks, every optional field populated where the kind has it and `null` where it does not, in this order:

1. agent `a-run`, `general-purpose`, label "Survey the session bridge", tool_use_id `toolu_bg1`, state `running`, steps 12, last_tool `Read`, model `claude-sonnet-5-5`, total 48200, tokens_per_min 3100, active true, report_preview null;
2. shell `bshell01`, label "Run sleep in background", tool_use_id null (adopted from a `Stop` list), state `running`, every agent and token field null;
3. agent `a-quiet`, `Explore`, label "Find the token tracker", tool_use_id `toolu_bg2`, state `stalled`, steps 4, last_tool `Grep`, model `claude-haiku-5`, total 9100, tokens_per_min 0, active false;
4. agent `a-done`, `general-purpose`, label "Check the fixtures", tool_use_id `toolu_bg3`, state `done`, steps 20, last_tool `SubagentHandback`, report_preview "DONE", model `claude-sonnet-5-5`, total 61000, tokens_per_min 0, active false;
5. shell `bshell02`, label "Run the failing check", tool_use_id `toolu_bg4`, state `failed`.

So the fixture has three unended tasks (rows 1 to 3), and the web line reads "Waiting on 3 background tasks".

### W5 provides the web wire and the chat selectors (A5 web)

- `web/src/plan/api/schema-background.ts`: `backgroundTaskViewSchema` (strict, required nullable fields) and `backgroundMessageSchemas`, spread into `planMessageSchema` in `schema.ts`, with `"background"` in `PLAN_MESSAGE_TYPES`.
- `web/src/plan/state/atoms.ts`: `backgroundAtom = atom<BackgroundFrame | null>(null)`, set by the socket's `apply` on a `background` frame and reset to `null` on hello beside `draftTeamAtom` (`socket.ts:134`).
- `web/src/plan/chat/background-tasks.ts` (new; the chat layer, which may read `sessionAtom` from `chat/model.ts:25`): `chatBackgroundAtom` (the frame's tasks when `frame.session` is the chat session's id, else `[]`: this is where a frame for another session is ignored), `unendedBackgroundCountAtom`, `isUnended(state)`, and `backgroundEntryAtom(toolUseId: string)`, a per-`tool_use_id` selector built with `selectAtom` from `jotai/vanilla/utils` and cached in a `Map`, as `web/src/state/atoms.ts:45` builds its per-stage selectors. `planner-web` builds on these and does not change them.

## The gate

The stage's acceptance list is the gate; the main agent runs it with one `loom check`. It holds `cargo build` with `--version` and `--help`, clippy with `-D warnings`, `cargo fmt --check`, `cargo doc` with `-D warnings`, `cargo test --lib` for `agent_state::`, `commands::subagents::`, `models::forward_receipt::`, `tokens::`, `plan_session::`, `plan_server::chat::`, `plan_server::wire::`, `plan_server::summaries::` and `plan_server::pressure::`, `cargo test --test token_flow_contracts`, `--test plan_session_bridge_core_contracts` and `--test maintainability`, no `subagents_running` left under `src` or `web/src`, the CI filter line, and the web typecheck, lint, format check, the planner tests under `src/plan/{api,state,chat,header,help}` and the scratch dist build diffed against `web/dist`.
