---
kind: document
of: doc/product/planner-ui/section_06.md
file: sha256:400fc3e55e1c65eab6a32fd4c5066b7c7a800516c4039119a0f6825c2126d65c
cites:
  "doc/product/planner-ui/section_06.md#diffs": "sha256:c5dc14648848bf182f66d4ba3819f4cd646cb903ac539849ab758b5fa920d042"
  "doc/product/planner-ui/section_06.md#how-plans-render": "sha256:b89000db25150de62ddfc6dec1ca6c766938ea17a5f83b0696e7b6566fcb6dee"
  "doc/product/planner-ui/section_06.md#layout-and-addresses": "sha256:569617775a987662f8dafe8c2d1d0a9eda5205d02c367a4020af7a0f34efd531"
  "doc/product/planner-ui/section_06.md#plan-validation": "sha256:d4480c96fd1a14fdd8a7a46a175a5a8292148f10171462c940ecaca60140a8b0"
  "doc/product/planner-ui/section_06.md#rendering-markdown": "sha256:d5c94299e9d575efbcb9817e4c48a17fbe3770a201feac7cad1b9515a632b39f"
  "doc/product/planner-ui/section_06.md#search": "sha256:bcb67e61950e50b35cfb540f88ba837037852bc7895d804f2119d020609140ee"
  "doc/product/planner-ui/section_06.md#taking-a-passage-into-chat": "sha256:f34c244848d8ffb1d9ba5cefe4d7e6f48ddb6239a5bb79b525d91c5c4d8e180a"
  "doc/product/planner-ui/section_06.md#the-model-in-one-paragraph": "sha256:c59093d7a1d0257b1a9cb4707184514d7405a677b9e5def492c3b3c5f075b898"
  "doc/product/planner-ui/section_06.md#the-navigation-tree": "sha256:eec81763d8df9a69dd617455561b2d43eef158d39cfe4de4da4a795375749a21"
  "doc/product/planner-ui/section_06.md#viewing-and-editing": "sha256:b1d65787813513ee526cf9c37f329fc2133364faad33b450ff9b510f3882c491"
  "doc/product/planner-ui/section_06.md#what-the-server-does-on-save": "sha256:1606300fbb1807475a52370a0fd9f6185f508fd679ab287687af89eb9816e2b2"
  "doc/product/planner-ui/section_06.md#when-the-file-changed-underneath": "sha256:6c6d0c13bdc1a6134ad04abe474dbdf27cf682af60383d02bec010ed90057415"
---

# 06 File Mode

## The model

File mode is a reader first, and it shows every document under `doc/` that section 03 discovers, never the summary store. [The model in one paragraph](../../../../product/planner-ui/section_06.md#the-model-in-one-paragraph)

## Layout and addresses

Every document has an address under `/files/` made of its path relative to `doc/` and an optional anchor, so the back button, reloads, and pasted links name a place in a document. [Layout and addresses](../../../../product/planner-ui/section_06.md#layout-and-addresses)

## The navigation tree

The tree has four groups, Product, Plans, Knowledge, and Other, and a missing section or plan file shows dimmed as "missing" at its position. [The navigation tree](../../../../product/planner-ui/section_06.md#the-navigation-tree)

Plan entries show the plan's state as a glyph and a word, and the Archive group holds done plans, other archive files, and orphan brief bundles. [The navigation tree](../../../../product/planner-ui/section_06.md#the-navigation-tree)

## Rendering Markdown

Raw HTML is never rendered, and HTML comments such as the loom METADATA markers are dropped from the view. [Rendering Markdown](../../../../product/planner-ui/section_06.md#rendering-markdown)

Relative links to files under `doc/` open in file mode, and links to paths outside `doc/` render as plain text. [Rendering Markdown](../../../../product/planner-ui/section_06.md#rendering-markdown)

## How plans render

A plan view shows a header, the prose, a Stages panel drawn from the server's plan parse, and the pressure-test record. [How plans render](../../../../product/planner-ui/section_06.md#how-plans-render)

## Viewing and editing

Only the browser holding authoring sees the Edit control, and the server refuses saves to running, done, and retired plans, their briefs, and anything under `doc/loom/`. [Viewing and editing](../../../../product/planner-ui/section_06.md#viewing-and-editing)

File mode never creates, renames, moves, or deletes a file, and a save writes atomically and leaves the rest to the file watcher. [Viewing and editing](../../../../product/planner-ui/section_06.md#viewing-and-editing) [The editor](../../../../product/planner-ui/section_06.md#what-the-server-does-on-save)

On save the server compares the file's hash with the one the editor opened, and a mismatch returns the current text for a side-by-side merge. [When the file changed underneath](../../../../product/planner-ui/section_06.md#when-the-file-changed-underneath)

## Plan validation

Errors block a save unless the user confirms "Save with errors", while warnings never block. [Plan validation](../../../../product/planner-ui/section_06.md#plan-validation)

## Diffs

Diffs show source text, not rendered Markdown, for three kinds of change: what a turn changed, changes while a document is open, and uncommitted work against `HEAD`. [Diffs](../../../../product/planner-ui/section_06.md#diffs)

## Search

Search filters the tree by title as the user types, and on Enter it searches document text on the server and caps results at 50. [Search](../../../../product/planner-ui/section_06.md#search)

## Taking a passage into chat

Selecting text offers "Add to chat", which attaches the document path, the heading anchor, the selected text, and either a line range or a stage id. [Taking a passage into chat](../../../../product/planner-ui/section_06.md#taking-a-passage-into-chat)
