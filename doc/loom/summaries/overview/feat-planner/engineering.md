---
kind: overview
of: overview/feat-planner/engineering
cites:
  "doc/plans/feat-planner/index.md#plan-map": "sha256:edb6365c2d67ba86f380a42976159845cfcf7acc052839e8fda7d70452c9d01a"
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
  "plans/feat-planner-12-ui-repair": "sha256:b8fb23bc5401805a878a13d888284ef2adf5a695481992663ac05f500349a295"
  "plans/feat-planner-13-planner-redesign": "sha256:6fed23a4f2c300ef6ace6a43af77779b2c34b7773d26cc4646185579f762be8e"
  "plans/feat-planner-14-live-preview-editor": "sha256:e2c135dd13cbe595db8b2d0ccd18e0e1272a56627674f35650e3fe5fdeea296a"
---

# Planner Engineering Overview

## Programme

The `loom plan` programme orders fourteen plans that build the planning app from the document library up to the execution handoff, then repair and redesign its web screens and add a live preview to its editor. [Plan map](../../../../plans/feat-planner/index.md#plan-map)

## Foundations

The documents plan builds the `loom::planning` library as a pure function of the file tree, with no app state or sidecar files, and plans 2 to 11 call it and never re-derive its rules. [01](../../plans/feat-planner-01-documents.md#s-049508e2) [01](../../plans/feat-planner-01-documents.md#s-65ca5358) [01](../../plans/feat-planner-01-documents.md#s-74c9d5a3)

The library reads every document through no-follow opens and reuses the brief collection that `loom run` already has, so the app and the checker agree on which files are documents and briefs. [01](../../plans/feat-planner-01-documents.md#s-1f66212c) [01](../../plans/feat-planner-01-documents.md#s-959d80a1)

The server plan moves the dashboard's server parts into a shared `loom::web_server` module, so bare `loom plan` serves an authenticated planning app from its own router. [02](../../plans/feat-planner-02-server.md#s-76ec62dc) [02](../../plans/feat-planner-02-server.md#s-9656fb41)

Plan 2 also owns the authoring grace timer and checks bundle separation from Vite's manifest, so the planning shell stays out of the dashboard bundle. [02](../../plans/feat-planner-02-server.md#s-d3f6ec34) [02](../../plans/feat-planner-02-server.md#s-778af8c1)

## Reading and summaries

File mode adds document, tree, save, diff and search routes with a renderer that builds React elements from a Markdown tree, so reading and editing need no HTML sanitiser. [03](../../plans/feat-planner-03-file-mode.md#s-b29534f0) [03](../../plans/feat-planner-03-file-mode.md#s-e7b3d16c)

Saves are atomic through a no-follow descriptor, validate plans over the proposed text, and never create, rename or delete files. [03](../../plans/feat-planner-03-file-mode.md#s-5cc599b9) [03](../../plans/feat-planner-03-file-mode.md#s-e01a183f)

The summary store gives a summary engine that needs no session, driven by four `loom plan summary` commands that work from any terminal. [04](../../plans/feat-planner-04-summary-store.md#s-1ca5df76) [04](../../plans/feat-planner-04-summary-store.md#s-283c6ede)

Seal is safe and repeatable: it writes through the store lock without following symlinks, and two seals of the same draft are byte-identical. [04](../../plans/feat-planner-04-summary-store.md#s-a4d153b6) [04](../../plans/feat-planner-04-summary-store.md#s-bec2fcc6)

Concept mode adds the Product and Engineering tabs and the document key, driven by one route module that pushes a delta when a batch changes a page. [05](../../plans/feat-planner-05-concept-mode.md#s-f7abf710) [05](../../plans/feat-planner-05-concept-mode.md#s-1f10e8f0)

The route returns each page fully resolved and takes passages from plan 3's document route, so the browser computes nothing from summary files. [05](../../plans/feat-planner-05-concept-mode.md#s-0a888e1c) [05](../../plans/feat-planner-05-concept-mode.md#s-502c9c34)

## Sessions

Planning sessions are confined by a settings file that loom writes for each one and by a Rust path guard that only denies or stays silent, so Claude Code's own permission rules and the holder's prompt still apply. [06](../../plans/feat-planner-06-session-sandbox.md#s-5a053e5a) [06](../../plans/feat-planner-06-session-sandbox.md#s-0416bee5)

The bridge runs each interactive session in its own tmux server and rebuilds its chat from the transcript, with permission requests answered through files the session cannot write. [07](../../plans/feat-planner-07-session-bridge.md#s-ba8caf11) [07](../../plans/feat-planner-07-session-bridge.md#s-d0b1facd) [07](../../plans/feat-planner-07-session-bridge.md#s-3c496b62)

The server denies a pending request at 3590 seconds, so no session waits on a dialog nobody sees past Claude Code's hook timeout. [07](../../plans/feat-planner-07-session-bridge.md#s-ab09513c) [07](../../plans/feat-planner-07-session-bridge.md#s-8cb527d9)

The guidance plan renders the `loom-plan` plugin and a brief for every session, with the plugin text embedded in the binary and never installed into the user's `~/.claude`. [08](../../plans/feat-planner-08-guidance.md#s-232342b2) [08](../../plans/feat-planner-08-guidance.md#s-d6db1d3f) [08](../../plans/feat-planner-08-guidance.md#s-7b92aa0b)

## Chat and review

The chat plan builds the panel that docks on every page and adds the review routes, so a reviewer can attach passages, send suggestions and restore a wrong edit. [09](../../plans/feat-planner-09-chat.md#s-6d38fdd1) [09](../../plans/feat-planner-09-chat.md#s-c52606ad) [09](../../plans/feat-planner-09-chat.md#s-57893d9f)

Restore reads text from the turn-start pin and is refused during a turn, and suggestions reach only the holder's connections, so review cannot overwrite or leak into a live turn. [09](../../plans/feat-planner-09-chat.md#s-c8d3a5b2) [09](../../plans/feat-planner-09-chat.md#s-b1046083)

The rebuild plan keeps summaries current with one background session for up to five stale summaries and asks the browser first above five. [10](../../plans/feat-planner-10-summary-rebuilds.md#s-7fbc93c4) [10](../../plans/feat-planner-10-summary-rebuilds.md#s-2633511b)

Nothing rebuilds during an authoring turn or a pressure run, and costs match `loom status` because both use the same accumulator. [10](../../plans/feat-planner-10-summary-rebuilds.md#s-9c503e11) [10](../../plans/feat-planner-10-summary-rebuilds.md#s-3fb68e6a)

## Execution handoff

The handoff plan moves the pressure pipeline into `loom::pressure` so the terminal and the app share one record writer, and a recorded plan still has to pass `loom plan verify`. [11](../../plans/feat-planner-11-execution-handoff.md#s-7b270c42) [11](../../plans/feat-planner-11-execution-handoff.md#s-48f50089) [11](../../plans/feat-planner-11-execution-handoff.md#s-0e223972)

The app drives Claude steps as pressure sessions and Codex as a child of the server, and the run side adds the run dialog, `loom init` and `loom run` as children and Abandon run. [11](../../plans/feat-planner-11-execution-handoff.md#s-864542e4) [11](../../plans/feat-planner-11-execution-handoff.md#s-2159aacc)

## Repair and redesign

The repair plan rebuilds every planner screen to the transcribed mockup and moves summary generation to Haiku at high effort with a Codex spawner, and it drops the home screen's chat panel for one app bar. [12](../../plans/feat-planner-12-ui-repair.md#s-75a11405) [12](../../plans/feat-planner-12-ui-repair.md#s-95f02975) [12](../../plans/feat-planner-12-ui-repair.md#s-9ae3a772)

The redesign plan makes the feature screen a step spine that opens real content and makes the chat follow the document the reader views. [13](../../plans/feat-planner-13-planner-redesign.md#s-4b459a8f) [13](../../plans/feat-planner-13-planner-redesign.md#s-fe345ecf)

It also stops authoring chats from prompting for innocuous actions and bounds how long a planning session or tmux server may outlive its server to a 15-minute grace. [13](../../plans/feat-planner-13-planner-redesign.md#s-b8b930ec) [13](../../plans/feat-planner-13-planner-redesign.md#s-8e462c5c)

## Editor

The live preview plan draws Markdown styling over the source text in file mode's CodeMirror editor, with a Live preview and Source toggle and a formatting toolbar. [14](../../plans/feat-planner-14-live-preview-editor.md#s-e1f7d5b5) [14](../../plans/feat-planner-14-live-preview-editor.md#s-9b4c092d)

Saving still writes exactly the editor's text, and a named byte-integrity contract guards that the preview never changes what is saved. [14](../../plans/feat-planner-14-live-preview-editor.md#s-d45b1a8f) [14](../../plans/feat-planner-14-live-preview-editor.md#s-6d80881a)

One implementation stage runs two parallel workers against a frozen interface and adds no package, and a real-browser check needs a host Chromium. [14](../../plans/feat-planner-14-live-preview-editor.md#s-f8edb11d) [14](../../plans/feat-planner-14-live-preview-editor.md#s-92ce3e6f) [14](../../plans/feat-planner-14-live-preview-editor.md#s-15c6db61)
