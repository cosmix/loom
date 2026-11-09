# Plan: background activity and slash commands in the planner chat

The planner chat falls short of Claude Code in a terminal in two ways:

1. **(A)** The user cannot tell an idle chat from one waiting on background work (subagents, background shell tasks).
2. **(B)** The user cannot see or run the skills and commands the terminal's `/` menu offers.

## A. Background activity

### What exists, and what can be reused

The stage modal on the dashboard already monitors a stage's agents. Its token lanes (`web/src/components/stage-token-lanes.tsx`, `TokenLanes` L150-179) show:

- one lane per agent (main, each subagent, each codex job, the codex job grouped with its forwarder in `CodexCluster`);
- each agent's type, model and short id, and a live or idle dot;
- each agent's token total, tokens per minute and a series of 30 s buckets;
- idle agents folded into "N earlier agents" (`EarlierAgents`).

Transcript content is not shown. The CLI `loom subagents list|harvest|watch|wait` adds a per-agent state classification. Nothing tracks background shell tasks.

The token pipeline behind the lanes is generic except for one function:

- Only discovery is keyed to stages. `claude_sources` (`loom/src/tokens/discover.rs:107-122`) walks loom `Session` records that have a `stage_id`.
- `session_sources` (`discover.rs:127-165`) takes any transcript path and adds its `subagents/agent-*.jsonl` files.
- `Source` (`discover.rs:27-37`) carries the group id, key, kind, path, agent type and forwarder.
- The reader (`tokens/tracker.rs` `read_sources`/`read_source`, L139-200: checkpointed offsets, inode and shrink restarts) works on any `Source`.
- The flow builder (`tokens/flow_build.rs` `agent_flow`/`stage_flow`, `rate`, `series`) does too.
- `AgentFlow` (`tokens/model.rs:71-82`) and `agentFlowSchema` (`web/src/api/schema.ts`) are the wire shape.
- "Active" there means the agent drew tokens in the last 4 buckets, about 2 minutes. That signals activity, not that the agent is still alive.

| Piece | Where | Keyed to | Use |
| --- | --- | --- | --- |
| Token-flow pipeline: per-agent model, totals, rate, series, codex pairing | `tokens/discover.rs` `session_sources`, `tokens/tracker.rs` reader, `tokens/flow_build.rs`, `tokens/model.rs` `AgentFlow` | Stages, in discovery only | **Generalize**: a plan-session discovery feeds the same reader and flow builder, grouped by plan session id instead of `stage_id` |
| Lane logic: idle-row fold, codex pairing, labels | `stage-token-lanes.tsx` `isIdleRow`/`EarlierAgents`/`CodexCluster`, `web/src/lib/tokens.ts` (`agentLabel`, `codexPairs`, `liveCounts`) | `AgentFlow` | **Reuse**: `@/lib/tokens` as is. The lane rendering is adapted to planner styling (see A-D5) |
| Subagent classifier: state Done/Failed/Cancelled/ToolWait/Generating, `idle_secs`, turns, last tool, model, final report | `loom/src/commands/subagents/classify.rs` (~L228 `analyze_with_evidence_at_ceiling`), `classify/entry.rs` | A transcript path. The stage ledgers are `Option` arguments | **Generalize.** Expose it as a library API and call it with `lifecycle: None, forward_index: None` |
| Stall rule (Agent/Task calls exempt, Bash floor 1800 s) | `commands/subagents/wait/stall.rs` | Stage budget | **Generalize** into a pure function that takes a budget |
| Subagent discovery: `subagents/agent-*.jsonl`, `agent-<id>.meta.json` (`toolUseId`, `taskKind`), SubagentStart/Stop hooks, final-record detection | `plan_session/bridge/registry.rs:143-149,307-342`, `bridge/agents.rs:33-65`, `driver.rs:212-225,275-340`, `spool.rs:22-78` | Plan session | **Reuse as is**: already live for every plan session |
| Background shell task id from a Bash `toolUseResult.backgroundTaskId`; validated output path `/tmp/claude-<uid>/<workspace>/<session>/tasks/<id>.output` | `models/forward_receipt/transcript.rs` (~L195), `forward_receipt/locator.rs` | Codex receipts | **Generalize**: lift the extraction and the path check into a shared module |
| Per-agent status pushed as a `PlanMessage` and replayed to new connections by `snapshots()`, held in a web atom, checked by a zod schema | `plan_server/chat/team.rs:130-145`, `wire/team.rs`, `web/src/plan/state/atoms.ts:36`, `api/schema-kickoff.ts:60-80` (the draft-team path) | Draft team | **Reuse the pattern** for a new message. The draft-team content is document-specific, so it stays as is |
| Planner UI primitives: `StateMark`, `ActivityGlyph`, `ActivityTile`, `LiveElapsed`, `Untrusted`, `plural`, `compactTokens`; header `useActivities` popover | `web/src/plan/ui/activity-glyphs.tsx`, `header/use-activities.ts`, `@/lib/tokens` | — | **Reuse** |
| `subagent_lifecycle`, `codex_lifecycle`, `loom subagents watch`, `DraftTeamPanel` | — | Stage env or draft scope | **Do not reuse**: they need the stage env, the hook-written stage journal or the draft scope |

