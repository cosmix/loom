# Probes P1 and P2: background tasks and slash commands

These are the answers that `doc/plans/PLAN-planner-background-activity-and-skills.md` takes from a real Claude Code. Every fixture below comes from **Claude Code 2.1.321**, except `p1-status`, which ran on 2.1.322: the version line opens each note under `probe-notes/`, and every transcript record carries its `"version"`.

Evidence pointers name a fixture under this directory (`transcripts/`, `spool/`) or a raw note as `probe-notes/<file>.md#<heading>`. "Not observed" means no fixture or note shows it.

## How the probes ran

The harness is `loom/tests/plan_session_bridge_e2e/probes.rs` and its modules under `probes/`. Each probe is one `#[ignore]` test. The operator runs them from `loom/`, on the host, outside any sandbox, with a logged-in `claude`:

```bash
cargo test --test plan_session_bridge_e2e probes:: -- --ignored --test-threads=1
```

A single probe runs as `cargo test --test plan_session_bridge_e2e probes::p1_status::p1_failed_and_stopped_task_statuses -- --ignored --exact`.

Each test:

- drives a plan session through the session bridge over tmux. The bridge pastes with `paste-buffer -p` and sends Enter 150 ms later, so pane frames up to about 150 ms show the state Enter met;
- starts the session with `--model sonnet --effort low --permission-mode auto`, `--setting-sources user,project`, `--strict-mcp-config` and a `--plugin-dir` holding the `loom-plan` plugin (`/loom-plan:probe`, `/loom-plan:args`);
- runs in a throwaway git project under a temp base, under a copied HOME (`probes/home.rs`). Skills, commands and plugin trees are copied, and the plugin registries point at the copy. The real `~/.claude/settings.json` is hashed before and after; a change fails the test. The real `~/.claude.json` is checked never to name the temp base;
- installs its test skills in the copied HOME (`probes/setup.rs`, `probes/p2.rs`), and for P2 registers recording hooks for `UserPromptExpansion` and `PostModelSwitch`, which loom does not spool. Their payloads go to `spool/p2-*.extra.jsonl`. A P2 fixture with no `.extra` file means neither hook fired in that run;
- has a 180 s deadline, and kills the tmux server, the side process and every descendant on any exit.

Scrubbing:

- `probes/redact.rs` replaces the home and temp paths (`<HOME>`, `<BASE>`), the user name (`<USER>`), and session and agent ids (`<SESSION-n>`, `<AGENT-n>`, numbered in the order first seen).
- `probes/names.rs` replaces the operator's personal skill, command, agent and plugin names with `<SKILL-n>`, `<PLUGIN-n>` and `<AGENT-DEF-n>`.
- `probes/scrub.rs` removes proxy credentials and sockets (`<PROXY-TOKEN>`, `<SOCK>`), the Claude Code temp root (`<CLAUDE-TMP>`), attribution text, and the plan, usage, reset and zone figures (`<PLAN>`, `<PCT>`, `<RESET>`, `<TZ>`). It drops the attachments that list the operator's skills, agents and tools, and the API request blobs; a note marks each as `(record omitted: …)`.
- `probes/home.rs` maps account fields to `<EMAIL>`, `<NAME>`, `<ORG>`, `<ORG-ID>` and `<ACCOUNT>`.
- Shell task ids (`bmiyho9ji` style) and `toolu_` ids stay raw.

`probes/rescrub.rs` applies the current redaction and scrubbing to the committed `p1-*` and `p2-*` files, then fails on any line that is not JSON or that still holds a forbidden string:

```bash
cargo test --test plan_session_bridge_e2e probes::rescrub::rescrub_existing_fixtures -- --ignored --exact
```

## Fixture index

| Test | Prefix | What it does |
| --- | --- | --- |
| `p1::p1_background_agent_and_idle_notification` | `p1-background-agent` | A background Agent; its notification arrives while idle |
| `p1::p1_background_bash_and_busy_notification` | `p1-background-bash` | A background Bash plus an Agent sent with `run_in_background: "false"` |
| `p1_more::p1_interrupt_with_running_tasks` | `p1-interrupt` | One Escape while a background Bash, a background Agent and a foreground Bash run |
| `p1_more::p1_send_message_resumes_a_finished_agent` | `p1-resume` | An Agent, then a request to resume it through `SendMessage` |
| `p1_more::p1_compact_while_a_task_runs` | `p1-compact` | `/compact` while a background Bash runs |
| `p1_status::p1_failed_and_stopped_task_statuses` | `p1-status` | A background Bash that exits 7, then a background Bash and a background Agent stopped with `TaskStop` |
| `p2::p2_plugin_personal_and_fork_skills` | `p2-skills` | A plugin skill with args, a personal skill, a `context: fork` skill |
| `p2::p2_inline_shell_and_webfetch_skills` | `p2-shell-webfetch` | A skill with inline shell, a skill with `allowed-tools: WebFetch` |
| `p2_more::p2_cost_usage_and_context` | `p2-usage-context` | `/cost`, `/usage`, `/context` |
| `p2_more::p2_effort_and_model_with_arguments` | `p2-effort-model` | `/effort high`, `/effort turbo`, `/model haiku` with a settings diff |
| `p2_more::p2_dialog_commands` | `p2-dialogs` | `/model`, `/help`, `/status` |
| `p2_more::p2_unlisted_name_and_paste_lengths` | `p2-unlisted-lengths` | An unknown name, a 29-character and a 300-character slash line |
| `p2_more::p2_compact_through_the_queue` | `p2-compact` | `/compact` through the chat queue |
| `side_tests::p2_side_process_initialize_and_commands_changed` | `p2-side-initialize` | The headless side process; a skill file added while it runs |
| `side_tests::p2_side_process_max_turns_and_loom_ui` | `p2-side-variants` | The same with `--max-turns 0`; the `loom-ui` check |

