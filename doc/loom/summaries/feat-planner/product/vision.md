---
kind: document
of: doc/product/feat-planner/index.md
file: sha256:511f204f9af6d451d3d8fe62f0dde7bb796fb73999269e58ec09027162e4d9bb
cites:
  "doc/product/feat-planner/index.md#problem": "sha256:436d1c907b1d4211838b9a20f25a95234c9b1b8c2e6256ac3827db32b8a42f91"
  "doc/product/feat-planner/index.md#the-screen": "sha256:3b70baec5aea1ce3d7ab455ceda5784658f33eecbba10d9aab5733c23243fa47"
  "doc/product/feat-planner/index.md#vision": "sha256:52400b8a25b2743b5354dfc3a5b70377a0664ef9ece0585d930327c86cf29682"
---

# Loom Plan vision

Planning a loom project today means a user drives six steps by hand from a terminal, knowing each step's skills and file conventions. [Problem](../../../../product/feat-planner/index.md#problem)
By the review step the output is too large to read, so a PM or engineer skims it or gives up on approving it. [Problem](../../../../product/feat-planner/index.md#problem)

Loom plan is a browser app served by the loom binary that takes a project from an empty doc tree to approved, pressure-tested loom plans. [Vision](../../../../product/feat-planner/index.md#vision)
Authoring runs Claude Code sessions that loom owns in tmux, relays their output to a chat panel, and guides them to write documents in loom's conventions. [Vision](../../../../product/feat-planner/index.md#vision)
Reviewing has a file mode for reading any planning document and a concept mode that starts from a short overview and expands sentence by sentence down to the source passage. [Vision](../../../../product/feat-planner/index.md#vision)
A passage can be taken from either mode into a chat, and the session that answers edits the documents in doc. [Vision](../../../../product/feat-planner/index.md#vision)

The project home shows each feature by slug with a pitch from its vision, progress across six steps and document counts. [The screen](../../../../product/feat-planner/index.md#the-screen)