Gaps in the planner today:

- `SessionView.subagents_running` is a bare count, and only Restore reads it (`chat/restore.tsx:40`).
- The count resets on startup, compact and interrupt (`state.rs:98-103,199-215`).
- The `<task-notification>` body is thrown away (`transcript/records.rs:188-192`).
- An Agent or background Bash row reads Done as soon as it launches.
- An idle chat shows nothing (`chat/state-line.tsx:12-24`).

### Decisions

- **A-D1.** `Idle` keeps its meaning: the main agent waits for input. Delivery gating, Stop, the composer gate and the frozen idle contracts depend on it. Background work is a separate model.
- **A-D1b. Two sources, joined by agent id.** The token pipeline gives each agent's model, totals, rate and series, the same numbers the stage modal shows. The bridge table (A-D2) gives lifecycle: running, done, stalled?, and which call spawned the agent. Neither source alone answers "is the chat still waiting on something".
- **A-D1c. Token flow for plan sessions.** A new discovery function builds `Source`s for each live plan session: the main transcript the bridge learned at `SessionStart`, its subagents, and any codex rollouts its forward receipts name. Their group id is the plan session id. The same reader and flow builder produce one `StageFlow`-shaped group per session, renamed to a neutral `AgentGroupFlow` with `group_id` in place of `stage_id`, because the plan session is not a stage. The plan server runs this off the bridge's driver thread, on the tracker's 5 s cadence, with its own checkpoint under the plan runtime dir.
- **A-D2.** The bridge keeps one `BackgroundTasks` table per session. An entry has an id, kind (`agent` | `shell`), label (the spawning call's `description`), `agent_type`, the spawning `tool_use_id`, `started_at` and `last_activity_at`. It also has a state: running / tool-wait / done / failed / stalled?. Subagent entries add turns, last tool and a final-report preview. The table is classified incrementally from the records the bridge already tails, so no transcript is re-read each tick. The classification shares `classify.rs`'s rules, factored so both callers use one implementation.
- **A-D3.** A task ends only on evidence: `SubagentStop`, a parsed `<task-notification>` naming its id, or the subagent's final record. The stall rule marks a task `stalled?` and keeps it in the table; the synthetic `SubagentStop` in `driver.rs:312-329` goes away. Interrupt and `agents_killed` clear the table. A compact `SessionStart` does not.
- **A-D4.** Transport is a new `PlanMessage::Background { session, tasks }`, published on change even between turns, and replayed by `snapshots()`. `ChatSnapshot` and `ChatDelta` stay unchanged. `subagents_running` is removed, and Restore reads the table's running count.
- **A-D5.** The idle sign is a text line, not a live roundel: `StateLine` reads "Waiting on 2 background tasks". Its disclosure lists each task as glyph, state word, label, elapsed time, and for agents the model, token total and rate from A-D1c. Agents whose tasks ended fold into "N earlier agents", following the lanes' `EarlierAgents` logic from `@/lib/tokens`. The rows use the planner's list and glyph styles, not the dashboard's heat strip or cluster box, which break `ui/DESIGN.md` (no box in a box). The frozen hearth contract (no bubble or roundel when idle) holds. The header activity popover gains a `background` kind. Agent and background Bash tool rows show their task's state until it ends, and `Task` rows nest like `Agent` rows.

### Steps

1. **Fixtures.** Capture redacted transcript lines and hook payloads from a real plan session that does three things: a background Agent run, a `run_in_background` Bash, and both task notifications. Pin the field names: launch result agent and task ids, the notification body, and whether the SubagentStart payload carries `agent_type`.
2. **Token flow for plan sessions.** Rename the stage-specific parts of `tokens/` (`stage_id` becomes `group_id`, `StageFlow` becomes `AgentGroupFlow`), keeping the dashboard's behaviour and tests unchanged. Add plan-session discovery per A-D1c. Tests: a fixture plan session with one subagent and one codex rollout yields three agents with models and totals, and the dashboard flow tests stay green.
3. **Shared classifiers.** Move the entry classification and stall rule out of `commands/subagents` into a library module (`provider_data/` or a new `agent_state/`). Move the background-task-id extraction and the output-path check out of `forward_receipt`. The `loom subagents` tests must stay green unchanged.
4. **Bridge.** Parse the notification and the launch result in `records.rs`. Build the `BackgroundTasks` table in `bridge/`, replacing the `subagents` counter in `state.rs`. Change the stall path, and publish on change. Tests driven by the fixtures cover idle with 2 tasks, a notification ending one, SubagentStop ending the other, interrupt clearing the table, compact keeping it, and a stall.
5. **Wire.** `PlanMessage::Background` carries the tasks joined with their `AgentFlow`. Reuse `agentFlowSchema` in the zod schema. Add a shared fixture and a parity test.
6. **Web.** Add an atom, the `StateLine` idle variant, the header kind, tool-row states and `Task` nesting. Update the help scenes and `ui/DESIGN.md`. Tests: idle with no tasks draws nothing; idle with tasks shows the line and the list; a background row goes from running to done.

## B. Slash commands and skills

### Why `/` is refused today

The rationale is one note in `doc/product/feat-planner/section_07.md:292-294`. `/clear` and `/exit` change or end the session outside the bridge, and other commands open terminal dialogs the browser cannot see. Section 09:548 adds `/model`.

The code supports that concern:

- `/clear` writes a new transcript under a new session id. The bridge accepts only `<session id>.jsonl` (`driver.rs:153-168`), so it would go blind.
- A picker left open captures the next pasted Enter.

Nothing else was analysed, and **no pasted slash command has ever been probed**: the e2e probe (`loom/tests/plan_session_bridge_e2e/operator.rs:320-376`) was never reached.

What lifting the block runs into:

- **Delivery confirmation.** Delivery is confirmed on `UserPromptSubmit` or a matching human transcript record (`bridge/flow.rs:240-293`). Real transcripts suggest `UserPromptSubmit` does not fire for slash commands. It is unproven. The command's own user record is wrapped in `<command-name>` tags, which the mapper skips (`records.rs:28-34,193-198`). So a pasted `/skill` likely times out after 10 s as undelivered and pauses the queue while the skill runs, and a resend would run it twice. A local command such as `/cost` would time out and leave no turn.
- **Hidden output.** A local command's output (`<local-command-stdout>` records, or `system` subtype `local_command`) is dropped.
- **Paste wrapping.** Claude Code sometimes records long or multi-line pastes inside a `<pasted_content>` wrapper; a short single-line paste arrived bare. A wrapped `/x` would probably not run as a command.
- **Documented hook.** `UserPromptExpansion` fires when a skill, custom command or MCP prompt expands. It carries `expansion_type`, `command_name`, `command_args` and `command_source`. Claude Code docs: hooks.

### What the terminal menu lists, and how to get that list

The menu is the union of:

- built-in commands (filtered by platform, plan and environment);
- bundled skills (compiled into the binary, so not discoverable from files);
- enterprise, personal and project skills;
- legacy `.claude/commands` files;
- plugin skills and commands as `plugin:name`, from `enabledPlugins` and `--plugin-dir`;
- claude.ai account skills;
- MCP prompts as `/mcp__server__prompt`.

Collision rules apply: enterprise beats personal, personal beats project, and a skill beats a command. `user-invocable: false` hides a skill from the menu, and so does `skillOverrides: "off"`. `disable-model-invocation` does not hide it. The pinned `HOME`, `--setting-sources user,project`, `--plugin-dir`, `--strict-mcp-config` and `--settings` all change the list. See the Claude Code docs: commands, skills, mcp.

The planner leaves MCP prompts out. A plan session starts with `--strict-mcp-config` and no `--mcp-config` (`plan_session/launch/script.rs`), so it loads no MCP servers and its menu never holds an MCP prompt. If the planner ever loads MCP servers, this changes.

No documented call returns the menu of a running interactive session. The documented source is a Claude Code process in stream-json mode:

- the `system/init` message: `slash_commands`, `skills`, `plugins`, `mcp_servers`, `terminal_slash_commands`;
- the SDK's `supportedCommands()`: `{name, description, argumentHint, aliases, builtin}`, kept current by `commands_changed`. The SDK gets this over the CLI's stream-json control protocol.

Commands the docs call terminal-only or dialog-only are partly missing from those lists.

### Decisions

- **B-D1. The list comes from Claude Code itself.** When an authoring session starts, the server runs a side `claude` process in stream-json mode. It uses the session's exact flags, `HOME`, env, cwd, `--plugin-dir`, `--settings` and `--setting-sources`. The server reads the command list (names, descriptions, argument hints, aliases, built-in flag), caches it per session, then kills the process before any model turn. A `/reload-skills` or a skill file change re-runs it. If the probe (step B1) shows the control protocol cannot return descriptions without a turn, the fallback has three parts:
  - names from `system/init`;
  - descriptions read from the files for the file-backed sources, with an extended `loom/src/skills/index.rs`;
  - a checked-in table for built-ins.
- **B-D2. The menu shows everything the terminal shows.** Each entry carries a class:
  - **Run**: prompt-type entries (skills, bundled skills, plugin and custom commands) and local commands that just print output.
  - **Run with handling**: local commands the bridge must follow, such as `/compact`, `/context`, `/cost`/`/usage`, `/help`, `/status`, `/effort <level>`, and `/model <name>` (B-D5).
  - **Terminal only**: listed but disabled, with the reason ("Changes the session outside the planner", "Opens a terminal dialog"). This covers `/clear` and its aliases, `/resume`, `/exit`, `/rewind`, `/branch`, `/fork`, `/background`, `/restart`, `/cd`, `/login`, `/logout`, `/permissions`, `/config`, `/hooks`, `/plugin`, `/mcp`, `/agents`, `/memory`, `/add-dir`, `/ide`, `/theme`, and pickers called with no argument.

  The classes live in one server-side table keyed by built-in name. Every non-built-in entry is Run. `check_send` and the web `messageProblem` both read that table from the server, so they cannot drift. Unknown built-ins default to Terminal only.
- **B-D3. Slash messages are pasted as typed**, so the terminal's behaviour (argument substitution, `context: fork`, `allowed-tools`) is preserved. Delivery is confirmed on a newly spooled `UserPromptExpansion` for prompt-type commands, and on the `<command-name>` user record for every slash command, matched by name and args. The mapper turns that record into the user's chat item, showing the command chip and its args. Slash messages may not carry attachments or more than one line, so the paste stays a short bare line (see "Paste wrapping" above).
- **B-D4. Local output reaches the chat.** `<local-command-stdout>` and `system local_command` records become a notice item under the command. Local commands get a `local` delivery flag, generalized from `Delivering.compact`. It confirms on the command record, and Idle follows on a settle timer, as for `/compact`; the timeout never pauses the queue. `/compact` gets the same timeout fix.
- **B-D5. `/model <name>` and `/effort <level>` are allowed**, in the "run with handling" class. Called with no argument, they open a picker and stay terminal only. The session view shows the current model and effort, read from the command's `<local-command-stdout>` record. Section 09:548 changes from "one session keeps one model" to "the session's model and effort are shown and can change from the chat".
- **B-D6. Composer.** A `/` typed as the first character opens a combobox listing every entry. The textarea is `role="combobox"`, the list is `role="listbox"`, and focus stays in the textarea. The list is anchored with `attachments/floating-position.ts` `useFloatingPlacement`, not Radix Popover, which takes focus. Each entry shows name, source (built-in / bundled / user / project / plugin), description and argument hint; disabled entries show their reason. A "Commands" button in `ComposerActions` opens the same list. Picking an entry inserts `/name ` with the argument hint as placeholder text.
- **B-D7. Agent guidance.** `loom/src/planning/guidance/text/common_brief.md:5` and spec section 09:115 currently say "Never suggest a typed slash command". They change to "Slash commands run as in the terminal, except those the planner marks terminal only".

### Steps

1. **Probe** (prerequisite). This is the pasted-slash probe that has never run. In a real plan session, paste each of these and record the spool events, transcript records and pane state after Enter:
   - a plugin skill with args;
   - a personal skill;
   - `/cost`, `/context`, `/compact`, `/effort high`;
   - `/model` with no argument (to see the picker);
   - a 300-character single-line `/skill`.

   Also check, against the session's flags:
   - whether `claude -p --input-format stream-json --output-format stream-json --verbose` returns the command list through the control `initialize` request before any user message;
   - whether `--max-turns 0` or no user message keeps it free.

   Save the results as fixtures. Every later step reads only fields the fixtures contain.
2. **Command list.** Implement the side process for B-D1, or its fallback. Add a viewer route `GET /api/session/commands`, modelled on `plan_server/guidance/brief_route.rs:20-42`. It returns `{name, aliases, description, argument_hint, source, class, reason}`, with the B-D2 class table, a zod schema and a parity fixture. Tests: plugin, personal and project skills and a built-in all appear; `user-invocable: false` is absent; `/clear` is terminal only.
3. **Gate.** Replace `check_send`'s slash rule (`plan_session/input.rs:13,27-31,58-68`) with the class table. Make the web `message-problem.ts` read the same list. Refuse a slash message with attachments or more than one line. Tests: `/clear` is refused with its reason; `/loom-plan:kickoff x` is accepted; an unknown built-in is refused; `/` followed by an unlisted name is accepted as a prompt, which matches the terminal: Claude reports "Unknown command".
4. **Bridge.** Add `UserPromptExpansion` to `SPOOL_EVENTS` and the hook CLI allowlist (`spool.rs`, `hook_cli.rs:28`). Add the `local` delivery flag, with the settle timer and no pause on timeout. Map the `<command-name>` and local-output records in `records.rs`. Fixture tests per class.
5. **Composer.** Build the combobox and the Commands button from B-D6. Tests: `/` opens the list; filtering; keyboard selection; disabled entries show their reason; Send emits the literal `/name args`.
6. **Guidance and spec.** Apply B-D7. Rewrite section_07:292-294, section_08:340-341 and section_09:548 to the new rules.

## Knowledge (both parts)

Rewrite these to the new truth:

- `architecture/planning-session-bridge.md`: state rule 2, background tasks, slash delivery;
- `architecture/planner-web-shell.md`: idle sign, command menu;
- `architecture/skill-catalog.md`, if the fallback extends the frontmatter reader;
- `architecture/token-flow-sources.md`: plan-session discovery, `group_id`.

## Risks

- Claude Code's record shapes for background tasks and slash commands are undocumented. The fixtures pin them, so a version change shows up as a failing fixture test, never as a silent miscount.
- The side process for the command list costs one extra Claude Code start per session. It has to send no model request; step B1 proves that or the fallback is used.
- Running user and project skills in a planner session: a skill's `allowed-tools` can widen what auto mode permits. The scope guard still binds every file tool (`plan_session/guard`), so a skill gains no write access outside the granted scope.
- The bridge's single driver thread must not block. The side process and the command listing run off the driver.