The side-process stdout is in `probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl` and `probe-notes/p2-side-variants.side-max-turns-0.jsonl`.

## P1. Background tasks

### Agent launch result

Every Agent call launches asynchronously. The `tool_result` user record carries:

```json
{"agentId":"<AGENT-1>","canContinueAgent":false,"canReadOutputFile":true,"description":"probe background agent","isAsync":true,"outputFile":"<CLAUDE-TMP>/<BASE>-project/<SESSION-1>/tasks/<AGENT-1>.output","resolvedModel":"claude-sonnet-5-5","status":"async_launched"}
```

The result text begins `Async agent launched successfully.`. The `tool_use` input is `{description, prompt, run_in_background, subagent_type}`, and the tool name is `Agent`. The model sent `run_in_background` as the string `"true"`, the string `"false"`, or left it out; all three gave this same result. There are no foreground agents. The launch result has no `backgroundTaskId`.

Evidence: `probe-notes/p1-background-agent.md#Background Agent launch result`, `probe-notes/p1-background-bash.md#Foreground Agent result`, `probe-notes/p1-resume.md#First Agent run result`.

### Bash launch result

Only a Bash launch with `run_in_background: true` carries a task id:

```json
{"backgroundTaskId":"bmiyho9ji","interrupted":false,"isImage":false,"noOutputExpected":false,"stderr":"","stdout":""}
```

The result text is `Command running in background with ID: <id>. Output is being written to: …/tasks/<id>.output. …`. The same id string is the notification's `<task-id>`, the `background_tasks[].id` of a `type:"shell"` entry, and the `taskId` of a compact-time `task_status` attachment. Shells have no transcript and no meta file.

Evidence: `probe-notes/p1-background-bash.md#Background Bash launch result`, `probe-notes/p1-status.md#Failing Bash launch result`, `probe-notes/p1-status.md#Bash to stop: launch result`.

### Notification carriers and body

A notification arrives twice in the transcript: first as a `{"type":"queue-operation","operation":"enqueue","content":"<task-notification>…"}` record, then as a `type:"user"` record whose `message.content` is the bare XML. The user record has `origin: {"kind":"task-notification","producer":"session-task","runId":"…"}`, `promptSource: "system"` and `turnOrigin: "task_notification"`. The same XML is the `prompt` of a `UserPromptSubmit` spool event.

A shell body has `task-id`, `tool-use-id`, `output-file`, `status` and `summary`:

```text
<task-notification>
<task-id>bmiyho9ji</task-id>
<tool-use-id>toolu_01DfQmxWFkRxjV3MGY5sKeAx</tool-use-id>
<output-file><CLAUDE-TMP>/<BASE>-project/<SESSION-1>/tasks/bmiyho9ji.output</output-file>
<status>completed</status>
<summary>Background command "Run background sleep and echo" completed (exit code 0)</summary>
</task-notification>
```

An agent body adds `note`, `result` and `usage`. The `result` points at the hand-back; the report text is not in the notification. A fork skill's notification has no `tool-use-id` (`transcripts/p2-skills.jsonl`). Agents send two variants, both with `<status>completed</status>`:

```text
<summary>Agent "probe background agent" finished</summary>
<summary>Agent "probe background agent" is waiting on its own background work</summary>
```

The second, interim variant fires when the agent stops while a background task of its own still runs. Its note says the same task id notifies again if the agent resumes.

Evidence: `probe-notes/p1-background-bash.md#Background Bash task notification`, `probe-notes/p1-background-agent.md#Task notification arriving while idle`, `probe-notes/p1-interrupt.md#Interrupt with three tasks running`, `transcripts/p1-interrupt.jsonl` (interim variant).

### Notification status values

