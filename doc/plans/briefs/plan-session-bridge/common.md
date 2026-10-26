# plan-session-bridge: common brief

Read this in full before your own brief. It is the authoritative design of plan 7: the modules,
their public API, the wire model sent to browsers, and the rules every worker follows. Where your
brief and this file differ, this file wins; report the difference.

Spec: `doc/product/section_07.md` (whole), section 12 "Input delivered to the pane" (lines 515-521),
"No way for a session to approve itself" (303-314) and "Request limits" (546-560), section 10 "What
the session receives" (51-98, the attachment block), section 08 "Tool activity", "Session states",
"Permission prompts", "Composing and sending" (105-264: what the browser needs from the model).
Metaplan entry 7 of `doc/plans/metaplan-planner-ui.md`.

## What is already merged when this plan runs

Plans 1 to 6 run before this one (one checkout runs one plan, in metaplan order). Confirm every
signature below with `loom map --find-all <name>` before using it; if one differs, use the tree's
and report it; if one is missing, stop that piece and report it.

- Plan 1 `loom::planning`: `runtime::{RuntimePaths { dir, lock, drafts, claims, summaries_lock, summary_cost, sessions }, runtime_paths, runtime_paths_under}`,
  `paths::{LockedSet, LockReason, check_relpath_syntax, symlink_component}`, `project::Project`
  (`content_hash(rel)`, `section_text(rel, anchor)`, `locked`, `tree.get(rel)`), `checklist::Offer`.
- Plan 2 `loom::plan_server`: `PlanContext` (`root()`, `runtime()`, `project()`, `authoring()`,
  `presence()`, `hub()`, `shutdown()`, `read_only()`, `is_stopping()`, `extensions()`),
  `ctx.require_authoring(&BrowserId, Take::{HolderOnly, TakeIfFree}) -> Result<(), Refusal>` (use this, never the raw
  `Authoring::require`, which also takes the public id and the stopping flag; `Refusal::NotHolder { holder: PublicId }`,
  its `Display` names the holder), `Authoring::{holder() -> Option<HolderView { public_id, name, away_since }>, is_holder, release(FreeReason::Released(&'static str)), subscribe(Box<dyn Fn(&AuthoringEvent)>)}`
  with `AuthoringEvent::{Taken, Away { since }, Back, Freed { reason: FreeReason::{GraceExpired, Released(_)} }}`
  (plan 2 owns the 10-minute grace timer), `hub().{broadcast(&PlanMessage), send_to_conn, send_to_browser, send_to_holder, on_open(Box<dyn Fn(&ConnCtx) -> Vec<PlanMessage>>), on_close}`,
  `ConnCtx { id: ConnId, browser: BrowserId, public_id: PublicId }` (dispatch runs on the
  connection's own thread: return quickly; a frame that does not parse never reaches dispatch, the
  router answers `Refused` itself; `send_to_holder` drops a message while the holder has no tab open),
  `shutdown().register(ShutdownHook { name, drain: Box<dyn FnOnce() + Send>, abort: Box<dyn FnOnce() + Send> })`
  (drain runs on its own thread and may block; abort must be fast), `ReadOnly::is_read_only()`,
  `lifecycle::{line(&str), safe(&str) -> String}`, `Extensions::{insert<T>(Arc<T>), get<T>() -> Option<Arc<T>>}`.
  Wire: `plan_server/wire/message.rs` `PlanMessage` and `ClientMessage` (`#[serde(tag = "type", rename_all = "snake_case")]`),
  dispatch `plan_server/ws/dispatch.rs::dispatch(ctx, conn: &ConnCtx, msg: ClientMessage)`; a later plan
  adds ONE variant to each enum and ONE arm; route modules register through ONE line in
  `plan_server/routes.rs::install_all()` calling `pub fn install(ctx: &Arc<PlanContext>)`.
  Fixture-parity pattern: `plan_server/wire/tests_<area>.rs`, per fixture
  `<name>_fixture_matches_serde_output` and `<name>_fixture_deserializes`, fixtures at
  `web/src/plan/api/fixtures/<name>.json` (`include_str!`).
- Plan 3 `plan_server::files::texts::TextStore` in `ctx.extensions()`:
  `begin_turn(&self, &TurnId)`, `end_turn(&self, &TurnId, root: &Path, project: &Project) -> Vec<TurnChange { path, before_hash: Option<String>, after_hash: Option<String> }>`,
  fetched with `ctx.extensions().get::<TextStore>()`,
  `TurnId(pub String)`. Plan 7 drives it (section 06 "Turn diffs").
- Plan 6 `loom::plan_session` (see `doc/plans/briefs/plan-session-sandbox/common.md`):
  `kind::{SessionKind::{Authoring, Summary, Pressure { plan }}, SessionDir}`,
  `sandbox::{SandboxInputs, HookSpec, write_session_sandbox -> SandboxFiles { settings, guard }}`,
  `guard::{GuardPolicy, load_policy}`, `hook_cli::run(event, session_dir, stdin, stdout) -> i32`
  (plan 6 handles `PreToolUse`; plan 7 adds the arms below).

## Module map (all under `loom/src/plan_session/` unless noted)

| Module | Stage | Purpose |
| --- | --- | --- |
| `model/` | bridge-core | the chat model types (below), pure data |
| `attachments/` | bridge-core | section 10's block: `format_message`, `parse_message`, `resolve` |
| `input.rs` | bridge-core | delivery sanitising and the send checks |
| `transcript/` | bridge-core | transcript records to model items (incremental) |
| `spool.rs` | bridge-core | hook event spool: the hook-side append, the server-side reader |
| `permission.rs` | bridge-core | the blocking `PermissionRequest` hook and the server's answer file |
| `state.rs` | bridge-core | the session state machine (pure) |
| `hook_cli.rs` | bridge-core | adds the nine event arms to plan 6's dispatcher |
| `launch.rs`, `launch/script.rs` | bridge-runtime | session directory, `launch.sh`, spawn, pane identity |
| `tmux_io.rs` | bridge-runtime | `load-buffer`/`paste-buffer`/`send-keys`, `display-message` |
| `bridge/` | bridge-runtime | the `Bridge`: sessions, queue, timers, commands, end, recovery, `bridge/registry.rs` (`session.json`) |
| `tmux_host.rs` | bridge-runtime | `SessionHost` implemented with `launch` and `tmux_io` |
| `loom/src/plan_server/chat/` | bridge-runtime | server wiring: install, WS commands, snapshot on open, shutdown, grace |
| `loom/src/plan_server/wire/chat.rs` | bridge-core | `ChatFrame` and `ChatCommand` wire enums |

