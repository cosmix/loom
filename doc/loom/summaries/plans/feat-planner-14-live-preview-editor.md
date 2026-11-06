---
kind: document
of: doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md
file: sha256:2edce2dbb8ce4c67801afce4c4c547bf794cac8cdcdfe28ae4cd55b9dd20d80e
cites:
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#1-live-preview": "sha256:8aa63fdba007aa641592a377372348270cc16515064cd137be7326addf555b1f"
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#after-the-run": "sha256:721e21c9bc06b8da77c131acbcc42ccb4e33b0778a89461f710729a4c66d1567"
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#environment-inventory": "sha256:b2ae3cc29ae4624579ec12780d4c6d1b2bd02fa97bdb8a867ae7d4c77c7e5b35"
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#execution": "sha256:2a86d084be360e4216f390d102540b1e2774c0c43301b88905a04ecb5d07464b"
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#goals": "sha256:c4cb27f1fcd0a0540495a8a71943ff24e613c340e7f129b3e4897f2c77feb665"
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#integration-verification": "sha256:a2ba445e88c31d00bfda14865f1f0a7477ba64ed288d722b4903c7e69c4f2854"
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#knowledge-distillation": "sha256:5e6a3cfa012adcb782c5791b4e02c82a7d07a145b6037ef0b3bc9442e18ab235"
  "doc/plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#overview": "sha256:5c19af7cbd4af2d1472eb6836d224f26eb59302a31a962a23f6eeff4a1a4cf08"
---

# Live preview in file mode's editor

Live preview gives file mode a rendered look while the saved text stays exactly what the user typed. [d](#s-e1f7d5b5) [d](#s-9b4c092d) [d](#s-d45b1a8f)

Together the goals fix what the editor shows, which bytes survive an open and save, and how modes, commands and attachments behave. [d](#s-9962b472) [d](#s-c50fe233) [d](#s-da5b1d46) [d](#s-a8a94887) [d](#s-efd2502f)

Delivery is one implementation stage whose two workers build against a frozen interface, followed by integration verification and knowledge distillation. [d](#s-f8edb11d) [d](#s-6d80881a) [d](#s-063fe7da) [d](#s-a6889f4a)

No package is added, and the browser check depends on a host Chromium. [d](#s-92ce3e6f) [d](#s-15c6db61)

Final acceptance rests with the operator's trial of both modes. [d](#s-ebe5b5ca)

The live preview draws Markdown styling over the source text inside the existing CodeMirror editor of file mode, so the user edits text that looks rendered. [Overview](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#overview)

A Live preview / Source toggle switches between the two views, and a formatting toolbar with keyboard shortcuts inserts or removes the syntax. [Overview](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#overview)

Saving still writes exactly the editor's text, while the save path, drafts, merge view, diffs and plan validation stay unchanged. [Overview](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#overview)

Markers hide off the cursor's line, a click toggles a task box, and a code fence shows only its language label until the cursor enters the block, while tables and HTML comments stay visible, with table pipes and comments dimmed. [Goals](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#goals)

Switching modes keeps the text, cursor and undo history, the chosen mode is remembered, and validation marks show in both modes while line numbers show in source only. [Goals](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#goals)

Toolbar buttons and the bold, italic and link shortcuts each insert or remove their syntax as one undo step. [Goals](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#goals)

Opening, toggling and saving without an edit write identical bytes, including CRLF, CR and mixed line endings, and Add to chat passes the selected source on with its markers intact. [Goals](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#goals)

Drawing tables as grids, rendering diagrams and changing diffs or the merge view are out of scope. [Goals](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#goals)

One implementation stage, live-preview, runs two parallel workers against an interface frozen in the shared brief, and no second stage is needed because nothing orders the merges or shares a file. [Execution](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#execution) [Live-preview stage](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#1-live-preview)

Named contracts cover the main risks: byte integrity, comment markers, user state, and reachability through the real edit body. [Live-preview stage](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#1-live-preview)

Integration verification rebuilds web/dist and the binary, runs the full web suite and the bundle check that keeps CodeMirror out of the entry bundle, walks the built app in a browser, and reviews the change before web/dist is committed last. [Integration Verification](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#integration-verification)

Knowledge distillation records the live preview design in the planner web knowledge and curates the stage memories. [Knowledge Distillation](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#knowledge-distillation)

No new packages are needed, because the Markdown language is already exported by the CodeMirror package and the icon library is an existing dependency. [Environment inventory](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#environment-inventory)

The browser check needs Chromium from the host cache, and a stage that cannot launch it records that and tells the operator. [Environment inventory](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#environment-inventory)

The operator's own trial of a real document in both modes decides whether the editing feels right. [After the run](../../../plans/feat-planner/PLAN-feat-planner-14-live-preview-editor.md#after-the-run)