| Outcome | `<status>` | `<summary>` |
| --- | --- | --- |
| Agent finished, or stopped waiting on its own work | `completed` | `Agent "<description>" finished` or `… is waiting on its own background work` |
| Shell exited 0 | `completed` | `Background command "<description>" completed (exit code 0)` |
| Shell exited non-zero | `failed` | `Background command "<description>" failed with exit code 7` |
| Shell stopped with `TaskStop` | no notification | — |
| Agent stopped with `TaskStop` | `killed` | `Agent "<description>" was stopped by Claude` |

A shell stopped with `TaskStop` sent no notification within 20 s of the stop. An agent stopped with `TaskStop` sent a `killed` notification, whose `<tool-use-id>` is the spawning Agent call.

Evidence: `probe-notes/p1-status.md#Failed background Bash: task notification`, `#Stopped Bash: task notification after the stop`, `#Stopped Agent: task notification after the stop`, `transcripts/p1-status.jsonl`, `spool/p1-status.jsonl`.

### TaskStop

A plan session lists `TaskStop` among its deferred tools (loaded through `ToolSearch`), and it has no `KillShell`. The call is `{"name":"TaskStop","input":{"task_id":"<id>"}}` for a shell id or an agent id. Its `toolUseResult` carries `task_type` `local_bash` for a shell and `local_agent` for an agent:

```json
{"message":"Successfully stopped task: bx0h460uz (sleep 60; echo LATE)","task_id":"bx0h460uz","task_type":"local_bash","command":"sleep 60; echo LATE"}
{"message":"Successfully stopped task: <AGENT-1> (probe background agent)","task_id":"<AGENT-1>","task_type":"local_agent","command":"probe background agent"}
```

The task drops out of the next `Stop` event's `background_tasks` in both cases (`probe-notes/p1-status.md#Stopped Bash: Stop events after the stop request`, `#Stopped Agent: Stop events after the stop request`).

Evidence: `probe-notes/p1-status.md#Stopped Bash: stop call and result`, `#Stopped Agent: stop call and result`, `transcripts/p1-status.jsonl`, `spool/p1-status.jsonl`. This probe ran on Claude Code 2.1.322 (auto-updated); its records carry that version.

### Notifications and turns

- **Idle.** A notification opens its own turn: the enqueue and dequeue records, the user record, a `UserPromptSubmit`, an assistant reply, then `Stop`.
- **Working.** A notification that arrives while the main agent works is queued. After that turn's `Stop` it is dequeued and becomes a new turn with its own `promptId`. It never joins or interrupts the working turn (`transcripts/p1-interrupt.jsonl`: enqueue 07:20:31.495, `Stop` 07:20:32.465, dequeue 07:20:32.517, `UserPromptSubmit` 07:20:32.530).

Evidence: `transcripts/p1-interrupt.jsonl`, `transcripts/p1-resume.jsonl`, `spool/p1-interrupt.jsonl`, `probe-notes/p1-compact.md#Compact while a background Bash runs`.

### Agent hand-back

A finished agent's report reaches the main session as a separate message before its notification. The transcript has a `queue-operation` enqueue of `<agent-message from="<AGENT-1>">\n[Subagent hand-back] …</agent-message>`, then a user record with `origin: {"kind":"peer","from":"<AGENT-1>","senderTaskId":"<AGENT-1>","name":"general-purpose","handback":true,"handbackReportText":"DONE",…}`. A `UserPromptSubmit` carries the same XML as its `prompt`, and the hand-back opens a turn as a notification does. Order in `p1-background-agent`: hand-back enqueue 07:06:58.724, last agent record 58.726, `SubagentStop` 58.752, notification enqueue 58.759.

Evidence: `transcripts/p1-background-agent.jsonl`, `probe-notes/p1-background-agent.md#Spool SubagentStop entries: 2`.

### Meta file

Every agent meta file has this shape:

```json
{"agentType":"general-purpose","isBuiltIn":true,"launchedWithDefaults":true,"description":"probe background agent","toolUseId":"toolu_01R8syXXM5FyPbtwVW56YbLd","spawnDepth":1,"requestShape":"background","requestNonInteractive":true,"pickedEffortModel":"claude-sonnet-5-5","finished":true}
```

- `toolUseId` is the spawning call's id.
- `requestShape` is `"background"` in every file, whatever the call's flag, so it tells nothing.
- `finished: true` appears once the agent has handed back; a file captured mid-run lacks it.
- A fork skill's meta adds `"name":"probe-fork"`, and its `description` is `/probe-fork`.
- There is no `agentId` and no `taskKind` field.

Evidence: `transcripts/p1-*-agent-1.meta.json`, `transcripts/p2-skills-agent-1.meta.json`.

### SubagentStart

`SubagentStart` fires about 13 ms after the launch result. Its payload keys are `agent_id`, `agent_type`, `cwd`, `hook_event_name`, `prompt_id`, `scratchpad_dir`, `session_id` and `transcript_path`. `agent_type` is present (`general-purpose`). There is no `tool_use_id` and no description. A fork skill also gets a `SubagentStart`.

