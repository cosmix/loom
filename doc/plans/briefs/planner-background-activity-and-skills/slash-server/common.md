# slash-server: shared brief

Every worker of stage `slash-server` reads this file in full, then its own brief. This file settles the interfaces and the exact texts; the plan settles the behaviour.

## Where things are

- **The plan.** `doc/plans/IN_PROGRESS-PLAN-planner-background-activity-and-skills.md` once `loom run` has started (`PLAN-…` before). Read "B. Slash commands and skills": "Why `/` is refused today", "What the terminal menu lists", Decisions B-D1 to B-D4 and B-D6, Steps B2 to B4 and B6. Where this file and the plan differ on a name or a signature, this file wins.
- **The evidence.** `loom/tests/fixtures/plan_session/probe-notes.md` ("P2. Slash commands and skills": the per-paste matrix, the record shapes per class, the side process, the row shape and source markers, the built-in rows), the `p2-*` files under `loom/tests/fixtures/plan_session/transcripts/` and `spool/`, and `loom/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl`: line 1 is the `control_response` to `initialize` (117 rows at `.response.response.commands`), line 2 the `system` `commands_changed` line (118 rows at `.commands`). They are read-only. Names in them are redacted (`<SKILL-2>`, `<PLUGIN-2>:docs`): treat a name as any text. A shape the fixtures lack: stop and report it; the main agent blocks the stage.
- **The probe harness** (`loom/tests/plan_session_bridge_e2e/probes/side.rs` `side_argv`, `side_settings`; `probes/side_run.rs` the `initialize` line) is the evidence of how the side process was launched in P2. It is test code of another target: read it, never import or edit it.
- **The merged background table** (stage `background-server`): `plan_session::bridge::background`, the notification and hand-back evidence in `plan_session::transcript::notification`, and `confirm_prompt` ignoring `<task-notification>` and `<agent-message` prompts. A fork skill's agent is a table entry; its notification opens the next turn.
- **Paths.** The stage's `working_dir` is `loom/`. Every command in a brief runs from `loom/` unless it starts with `cd ..`.

## Rules for every worker

- Write only the files your brief's "Files owned" lists; stop and report when you need another worker's file changed.
- Frozen and read-only to every worker: `src/plan_session/commands/contract_tests.rs`, `src/plan_session/input_contract_tests.rs`, `src/plan_session/bridge/slash_contract_tests.rs`, `src/plan_session/transcript/commands_contract_tests.rs`, `src/plan_session/sandbox/plugins_contract_tests.rs`, `src/plan_server/commands/contract_tests.rs` (`loom stage contracts show slash-server`). The `mod` lines declaring them are yours to keep; a declaration-only module root the contract session created becomes the production module.
- Compile and format only your own files (`cargo check`, `cargo build`, `rustfmt --edition 2021 <files>`; no `cargo fmt`, no linter). Run your own module's tests once ready; never a whole suite.
- Size caps: 400 lines per file, 50 per function, after `rustfmt`. Near the cap: `bridge/mod.rs`, `driver.rs`, `flow.rs`, `session.rs`, `transcript/records.rs`, `plan_server/chat/mod.rs` (count them when you start; background-server grew some). No `maintainability-baseline.txt` row may be added or raised.
- Every string from a transcript, a spool payload or an `initialize` row is untrusted: cap what you store, and flatten what leaves the server with `crate::context::untrusted::inline_safe` (the commands route and frame).
- MEMORY: record mistakes, decisions and surprises at once with `loom memory note` / `loom memory decision`, with `--evidence file:line`. Never Claude Code auto-memory, never `loom knowledge`.
- Report: files changed, tests run and their result, assumptions, anything unresolved. Do not run git.

## Interfaces

### The command table (W1, `src/plan_session/commands/`)

`pub mod commands;` in `src/plan_session/mod.rs`; files `mod.rs`, `builtins.rs` (the native table), `list.rs` (the merge, the source markers, the classes), `side.rs` (the process). Types, as the stage description's contract surface fixes them:

```rust
pub struct CommandEntry { pub name: String, pub aliases: Vec<String>, pub description: String, pub argument_hint: String, pub source: CommandSource, pub class: CommandClass, pub reason: Option<String> }
#[serde(rename_all = "kebab-case")] pub enum CommandSource { BuiltIn, Bundled, Plugin, Personal, Account, Skill }
#[serde(rename_all = "kebab-case")] pub enum CommandClass { Run, RunWithHandling, TerminalOnly }
pub enum CatalogueState { Loading, Ready { generation: u64 }, Unavailable { reason: String } }
pub struct CommandTable { /* state + entries */ }
impl CommandTable {
    pub fn loading() -> Self;                 // native rows only
    pub fn unavailable(reason: &str) -> Self; // native rows only
    pub fn from_initialize(commands: &[serde_json::Value], generation: u64) -> Self;
    pub fn state(&self) -> &CatalogueState;
    pub fn entries(&self) -> &[CommandEntry];
    pub fn lookup(&self, name: &str) -> Option<&CommandEntry>; // without the slash; a name or an alias
}
pub type CommandCell = Arc<RwLock<Arc<CommandTable>>>;
```

`CommandEntry`, `CommandSource` and `CommandClass` derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`; the wire keys are the field names (`argument_hint`, snake_case). Entry order: the native rows in `builtins.rs` order are last; every other row keeps its `initialize` order before them. Descriptions have their source marker stripped.

### Classes and reasons (exact texts)

| Class | Rows | `reason` |
| --- | --- | --- |
| Run | every non-native row: bundled, plugin, personal, account and project skills and commands, fork skills | none |
| Run with handling | `compact`, `context` | none |
| Terminal only | `clear` (`reset`, `new`), `resume`, `exit`, `rewind`, `branch`, `fork`, `background`, `restart`, `cd`, `add-dir`, `rename`, `login`, `logout` | `Changes the session outside the planner` |
| Terminal only | `usage` (`cost`, `stats`), `model`, `help`, `status`, `permissions`, `config`, `hooks`, `plugin`, `mcp`, `agents`, `memory`, `ide`, `theme` | `Opens a terminal dialog` |
| Terminal only | `effort` | `Saves your default for new sessions` |
| Terminal only | every other native row (`init`, `security-review`, `recap`, `goal` and the rest of the 57) | `Not verified in the planner` |

The native table holds the 36 native rows of the headless list and the 21 missing from it, as `probe-notes.md` "Built-in rows" lists them. A native row in the headless list copies its description, aliases and argument hint from the fixture's row; a missing one gets a description of at most eight words that says what the command does in the terminal (no claim the fixtures do not support).

The gate's other texts: `Not in this session's command list` (an unlisted name), `Commands are still loading` (a non-native name while `Loading`), `This session's command list is unavailable` (a non-native name while `Unavailable`), `Too long for a command` (more than `MAX_SLASH_LEN` = 300 characters), `Send a command alone, on one line` (attachments, or more than one line).

### The side process (W1, `side.rs`)

`side_argv`, `side_settings`, `SideSpec`, `SideProcess::{start, pid, stop}` and `STARTUP_DEADLINE` as the contract surface fixes them. The side process's bounds: a stdout line over 1 MiB, or 8 MiB in total before `initialize` answers, kills it; it is killed, waited for and its sibling settings file deleted on the deadline, an output cap, `stop()` and drop. While it lives, each `system` `commands_changed` line replaces the table (generation + 1) through `on_table`. It never receives a user message.

### The argv builder (W1, `launch/script.rs`)

`pub fn claude_argv(spec: &LaunchSpec, settings: &Path) -> Vec<OsString>` (the words after the environment) and `pub fn claude_env(spec: &LaunchSpec, env: &[(OsString, OsString)]) -> Vec<(OsString, OsString)>` (what `env -i` sets today, `HOME`, `LOOM_PLAN_STATE`, `LOOM_PLAN_SESSION` included). `launch_script` renders both, shell-escaped, and its output is byte-identical to today's (`tests/plan_session_bridge_runtime_contracts.rs` and the `script.rs` tests pin it).

### The bridge (W2)

- `Bridge::command_cell(&self, id: &str) -> Option<CommandCell>`: each session's cell, created with `CommandTable::loading()`.
- `Bridge::launch_spec(&self, id: &str) -> Option<LaunchSpec>`: the spec the live session was spawned with (kept on the session when its phase leaves `Spawning`).
- Before it pastes a queued slash item, the delivery path reads the session's cell and checks the item's first token with the table (the class rows above; a name `lookup` misses is `Not in this session's command list`). A now-refused item is dropped from the queue, never pasted, and the chat gets `Item::Notice { kind: NoticeKind::Info, text: "Not sent (<reason>): <first line of the message>" }`; the next queued item is eligible in the same step.
- `Item::User` gains `command: Option<CommandRef>` with `pub struct CommandRef { pub name: String, pub args: String }` (`model/mod.rs`): set on the user item of a prompt-type command record, `None` everywhere else. Serde writes `null` for `None`.

