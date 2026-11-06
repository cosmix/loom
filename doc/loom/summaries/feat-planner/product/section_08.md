---
kind: document
of: doc/product/feat-planner/section_08.md
file: sha256:797c83f6c237ec4fd29f33894a69fa3e3031a566ff7103ce772952c2ef1966a9
cites:
  "doc/product/feat-planner/section_08.md#attachments": "sha256:58af3527fdf3214c0d3916fa6f27c1cf9419246fd7e520d531527dd4215a897a"
  "doc/product/feat-planner/section_08.md#composing-and-sending": "sha256:c8b8c2d20dc5b0ac736f73ba974048a487fa2ea3f00600d944fff31c5cdfe878"
  "doc/product/feat-planner/section_08.md#interrupt": "sha256:143bb99c6ed7fb5f57efad17340aa0d904060460d1376ee1307a478dd28b37b7"
  "doc/product/feat-planner/section_08.md#layout": "sha256:b81bcbfe5995dffc6ea4c7b1a09399e8f6cf4f7f0ad9d6952782a9aac6e09046"
  "doc/product/feat-planner/section_08.md#message-rendering": "sha256:8fcecef6f1f9af633411cb142aeed0ed0dcc2a3fbe68016b0bc1ceb60484189b"
  "doc/product/feat-planner/section_08.md#permission-prompts": "sha256:11a0caf008c5809c7a6493cc619f049afff92de57d2ac8755a02500a0dcc2ff1"
  "doc/product/feat-planner/section_08.md#queued-messages": "sha256:922fe1806e094e639859e7bf9269013d1dc5c4f6e93bec505a2938d5557ffc88"
  "doc/product/feat-planner/section_08.md#read-only-browsers": "sha256:fa46b40962cca1cfb703081713bb440c58a51bd973a1137ecd388d2f365e4642"
  "doc/product/feat-planner/section_08.md#session-states": "sha256:c0d95879e6a1af78760cf129ea1e99b1ebe038d8af40f71ba24999f84507501c"
  "doc/product/feat-planner/section_08.md#the-model-in-one-paragraph": "sha256:37635b0417de92b86c27b4db91799f5bc59a038c06bed50dcf02cce4bc07764b"
  "doc/product/feat-planner/section_08.md#tool-activity": "sha256:331632eded576a3bac9f1ef54531601aed575d3b5c9c2208aaf4c4e39b0496bf"
  "doc/product/feat-planner/section_08.md#what-a-turn-changed": "sha256:fd5b868198804527f55a7c46a617e7d38eb15b5cd786a8d678379ea2974dde24"
---

# Chat interface

## Panel

The chat panel docks on the right of every page except `/new`, and shows the project's authoring session as a list of turns. [The model in one paragraph](../../../../product/feat-planner/section_08.md#the-model-in-one-paragraph)

While one browser holds authoring, only it has a composer, and every other browser sees the same conversation live without an input box. [The model in one paragraph](../../../../product/feat-planner/section_08.md#the-model-in-one-paragraph) [Read-only browsers](../../../../product/feat-planner/section_08.md#read-only-browsers)

The panel is 21.25rem wide by default, resizes between 20rem and 40rem, and cannot be collapsed; below 1280px it becomes a sheet opened from the header's Chat button. [Layout](../../../../product/feat-planner/section_08.md#layout)

## Messages and tools

User messages are a tinted block aligned to the right with attachment chips beneath, and thinking blocks draw nothing. [Layout](../../../../product/feat-planner/section_08.md#layout) [Message rendering](../../../../product/feat-planner/section_08.md#message-rendering)

Assistant text appears one whole block at a time, with no typing animation. [Message rendering](../../../../product/feat-planner/section_08.md#message-rendering)

Each tool call is one row with a summary from its input and a status of pending, done, error, denied, or not finished. [Tool activity](../../../../product/feat-planner/section_08.md#tool-activity)

Clicking an Edit or Write row expands it to a unified diff built from the result's structuredPatch. [Tool activity](../../../../product/feat-planner/section_08.md#tool-activity)

## States and permissions

The head's sub-line names the session's state in words: Starting, Working, Needs your answer, Wrapping up, Ended, or Stopped, and an idle chat shows no words. [Layout](../../../../product/feat-planner/section_08.md#layout) [Session states](../../../../product/feat-planner/section_08.md#session-states)

A permission card shows the tool, its input, and the time left before the automatic deny, and stays pinned above the composer while pending. [Permission prompts](../../../../product/feat-planner/section_08.md#permission-prompts)

Read-only browsers see the card without buttons, and a holder's deny note reaches the session as the deny message. [Permission prompts](../../../../product/feat-planner/section_08.md#permission-prompts)

## Sending and queueing

The composer grows to eight lines, Enter sends, Shift+Enter inserts a newline, and unsent drafts persist in browser local storage. [Composing and sending](../../../../product/feat-planner/section_08.md#composing-and-sending)

Messages sent while the session is Starting, Working, or waiting on permission are queued on the server and shown in a strip above the composer. [Queued messages](../../../../product/feat-planner/section_08.md#queued-messages)

Interrupting moves queued messages back into the composer, joined with blank lines, instead of delivering them. [Interrupt](../../../../product/feat-planner/section_08.md#interrupt)

## Attachments and turn footers

Each attachment chip labels its document and any passage, line range, or stage, and a message takes at most 10 attachments. [Attachments](../../../../product/feat-planner/section_08.md#attachments)

The turn footer lists each changed file with its line counts and a Diff link that stays disabled until the turn ends, and lists each rebuilt summary without token counts. [What a turn changed](../../../../product/feat-planner/section_08.md#what-a-turn-changed)