Evidence: `probe-notes/p1-background-agent.md#Spool SubagentStart entries: 1`, `spool/p2-skills.jsonl`.

### Agent end records

`stop_reason: "end_turn"` is never written. Every assistant record of the four agent transcripts has `stop_reason: "tool_use"`. The last assistant record is a `SubagentHandback` `tool_use`, and a finished agent's last user record is its result:

```json
{"type":"user","toolEndsTurn":true,"toolUseResult":{"success":true,"message":"Report delivered to your caller."},"message":{"role":"user","content":[{"type":"tool_result","content":[{"type":"text","text":"{\"success\":true,\"message\":\"Report delivered to your caller.\"}"}]}]}}
```

A `prompt_snapshot` attachment may follow it (`transcripts/p1-resume-agent-1.jsonl`).

Evidence: `transcripts/p1-background-agent-agent-1.jsonl`, `transcripts/p1-interrupt-agent-1.jsonl`, `transcripts/p1-resume-agent-1.jsonl`.

### SubagentStop

- It fires for a background agent within about 30 ms of its last record, with `agent_id`, `agent_type`, `agent_transcript_path`, `background_tasks` and `stop_hook_active`.
- It also fires for internal forks that never had a `SubagentStart`, with `agent_type: ""`. Their `last_assistant_message` is a guessed next prompt or a compaction summary, so they are the prompt-suggestion and summariser agents. 6 of the 10 P1 `SubagentStop` events are of this kind, and P2 shows more.
- In `p1-interrupt`, the agent's `SubagentStop` fired while its own background shell still ran. The interim notification followed.

Evidence: `probe-notes/p1-background-agent.md#Spool SubagentStop entries: 2`, `probe-notes/p1-interrupt.md#Spool SubagentStop entries: 3`, `probe-notes/p1-compact.md#Compact while a background Bash runs`.

### background_tasks on Stop and SubagentStop

Every `Stop` and `SubagentStop` payload carries `background_tasks`; no other event does. The entries look like this:

```json
{"agent_type":"general-purpose","description":"probe background agent","id":"<AGENT-1>","status":"running","type":"subagent"}
{"command":"sleep 45","description":"Run sleep in background","id":"bhupkzrs5","status":"running","type":"shell"}
```

- `status` is always `running`. A finished or stopped task drops out of the list.
- The list is session-wide. It includes shells launched inside a subagent (`bhupkzrs5` in `p1-interrupt`), with no field naming the owner.
- Entries carry no timestamp and no `tool_use_id`.
- A stopping agent's own `SubagentStop` still lists it as running, so `SubagentStop` lists lag. The next `Stop` is the first list without it.
- Every subagent entry seen had a `SubagentStart` first. The shells launched inside subagents are the only entries with no launch record in the main transcript.
- The transcript's `turn_duration` records carry `pendingBackgroundAgentCount` for agents only, and it can disagree with the `Stop` list (`transcripts/p1-resume.jsonl`).

Timelines:

```text
p1-background-agent  Stop 07:06:34.604          [agent]
                     SubagentStop 07:06:58.752  [agent]         (the stopping agent lists itself)
                     Stop 07:07:00.283          []
p1-background-bash   Stop 06:37:40.648          [agent, shell bmiyho9ji]
                     Stop 06:37:45.834          [agent]         (shell notified at 41.7)
p1-interrupt         Stop 07:20:32.465          [shell bsngt0yiw, shell bhupkzrs5]   (agent gone)
                     Stop 07:20:33.890          [same two shells]
p1-status            Stop (spool line 11)       [shell bx0h460uz]
                     Stop (spool line 14)       []              (after the TaskStop turn)
                     Stop (spool line 18)       [agent]
                     Stop (spool line 21)       []              (after the agent's TaskStop)
```

Evidence: `spool/p1-*.jsonl`, `probe-notes/p1-interrupt.md#Spool SubagentStop entries: 3`, `probe-notes/p1-status.md#Stopped Bash: Stop events before the stop request`.

### Interrupt

One Escape at 07:20:25.054, with a background Bash, a background Agent and a foreground Bash running.

- The foreground Bash result is `is_error: true` with the text `The user doesn't want to proceed with this tool use. …`, `toolUseResult: "User rejected tool use"` and `toolDenialKind: "user-rejected"`. A text record `[Request interrupted by user for tool use]` follows 1 ms later.
- The background agent survived: it launched its own background shell after the interrupt, handed back, and notified.
- Both background shells survived: two `sleep 45` processes 15 s later, and two `Stop` lists that name both as running.

An interrupt therefore ends no background task.

Evidence: `probe-notes/p1-interrupt.md#Interrupt with three tasks running`, `spool/p1-interrupt.jsonl`.

### Resume and SendMessage

