# chat-ending: shared context

Read this before your own brief. The plan's Decisions D6 and D7 are authoritative
(`doc/plans/PLAN-planner-feedback-fixes.md`, renamed `IN_PROGRESS-` once the run starts). The
operator set D6.

## The two ways a chat ends

| Who ends it | How | Closing write | What the web shows |
| --- | --- | --- | --- |
| The agent, when it judges the work done | Its final reply ends with a `loom-done` fence | None: the agent already wrote | The left-behind card, from the fence |
| The user: New chat, End, the planner stopping, the holder leaving | `ChatCommand::End` or the server's own end paths, as today | Yes, as today (`EndMode::FinalWrite`, `Closing::NotSent` when the user wrote) | The wrap-up card, then the left-behind card, as today |

## The done fence (pinned)

The fence is the last block of the turn's last assistant text item, with nothing but whitespace
after it:

````text
```loom-done
{"decisions": ["..."], "questions": ["..."]}
```
````

- The info string is exactly `loom-done`. `loom-done-later` or `loom-done x` does not count.
- The body must parse as a JSON object with arrays `decisions` and `questions` of strings. A body
  that does not parse means no agent end, and the bridge records a warning.
- Only assistant text counts. A user message, a tool result or a quoted fence followed by more text
  never ends a chat.
- The web reads the same body with the same limits as `loom-closing` (`web/src/plan/chat/closing.ts`:
  `MAX_ENTRIES` 50, `MAX_ENTRY_CHARS` 500).

## The agent end path (pinned)

1. At the main Stop that ends such a turn, if the session kind is Authoring, no end is planned yet,
   no user item is queued, and no delivery is pending, the bridge plans an end with `Closing::Skip`
   and the new `EndCause::Agent` (`loom/src/plan_session/bridge/mod.rs` `EndCause`). No closing
   message is pasted.
2. The session closes through the existing close gate (`driver.rs` `on_idle`, which reads the
   background table). With no running background agent, it closes at that Stop. With one running,
   it closes once the table holds no running agent. The driver's tick re-checks this, with no
   further main Stop needed.
3. A queued or delivering user message at the Stop cancels the agent end: nothing is planned, and
   the queue proceeds.
4. The final-write bound and the other fallbacks stay.

## Guidance text (W1 writes it)

`loom/src/planning/guidance/text/common_brief.md` gains a section, `## Ending the chat`, beside
`## The closing message`. It tells the agent:

- When the work the user asked for is finished: the documents are written, `loom plan check` is
  clean, nothing waits on the user, and no background agent it started is running.
- First, write every decision the conversation settled into the documents, and every point still
  open under `## Open questions` in the document it concerns.
- Then end the reply with the `loom-done` fence.
- Never use the fence while asking the user anything.
- `## The closing message` stays for chats the user ends.

## Rules for every worker

- Write only the files your brief owns. No git commands. Never touch any `.loom/` path.
- Codex workers compile only. The orchestrator runs the fake-host bridge tests and vitest after
  you.
- Size limits: file 400 lines, function 50 lines, with a 10% margin.
- No backwards compatibility code. No mention of AI tools in code, comments or docs.
- Frozen contract files (`agent_end_contract_tests.rs`) are the spec. Never edit them.
