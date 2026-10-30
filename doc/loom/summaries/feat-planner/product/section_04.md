---
kind: document
of: doc/product/feat-planner/section_04.md
file: sha256:35809c1095a5961ea8a1e7a3692d8333bef62d220ea76db69d6a234a0fd68dca
cites:
  "doc/product/feat-planner/section_04.md#asking-before-a-large-rebuild": "sha256:179b382fa17da7481c486b54298906ea21c8fbf9b15eb403b275b2b7884eb598"
  "doc/product/feat-planner/section_04.md#cost": "sha256:bc793e6ad7bb50fb6671aa456e06b3d246c8309a43e46695ea862028750e2a39"
  "doc/product/feat-planner/section_04.md#hashing": "sha256:cc0eaa5f7f4f384c4eb7d440abd0097f2310ff1d13e1094a945edf8a19b69fd8"
  "doc/product/feat-planner/section_04.md#sealing-a-summary": "sha256:980e0ac047534d60340c9fdc88c9c76a3707bc910d29836df5ae15b3cb4ff168"
  "doc/product/feat-planner/section_04.md#the-front-matter": "sha256:39aa73b8dbcc46e2b23db3e1f8854ea3b29b8680d2b5cf8b412758f65fb2340d"
  "doc/product/feat-planner/section_04.md#the-model-in-one-paragraph": "sha256:be8f6f8acb4144939ec53d970b9b413383d49674babaed889811da3a9ec6565b"
  "doc/product/feat-planner/section_04.md#the-store": "sha256:417910f2bd06529bfb98ca599f8c0a47d48dc06b05ba02bd1c52e72e903130ab"
  "doc/product/feat-planner/section_04.md#the-summary-file": "sha256:3f47b1bdf5758f98d84edd0fd36852a0ffd43ae904ea6a8a75fcb8eff8aaa096"
  "doc/product/feat-planner/section_04.md#the-watcher": "sha256:aaa58f3e2b97356965e6ef64808b39f127e2b8df538b32ff7b6503e086dddad2"
  "doc/product/feat-planner/section_04.md#what-out-of-date-means": "sha256:321933f5cc98a62189f076cf40fa79d97bb097c01e2018a91af0139c24316489"
  "doc/product/feat-planner/section_04.md#who-rebuilds-what": "sha256:17ac66d5814b44b7a77787e2fc435eac1af720f61d7842a2cb51a44105e97200"
---

# 04 — Summary Engine

## The model in one paragraph

Every summary is a Markdown file under `doc/loom/summaries/`, one sentence per line, with a front matter manifest of content hashes taken when it was written. [§](../../../../product/feat-planner/section_04.md#the-model-in-one-paragraph)
The engine stores no other state and rehashes sources on start and on every watcher change to find out-of-date sentences. [§](../../../../product/feat-planner/section_04.md#the-model-in-one-paragraph)

## The store

A summary id is its path under `doc/loom/summaries/` without `.md`, such as `plans/delivery-truth`. [§](../../../../product/feat-planner/section_04.md#the-store)
Plan summaries are keyed by slug, so lifecycle renames and moves keep the same id while the plan stays eligible. [§](../../../../product/feat-planner/section_04.md#the-store)

## The summary file

Each document summary cites only heading sections of its own source file, and a link without a fragment cites the text before the first heading. [§](../../../../product/feat-planner/section_04.md#the-summary-file)
A sentence id is `s-` plus the first 8 hex digits of the SHA-256 of the sentence text with its links removed and its whitespace collapsed, and the id is never written into the file. [§](../../../../product/feat-planner/section_04.md#the-summary-file)

## The front matter

`members` hashes the ordered sentence ids, so rewriting a link in a document summary does not make its overview out of date. [§](../../../../product/feat-planner/section_04.md#the-front-matter)

## Hashing

The unit of hashing is the heading section, and line endings and trailing whitespace are normalized before hashing. [§](../../../../product/feat-planner/section_04.md#hashing)

## What "out of date" means

A summary is missing when its file is absent, fails validation, or has git conflict markers, and missing summaries are regenerated. [§](../../../../product/feat-planner/section_04.md#what-out-of-date-means)

## The watcher

The watcher polls `doc/` every second and hashes only files whose size, modification time, or inode changed. [§](../../../../product/feat-planner/section_04.md#the-watcher)

## Sealing a summary

The store lock is an exclusive OS lock held only for the write, so one writer at a time holds across the server, its sessions, and a hand-run seal. [§](../../../../product/feat-planner/section_04.md#sealing-a-summary)

## Who rebuilds what

Background rebuilds start at once for up to 5 out-of-date summaries, and above 5 the app asks first. [§](../../../../product/feat-planner/section_04.md#who-rebuilds-what)
Document summaries run on Haiku at high effort by default, and overviews and the background summary session also default to Haiku at high effort. [§](../../../../product/feat-planner/section_04.md#who-rebuilds-what)

## Asking before a large rebuild

Declining leaves summaries marked out of date with a rebuild action, and the next batch counts only newly invalidated summaries. [§](../../../../product/feat-planner/section_04.md#asking-before-a-large-rebuild)

## Cost

Each rebuild's cost is read from its Claude Code transcript with the token tracker's parser, so the counts match `loom status`. [§](../../../../product/feat-planner/section_04.md#cost)