A plan session has no `SendMessage` tool. The model made no such call when asked, the deferred-tool list (`transcripts/p1-status.jsonl`, line 12) does not name it, the launch result says `canContinueAgent: false`, and in these probes the spool has one `SubagentStart` per agent. No agent is resumed by the main agent. An agent can still notify twice for one task id (interim, then finished) without a resume. A field session (product-media-kit, 2026-11-10) shows a background coordinator agent woken again by its own children's notifications: Claude Code fired a `SubagentStart` and a `SubagentStop` with the same `agent_id` for each of its nine wakes, so one agent id can start many times.

Evidence: `probe-notes/p1-resume.md#SendMessage unavailable`.

### Compact while a task runs

`/compact` gives `PreCompact` (`trigger: "manual"`), two `SubagentStop` events with `agent_type: ""`, then `SessionStart` with `source: "compact"`. The transcript gets a `system` `compact_boundary` record, then a `task_status` attachment for the running shell:

```json
{"type":"task_status","taskId":"bymo0084r","taskType":"local_bash","description":"Sleep then echo done","status":"running","deltaSummary":null,"outputFilePath":"…/tasks/bymo0084r.output","shell":{"command":"sleep 30; echo BASH-DONE","kind":"bash","toolUseId":"toolu_01UgqyxR8pp6nHs8fG8stkjh"}}
```

The task survived the compact; its notification later opened a turn.

Evidence: `probe-notes/p1-compact.md#Compact while a background Bash runs`, `spool/p1-compact.jsonl`, `transcripts/p1-compact.jsonl`.

### Toolset

The plan session has `TaskStop` and no `SendMessage` or `KillShell`. The Bash tool refuses a foreground standalone `sleep` (`<tool_use_error>Blocked: standalone sleep 45. … use run_in_background`, `transcripts/p1-interrupt-agent-1.jsonl`).

## P2. Slash commands and skills

### Per-paste matrix

UPS is `UserPromptSubmit` in the spool, UPE is `UserPromptExpansion` in the recording hooks.

| Paste | UPS | UPE | Command record | Turn | Local output | Dialog |
| --- | --- | --- | --- | --- | --- | --- |
| `/loom-plan:args alpha beta` | yes | yes (`plugin`) | user, prompt-type shape | yes | none | no |
| `/probe-personal hello` | yes | yes (`userSettings`) | user, prompt-type shape | yes | none | no |
| `/probe-fork` | yes | yes | plain user record `/probe-fork` | no `Stop`; the result is a notification turn | system `local_command` | no |
| `/probe-shell`, `/probe-webfetch` | yes | yes | user, prompt-type shape, no args element | yes | none | no |
| `/cost`, `/usage` | no | no | none | no | none | yes, usage dialog |
| `/context` | no | no | system `local_command` | no | yes | no |
| `/compact` | no | no | user, local shape | no UPS, no `Stop`; `PreCompact`, then `SessionStart` `compact` | user `<local-command-stdout>` | no |
| `/effort high`, `/effort turbo` | no | no | user, local shape | no | user `<local-command-stdout>` | no |
| `/model haiku` | no | no | none until Escape | no | after Escape only | yes, confirm |
| `/model`, `/help`, `/status` | no | no | none (`/model`: after Escape) | no | `/model` after Escape | yes |
| `/probe-unlisted-zz` | no | no | none | no | system `informational` | no |
| 29- and 300-character `/probe-personal …` | yes | yes | user, prompt-type shape | yes | none | no |

Evidence: `probe-notes/p2-skills.md`, `probe-notes/p2-shell-webfetch.md`, `probe-notes/p2-usage-context.md`, `probe-notes/p2-compact.md`, `probe-notes/p2-effort-model.md`, `probe-notes/p2-dialogs.md`, `probe-notes/p2-unlisted-lengths.md`, one `#Paste: <text>` heading per row.

### UserPromptSubmit and UserPromptExpansion

- `UserPromptSubmit` fires for every prompt-type command, the fork skill included, about 0.17 s after the paste (UPS `received_at` 06:39:54.963, command record 06:39:55.120). Its `prompt` is the raw typed line; the expansion is absent.
- It never fires for a local command, a dialog command, `/compact` or an unknown name.
- `UserPromptExpansion` fires for the same prompt-type pastes and no others, after the UPS hooks. Its keys are `command_args`, `command_name`, `command_source`, `cwd`, `expansion_type`, `hook_event_name`, `permission_mode`, `prompt`, `prompt_id`, `scratchpad_dir`, `session_id` and `transcript_path`. `command_name` is the name as typed, with no slash. `command_args` is `""` when there are none. `command_source` was `plugin` or `userSettings`. Its `prompt_id` equals the UPS `prompt_id`.
- `PostModelSwitch` never fired.

Evidence: `spool/p2-skills.jsonl`, `spool/p2-skills.extra.jsonl`, `spool/p2-shell-webfetch.extra.jsonl`, `spool/p2-unlisted-lengths.extra.jsonl`.

### Record shapes per class