## Session kinds, naming, directory

- Session id: a fresh `uuid::Uuid::new_v4()` (hyphenated, lowercase), also the Claude Code session id.
- tmux socket `loom-plan-<first 12 hex digits of the id with hyphens removed>`; tmux session
  `plan-<kind label>`; Claude display name from the guidance (`loom plan: <step or task>`).
- `list_loom_sockets` (`orchestrator/terminal/tmux/socket.rs:54-87`) skips names whose decoded id
  starts with `plan-`, beside the `view-` skip, with a comment, so `loom init`/`loom clean` never
  report planning sockets (bridge-runtime W5).
- Directory: `RuntimePaths::sessions/<id>/` (0700), layout of `SessionDir` (plan 6) plus
  `answers/` (0700). Deleted after the session's teardown (spool and answers are runtime state).

## Hooks registered per session (bridge-runtime passes them to `write_session_sandbox`)

```rust
pub const SPOOL_EVENTS: [HookSpec; 9] = [
    HookSpec { event: "SessionStart", matcher: None, timeout_secs: None },
    HookSpec { event: "UserPromptSubmit", matcher: None, timeout_secs: None },
    HookSpec { event: "PermissionRequest", matcher: Some("*"), timeout_secs: Some(3600) },
    HookSpec { event: "Notification", matcher: None, timeout_secs: None },
    HookSpec { event: "Stop", matcher: None, timeout_secs: None },
    HookSpec { event: "SubagentStart", matcher: None, timeout_secs: None },
    HookSpec { event: "SubagentStop", matcher: None, timeout_secs: None },
    HookSpec { event: "PreCompact", matcher: None, timeout_secs: None },
    HookSpec { event: "SessionEnd", matcher: None, timeout_secs: None },
];
```

Defined in `spool.rs` (bridge-core). Each runs `loom plan hook <Event> --session-dir <dir>`.

## Spool (`spool.rs`)

- Hook side: `pub fn append_event(session_dir: &Path, event: &str, payload: &serde_json::Value) -> anyhow::Result<()>`.
  The hook reads stdin bounded at 1 MiB + 1 (as plan 6's guard), parses JSON, and when
  `payload["hook_event_name"]` is present and differs from `event` it exits 0 silently
  (`mistakes/spurious-waiting-for-input.md`). The line is
  `{"received_at": <RFC 3339 UTC, millis>, "event": <event>, "session_id": .., "transcript_path": .., "payload": <the payload object with any string field over 64 KiB cut to 64 KiB and "…[truncated]" appended>}`
  plus `"request_id"` for `PermissionRequest`. Appended with `fs::safe_fs::safe_append(session_dir, "events.jsonl", line + "\n")`
  (single `write(2)` under an exclusive flock; `safe_fs.rs:360-372`). Any error: exit 0, no output
  (except `PermissionRequest`, below).
- Server side: `pub struct SpoolReader { path, offset }`, `pub fn read_new(&mut self) -> Vec<SpoolEvent>`,
  `pub fn was_reset(&self) -> bool` (the last `read_new` restarted at offset 0 because the file shrank
  or its inode changed; the reader never de-duplicates, the bridge skips already-applied events)
  over `tokens::tail::read_lines` (widened from `pub(super)` to `pub(crate)` with `stat` and `Line`,
  and `mod tail` in `tokens/mod.rs:15` to `pub(crate) mod tail;`, no other change), complete lines
  only, unparsable lines skipped with a `tracing::warn!`.
- `#[derive(Debug, Clone, PartialEq, Deserialize)] pub struct SpoolEvent { pub received_at: DateTime<Utc>, pub event: String, pub session_id: Option<String>, pub transcript_path: Option<PathBuf>, pub request_id: Option<String>, pub payload: Value }`.
  Payload fields read with fallbacks (`payload["source"]`, `payload["notification_type"]` or a
  `message` containing "waiting for your input", `payload["agent_id"]`); an event whose fields are
  missing is logged and never moves the state.

## Permission flow (`permission.rs`)

- Hook side, `loom plan hook PermissionRequest`: load `guard.json` (plan 6 `load_policy`); a
  `Summary` kind answers deny at once with message "summary sessions have no one to ask; their
  rules are wrong if this appears" after appending the spool line with `"auto_denied": "summary_session"`.
  Otherwise mint `request_id` (uuid v4), append the spool line, then poll
  `answers/<request_id>.json` every 200 ms for at most `HOOK_WAIT = 3595 s`. Read the answer
  bounded (4 KiB) and no-follow. Answer file: `{"behavior": "allow"|"deny", "message": string|null, "reason": DenyReason|null}`.
  Deny with reason `hook_error` and a message when: the spool append fails, the session directory
  or `answers/` disappears or is not a directory (session ended), the answer is unparsable, or the
  wait expires. Output exactly
  `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}` or
  `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"<msg>"}}}`
  through `serde_json`, exit 0. The hook never prints anything else.
- Server side: `pub fn write_answer(session_dir: &Path, request_id: &str, answer: &Answer) -> anyhow::Result<()>`:
  `request_id` must be a uuid (refuse anything else: it becomes a file name); write a temp file
  `answers/.<request_id>.tmp` with `safe_fs::safe_create_new_in_workdir` then
  `safe_rename_in_workdir` to `answers/<request_id>.json`, so the hook never reads a partial answer.
  `pub struct Answer { pub behavior: Behavior::{Allow, Deny}, pub message: Option<String>, pub reason: Option<DenyReason> }`.
- The server denies at `expires_at = received_at + 3590 s` with reason `timeout` (before the hook's
  own 3595 s and Claude Code's 3600 s hook timeout: a hook killed by its timeout is a non-blocking
  error and Claude Code would show its own dialog in a pane nobody watches).

## State machine (`state.rs`, pure)

`pub enum SessionState { Starting, Idle, Working, Permission, Ending, Ended, Failed }` (serde
snake_case). `pub enum Activity { Idle, Working, Permission }`. `pub struct StateMachine`,
`pub enum Input<'a> { Spool(&'a SpoolEvent), Record { at: DateTime<Utc>, main: bool }, RequestResolved { request_id: String }, Interrupted { at: DateTime<Utc> }, AgentsKilled { at: DateTime<Utc> }, SelfStartedTurn { at: DateTime<Utc> }, ProcessGone, SpawnFailed, EndRequested, PermissionAnswered { request_id: String }, PermissionExpired { request_id: String }, CompactDelivered }`,
`pub fn apply(&mut self, input: Input<'_>) -> Option<SessionState>` (Some when the state changed),
`pub fn state(&self) -> SessionState`, `pub fn activity(&self) -> Activity` (what the session is
doing underneath Ending, so the bridge knows when an Ending session is idle),
`pub fn pending_requests(&self) -> &BTreeSet<String>`, `pub fn subagents_running(&self) -> u32`.
Rules (section 07 table and its two honesty rules, made exact by the fix round):

- Starting -> Idle on `SessionStart` with `source == "startup"`. Idle on a main `Stop`, an
  interrupt record (`Interrupted`), or `Notification` idle_prompt.
- Working on `UserPromptSubmit`, on `SelfStartedTurn` (a turn Claude Code starts without a prompt:
  a `task-notification` user record, px-state-08), and on a main record whose `at` is later than
  the last idle event's `received_at` (a record after the last `Stop` means working, whatever the
  spool says). An `idle_prompt` or `Stop` received before a later main record does not win.
