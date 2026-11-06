---
kind: document
of: doc/product/feat-planner/section_04.md
file: sha256:d358645c1f0c11efda46a2090a89710ae70115c73e03e8d975f0cd91eeef6e9d
cites:
  "doc/product/feat-planner/section_04.md#asking-before-a-large-rebuild": "sha256:179b382fa17da7481c486b54298906ea21c8fbf9b15eb403b275b2b7884eb598"
  "doc/product/feat-planner/section_04.md#cost": "sha256:991377d7e27023c1bd457beaecaf9d8422f9a00969a69dc85c5ce2e8e4277ac3"
  "doc/product/feat-planner/section_04.md#hashing": "sha256:cc0eaa5f7f4f384c4eb7d440abd0097f2310ff1d13e1094a945edf8a19b69fd8"
  "doc/product/feat-planner/section_04.md#sealing-a-summary": "sha256:980e0ac047534d60340c9fdc88c9c76a3707bc910d29836df5ae15b3cb4ff168"
  "doc/product/feat-planner/section_04.md#the-front-matter": "sha256:39aa73b8dbcc46e2b23db3e1f8854ea3b29b8680d2b5cf8b412758f65fb2340d"
  "doc/product/feat-planner/section_04.md#the-model-in-one-paragraph": "sha256:be8f6f8acb4144939ec53d970b9b413383d49674babaed889811da3a9ec6565b"
  "doc/product/feat-planner/section_04.md#the-store": "sha256:fe60f298328eb1d996a9b6ad70cb52b06b737db774f5ffc095d36f67b27e7749"
  "doc/product/feat-planner/section_04.md#the-summary-file": "sha256:4da2a17c3d174119e26f7aa5196e81c1c19572f15e72bb2117d8cc94c7317e07"
  "doc/product/feat-planner/section_04.md#the-watcher": "sha256:aaa58f3e2b97356965e6ef64808b39f127e2b8df538b32ff7b6503e086dddad2"
  "doc/product/feat-planner/section_04.md#what-out-of-date-means": "sha256:321933f5cc98a62189f076cf40fa79d97bb097c01e2018a91af0139c24316489"
  "doc/product/feat-planner/section_04.md#who-rebuilds-what": "sha256:374338c4f002684e7d9592517fd603098874272369ad116b441c08f9aa5ba387"
---

# 04 — Summary Engine

## The model in one paragraph

Every summary is a Markdown file under `doc/loom/summaries/`, one sentence per line, with a front matter manifest of content hashes taken when it was written. [§](../../../../product/feat-planner/section_04.md#the-model-in-one-paragraph)
The engine stores no other state and rehashes sources on start and on every watcher change to find out-of-date sentences. [§](../../../../product/feat-planner/section_04.md#the-model-in-one-paragraph)

## The store

A summary id is its path under `doc/loom/summaries/` without `.md`, such as `plans/delivery-truth`. [§](../../../../product/feat-planner/section_04.md#the-store)
Plan summaries are keyed by slug, unique repository-wide, so renames and moves between homes keep the same id, and a move into an archive keeps the summary only while the plan is eligible. [§](../../../../product/feat-planner/section_04.md#the-store)

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

After each watcher batch, the engine starts a background rebuild at once for up to 5 summaries the batch made stale, incomplete, or missing, and above 5 it asks first. [§](../../../../product/feat-planner/section_04.md#who-rebuilds-what)
Document summaries are built on Haiku at high effort by default, and overviews and the background summary session default to Sonnet at high effort. [§](../../../../product/feat-planner/section_04.md#who-rebuilds-what)

## Asking before a large rebuild

Declining leaves summaries marked out of date with a rebuild action, and the next batch counts only newly invalidated summaries. [§](../../../../product/feat-planner/section_04.md#asking-before-a-large-rebuild)

## Cost

Each rebuild's cost is read from its Claude Code transcript with the token tracker's parser and its streamed-message merge, so the counts match `loom status`. [§](../../../../product/feat-planner/section_04.md#cost)