Prompt-type command: a user record with `origin.kind: "human"`, no indentation, the args element left out when empty:

```json
{"type":"user","message":{"role":"user","content":"<command-message>loom-plan:args</command-message>\n<command-name>/loom-plan:args</command-name>\n<command-args>alpha beta</command-args>"},"origin":{"kind":"human"}}
```

An `isMeta` user record with the expanded skill body follows (`Base directory for this skill: …`), then a `command_permissions` attachment (`allowedTools`).

Local command recorded as user records (`/effort`, `/compact`): a `<local-command-caveat>` `isMeta` record, then the command record with no `origin`, the order name, message, args, 12 spaces of indentation after each newline, and the args element always present. The stdout record follows:

```json
{"type":"user","message":{"role":"user","content":"<command-name>/effort</command-name>\n            <command-message>effort</command-message>\n            <command-args>high</command-args>"}}
{"type":"user","message":{"role":"user","content":"<local-command-stdout>Set effort level to high (saved as your default for new sessions): Comprehensive implementation with extensive testing and documentation</local-command-stdout>"}}
```

Local command recorded as system records (`/context`, `/model` after Escape): the same indented content in a `system` `local_command` record, then a stdout record with `commandRun` and `isDisplayOnly`. The stdout holds ANSI escapes. `/context` adds an `isMeta` user record with a markdown copy (`## Context Usage …`):

```json
{"type":"system","subtype":"local_command","level":"info","content":"<command-name>/context</command-name>\n            <command-message>context</command-message>\n            <command-args></command-args>"}
{"type":"system","subtype":"local_command","level":"info","isDisplayOnly":true,"commandRun":{"command":"context","args":""},"content":"<local-command-stdout> \u001b[1mContext Usage\u001b[22m\n…</local-command-stdout>"}
```

Fork skill: no `<command-name>` record. A caveat record, a plain user record `/probe-fork` with `origin.kind: "human"`, then:

```json
{"type":"system","subtype":"local_command","level":"info","content":"<local-command-stdout>Running in the background as @probe-fork</local-command-stdout>\n<forked-skill-launch>{\"agentId\":\"<AGENT-1>\",\"skillName\":\"probe-fork\",\"description\":\"/probe-fork\"}</forked-skill-launch>"}
```

`/compact`: a `compact_boundary` record, the continuation summary, the caveat, the indented `<command-name>/compact</command-name>` user record, and `<local-command-stdout>` with ANSI dim codes around `Compacted (ctrl+o to see full summary)`.

Unknown name: `{"type":"system","subtype":"informational","level":"warning","content":"Unknown command: /probe-unlisted-zz"}`.

No fixture holds `<local-command-stderr>`, tag-escaped args (`&lt;`) or a `***` args form.

Evidence: `transcripts/p2-skills.jsonl`, `transcripts/p2-shell-webfetch.jsonl`, `transcripts/p2-effort-model.jsonl`, `transcripts/p2-usage-context.jsonl`, `transcripts/p2-dialogs.jsonl`, `transcripts/p2-compact.jsonl`, `transcripts/p2-unlisted-lengths.jsonl`.

### Timing

| Command | First evidence after the paste |
| --- | --- |
| Prompt-type skill | UPS at about 0.17 s |
| `/context` | result in the pane at 0.72 s; first record within 0.83 s |
| `/compact` | `PreCompact` at once; first transcript record at 9.94 s (`compactMetadata.durationMs` 9568) |
| `/usage`, `/cost` | the dialog by 0.19 s and 0.23 s; no record within 30 s |
| `/model haiku` | the confirm dialog; no record within 15 s |

Evidence: `probe-notes/p2-usage-context.md#Paste: /context`, `probe-notes/p2-compact.md#Paste: /compact`, `spool/p2-compact.jsonl`, `probe-notes/p2-usage-context.md#Paste: /usage`.

### Paste length and wrapping

Lines of 21, 26, 29 and 300 characters dispatched unwrapped. For the 300-character line the UPS `prompt` holds all 300 characters, `command_args` holds all 57 words, and the skill echoed them. No `<pasted_content>` wrapper appears anywhere. The pane wraps long lines visually only. Exactly 20 characters and lengths above 300 were not pasted.

Evidence: `probe-notes/p2-unlisted-lengths.md#Paste: /probe-personal w001 w002 w003 w004 w005 w006 w007 w008 w009`, `probe-notes/p2-unlisted-lengths.md#The 300-character line's arguments`.

### Suggestion menu at Enter

The menu is closed when the line has arguments. It is open for a bare name, with `❯` on the first row. In all ten bare-name pastes the first row was the typed name, or its canonical name for an alias (`/usage (cost)`), and Enter ran the typed command. For an unknown name the pane shows `No commands match "/probe-unlisted-zz"`, and Enter still dispatched it. `paste-buffer -p` plus Enter worked in all 18 pastes. A typed name that is a prefix of another name, with the exact name not first, was not observed.

