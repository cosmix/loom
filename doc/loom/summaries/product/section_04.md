---
kind: document
of: doc/product/section_04.md
file: sha256:0b3adf618ce696d20547fee58332fc5f0a2007186969badca69ed1bc36a50555
cites:
  "doc/product/section_04.md#asking-before-a-large-rebuild": "sha256:5fbdccb11be3464fbb9f9029a381263d963e7c089e0cdbc8130144d2419292d3"
  "doc/product/section_04.md#cost": "sha256:bc793e6ad7bb50fb6671aa456e06b3d246c8309a43e46695ea862028750e2a39"
  "doc/product/section_04.md#hashing": "sha256:cc0eaa5f7f4f384c4eb7d440abd0097f2310ff1d13e1094a945edf8a19b69fd8"
  "doc/product/section_04.md#sealing-a-summary": "sha256:980e0ac047534d60340c9fdc88c9c76a3707bc910d29836df5ae15b3cb4ff168"
  "doc/product/section_04.md#the-model-in-one-paragraph": "sha256:be8f6f8acb4144939ec53d970b9b413383d49674babaed889811da3a9ec6565b"
  "doc/product/section_04.md#the-store": "sha256:859798742260f682e27be1ac6896f046f9a9224a848c177f53b701112de27cb7"
  "doc/product/section_04.md#the-summary-file": "sha256:6ac55bc8f7202a185894d915d183dc470c32a82c68c7f8f64febcd6a7acb8a6c"
  "doc/product/section_04.md#the-watcher": "sha256:aaa58f3e2b97356965e6ef64808b39f127e2b8df538b32ff7b6503e086dddad2"
  "doc/product/section_04.md#what-out-of-date-means": "sha256:c525551c70e7bd89255e370ad62a62b6c544572ef024d96fff2fd445917d0bf7"
  "doc/product/section_04.md#who-rebuilds-what": "sha256:b9fff286ad81fe020b0829b15d43aeaa50490e160bdf4e94bf0f8977eb08f1c6"
---

# 04 — Summary Engine

Each summary is a Markdown file under the summaries directory, one sentence per line, and each sentence ends in links to what it draws on. [§](../../../product/section_04.md#the-model-in-one-paragraph)

The engine stores no state beyond the front matter manifest, and it compares cited hashes whenever a watcher change arrives. [§](../../../product/section_04.md#the-model-in-one-paragraph)

A summary's id is its path under the summaries directory without the `.md` extension. [§](../../../product/section_04.md#the-store)

Plan summaries are keyed by slug, so renames and moves into archive keep the same summary, and archived plans that no metaplan names get no summary. [§](../../../product/section_04.md#the-store)

A document summary cites only heading sections of its own source, and an overview cites document-summary sentences and raw passages. [§](../../../product/section_04.md#the-summary-file)

A sentence id is a short SHA-256 of its text with links removed, so a sentence kept word for word keeps its id. [§](../../../product/section_04.md#the-summary-file)

Hashes cover heading sections, and the engine also hashes the whole source file to notice uncited changes. [§](../../../product/section_04.md#hashing)

A summary is current when every sentence is current and its source hash matches, and a stale sentence stays readable and marked until a replacement arrives. [§](../../../product/section_04.md#what-out-of-date-means)

A summary file that fails validation counts as missing and is regenerated, which resolves the conflict. [§](../../../product/section_04.md#what-out-of-date-means)

The watcher polls `doc/` every second, compares size, modification time, and inode, and delivers changes as one batch. [§](../../../product/section_04.md#the-watcher)

Agents write sentences to a draft and run the seal command, which validates the body, computes every hash, and writes the file under a store lock. [§](../../../product/section_04.md#sealing-a-summary)

Brief creates a claim file per summary and refuses a summary that already has one. [§](../../../product/section_04.md#sealing-a-summary)

Document summaries run on Haiku at medium effort by default and overviews run on Sonnet at high effort, and config keys override both. [§](../../../product/section_04.md#who-rebuilds-what)

Rebuilds above five summaries ask first and show an estimated cost. [§](../../../product/section_04.md#asking-before-a-large-rebuild)

Cost is measured from transcript token counts and recorded per rebuild in a runtime log. [§](../../../product/section_04.md#cost)
