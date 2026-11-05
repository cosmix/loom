---
kind: document
of: doc/product/feat-planner/section_08.md
file: sha256:66252abacebb82ffe69bf8d697b65f8b45e8641db76ff38e0b4ffb13f5280803
cites:
  "doc/product/feat-planner/section_08.md#attachments": "sha256:e5d8341c3a4f0f6573f71c86b5e40a2b3d386025590b3e5989f795e087cf3944"
  "doc/product/feat-planner/section_08.md#composing-and-sending": "sha256:7a80f7feb3248c2476b414c2cc059e9c76a5e936532f17a79720f02f7ddfdaa5"
  "doc/product/feat-planner/section_08.md#interrupt": "sha256:83b7c1a08ee5324b53dd2e4a3edcd364df071b9fa19bfc8ad868555b7baa769a"
  "doc/product/feat-planner/section_08.md#layout": "sha256:e4658c964d6c5170af5b0d9e2a49a0e0f25038e208d08551bdac1b4927f382f0"
  "doc/product/feat-planner/section_08.md#message-rendering": "sha256:c7152a8642146d4c5f5ad3dd9f2b6714b1878e8e60f6f724a735ece1dc3ee529"
  "doc/product/feat-planner/section_08.md#permission-prompts": "sha256:11a0caf008c5809c7a6493cc619f049afff92de57d2ac8755a02500a0dcc2ff1"
  "doc/product/feat-planner/section_08.md#queued-messages": "sha256:922fe1806e094e639859e7bf9269013d1dc5c4f6e93bec505a2938d5557ffc88"
  "doc/product/feat-planner/section_08.md#session-states": "sha256:a63546023bb8df9bf4dcd6fed85571eb3c811eaf7c1847326c211cb2caf138d4"
  "doc/product/feat-planner/section_08.md#the-model-in-one-paragraph": "sha256:e80905946d108346ffbcd89c576c0f24ae9c63f58dd1c0eaff08c305b092f899"
  "doc/product/feat-planner/section_08.md#tool-activity": "sha256:521f79f2544e1c11de645f123c7a8228f6edfcd7a4ca0816c5150c236f10fe67"
  "doc/product/feat-planner/section_08.md#what-a-turn-changed": "sha256:1169a1b9d403f10490e71f39f21ed0650bec4adeedb2d399561d61113cded34c"
---

# Chat interface

## Panel

The chat panel docks on the right of every page except `/new` and shows the project's authoring session as a list of turns. [The model in one paragraph](../../../../product/feat-planner/section_08.md#the-model-in-one-paragraph)
Only the browser holding authoring has a composer, and every other browser sees the same conversation live without an input box. [The model in one paragraph](../../../../product/feat-planner/section_08.md#the-model-in-one-paragraph)
The panel is 26rem wide by default, resizes between 20rem and 40rem, and collapses to a 2.5rem rail that keeps the state glyph and a waiting permission badge. [Layout](../../../../product/feat-planner/section_08.md#layout)

## Messages and tools

User messages are set apart by type and space rather than a bubble in a box, and consecutive thinking markers in one message merge into one line. [Message rendering](../../../../product/feat-planner/section_08.md#message-rendering)
Assistant text appears one whole block at a time, with no typing animation. [Message rendering](../../../../product/feat-planner/section_08.md#message-rendering)
Each tool call is one row with a summary from its input and a status of pending, done, error, denied, or not finished. [Tool activity](../../../../product/feat-planner/section_08.md#tool-activity)
Clicking an Edit or Write row expands it to a unified diff built from the result's structuredPatch. [Tool activity](../../../../product/feat-planner/section_08.md#tool-activity)

## States and permissions

The state line names the session's state in words with a glyph, covering none, Starting, Idle, Working, Permission, Ending, Ended, and Failed. [Session states](../../../../product/feat-planner/section_08.md#session-states)
A permission card shows the tool, its input, and the time left before the automatic deny, and stays pinned above the composer while pending. [Permission prompts](../../../../product/feat-planner/section_08.md#permission-prompts)
Read-only browsers see the card without buttons, and a holder's deny note reaches the session as the deny message. [Permission prompts](../../../../product/feat-planner/section_08.md#permission-prompts)

## Sending and queueing

The composer grows to eight lines, Enter sends, Shift+Enter inserts a newline, and unsent drafts persist in browser local storage. [Composing and sending](../../../../product/feat-planner/section_08.md#composing-and-sending)
Messages sent while the session is Starting, Working, or waiting on permission are queued on the server and shown in a strip above the composer. [Queued messages](../../../../product/feat-planner/section_08.md#queued-messages)
Interrupting moves queued messages back into the composer, joined with blank lines, instead of delivering them. [Interrupt](../../../../product/feat-planner/section_08.md#interrupt)

## Attachments and turn footers

Each attachment chip labels its document and any passage, line range, or stage, and a message takes at most 10 attachments. [Attachments](../../../../product/feat-planner/section_08.md#attachments)
The turn footer lists each changed file with its line counts and a Diff link that is disabled until the turn ends, and each rebuilt summary with its tokens. [What a turn changed](../../../../product/feat-planner/section_08.md#what-a-turn-changed)
