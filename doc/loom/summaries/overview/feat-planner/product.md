---
kind: overview
of: overview/feat-planner/product
cites:
  "doc/product/feat-planner/index.md#principles": "sha256:893f636f87964ab0f3fe68e2645739d21f759c9b6f6bc520678beebb5405924e"
  "doc/product/feat-planner/index.md#problem": "sha256:436d1c907b1d4211838b9a20f25a95234c9b1b8c2e6256ac3827db32b8a42f91"
  "doc/product/feat-planner/index.md#vision": "sha256:10575e2e2451ea4fa762d93bfaaa91e07fa65ff5a734b6755789c3e040df5431"
members:
  "feat-planner/product/section_01": "sha256:724b5b8338ccd00c0a4d10b112d82528ff2706286ec6c79948ea516d2ac4e454"
  "feat-planner/product/section_02": "sha256:3927b613c9c813f4f81fd71adf33287832be00898528fbaf43083ba2e906c794"
  "feat-planner/product/section_03": "sha256:cc8404b3c8884a86f193143990b0679944d39bb1c04c9b39348f3ab62b571f72"
  "feat-planner/product/section_04": "sha256:0472610608f90e74d8b729681374a179af9b8d3ab936a51847ff40781362b096"
  "feat-planner/product/section_05": "sha256:70959f3062922c236d4e3a964bc973446b00e6a5b7262e8b5b9071014226d322"
  "feat-planner/product/section_06": "sha256:52acfaf0406393f59afa2a5b89257b5710cde667af18288fd4226d7b638778e2"
  "feat-planner/product/section_07": "sha256:6526e4481586adfac88b7875d3a87f321000d5cb178710b0672666fe1bc5b43c"
  "feat-planner/product/section_08": "sha256:7498be33b5b2fffc1bb2a8769661b63e32ca75c2211ccf02eb8a613e0ebf31cb"
  "feat-planner/product/section_09": "sha256:18c4acff559940acbd8a4230371102915acfb19f65b1f6a88f108b3b67341795"
  "feat-planner/product/section_10": "sha256:95d11c8478cf0c42a71c6988182725ff457a12dd3d071cf7dbb6bf3e66ea65e3"
  "feat-planner/product/section_11": "sha256:e2129a23cc1f3939b5dda3bd3dc6ee89a43acc3dc9b09231044b6aae249a24a7"
  "feat-planner/product/section_12": "sha256:7080b18c019ea7091d6f609e31bbdcf5c580441ca628e08f42c0ae02fca844fd"
---

# Planner Product Overview

## Vision and problem