- Pending permissions are a SET of request ids (px-state-05). `PermissionRequest` (not
  auto-denied) adds one; `PermissionAnswered { id }` and `PermissionExpired { id }` remove one;
  Permission while the set is non-empty, then Working. Requests are removed only by evidence that
  names them (px-state-R03): a main `Stop` or an interrupt record clears the whole set, and
  `RequestResolved { request_id }` removes one. The bridge applies `RequestResolved` when the
  `tool_result` of the very `tool_use` the request names arrives: it binds each request to the
  pending `tool_use` item with the same tool name and an equal `tool_input` (the earliest unbound
  one), and that tool's result means Claude Code no longer waits on it. A result of ANY OTHER tool
  (two tools from one message, one allowed by the rules) never touches the request. The bridge
  closes each cleared card as denied with `note` "the session no longer waits for this answer" AND
  writes a deny answer file for it (`write_answer`, reason `session_ended`; harmless when the hook
  is gone). An answer or expiry for an id not in the set changes nothing (it never moves an Idle
  session to Working). The 3590 s expiry runs per request id in the bridge, independent of the set.
- `/compact` (px-state-10): Claude Code's event order for a built-in slash command is not observed
  on this host, so the machine accepts both. After the bridge delivered a `/compact` it applies
  `CompactDelivered`; then any of `UserPromptSubmit`, a `compact_boundary` record or
  `SessionStart { source: "compact" }` confirms the delivery (the bridge's concern), and
  `SessionStart { source: "compact" }` or a `compact_boundary` record with no Stop following
  within 2 s moves the activity to Idle. Outside a delivered `/compact`, a `compact` source
  changes nothing (auto-compaction mid-turn).
- Ending on `EndRequested`; `state()` stays Ending until Ended while `activity()` keeps tracking
  Idle/Working/Permission underneath.
- Ended on `SessionEnd` or `ProcessGone` while Ending; Failed on `SpawnFailed` or `ProcessGone`
  outside Ending.
- Running subagents: `SubagentStart` +1, `SubagentStop` -1 (never below 0); the count resets to 0
  on an interrupt record, on `AgentsKilled` (the `system` record subtype `agents_killed`, which the
  mapper now reports instead of skipping), and on `SessionStart` (px-state-09). A subagent whose
  transcript's last record is a final assistant message (`stop_reason` `end_turn`) and that has
  not grown for 30 s is treated as stopped by the bridge (it applies a synthetic `SubagentStop`
  once). Subagent transcript records (`main: false`) never move the state.

Claude Code behaviour no one has observed on this host (lead decision): whether a Stop follows an
interrupt, the `/compact` order, the double-Escape rewind picker, and the hook payload field names
are handled for both possibilities above and below; bridge-runtime's operator end-to-end run
asserts what the installed Claude Code actually does (it interrupts, compacts, and checks each
`TurnEnded`), and knowledge-distill records the observed sequence.

## Transcript mapping (`transcript/`)

Source: the main transcript, path from each `SessionStart` event's `transcript_path` (followed when
it changes), and each subagent transcript `<transcript dir>/<session-id>/subagents/agent-<id>.jsonl`
(`provider_data::resolve::{list_agent_files, agent_id_from_path}`, `resolve.rs:62,83`); the
subagent's spawning tool call is the `toolUseId` in the sibling `agent-<id>.meta.json`, falling
back to the Agent tool result's `agentId`. Complete lines only; every record keyed by `uuid`, a
seen uuid ignored; unknown `type`s skipped silently (`attachment`, `file-history-snapshot`,
`file-history-delta`, `mode`, `ai-title`, `api-request`, `api-request-blob`, `api-request-shape`,
`queue-operation`, `last-prompt`, `atis-latch`, `cost-state` and any other).