### The server (W3)

- `src/plan_session/input.rs`: `check_send(text, table)`, `InputRefusal::Command(String)`, `MAX_SLASH_LEN` as fixed above.
- `src/plan_server/commands/mod.rs`: the route `GET /api/session/commands?session=<id>` (Role::Viewer), the catalogue owner, and `PlanMessage::Commands`. The route body and the frame's fields are one shape, `CommandsView`:

```rust
pub struct CommandsView { pub state: CatalogueWireState, pub generation: Option<u64>, pub reason: Option<String>, pub entries: Vec<CommandEntry> }
#[serde(rename_all = "snake_case")] pub enum CatalogueWireState { Loading, Ready, Unavailable }
PlanMessage::Commands { session: String, state: CatalogueWireState, generation: Option<u64>, reason: Option<String>, entries: Vec<CommandEntry> } // tag "commands"
```

- `web/src/plan/api/fixtures/session-commands.json` is a `CommandsView` in state `ready`, generation 1, written from the Rust serializer with a trailing newline. Its entries, in order: `simplify` (bundled, run), `loom-plan:args` (plugin, run, alias `args`), `probe-personal` (personal, run), `docs` (account, run), `context` (built-in, run-with-handling), `compact` (built-in, run-with-handling), `clear` (built-in, terminal-only, aliases `reset`, `new`, reason `Changes the session outside the planner`), `usage` (built-in, terminal-only, aliases `cost`, `stats`, reason `Opens a terminal dialog`), `effort` (built-in, terminal-only, reason `Saves your default for new sessions`), `help` (built-in, terminal-only, reason `Opens a terminal dialog`). Descriptions and argument hints come from the probe rows (markers stripped) or the native table.
- `web/src/plan/api/fixtures/slash-vectors.json` is `{"tables": {"ready": <CommandsView>, "loading": <CommandsView>, "unavailable": <CommandsView>}, "cases": [{"text": "...", "table": "ready", "expect": "ok" | "<reason>"}]}`. `ready` is the `session-commands.json` view; `loading` and `unavailable` hold the native rows only. The cases cover every row of the plan's B3 server list: `/clear`, `/model`, `/model opus`, `/effort high`, `/usage`, `/cost`, `/context`, `/compact`, `/compactx`, `/loom-plan:args x`, `/simplify`, an unlisted name, `\u0001/clear`, `\ufeff/exit`, `\u0085/clear`, a 300-character and a 301-character slash line, a two-line slash message, plain text on every table, and a skill on `loading` and on `unavailable`. The Rust test of `check_send` reads it with `include_str!`; `planner-web`'s gate test imports it.

### The web wire (W4)

- `web/src/plan/api/schema-session-commands.ts`: `commandEntrySchema`, `commandsViewSchema` (strict) and `commandsMessageSchemas` (`type: "commands"`, `session`, and the view's fields), spread into `planMessageSchema`, with `"commands"` in `PLAN_MESSAGE_TYPES`.
- `schema-chat.ts`: the user item gains `command: commandRefSchema.nullable()` with `commandRefSchema = z.strictObject({ name: z.string(), args: z.string() })`.

## The gate

The acceptance list is the gate (one `loom check` by the main agent): build with `--version` and `--help`, clippy, `cargo fmt --check`, `cargo doc`, `cargo test --lib` for `plan_session::commands::`, `plan_session::`, `plan_server::commands::`, `plan_server::chat::`, `plan_server::wire::` and `planning::guidance::`, `cargo test --test` for `plan_session_bridge_core_contracts`, `plan_session_bridge_runtime_contracts`, `plan_session_sandbox_contracts` and `maintainability`, the old guidance and spec sentences gone, markdownlint on the five rewritten files, and the web typecheck, lint, format check, the tests under `src/plan/{api,chat,help}` and the scratch dist build diffed against `web/dist`.
