---
kind: document
of: doc/product/feat-planner/index.md
file: sha256:33dac41d56dc6af0d35da4c26b3805f320dabadf30ad13472ce3e0bf281cd3a0
cites:
  "doc/product/feat-planner/index.md#problem": "sha256:5bde7e20c31d49eae53384ad33536559eeec7a81446ea03f7933c57b67f47355"
  "doc/product/feat-planner/index.md#the-screen": "sha256:011df67c8ed7e11e68c9f2f209c9ad5348e3afbb41dc0c7c0135f521bd12198e"
  "doc/product/feat-planner/index.md#vision": "sha256:e4724a0cef216bf2847d679ceeec067bcab327877c111b5df7af9fb0c27b4d18"
---

# Loom Plan vision

Planning a loom project today runs in Claude Code, and the user drives each step by hand while knowing the skills and file conventions behind it. [Problem](../../../../product/feat-planner/index.md#problem)
The review step arrives with more output than anyone can read, so approvers skim it or give up. [Problem](../../../../product/feat-planner/index.md#problem)

A browser app served by the loom binary carries a project from an empty doc tree to approved, pressure-tested plans with no terminal needed, and it reopens later for reading and refining. [Vision](../../../../product/feat-planner/index.md#vision)
Authoring runs chat sessions that loom starts and owns in tmux, relays their output to a panel, and steers toward loom's document conventions. [Vision](../../../../product/feat-planner/index.md#vision)
Reviewing has two modes: file mode reads any document from product spec to knowledge base, and concept mode opens a short overview of a feature down to its source text. [Vision](../../../../product/feat-planner/index.md#vision)
Either mode can send a passage into a chat, and the session that answers edits the documents under doc. [Vision](../../../../product/feat-planner/index.md#vision)

The project home lists features by slug, each with a pitch drawn from its vision, a progress strip of six steps and counts of its documents. [The screen](../../../../product/feat-planner/index.md#the-screen)
