---
kind: document
of: doc/product/section_07.md
file: sha256:732ddb3ff18e83aebc302e90e423d04e7a325b060a0e0f7dcdeebc19c561b08c
cites:
  "doc/product/section_07.md#crashes-and-kills": "sha256:60248250b94afb845e8494e177e72faeb319b50a088f80b301369b4eddf24897"
  "doc/product/section_07.md#delivering-input": "sha256:afc256f63b7d39a3fce86ef4c3f4573a3d063af02ffec46900dce2dae1212135"
  "doc/product/section_07.md#hooks-and-the-event-spool": "sha256:285c902d070f8d150d38fff04adcf92999831f6c367cf0afa060b1da2cb19609"
  "doc/product/section_07.md#interrupt-and-end": "sha256:60a84e9913bfad5b44073d64658a5540b2b3f9d40df72689263214bf18d12069"
  "doc/product/section_07.md#permission-prompts": "sha256:1539528d483127c28540a25f89ace2239e2be8429a1fc2ba16b37a6a545eb6a1"
  "doc/product/section_07.md#prerequisites": "sha256:b0ea5ecbff321bdf33795043c4195190f9ef6c6d472c57bc1cd2d585e7728422"
  "doc/product/section_07.md#reattaching-and-resume": "sha256:0ca1d3ce0b7d870bee3081baaf567a0b0bc8e553a49add8947b7fd0aaaa83426"
  "doc/product/section_07.md#session-kinds": "sha256:bfdaf82a19a3e95fb64a0c57b66a00e344c09883b51b8bd7b650f01bff446457"
  "doc/product/section_07.md#session-state": "sha256:65fde987e26f35c395d22d1779623bd48a85ba2608be54a08e50283c09146f84"
  "doc/product/section_07.md#starting-a-session": "sha256:4846492786062e9fbcd526e0f84042188eef3412f83b4494f238d9d2bebe37a9"
  "doc/product/section_07.md#the-transcript": "sha256:abf0b9eab1e859c7929a1c94897e7fb89df3fcfe17da4a7b067b86bef96721e0"
  "doc/product/section_07.md#what-the-chat-receives": "sha256:8d563a3f13fff02679d9434088ee551f52cf19472d77c96d3da5585adad6251e"
---

# Session Bridge

## Session kinds

The bridge drives three kinds of session, authoring, summary, and pressure, and refinement subagents run inside the authoring session. [Session kinds](../../../product/section_07.md#session-kinds)

## Starting a session

Each session gets a fresh UUID that is also its Claude Code session id, and `launch.sh` execs Claude Code so the pane's process is `claude` itself. [Starting a session](../../../product/section_07.md#starting-a-session)

The launch line carries no prompt because a command line is readable by every local user through `/proc/PID/cmdline`, and an opening message can quote documents. [Starting a session](../../../product/section_07.md#starting-a-session)

## Prerequisites

At start the server checks for `tmux` and `claude` on `PATH` and runs `claude auth status --json`, and any failure puts the server in read-only mode. [Prerequisites](../../../product/section_07.md#prerequisites)

## Hooks and the event spool

Hook commands never fail the session, and each one appends a JSON line to `events.jsonl` under an exclusive file lock. [Hooks and the event spool](../../../product/section_07.md#hooks-and-the-event-spool)

## The transcript

The server tails the transcript, reads complete lines only, and keys every record by its `uuid` so rereading never duplicates an item. [The transcript](../../../product/section_07.md#the-transcript)

Thinking blocks carry a signature and no text, so the chat shows that the session thought and for how long, never what. [The transcript](../../../product/section_07.md#the-transcript)

## Session state

Only the main agent's events move the state, and a hook event never moves it alone when the transcript contradicts it. [Session state](../../../product/section_07.md#session-state)

## Delivering input

The server delivers a message only while the session is Idle, and a message sent mid-turn is queued and delivered at the next Idle. [Delivering input](../../../product/section_07.md#delivering-input)

## Permission prompts

Permission decisions go through the `PermissionRequest` hook, and the bridge never answers a dialog by keystrokes. [Permission prompts](../../../product/section_07.md#permission-prompts)

## Interrupt and end

Stop sends at most one Escape per turn, because a second Escape at an idle prompt opens Claude Code's rewind picker. [Interrupt and end](../../../product/section_07.md#interrupt-and-end)

## Reattaching and resume

Sessions start cold, and the bridge never uses `claude --resume`, so resume means a browser reattaching to a session still running in the same server. [Reattaching and resume](../../../product/section_07.md#reattaching-and-resume)

The model is rebuilt from the transcript, which is the source of truth, and the server keeps it in memory only as a cache. [Reattaching and resume](../../../product/section_07.md#reattaching-and-resume)

## Crashes and kills

If the `loom plan` server dies, its sessions keep running, and the next server closes each one whose process identity verifies as alive. [Crashes and kills](../../../product/section_07.md#crashes-and-kills)

## What the chat receives

The server sends each session to the browser as one model, whole on connect and as deltas after. [What the chat receives](../../../product/section_07.md#what-the-chat-receives)
