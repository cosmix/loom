---
kind: overview
of: overview/feat-planner/engineering
cites:
  "doc/plans/feat-planner/index.md#plan-map": "sha256:37931643933d66da9cff1cccb5a7cfcc5622fdca6d412da87d5fa847a02a335c"
members:
  "plans/feat-planner-01-documents": "sha256:dbca69e697b2c8feece7246aa693ee01f12b2eaad9824120554c9b05eb68aa12"
  "plans/feat-planner-02-server": "sha256:b49f00e1419f45ae4802d7c76bc546c7f78b84732941f4cc8d104410ecef4615"
  "plans/feat-planner-03-file-mode": "sha256:1eafb55edced2041de5d8ef325e2d4dafcb32efa1195ecc38255a2eace4063d0"
  "plans/feat-planner-04-summary-store": "sha256:b271b432223b98650feeb919fd5f64178caa2e34c048a7167364626fcb386055"
  "plans/feat-planner-05-concept-mode": "sha256:387253e0a4f713c42e6143d5b1ec24020ceb6a150d49eacc808abb0edf41dc3b"
  "plans/feat-planner-06-session-sandbox": "sha256:0735cfda2752b0591f1ea117cb2f7d79e4be0aa0a21b4e29a060a06f3a6e5bee"
  "plans/feat-planner-07-session-bridge": "sha256:9d390308075c7f7a16d2f9b3c7f57a8b1672a2c90ba9bd4767d0c3c1a668251e"
  "plans/feat-planner-08-guidance": "sha256:3d6d835a77bcf56f18b41ba19d62f34cefe9fdf4082fe953f61fa83244882777"
  "plans/feat-planner-09-chat": "sha256:983e77ebd7b40c25f4e24f61c6f7eadf376b10ebc50142e330114cb29626424b"
  "plans/feat-planner-10-summary-rebuilds": "sha256:6e5465314b4ab6cc718d5a463cf81cd674bddd92e31624a3be418e81d3d21233"
  "plans/feat-planner-11-execution-handoff": "sha256:6770f2f165d39d82cfe83210361bec06b81cecbe3492e93609be668f12c4899b"
---

# Planner Engineering Overview

## Programme

The `loom plan` programme orders eleven plans that build the planning app from the document library up to the execution handoff. [Plan map](../../../../plans/feat-planner/index.md#plan-map)

## Foundations

The documents plan builds the `loom::planning` library and `loom plan check`, which later plans call and never re-derive. [01](../../plans/feat-planner-01-documents.md#s-049508e2) [01](../../plans/feat-planner-01-documents.md#s-65ca5358)

The checklist is a pure function of the file tree, with no app state, front matter or sidecar files. [01](../../plans/feat-planner-01-documents.md#s-74c9d5a3)

The server plan moves the dashboard's server parts into a shared `loom::web_server` module and gives the planning server its own router. [02](../../plans/feat-planner-02-server.md#s-76ec62dc)

Bare `loom plan` serves an authenticated planning app, and plan 2 also owns the authoring grace timer. [02](../../plans/feat-planner-02-server.md#s-9656fb41) [02](../../plans/feat-planner-02-server.md#s-d3f6ec34)

## Reading and summaries

File mode adds the server's document, tree, save, diff and search routes, with in-process plan validation. [03](../../plans/feat-planner-03-file-mode.md#s-b29534f0)

The renderer builds React elements from a Markdown tree so no HTML sanitiser is needed, and saves never create, rename or delete files. [03](../../plans/feat-planner-03-file-mode.md#s-e7b3d16c) [03](../../plans/feat-planner-03-file-mode.md#s-5cc599b9)

The summary store builds a summary engine that needs no session, with four `loom plan summary` commands that work from any terminal. [04](../../plans/feat-planner-04-summary-store.md#s-1ca5df76) [04](../../plans/feat-planner-04-summary-store.md#s-283c6ede)

Seal writes through the store lock without following symlinks, and two seals of the same draft are byte-identical. [04](../../plans/feat-planner-04-summary-store.md#s-a4d153b6) [04](../../plans/feat-planner-04-summary-store.md#s-bec2fcc6)

Concept mode adds the Product and Engineering tabs, the summary pages and the document key, driven by one route module that pushes a delta when a batch changes a page. [05](../../plans/feat-planner-05-concept-mode.md#s-f7abf710) [05](../../plans/feat-planner-05-concept-mode.md#s-1f10e8f0)

The route returns each page fully resolved, and passages come from plan 3's document route. [05](../../plans/feat-planner-05-concept-mode.md#s-0a888e1c) [05](../../plans/feat-planner-05-concept-mode.md#s-502c9c34)

## Sessions

Every planning session runs under a settings file that loom writes for it, plus a Rust path guard that Claude Code calls before each file-tool use. [06](../../plans/feat-planner-06-session-sandbox.md#s-5a053e5a)

The guard never allows and only denies or stays silent, so Claude Code's permission rules and the holder's prompt still apply. [06](../../plans/feat-planner-06-session-sandbox.md#s-0416bee5)

The bridge starts each interactive session in its own tmux server and rebuilds its chat from the session transcript. [07](../../plans/feat-planner-07-session-bridge.md#s-ba8caf11) [07](../../plans/feat-planner-07-session-bridge.md#s-d0b1facd)

Permission requests are answered through files the session cannot write, and the server denies a pending request before Claude Code's hook timeout. [07](../../plans/feat-planner-07-session-bridge.md#s-3c496b62) [07](../../plans/feat-planner-07-session-bridge.md#s-ab09513c)

The guidance plan writes the `loom-plan` plugin and the briefs that every planning session receives at launch. [08](../../plans/feat-planner-08-guidance.md#s-232342b2) [08](../../plans/feat-planner-08-guidance.md#s-7b92aa0b)

The plugin text is embedded in the binary and is never installed into the user's `~/.claude`. [08](../../plans/feat-planner-08-guidance.md#s-d6db1d3f)

## Chat and review

The chat plan builds the panel that docks on every page and connects review to authoring. [09](../../plans/feat-planner-09-chat.md#s-6d38fdd1) [09](../../plans/feat-planner-09-chat.md#s-c52606ad)

Restore takes each file's text from the turn-start pin and is refused while a turn runs. [09](../../plans/feat-planner-09-chat.md#s-c8d3a5b2)

Suggestions to the author are WebSocket commands that reach only the holder's connections. [09](../../plans/feat-planner-09-chat.md#s-b1046083)

The rebuild plan starts one background summary session for up to five stale summaries and asks the browser first above five. [10](../../plans/feat-planner-10-summary-rebuilds.md#s-7fbc93c4) [10](../../plans/feat-planner-10-summary-rebuilds.md#s-2633511b)

Nothing starts while an authoring turn or a pressure run is active, and costs match `loom status` because both use the same accumulator. [10](../../plans/feat-planner-10-summary-rebuilds.md#s-9c503e11) [10](../../plans/feat-planner-10-summary-rebuilds.md#s-3fb68e6a)

## Execution handoff

The handoff plan moves the pressure pipeline into `loom::pressure` so the terminal and the app share it. [11](../../plans/feat-planner-11-execution-handoff.md#s-7b270c42)

The app runs Claude steps as pressure sessions and Codex as a child of the server, and it stops cleanly on Stop, a failed step, Ctrl-C or a dead server. [11](../../plans/feat-planner-11-execution-handoff.md#s-864542e4)

The run side adds the run dialog, the `doc/` commit, `loom init` and `loom run` as children, and Abandon run. [11](../../plans/feat-planner-11-execution-handoff.md#s-2159aacc)
