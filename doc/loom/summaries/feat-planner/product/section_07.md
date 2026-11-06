---
kind: document
of: doc/product/feat-planner/section_07.md
file: sha256:6a85b94e159d8fbd0e9c5fa1c568875ad301f85f45b51db3dd52935213249cda
cites:
  "doc/product/feat-planner/section_07.md#crashes-and-kills": "sha256:3bd5cc733b6ec4cbeb998814edb024e891b42f4b98b928bb2579e234c0ff2413"
  "doc/product/feat-planner/section_07.md#delivering-input": "sha256:afc256f63b7d39a3fce86ef4c3f4573a3d063af02ffec46900dce2dae1212135"
  "doc/product/feat-planner/section_07.md#hooks-and-the-event-spool": "sha256:2b6451031c3862cebb4b249c0259c0e269f4a189e23722e999463c52f5b20ed3"
  "doc/product/feat-planner/section_07.md#kickoff": "sha256:1e93aba3df74e3463a07c20642145f928c1d609962db69ed33123d613b359071"
  "doc/product/feat-planner/section_07.md#permission-prompts": "sha256:10426bcc5e63c72c1e3be48645180eb18c4a2841d473d12309e5ead0cddfd1c7"
  "doc/product/feat-planner/section_07.md#prerequisites": "sha256:b0ea5ecbff321bdf33795043c4195190f9ef6c6d472c57bc1cd2d585e7728422"
  "doc/product/feat-planner/section_07.md#reattaching-and-resume": "sha256:0ca1d3ce0b7d870bee3081baaf567a0b0bc8e553a49add8947b7fd0aaaa83426"
  "doc/product/feat-planner/section_07.md#session-kinds": "sha256:65d474d6170b122a05cc3165fbb0f8ae574f179e137766a122b9965bb6d561af"
  "doc/product/feat-planner/section_07.md#session-state": "sha256:65fde987e26f35c395d22d1779623bd48a85ba2608be54a08e50283c09146f84"
  "doc/product/feat-planner/section_07.md#starting-a-session": "sha256:578e9a41668c9d321e9b3061c01c774abb8f48350e08b7396d5661ad2fe2004d"
  "doc/product/feat-planner/section_07.md#the-transcript": "sha256:abf0b9eab1e859c7929a1c94897e7fb89df3fcfe17da4a7b067b86bef96721e0"
---

# Session Bridge

## Kinds and scope

Four session kinds exist: authoring, summary, refinement and pressure, and every authoring session carries an Open, Kickoff or Team scope kept in `scope.json`. [Session kinds](../../../../product/feat-planner/section_07.md#session-kinds)

A refinement subagent is no separate session, and the bridge reads its transcript the same way as the main one. [Session kinds](../../../../product/feat-planner/section_07.md#session-kinds)

One summary session runs at a time, so a rebuild after a git pull does not compete with the authoring session for the user's subscription limits. [Session kinds](../../../../product/feat-planner/section_07.md#session-kinds)

## Starting a session

At start the server checks for `tmux` and `claude` on `PATH` and runs the login probe, and any failure puts the server in read-only mode with the missing piece named. [Prerequisites](../../../../product/feat-planner/section_07.md#prerequisites)

Each session runs in its own tmux server, and `launch.sh` sets the environment and execs Claude Code, so the pane's process is `claude` itself. [Starting a session](../../../../product/feat-planner/section_07.md#starting-a-session)

The launch line carries no prompt, because a command line is readable by every local user through `/proc`, so the server delivers the opening message as the session's first turn after its first `SessionStart`, except for a pressure session's slash command and plan path, which travel on the launch line. [Starting a session](../../../../product/feat-planner/section_07.md#starting-a-session)

Only the holder may use "Confirm name", which arms a one-time grant to create exactly one target that did not exist at session start. [Kickoff](../../../../product/feat-planner/section_07.md#kickoff)

## Events and transcript

The hook command appends one JSON line per event to `events.jsonl` under an exclusive lock and exits silently on a mismatched event name, and a spool error exits 0 with no output except for `PermissionRequest`, which answers deny. [Hooks and the event spool](../../../../product/feat-planner/section_07.md#hooks-and-the-event-spool)

The server reads the transcript in complete lines and keys every record by its `uuid`, so rereading never duplicates an item. [The transcript](../../../../product/feat-planner/section_07.md#the-transcript)

## State and input

A hook event never moves the state alone when the transcript contradicts it, and only main-agent events move the state. [Session state](../../../../product/feat-planner/section_07.md#session-state)

The server delivers input only while the session is Idle, queues any message sent during a turn, and caps delivered text at 100 KB. [Delivering input](../../../../product/feat-planner/section_07.md#delivering-input)

## Permissions, reattach, and crashes

Permission decisions go through the `PermissionRequest` hook, which polls for the server's answer file, and the bridge never answers a dialog by keystrokes. [Permission prompts](../../../../product/feat-planner/section_07.md#permission-prompts)

Sessions start cold and the bridge never uses `claude --resume`, so reattaching means a browser rejoining a session still running in the same server. [Reattaching and resume](../../../../product/feat-planner/section_07.md#reattaching-and-resume)

When the server dies its sessions keep running, and the next `loom plan` on the project delivers the closing message to each authoring session whose process identity verifies as alive, waits up to 10 minutes, and tears it down before accepting any new chat, while a pressure session is torn down at once and unverifiable records are left alone with their socket names printed. [Crashes and kills](../../../../product/feat-planner/section_07.md#crashes-and-kills)
