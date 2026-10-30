---
kind: document
of: doc/product/feat-planner/section_06.md
file: sha256:df4a61b23d0022d121e2cdc0423b2b596d978942353ff3d9a692117a50552745
cites:
  "doc/product/feat-planner/section_06.md#diffs": "sha256:c7ce7c5f90e2948358d5ebc835e6b486fe04d37da47935cd95b08cb6a8399c8b"
  "doc/product/feat-planner/section_06.md#how-plans-render": "sha256:9922a394f9eb281a15c232032c5780a4a1f00c8094fdb163193b508f0ee77e99"
  "doc/product/feat-planner/section_06.md#layout-and-addresses": "sha256:b4c5a12fc13ad33d4b935b8463611f90f7860f3e4d30a44d8d8cee3e52e69a58"
  "doc/product/feat-planner/section_06.md#plan-validation": "sha256:00325335241f0829cb87633a6c2def60db583b8f3b9231b0bb99123d44d895a2"
  "doc/product/feat-planner/section_06.md#rendering-markdown": "sha256:b98df215a9cf5674f3fd85b767a35710cc421a020ff0f15da71b5077ec08e681"
  "doc/product/feat-planner/section_06.md#search": "sha256:bcb67e61950e50b35cfb540f88ba837037852bc7895d804f2119d020609140ee"
  "doc/product/feat-planner/section_06.md#the-model-in-one-paragraph": "sha256:ffcca3f183d56197a94f40e69edf3ec6d92627a1718b0237baadcc2c23a87fd4"
  "doc/product/feat-planner/section_06.md#the-navigation-tree": "sha256:e25004d648908fdb07c5094682c486dac7c787e29916a56e4e14ff035b13ea66"
  "doc/product/feat-planner/section_06.md#viewing-and-editing": "sha256:5899d82865af1fb766c74cc7a8e1377dd93b057b11d64cbb03090bf600e7f1dd"
  "doc/product/feat-planner/section_06.md#what-the-server-does-on-save": "sha256:1606300fbb1807475a52370a0fd9f6185f508fd679ab287687af89eb9816e2b2"
  "doc/product/feat-planner/section_06.md#when-the-file-changed-underneath": "sha256:6c6d0c13bdc1a6134ad04abe474dbdf27cf682af60383d02bec010ed90057415"
---

# File Mode

## Model and layout

File mode is a reader first, with a tree of every document under `doc/` that section 03 discovers. [The model in one paragraph](../../../../product/feat-planner/section_06.md#the-model-in-one-paragraph)

The file-mode tree pane is about 18rem wide and becomes a drawer below 900px. [Layout and addresses](../../../../product/feat-planner/section_06.md#layout-and-addresses)

Every document has a file-mode address under `/files/` that takes its path relative to `doc/` and an optional anchor. [Layout and addresses](../../../../product/feat-planner/section_06.md#layout-and-addresses)

The shared document header shows a breadcrumb, a title row with Summary | Source, Edit and Discuss actions, state chips, and the path. [Layout and addresses](../../../../product/feat-planner/section_06.md#layout-and-addresses)

## Navigation and rendering

The navigation tree has five groups in order: Product, Plans, Knowledge, Other, and Archive. [The navigation tree](../../../../product/feat-planner/section_06.md#the-navigation-tree)

Gaps such as a missing section file appear dimmed with the word "missing" at their position in the tree. [The navigation tree](../../../../product/feat-planner/section_06.md#the-navigation-tree)

Markdown renders in the browser from raw file text as CommonMark with GitHub extensions, and raw HTML is never rendered. [Rendering Markdown](../../../../product/feat-planner/section_06.md#rendering-markdown)

A plan view shows a Stages panel in place of the metadata block, with a "Show YAML" control that displays the raw block. [How plans render](../../../../product/feat-planner/section_06.md#how-plans-render)

## Editing and saving

Only the browser holding authoring sees the Edit control, and the server decides which paths are editable. [Viewing and editing](../../../../product/feat-planner/section_06.md#viewing-and-editing)

Knowledge base files, running, done and retired plans, their briefs, and files over 1 MiB are not editable. [Viewing and editing](../../../../product/feat-planner/section_06.md#viewing-and-editing)

On save the server refuses on a hash mismatch, validates plan files, writes atomically, and does nothing else. [What the server does on save](../../../../product/feat-planner/section_06.md#what-the-server-does-on-save)

When the file changed underneath, the editor opens a side-by-side merge view with the newer text on the left. [When the file changed underneath](../../../../product/feat-planner/section_06.md#when-the-file-changed-underneath)

## Validation, diffs and search

Plan validation runs the same code as `loom plan verify` on proposed text, and errors block the save unless the user confirms "Save with errors". [Plan validation](../../../../product/feat-planner/section_06.md#plan-validation)

Diffs are read-only source-text diffs, and turn diffs live in server memory and are lost on restart. [Diffs](../../../../product/feat-planner/section_06.md#diffs)

Search filters the tree as you type and searches document text on Enter, capped at 50 results. [Search](../../../../product/feat-planner/section_06.md#search)