Record shapes captured from Claude Code transcripts on 2026-10-26 (values shortened):

```json
{"type":"assistant","uuid":"3454..","parentUuid":"35b7..","isSidechain":false,"timestamp":"2026-10-24T00:15:09.584Z","sessionId":"105e..","requestId":"req_..","message":{"id":"msg_011C..","model":"claude-opus-5-5","role":"assistant","content":[{"type":"text","text":"Latest run is still in progress"}],"stop_reason":"tool_use","usage":{"input_tokens":2,"cache_creation_input_tokens":706,"cache_read_input_tokens":55975,"output_tokens":165}},"apiBlockIndex":0}
{"type":"assistant", "...":"...", "message":{"id":"msg_011C..","content":[{"type":"thinking","thinking":"","signature":"Eq.."}]}, "apiBlockIndex":0}
{"type":"assistant", "...":"...", "message":{"id":"msg_011C..","content":[{"type":"tool_use","id":"toolu_01Rm..","name":"Bash","input":{"command":"gh run list --limit 5","description":"List recent CI runs"},"caller":{"type":"direct"}}]}, "apiBlockIndex":1}
{"type":"user","uuid":"e70d..","parentUuid":"3d03..","timestamp":"..","message":{"role":"user","content":[{"tool_use_id":"toolu_01Rm..","type":"tool_result","content":"in_progress ...","is_error":false}]},"toolUseResult":{"stdout":"..","stderr":"..","interrupted":false,"isImage":false,"noOutputExpected":false},"sourceToolAssistantUUID":"3d03.."}
{"type":"user", "...":"...", "toolUseResult":{"filePath":"/abs/path.rs","oldString":"..","newString":"..","originalFile":"..","structuredPatch":[{"oldStart":40,"oldLines":7,"newStart":40,"newLines":10,"lines":[" ctx","-old","+new"]}],"userModified":false,"replaceAll":false}}
{"type":"user","message":{"role":"user","content":"commit these fixes."},"promptId":"f79d..","permissionMode":"auto","origin":{"kind":"human"}}
{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>b3z..</task-id>..."},"origin":{"kind":"task-notification","producer":"session-task"}}
{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>The command below was run directly in ..."}}
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}
{"type":"system","subtype":"turn_duration","durationMs":676922,"messageCount":73,"uuid":"ae50..","timestamp":".."}
{"type":"system","subtype":"compact_boundary","compactMetadata":{"trigger":"auto","preTokens":243178,"postTokens":14113},"uuid":"c5d3..","parentUuid":null}
```

Other `system` subtypes seen: `stop_hook_summary`, `local_command`, `away_summary`,
`informational`, `bridge_status` (skipped), `agents_killed` (no item, reported to the state machine
as `AgentsKilled`), plus API errors (a `system`
record with `level: "error"` or an `apiError`/`error` field: a notice).

Mapping (section 07 "The transcript" table, made exact):

| Record | Item |
| --- | --- |
| `user`, content a string, not `isMeta`, `origin.kind` absent or `"human"`, text not starting with a wrapper tag (`<command-`, `<local-command-`, `<bash-`, `<task-notification>`, `<system-reminder>`) | `user` item: `attachments::parse_message` splits text and chips |
| `user`, `isMeta`, a wrapper tag, or `origin.kind` other than `human` | nothing; a `<task-notification>` record (or `origin.kind` `task-notification`) after the last idle event is reported as `SelfStartedTurn` so the bridge opens a turn for it (px-state-08) |
| `user`, a text block or string starting with `[Request interrupted by user` | `notice` "Interrupted", the open turn ends; every pending tool of the turn becomes `not_finished` |
| `user`, `tool_result` block | replaces the tool item with that `tool_use_id`: status `done`, or `error` when `is_error` (first line of the content as `error_line`), result text capped at 64 KiB with `result_truncated`; for a result with `toolUseResult.filePath`, `added`/`removed` counted from `structuredPatch` lines starting `+`/`-`, `patch` = unified text built from `structuredPatch`, and `filePath` joins the turn's by-tool changed paths |
| `assistant`, `text` block | `text` item, markdown |
| `assistant`, `thinking` block | `thinking` item; `duration_ms` = this record's timestamp to the next record's of the same session, filled when that record arrives |
| `assistant`, `tool_use` block | `tool` item, status `pending`, `summary` per section 08's table (below), `input` = the input object pretty-printed |
| `system`, `turn_duration` | the turn's `duration_ms` |
| `system`, `compact_boundary` | `notice` "Context compacted"; reported as `CompactBoundary` |
| `system`, API error | `notice` with the error's first line |

Assistant records sharing `message.id` are one message; its usage counts once per message id, the
LAST record's usage winning (later records carry the final counts), parsed with
`provider_data::claude::parse` (`provider_data/claude.rs:13`, `pub(crate)`), into the turn's
`tokens` (`input`, `cache_write` = `cache_creation`, `cache_read`, `output`). Subagent records map
the same way into `tool` items with `subagent: Some(<agent id>)` nested under the spawning tool
call; their text and thinking are not shown (only their tool rows), and their usage is added to the
turn's tokens. `Mapper::ingest_tokens_only(line)` adds a record's usage to the open turn and
produces no item; plan 11 routes in-process teammates' transcripts to it (section 11: teammates
count for tokens, none of their messages render) once it has confirmed where Claude Code writes
them; plan 7 routes every `subagents/agent-*.jsonl` through `ingest_subagent`.

Tool summaries (section 08 table): `Read` path plus `:a-b` when `offset`/`limit` given; `Edit`,
`Write`, `MultiEdit`, `NotebookEdit` the path; `Bash` the command's first line; `Grep`, `Glob`
the pattern and `in <path>` when given; `Agent`/`Task` the `description`; `WebFetch` the URL,
`WebSearch` the query; anything else the tool name and its first input field's value. Paths under
the project root are shown relative to it. Every summary is cut to 200 characters on a char
boundary.