Planning a loom project happens in Claude Code today, as a sequence the user drives by hand, and the review at the end is the step the current tooling serves worst. [Problem](../../../../product/feat-planner/index.md#problem)

`loom plan` serves a browser app that takes a project from an empty `doc/` to approved, pressure-tested loom plans without a terminal, and lets the user return later to read, question and refine them. [Vision](../../../../product/feat-planner/index.md#vision)

The files in `doc/` stay the truth, summaries are derived and disposable, and every summary claim traces to its source text. [Principles](../../../../product/feat-planner/index.md#principles)

## Workflow and server

The app stores no workflow state and rebuilds one six-step checklist per feature from the files under `doc/`, with the current step being the earliest gap. [01](../../feat-planner/product/section_01.md#s-282839d2) [01](../../feat-planner/product/section_01.md#s-abd7bbcb)

A feature is the shared name of its product directory and its plans directory under `doc/`, and standalone plans follow their own lifecycle with no six-step checklist. [01](../../feat-planner/product/section_01.md#s-b0ac3913) [01](../../feat-planner/product/section_01.md#s-ca0a1911)

Bare `loom plan` starts the app on port 7272 and opens the browser, while `loom plan verify` keeps its behaviour. [02](../../feat-planner/product/section_02.md#s-24b80597) [02](../../feat-planner/product/section_02.md#s-8cb10e48)

One server runs per project, one browser holds authoring at a time, and a missing `tmux`, `claude` or login leaves the server read-only. [02](../../feat-planner/product/section_02.md#s-af791ce9) [02](../../feat-planner/product/section_02.md#s-446077ff) [02](../../feat-planner/product/section_02.md#s-0bd67ff9)

## Documents and summaries

A document's kind comes from its path and name alone, and a plan names the product sections it implements in an `Implements:` line. [03](../../feat-planner/product/section_03.md#s-0d9953c9) [03](../../feat-planner/product/section_03.md#s-88bfca0e)

Knowledge files stay read-only in the app and change only through `loom knowledge`. [03](../../feat-planner/product/section_03.md#s-ca3852d4)

Every summary is a committed Markdown file with a manifest of content hashes, and the engine rehashes sources to find out-of-date sentences. [04](../../feat-planner/product/section_04.md#s-3bc8275e) [04](../../feat-planner/product/section_04.md#s-84992440)

Document summaries and overviews run on Haiku at high effort by default, and background rebuilds start at once for up to 5 out-of-date summaries and ask first above that. [04](../../feat-planner/product/section_04.md#s-7286fd62) [04](../../feat-planner/product/section_04.md#s-014f2b82)

## Reading in two modes

Concept mode has three levels of text, an overview, a document summary and a source passage, each a step closer to the files. [05](../../feat-planner/product/section_05.md#s-7c6230dc)

Clicking a summary sentence opens it in place to the sentences it cites, and its coloured marks turn dashed while the underlying summary is out of date. [05](../../feat-planner/product/section_05.md#s-d1fe0da4) [05](../../feat-planner/product/section_05.md#s-81d10b54)

Links between product sections and plans come only from `Implements:` lines and are never inferred. [05](../../feat-planner/product/section_05.md#s-c81ce472)

File mode is a reader first, with a tree of every document under `doc/` and a shared header that offers Summary, Source, Edit and Discuss. [06](../../feat-planner/product/section_06.md#s-6e10fc6c) [06](../../feat-planner/product/section_06.md#s-39f7ba31)

Only the browser holding authoring can edit, and a save is refused on a hash mismatch and validated for plan files. [06](../../feat-planner/product/section_06.md#s-defd12ce) [06](../../feat-planner/product/section_06.md#s-193ec3ed)

## Sessions and chat

Planning sessions are interactive Claude Code processes, each in its own tmux server, and the server reads their transcripts and hook events to follow their state. [07](../../feat-planner/product/section_07.md#s-b7f3b6da) [07](../../feat-planner/product/section_07.md#s-ff9354ef) [07](../../feat-planner/product/section_07.md#s-d4098782)

Sessions start cold and never use `claude --resume`, and permission decisions go through a hook rather than keystrokes. [07](../../feat-planner/product/section_07.md#s-7bb0401a) [07](../../feat-planner/product/section_07.md#s-444fd782)

The chat panel docks on the right and shows assistant text a whole block at a time with one row per tool call, and only the holder has a composer. [08](../../feat-planner/product/section_08.md#s-257c2d99) [08](../../feat-planner/product/section_08.md#s-f6d593b6) [08](../../feat-planner/product/section_08.md#s-bfd1930f)

The turn footer lists each changed file with its line counts and a Diff link, and each rebuilt summary with its tokens. [08](../../feat-planner/product/section_08.md#s-418e212d)

Every planning session gets a generated brief, an opening message naming its step and target, and a per-session plugin of skills and summary agents. [09](../../feat-planner/product/section_09.md#s-e2717c2d) [09](../../feat-planner/product/section_09.md#s-be412e4b)

After a turn that edits documents, the session rebuilds the affected summaries in two waves, document summaries first and then overviews. [09](../../feat-planner/product/section_09.md#s-52ae24da)

## Review, execution and safety

A reviewer attaches a doubtful passage to a chat message, and the session either answers or edits the documents. [10](../../feat-planner/product/section_10.md#s-e324c0d2)

The app records no approval, because reviewing is the user reading and has no step, and a reviewer can restore a file to its text from before a turn. [10](../../feat-planner/product/section_10.md#s-390ca474) [10](../../feat-planner/product/section_10.md#s-607384f2)

Pressure testing and starting a run are buttons the user presses and never happen automatically. [11](../../feat-planner/product/section_11.md#s-ebc06a53)

A pressure run takes the authoring slot until it ends, and starting a run executes `loom init` and `loom run` only after the app shows what they require. [11](../../feat-planner/product/section_11.md#s-4569fc78) [11](../../feat-planner/product/section_11.md#s-3b4a7fb4)

Every start mints a token that sets an access cookie, and the cookie is required on every API route and WebSocket. [12](../../feat-planner/product/section_12.md#s-9a0d1e1f) [12](../../feat-planner/product/section_12.md#s-465bfa4e)

Sessions write under `doc/` outside the locked set, and the OS sandbox, permission rules and planning hook enforce that table together. [12](../../feat-planner/product/section_12.md#s-2deab04f) [12](../../feat-planner/product/section_12.md#s-489509f3)
