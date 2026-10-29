---
kind: overview
of: product
cites:
  "doc/product/index.md#principles": "sha256:72f73063ff8d2f3fd9b13e141968a90a16602eb336ab943a4e37ebe5807964df"
  "doc/product/index.md#problem": "sha256:693456be75425b1057249f3e02c9895a0b4ebf7eff12fe6ae3ede2a48437cc26"
  "doc/product/index.md#vision": "sha256:edd232dbda4fa6a7b451426ecc6dadd8abf5e4dae494c5eb0fd8fd6f6b23a62b"
members:
  "product/section_01": "sha256:71ea202f794b9f1f50c63c42df84148d0ba976b16606ed272af37d40eec8b11a"
  "product/section_02": "sha256:831ac95bacfbb21798a9247cc57a7f9492345838d448ace2ad603c8f2a6608ae"
  "product/section_03": "sha256:c6d3eb16bb030572a5ab08ee8249e4352cf0694085a497f55e08ed49426e8650"
  "product/section_04": "sha256:65816da4078dfaa944232479b1127ac9401d00beb657b8ce4a5fa56b80ab6eee"
  "product/section_05": "sha256:7d30a09e9ef9619d7ca1d418e40bf1e334aab62abce0966936e0a874a69d5e53"
  "product/section_06": "sha256:d9ed8239c843b8e109633b5908c916d90ea07f2e94ad26e4b43fcf4aa9c9541a"
  "product/section_07": "sha256:2987312a0f6ebe30e30c09b42456d41a884ae100acbdf9f1900d92b1bf83ab74"
  "product/section_08": "sha256:fac51e69dcc5367b5f56ff5dd55b48ffba3ad763decf5916002a47d91ebd5451"
  "product/section_09": "sha256:b12da30c6faddbce8623ff841f3c7b014d8fc4fc03ddf7ffda7cfc5aa94661f4"
  "product/section_10": "sha256:b9d8a477e3b03a3edaf2deb06c38f71ce996943fc47697c5b6760e54b14df170"
  "product/section_11": "sha256:0c6610ded3021fddde20c1d8ffc2cbb7c31914a1c0244a976df44ae73204f2c8"
  "product/section_12": "sha256:de30297f0a467833c1e161649fafaa123adf0d516e4928d143532ea159b075e7"
---

# Loom Plan Product Overview

## Purpose