## The chat model (`model/`, sent to browsers; plan 9 renders it and owns its zod schema)

All structs `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`, enums
`#[serde(rename_all = "snake_case")]`, times RFC 3339 UTC strings.

```rust
pub struct SessionView {
    pub id: String, pub kind: String /* authoring|summary|pressure */, pub state: SessionState,
    pub name: String, pub model: String, pub socket: String,
    pub holder: Option<String> /* holder display name */, pub away_until: Option<String>,
    pub queued: u32, pub started_at: String, pub state_since: String,
    pub working_since: Option<String>, pub subagents_running: u32,
    pub last_activity: Option<String>, pub failure: Option<Failure>,
    pub offer: Option<String> /* the Offer::id the chat started from */,
}
pub struct Failure { pub kind: FailureKind /* spawn|died */, pub message: String }
pub struct TokenCounts { pub input: u64, pub cache_write: u64, pub cache_read: u64, pub output: u64 }
pub struct ChangedFile { pub path: String, pub before_hash: Option<String>, pub by_tool: bool }
pub struct Turn {
    pub id: String /* "t<n>", n from 1 per session */, pub started_at: String,
    pub duration_ms: Option<u64>, pub open: bool, pub tokens: TokenCounts, pub items: Vec<Item>,
    pub changed_files: Vec<ChangedFile>, pub summaries_rebuilding: Vec<String>,
}
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    User { id: String, text: String, attachments: Vec<Attachment>, delivery: Delivery, origin: Option<Origin> },
    Thinking { id: String, duration_ms: Option<u64> },
    Text { id: String, markdown: String },
    Tool { id: String, tool_use_id: String, name: String, summary: String, status: ToolStatus,
           subagent: Option<String>, input: String, result: Option<String>, result_truncated: bool,
           error_line: Option<String>, added: Option<u32>, removed: Option<u32>, patch: Option<String>,
           nested_tools: u32 },
    Permission { id: String, request_id: String, tool: String, input: String, status: PermissionStatus,
                 deny_reason: Option<DenyReason>, note: Option<String>, expires_at: String },
    Notice { id: String, kind: NoticeKind, text: String },
}
pub enum Delivery { Sent, Delivered, Undelivered }
pub enum Origin { Offer, Start, Closing }
pub enum ToolStatus { Pending, Done, Error, Denied, NotFinished }
pub enum PermissionStatus { Pending, Allowed, Denied }
pub enum DenyReason { Holder, Timeout, SessionEnded, ServerStopping, HookError, Interrupt, SummarySession }
pub enum NoticeKind { Interrupted, Compacted, ApiError, Stopped, Info }
pub struct QueuedMessage { pub id: String, pub first_line: String, pub text: Option<String> /* holder only */ }
pub struct EndedChat { pub session: SessionView, pub turns: Vec<Turn> }
pub struct ChatSnapshot { pub session: Option<SessionView>, pub turns: Vec<Turn>, pub queue: Vec<QueuedMessage>, pub previous: Option<EndedChat> }
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChatDelta {
    Session { session: SessionView },
    TurnAdded { turn: Turn },
    TurnUpdated { turn_id: String, duration_ms: Option<u64>, open: bool, tokens: TokenCounts,
                  changed_files: Vec<ChangedFile>, summaries_rebuilding: Vec<String> },
    ItemAdded { turn_id: String, item: Item },
    ItemReplaced { turn_id: String, item: Item },
    ItemRemoved { turn_id: String, id: String },
    Queue { queue: Vec<QueuedMessage> },
    Cleared,
}
```

Item ids: `u-<record uuid>`, `a-<record uuid>-<block index>`, `n-<record uuid>`, `p-<request id>`,
and `s-<n>` for a user item the server added before its transcript record arrived (replaced by an
`ItemRemoved` + `ItemAdded` of the transcript item when it does, matched by sanitised text). Tool
items use `a-<uuid>-<i>` and carry `tool_use_id`.