### /effort and /model

- `/effort high` prints `Set effort level to high (saved as your default for new sessions): …`, so it writes the user default. `/effort turbo` prints `Invalid argument: turbo. Valid options are: low, medium, high, xhigh, max, auto, ultracode [on|off]`. The headless `argumentHint` matches that list.
- `/model haiku` opens a `Switch model?` confirm dialog. Escape leaves ``Kept model as `Sonnet 5.5` ``. `PostModelSwitch` never fired. The settings diff around it is empty, but the switch was cancelled, so the diff proves nothing.
- Bare `/model` opens the `Select model` picker, which says `Your pick becomes the default for new sessions`.
- `Stop` and `SubagentStop` payloads carry `effort: {"level": "low"}`.

Evidence: `probe-notes/p2-effort-model.md#Paste: /effort high`, `probe-notes/p2-effort-model.md#Paste: /effort turbo`, `probe-notes/p2-effort-model.md#Paste: /model haiku`, `probe-notes/p2-effort-model.md#Copied ~/.claude/settings.json around /model haiku`, `probe-notes/p2-dialogs.md#Paste: /model`.

### Dialogs

`/usage`, `/cost`, `/model`, `/model <name>`, `/help` and `/status` open dialogs that never close by themselves. While one is open the pane has no prompt, and the bridge item stays `Sent`. `/help` and `/status` leave no record even after Escape. The bridge queue holds later items behind an unconfirmed paste.

Evidence: `probe-notes/p2-dialogs.md`, `probe-notes/p2-usage-context.md#Paste: /cost`.

### Unknown name

An unknown name is handled locally: the `informational` warning record and `● Unknown command: /probe-unlisted-zz` in the pane. There is no UPS, no assistant record and no model request. The typed text is not echoed as a user line.

Evidence: `probe-notes/p2-unlisted-lengths.md#Paste: /probe-unlisted-zz`.

### Fork skill

Spool order: UPS `/probe-fork`, `SubagentStart`, the fork's `SubagentStop` (its own list still names it running), a UPS whose prompt is the `<task-notification>`, `Stop`. The `/probe-fork` prompt itself never gets a `Stop`. The notification's `<summary>` is `Agent "/probe-fork" finished`, and its `<result>` holds the fork's output. The pane returns to the prompt at once (`⎿  Running in the background as @probe-fork`).

Evidence: `probe-notes/p2-skills.md#Paste: /probe-fork`, `probe-notes/p2-skills.md#Fork skill: the background result`, `transcripts/p2-skills-agent-1.meta.json`.

### allowed-tools

The `allowed-tools: WebFetch` skill's turn carries a `command_permissions` attachment with `allowedTools: ["WebFetch"]`. The model delegated the fetch to a built-in `web-fetch` subagent, whose WebFetch raised a `PermissionRequest`. Auto mode's classifier accepted it (`permissionDecision` `{"decision":"accept","source":"config","reasonType":"classifier"}`). A WebFetch made by the main agent under the skill's `allowed-tools` was not observed.

Evidence: `probe-notes/p2-shell-webfetch.md#Paste: /probe-webfetch`, `spool/p2-shell-webfetch.jsonl`.

### Inline skill shell

The harness does not run a skill's inline shell at expansion. The expansion record carries `promptShellHandOff` with the commands, and the model runs them as ordinary sandboxed `Bash` calls. The command inside the project succeeded. The one outside it failed with `Read-only file system` (`sandboxed: true`), and the file does not exist.

Evidence: `probe-notes/p2-shell-webfetch.md#Paste: /probe-shell`, `probe-notes/p2-shell-webfetch.md#Inline skill shell inside the sandbox`.

### Side process

The argv is the session's, minus `--session-id`, `--name` and `--append-system-prompt-file`, with `--settings <sibling>.json` (no `hooks`, `"disableAllHooks": true`) and `-p --input-format stream-json --output-format stream-json --verbose --no-session-persistence`. Stdin gets one `{"type":"control_request","request_id":"probe-init-1","request":{"subtype":"initialize"}}` and no user message.

- **`initialize`.** It answers in 383 ms, as a `control_response` holding `commands`, `agents`, `models`, `account`, `claude_code_version`, `session_state: "idle"` and more. `account` holds the e-mail and organization.
- **`system/init`.** It never arrives without a turn. The probe waited 5 s after the answer.
- **`--max-turns 0`.** It changes nothing observable: same rows, same timing, no `system/init`, and the process keeps running.
- **`commands_changed`.** Adding `~/.claude/skills/probe-added/SKILL.md` gives a `system` `commands_changed` line about 3 s after the write. It holds the whole list, 118 rows against 117.
- **No model request.** There is no assistant line, no `result` line and no stderr.
- **Writes.** The live spool stays byte-identical in both runs. The live transcript stays identical in the `--max-turns 0` run. It changed in the run that added a skill, and the changed file is the live session's own transcript. The writer is not established. The live session also sees the new skill, and every p2 transcript ends with metadata records the live session rewrites.