Loom Plan is a browser app served by the `loom` binary that takes a project from an empty `doc/` to reviewed loom plans without a terminal. [Vision](../../product/index.md#vision)

It exists because the review step is where the current tooling serves a human worst, since a vision, a dozen sections, a metaplan, plans and briefs are too much to approve by skimming. [Problem](../../product/index.md#problem)

The files in `doc/` stay the truth, summaries are derived and disposable, and every summary claim traces to its source text. [Principles](../../product/index.md#principles)

## Workflow and server

The app stores no workflow state and works out the current step from the earliest gap in the `doc/` tree. [section 01](product/section_01.md#s-bde6f08c) [section 01](product/section_01.md#s-4f481ee8)

Its offers close the first gap of the current step, and plans and sections are found by file names, vision tables and metaplan headings. [section 01](product/section_01.md#s-5ac2db9f) [section 01](product/section_01.md#s-61f33e6f)

Bare `loom plan` starts the app on port 7272, holds a single server per project, and opens the browser unless told not to. [section 02](product/section_02.md#s-c310c992) [section 02](product/section_02.md#s-41d7cb80) [section 02](product/section_02.md#s-bca0a596)

It runs read-only when tmux, the `claude` CLI or a Claude Code login is missing, and it writes nothing into the project outside `doc/`. [section 02](product/section_02.md#s-12dc6647) [section 02](product/section_02.md#s-5669cb2b)

A document's kind comes from its path and file name, plans are linked to product sections only by `Implements:` lines, and the walk skips the summary store. [section 03](product/section_03.md#s-c244fb34) [section 03](product/section_03.md#s-4dda5adf) [section 03](product/section_03.md#s-51ede676)

## Summaries and reading

Summaries are committed Markdown files whose sentences cite hashed heading sections, so a changed source invalidates exactly the summaries built on it. [section 04](product/section_04.md#s-7a8a3626) [section 04](product/section_04.md#s-4cc77de4) [section 04](product/section_04.md#s-fda91c30)

Agents rebuild summaries through a draft and a seal command, document summaries run on Haiku and overviews on Sonnet, and rebuilds above five summaries ask first. [section 04](product/section_04.md#s-e3f5f94c) [section 04](product/section_04.md#s-15b0c10e) [section 04](product/section_04.md#s-17f4706c)

Concept mode opens a sentence in place to the sentences or passages it cites, and each sentence carries coloured marks that turn dashed while its source is being rebuilt. [section 05](product/section_05.md#s-fc476eb5) [section 05](product/section_05.md#s-59392957) [section 05](product/section_05.md#s-5766eecd)

The product and engineering overviews are linked only through `Implements:` lines, and an overview never renders half-built. [section 05](product/section_05.md#s-aa684ebb) [section 05](product/section_05.md#s-26049381) [section 05](product/section_05.md#s-8c13d1fe)

File mode is a reader first, showing every document under `doc/` in four groups and never rendering raw HTML. [section 06](product/section_06.md#s-1e8cae4d) [section 06](product/section_06.md#s-ac14ecff) [section 06](product/section_06.md#s-12f29d46)

Only the holder of authoring can edit, and a save is refused when the file's hash changed since the editor opened it. [section 06](product/section_06.md#s-b7ed34b2) [section 06](product/section_06.md#s-fd0294e0)

## Sessions and chat

The bridge runs authoring, summary and pressure sessions as interactive Claude Code in tmux, reading the transcript for messages and delivering input only while the session is idle. [section 07](product/section_07.md#s-9ec2ac4c) [section 07](product/section_07.md#s-839258cf) [section 07](product/section_07.md#s-c2b270bd)

Sessions start cold, never use `claude --resume`, and the bridge never answers a permission request itself. [section 07](product/section_07.md#s-7324413d) [section 07](product/section_07.md#s-ed6537bb)

The chat panel shows turns with Markdown replies, folded tool rows, a state line and a pinned permission card, and each turn footer lists changed files and rebuilt summaries. [section 08](product/section_08.md#s-1a5f2fe8) [section 08](product/section_08.md#s-e326e0cd) [section 08](product/section_08.md#s-95e3c8bd)

Each session gets a brief, an opening message and a plugin with the step skills and summary agents, and sessions record each decision in the document it binds as it is settled. [section 09](product/section_09.md#s-9be16dec) [section 09](product/section_09.md#s-ac5ca398) [section 09](product/section_09.md#s-b8d753b3)

After a turn that edits documents, the session checks conventions with `loom plan check` and rebuilds the affected summaries. [section 09](product/section_09.md#s-52ae24da) [section 09](product/section_09.md#s-ee3f0df9)

## Review and execution

A reviewer attaches a doubted passage to chat, the session edits the documents, and four views show the effect of the turn. [section 10](product/section_10.md#s-22124e74) [section 10](product/section_10.md#s-3882bf50)

There is no approve button, so sign-off happens through a pull request, and done or running plans stay read-only in a refinement. [section 10](product/section_10.md#s-586aaf9d) [section 10](product/section_10.md#s-9a8f184f)

The app runs a pressure test with the same pipeline as `loom pressure`, and one run holds the project's authoring slot until it ends. [section 11](product/section_11.md#s-f231f1cf) [section 11](product/section_11.md#s-dd99325a)

The Start button runs `loom init` and then `loom run`, and stage monitoring is left to the dashboard. [section 11](product/section_11.md#s-b3ed393d) [section 11](product/section_11.md#s-7bc23389)

## Access and safety

The access cookie is required on every API route and WebSocket, and a request that changes anything must also pass the Host, Origin and role checks. [section 12](product/section_12.md#s-b361a83b) [section 12](product/section_12.md#s-e64e186b)

Planning sessions read the repository but not credentials, write only the document paths in the write table, and are held by three layers of enforcement. [section 12](product/section_12.md#s-a08d307c) [section 12](product/section_12.md#s-f4725b53) [section 12](product/section_12.md#s-2827ae50)