`Origin::Offer` marks the opening message when the chat was started from an offer (or a summary or
pressure session's opening); `Origin::Start` marks a generated opening for a chat started with no
step (plan 8's "no step chosen" opening); a chat started by the user's own first message has
`origin: null`.
`Origin::Closing` marks the user record whose text equals the closing message the bridge
delivered. The server knows both; the mapper is told (`Mapper::expect_opening(origin)`,
`Mapper::expect_closing(text)`).

`summaries_rebuilding` stays empty in plan 7 (section 04's engine fills it in plan 10).

Wire (`loom/src/plan_server/wire/chat.rs`):

```rust
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum ChatFrame { Snapshot(ChatSnapshot), Delta(ChatDelta), Result(CommandResult), QueueReturned { texts: Vec<String> } }
pub struct CommandResult { pub client_id: Option<String>, pub ok: bool, pub error: Option<String> }
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ChatCommand {
    Start { client_id: Option<String>, offer: Option<String>, text: Option<String>, attachments: Vec<AttachmentInput> },
    Send { client_id: Option<String>, text: String, attachments: Vec<AttachmentInput> },
    Withdraw { client_id: Option<String>, queue_id: String },
    Permission { client_id: Option<String>, request_id: String, allow: bool, note: Option<String> },
    Interrupt { client_id: Option<String> },
    End { client_id: Option<String> },
}
```

`PlanMessage::Chat(ChatFrame)` and `ClientMessage::Chat(ChatCommand)` are the one variant plan 7
adds to each enum (bridge-runtime). Fixtures: `web/src/plan/api/fixtures/chat-snapshot.json` (a
snapshot with one closed turn holding one of every item type and status, one open turn, a queue,
and a previous chat), `chat-deltas.json` (an array, one of every `op`), `chat-commands.json` (an
array, one of every command), `chat-frames.json` (one of every frame). These JSON files are data
for the Rust parity tests (and, from plan 9, its vitest schema test); no TypeScript is added.

## Attachments (`attachments/`)

```rust
pub enum AttachmentKind { Source, Summary }
pub enum LockedTag { Running, Done, Retired, Knowledge, Brief }
pub struct Attachment {
    pub kind: AttachmentKind, pub path: String /* repo-relative, starts with doc/ */,
    pub anchor: Option<String> /* GitHub anchor, "loom-metadata", "stage:<id>" or a sentence id */,
    pub lines: Option<(u32, u32)>, pub stage: Option<String>, pub summary_sentence: Option<String>,
    pub rests_on: Vec<String> /* "path#anchor" */, pub quote: String,
    pub changed_since_selected: bool, pub locked: Option<LockedTag>,
}
pub struct AttachmentInput {
    pub kind: AttachmentKind, pub path: String /* repo-relative, starts with doc/ (the wire rule) */,
    pub anchor: Option<String>, pub lines: Option<(u32, u32)>, pub stage: Option<String>,
    pub summary_sentence: Option<String>, pub rests_on: Vec<String>, pub quote: String,
    pub selected_hash: Option<String>,
}
pub fn format_message(text: &str, attachments: &[Attachment]) -> String;
pub fn parse_message(delivered: &str) -> (String, Vec<Attachment>);
pub fn resolve(project: &Project, input: AttachmentInput) -> Result<Attachment, AttachmentError>;
pub const MAX_ATTACHMENTS: usize = 10; pub const MAX_QUOTE_BYTES: usize = 16 * 1024;
pub const QUOTE_CHARS: usize = 2000;
```

Block (section 10, exact): the user's text, a blank line, `<loom-attachments>`, per entry
`[n] <kind> <address>[ lines a-b][ rests-on a,b][ locked=<tag>][ changed-since-selected]` (that
field order), then the quote, every line prefixed `> `; a quote over 2,000 characters is cut at
the last line boundary at or before 2,000 characters and ends with
`> […N more characters; read the source]`; `</loom-attachments>`. `<address>` = `path#anchor`,
`path#stage:<id>` for a stage, `<summary file>#<sentence id>` for a summary. A line of the
user's text matching `^\\*</?loom-attachments>$` (after trimming) gets one more leading
backslash (`<loom-attachments>` becomes `\<loom-attachments>`, `\<loom-attachments>` becomes
`\\<loom-attachments>`), and `parse_message` removes exactly one from such lines, so every user
text round-trips. `parse_message` reads only a block that starts at a line equal to
`<loom-attachments>` preceded by a blank line and runs to the last line `</loom-attachments>`; a
malformed block leaves the text whole and no chips. `quote` round-trips without the `> ` prefixes
(a cut quote keeps its marker line). Truncation and escaping are counted in characters, never
bytes, and cut on char boundaries.

Wire path rule (shared with plan 2): every wire field named `path` and every `rests_on` address is
repo-relative (`doc/...`); only plan 3's request parameters (`?path=`, `SaveRequest.path`) and
`/files/` addresses are relative to `doc/`. For a summary attachment, `path` =
`doc/loom/summaries/<summary id>.md`, `anchor` = `summary_sentence` = the sentence id
(`s-` + 8 hex); `selected_hash` is the hash the browser captured at selection (plan 3's draft
carries it), never one taken at send.

`resolve` (server side; never trusts the browser's provenance): `path` must start with `doc/` and
the remainder pass `check_relpath_syntax`; every `rests_on` entry must be `doc/<path>#<anchor>`
with no newline, its path passing the same checks and being a discovered document (emitted in that
repo-relative form); a summary's `summary_sentence` must equal its `anchor` and match `s-[0-9a-f]{8}`;
the path must be a discovered document (`project.tree.get`) or, for `Summary`, a file under
`doc/loom/summaries/`; quote over 16 KiB is refused; `changed_since_selected` = `selected_hash`
present and different from the current hash, which is `project.content_hash(path)` for a
`Source` and, for a `Summary` (discovery skips the store, so `content_hash` is `None` there,
px-seams-R02), `planning::sections::normalized_hash` of the summary file read bounded and no-follow
through `fs::safe_read` from the store path (plan 4's `planning::summary::store::summary_path`),
the same value plan 5's `ConceptPage.file_hash` carries; a summary file that cannot be read is
`AttachmentError::NotADocument`; `locked` from `project.locked`
(`PlanState::Running` -> running, `Done` -> done, `Retired` -> retired, archive -> done, run review
-> done, brief of a locked plan -> brief) and `knowledge` for any path under `doc/loom/knowledge/`.

## Input (`input.rs`)

`pub fn sanitize(text: &str) -> String`: remove every C0 control except `\n` and `\t`, DEL (0x7F),
and every C1 (U+0080-U+009F); `\r\n` -> `\n`, a lone `\r` removed (section 12).
`pub fn check_send(text: &str) -> Result<(), InputRefusal>` on the user's own text: a text whose
first non-whitespace character is `/` is refused unless its first word is exactly `/compact`.
`pub fn check_delivered(formatted: &str) -> Result<(), InputRefusal>`: over 100 KB (`100 * 1000`
bytes, measured after `format_message` and `sanitize`) is refused.
`pub enum InputRefusal { SlashCommand, TooLong { bytes: usize, limit: usize }, TooManyAttachments, Empty }`
with `Display` messages the browser shows.

## Bridge API (bridge-runtime; what plans 8, 9, 10 and 11 call)

```rust
pub struct LaunchGuidance { pub brief_path: PathBuf, pub plugin_dir: PathBuf, pub opening_message: String,
                            pub model: String, pub effort: String, pub display_name: String,
                            pub opening_on_argv: bool /* default false; see below */ }
pub type GuidanceFn = Box<dyn FnOnce(&SessionDir) -> anyhow::Result<LaunchGuidance> + Send>;
// `opening_message` is NOT a launch argument (px-security-04: argv is world-readable through
// /proc/<pid>/cmdline and would skip sanitising and the 100 KB check). The launch line has no
// positional prompt; after the first `SessionStart { startup }` the bridge delivers the opening
// message through the same path as every send (`sanitize`, `check_delivered`, `paste_text`), as
// the session's first turn, tagged with `opening_origin`.
// `opening_on_argv: true` (px-seams-R04, px-seams-F01) passes the opening as the positional prompt
// instead of pasting it. The bridge accepts it only when the opening matches
// `^/[a-z0-9:-]+ doc/plans/[^\s]+\.md$` (a command and a plan path, no document content);
// anything else with the flag set fails the start. Plan 11 ALWAYS sets it for its two pressure
// openings (`/loom-plan:pressure <plan>`, `/loom-plan:address <plan>`); plan 8's authoring openings
// never set it (`From<SessionGuidance>` writes false). The operator e2e's pasted-slash-command check
// covers authoring openings only.
pub struct StartRequest { pub kind: SessionKind, pub guidance: GuidanceFn, pub opening_origin: Option<Origin>,
                          pub offer: Option<String>, pub extra_env: Vec<(String, String)> }
pub struct BridgeConfig {
    pub project_root: PathBuf, pub runtime: RuntimePaths, pub loom_bin: PathBuf, pub claude_bin: PathBuf,
    pub home: PathBuf, pub closing_message: Arc<dyn Fn() -> String + Send + Sync> /* read when delivered */,
    pub delivery_timeout: Duration /* 10 s */, pub final_write_bound: Duration /* 600 s */,
    pub permission_deny_after: Duration /* 3590 s */, pub identity_interval: Duration /* 2 s */,
    pub tick: Duration /* 200 ms */,
}
pub enum BridgeEvent {
    Started { id: String, kind: SessionKind },
    State { id: String, state: SessionState },
    TurnStarted { id: String, turn_id: String },
    TurnEnded { id: String, turn_id: String, changed: Vec<ChangedFile> },
    SubagentStopped { id: String, agent_id: String },
    Ended { id: String, kind: SessionKind, outcome: EndOutcome },
}
pub enum EndOutcome { Closed, Marker, TornDown, Failed(String) }
// Marker ends keep the session's verified children alive and its directory (codex.log) on disk
// until `release_session`; every other end terminates children and removes the directory (px-seams-01).
pub trait SessionHost: Send + Sync {
    fn spawn(&self, spec: &LaunchSpec) -> anyhow::Result<Spawned /* { socket: String, identity: ProcessIdentity } */>;
    fn deliver(&self, socket: &str, target: &str, text: &str) -> anyhow::Result<()>;
    fn interrupt(&self, socket: &str, target: &str) -> anyhow::Result<()>;
    fn teardown(&self, socket: &str) -> anyhow::Result<()>;
    fn identity(&self, identity: ProcessIdentity) -> IdentityStatus;
}
impl Bridge {
    pub fn new(config: BridgeConfig, host: Arc<dyn SessionHost>, pins: Option<Arc<dyn TurnPins>>) -> Arc<Bridge>;
    pub fn start_session(&self, req: StartRequest) -> Result<String /* id */, BridgeError>;
    pub fn send(&self, id: &str, text: String, attachments: Vec<Attachment>) -> Result<SendOutcome /* Delivering | Queued { queue_id } */, BridgeError>;
    pub fn withdraw(&self, id: &str, queue_id: &str) -> Result<(), BridgeError>;
    pub fn answer_permission(&self, id: &str, request_id: &str, allow: bool, note: Option<String>) -> Result<(), BridgeError>;
    pub fn interrupt(&self, id: &str) -> Result<Vec<String> /* returned queued texts */, BridgeError>;
    pub fn end(&self, id: &str, mode: EndMode /* FinalWrite | Teardown */, cause: EndCause /* Holder | GraceExpired | Draining | Caller */) -> Result<Vec<String> /* returned queue */, BridgeError>;
    pub fn record_child(&self, id: &str, label: &str, identity: ProcessIdentity) -> Result<(), BridgeError>;
    /// After a Marker end: terminate any still-verified children the caller did not join and remove the
    /// session directory. Recovery of a later server does the same for a never-released directory.
    pub fn release_session(&self, id: &str) -> Result<(), BridgeError>;
    /// The driver also tails `path` and feeds each complete line to `Mapper::ingest_tokens_only`, so the
    /// turn's tokens and its `TurnUpdated` delta include it (plan 11's in-process teammates, px-seams-15).
    pub fn add_token_transcript(&self, id: &str, path: PathBuf) -> Result<(), BridgeError>;
    pub fn subscribe(&self, listener: Box<dyn Fn(&BridgeEvent) + Send + Sync>);
    pub fn subscribe_chat(&self, listener: Box<dyn Fn(&ChatDelta) + Send + Sync>);
    pub fn chat_snapshot(&self, for_holder: bool) -> ChatSnapshot;
    pub fn chat_session_id(&self) -> Option<String> /* the authoring or pressure session shown in the chat */;
    pub fn turn_running(&self) -> bool /* the chat session is Starting, Working or Permission (plan 9's Restore refusal) */;
    pub fn transcripts(&self, id: &str) -> Option<Transcripts> /* plan 10's cost reader */;
    pub fn sessions(&self) -> Vec<(String, SessionKind, SessionState)>;
    pub fn recover(&self) -> RecoveryReport;
    /// Runs `f` once with the recovery report when recovery finishes, or at once if it already has
    /// (plan 11 deletes an interrupted pressure run's Codex report from it).
    pub fn on_recovered(&self, f: Box<dyn FnOnce(&RecoveryReport) + Send>);
    pub fn drain(&self) /* first Ctrl-C: final writes, bounded */; pub fn abort(&self) /* second: teardown all */;
    pub fn run(self: &Arc<Self>, stop: Arc<AtomicBool>) -> JoinHandle<()> /* the driver thread */;
}
pub struct RecoveredSession { pub id: String, pub kind: SessionKind /* Pressure carries the plan */, pub children: Vec<ChildRecord> }
pub struct RecoveryReport { pub closed: Vec<RecoveredSession>, pub torn_down: Vec<RecoveredSession>, pub left_alone: Vec<(String /* id */, String /* socket */, String /* why */)> }
pub struct Transcripts { pub main: Option<PathBuf>, pub subagents: Vec<(String /* agent id */, PathBuf)> }
pub trait TurnPins: Send + Sync { fn begin_turn(&self, turn: &str); fn end_turn(&self, turn: &str) -> Vec<(String, Option<String> /* before hash */)>; }
```

`TurnPins` is implemented in `plan_server/chat/pins.rs` over plan 3's `TextStore` (turn key
`<session id>/<turn id>`, so turns of different sessions never collide); the bridge stays
testable without a server.

Guidance seam (`plan_server/chat/guidance.rs`, bridge-runtime), so plan 8 replaces the fixed
briefs without touching the bridge:

```rust
pub trait GuidanceProvider: Send + Sync {
    fn authoring(&self, ctx: &PlanContext, dir: &SessionDir, offer: Option<&str> /* Offer::id */,
                 first_message: Option<&str>) -> anyhow::Result<LaunchGuidance>;
    fn closing_message(&self) -> String;
}
pub struct GuidanceSlot(RwLock<Arc<dyn GuidanceProvider>>);
impl GuidanceSlot { pub fn get(&self) -> Arc<dyn GuidanceProvider>; pub fn set(&self, provider: Arc<dyn GuidanceProvider>); }
pub struct FixedGuidance; // plan 7's provider
```

`FixedGuidance::authoring` writes `brief.md` (0600, a fixed paragraph: "You are a planning
session started by `loom plan`. Write only under doc/. Ask in plain text, never with a question
tool.") and a minimal plugin (`plugin/.claude-plugin/plugin.json` =
`{"name": "loom-plan", "version": "0.0.0", "description": "loom plan session"}`), model `opus`,
effort `medium`, display name `loom plan: chat`, opening message = the user's first message, or
for an offer `Start the step "<offer id>".`. Its `closing_message` is the fixed text "Record the
remaining open questions and anything still pending into the documents now, then stop." Plan 8
supplies the real ones. `plan_server::chat::install` builds `BridgeConfig::closing_message` as a
closure over the slot (`move || slot.get().closing_message()`), so the bridge reads the current
provider's text at the moment it delivers it.

## Authoring holder rules on the server side (bridge-runtime W6; px-state-01, px-state-23)

```rust
// plan_server/chat/lease.rs, stored in ctx.extensions()
pub struct AuthoringLeases { .. }
impl AuthoringLeases { pub fn hold(&self) -> AuthoringLease; pub fn held(&self) -> bool; }
pub struct AuthoringLease { .. } // RAII: dropping it ends the hold
```

- Authoring is released by plan 7 only when a `SessionKind::Authoring` session ends, only while
  `AuthoringLeases::held()` is false, and only through plan 2's
  `Authoring::release_if(&starter_public_id, Released("chat ended"))` with the `PublicId` of the
  browser that started that chat (recorded by `dispatch` at `Start`), so a browser that took
  authoring meanwhile (after a grace expiry) keeps it (px-state-R02). A pressure or summary session's end never touches
  authoring. Plan 11 holds a lease for a whole pressure run (from its `require` to its own release),
  so ending the open chat before the run does not free authoring.
- `Freed { GraceExpired }` ends an `Authoring` chat with a final write; a running pressure session
  is not ended by plan 7 (plan 11 decides and tests what grace expiry does to a run).
- `QueueReturned` goes to the connection that sent the Interrupt (`send_to_conn`); the holder's
  other tabs get a `notice` item ("Stopped; the queued messages went back to the tab that
  stopped"), so returned texts are never duplicated across tabs.
- Grace expiry and `drain` deny every pending permission request at once and every new one on
  arrival while Ending (`holder` for grace, `server_stopping` for drain), so a closing write never
  waits on an answer nobody can give (px-state-06). An `End` the holder pressed keeps requests
  answerable. Plan 2 keeps WebSockets open until every drain returns.

## Rules for every worker

- 400 lines per file, 50 per function (`cargo test --test maintainability`; the exact ledger
  `loom/maintainability-baseline.txt` must not change: `orchestrator/terminal/tmux/mod.rs` is 355
  lines and gets visibility changes only; `socket.rs` 351 gets at most 4 lines; `tokens/tail.rs`
  107, `tokens/mod.rs` visibility only). New code in new modules.
- Host-side opens of session-writable paths (`doc/`, `drafts/`, `claims/`) go through
  `fs::safe_read`/`fs::safe_fs`; the session directory is not session-writable (plan 6) but is
  still written through `safe_fs` (no-follow), as `write_capsule_file` does.
- Every external command is bounded (`process::run_bounded_output`); tmux stderr with exit 0 is a
  failure (`mistakes/tmux-backend.md`); `load-buffer -w` is never used.
- Liveness and signals go through `process::identity::{verify_process_identity, terminate_verified}`,
  never `tmux has-session` (`tmux/backend.rs:159` explains why). A destructive step on missing or
  unverifiable evidence does nothing and says so.
- Every test file is declared inside the module it tests (`#[cfg(test)] #[path = "spool_tests.rs"] mod tests;`
  in `spool.rs`, as `tokens/tail.rs` does), so its tests run under that module's path and the stage's
  `cargo test --lib plan_session::` / `plan_server::` filters select them; a test file nothing
  selects never runs before integration-verify.
- No new dependency (`uuid`, `chrono`, `serde_json`, `shell-escape`, `tempfile`, `serial_test`
  exist). Comments at the surrounding density. Never run git. Record surprises with `loom memory note`.