Evidence: `probe-notes/p2-side-initialize.md#Side process: loom-ui-disabled`, `probe-notes/p2-side-initialize.md#Control request shape`, `probe-notes/p2-side-variants.md#Side process: max-turns-0`, the two `.side-*.jsonl` files.

### Row shape and source markers

Rows are `{name, description, argumentHint, aliases?, builtin?}`. `argumentHint` is on every row; `aliases` is on 29; `builtin: true` is on 55, and the key is absent on the rest. No row carries a source or a command type.

```json
{"name":"clear","description":"Start a new session with empty context; previous session stays on disk (resumable with /resume)","argumentHint":"[name]","aliases":["reset","new"],"builtin":true}
{"name":"loom-plan:args","description":"(loom-plan) Echo the arguments back","argumentHint":"","aliases":["args"]}
{"name":"probe-personal","description":"Probe skill probe-personal (user)","argumentHint":""}
```

The source is readable from the description:

| Marker | Source |
| --- | --- |
| `builtin: true` | a native command or a bundled skill |
| prefix `(<plugin>)`, name `plugin:name` | a plugin skill or command |
| suffix `(user)` | a personal skill or `~/.claude/commands` file |
| suffix `(claude.ai sync)` | a claude.ai account skill; bare name, alias `anthropic-skills:<name>` |
| suffix `(dynamic workflow)` | the bundled `deep-research` |
| none of these | not observed; project skills were not installed |

Headless descriptions differ from the pane's for `/context`, `/usage` and `/model`.

### Built-in rows

`builtin: true` covers native commands and bundled skills alike. The `initialize` response lists them in two contiguous groups: the bundled skills after the plugin rows (rows 51-68, with `deep-research` at row 33), then the native commands (rows 69-104). The split below is read from that order and from each description.

- **Bundled skills (19):** `deep-research`, `design`, `design-sync`, `dataviz`, `update-config`, `verify`, `debug`, `code-review`, `simplify`, `batch`, `fewer-permission-prompts`, `doctor`, `loop`, `schedule`, `claude-api`, `workflow-authoring`, `run`, `run-skill-generator`, `plugin-authoring`.
- **Native commands in the headless list (36):** `advisor`, `agents`, `auto-mode-setup`, `autocompact`, `clear`, `color`, `compact`, `config`, `output-style`, `context`, `effort`, `fast`, `focus`, `heapdump`, `init`, `mcp`, `import`, `model`, `__remote-workflow`, `workflow-launch-exec`, `reload-plugins`, `reload-skills`, `rename`, `ultrareview`, `security-review`, `usage-credits`, `extra-usage`, `usage`, `insights`, `recap`, `skill-doctor`, `goal`, `design-consent`, `design-revoke`, `list-agents`, `team-onboarding`.
- **Native commands missing from the headless list** (absent as name and alias; seen in the pane or the bundle): `help`, `status`, `hooks`, `theme`, `login`, `logout`, `resume`, `memory`, `ide`, `cd`, `branch`, `fork`, `background`, `add-dir`, `permissions`, `plugin`, `exit`, `rewind`, `restart`, `remote-control`, `statusline`.

Evidence: `probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl`, `probe-notes/p2-usage-context.md`, `probe-notes/p2-dialogs.md`.

### loom-ui

`loom-ui` is not installed in the probe HOME, so the enabled sub-check was skipped. With `enabledPlugins: {"loom-ui@skills-dir": false}`, `/loom` and `/loom-agents` are absent, which shows only that an uninstalled plugin adds no rows. Whether an installed and enabled `loom-ui` lists `/loom-agents` is not observed.

Evidence: `probe-notes/p2-side-variants.md#loom-ui enabled`, `probe-notes/p2-side-initialize.md#Side process: loom-ui-disabled`.

## Defects in the raw notes

- `probe-notes/p1-compact.md#Compact while a background Bash runs` says "notification opened a turn: false". Its own transcript and spool show the turn opened (`turnOrigin: "task_notification"`, an assistant reply, `turn_duration`).
- `probe-notes/p2-shell-webfetch.md` reports `a record shows OUTSIDE-RAN: true`. That matches the command text in the skill's prompt; no tool result holds it.

## Not observed

- A status string beyond `completed`, `failed` and `killed`.
- `SessionEnd`, and `SessionStart` with source `resume` or `clear`.
- A resumed agent.
- Whether a shell launched inside a subagent notifies the main session.
- The fate of the foreground `tail -f` process after the interrupt.
- `/stats`, `/effort auto`, `/effort` with no argument, and a confirmed `/model` switch.
- The project-skill description marker, and an alias typed in place of a name.
- `/loom-agents` with `loom-ui` installed and enabled.
