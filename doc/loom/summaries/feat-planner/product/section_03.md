---
kind: document
of: doc/product/feat-planner/section_03.md
file: sha256:18b933c7ac5ee30a3f32ba1758de3c93963da92615ee0a865455696ec5dac412
cites:
  "doc/product/feat-planner/section_03.md#briefs": "sha256:e9720fd34f5531e78ca733122b0677f39d8083bd323cb2c5ea6a6527ee82f0f2"
  "doc/product/feat-planner/section_03.md#cross-references": "sha256:c523bb1b5ef070b3cf9dbd77c8ea7d2640664e3110f35566665237616034b8ab"
  "doc/product/feat-planner/section_03.md#discovery": "sha256:56c50ab31c5b6e989c43012bccf6635d94179fef5136da9bc82dc92723832eab"
  "doc/product/feat-planner/section_03.md#document-kinds": "sha256:e20bf9e65bbf2e92ef4cc7671158caaefe8798a3128c5ec3bdecbb5f620ace44"
  "doc/product/feat-planner/section_03.md#headings-as-anchors": "sha256:1a3d00a0c3c212de093423fd16ef0e49489d19be0ccecd0093cb4f32718ec47b"
  "doc/product/feat-planner/section_03.md#metaplans": "sha256:26e5c2ab5c4875f03e16c16ff0e25783ac7d835f2e21decd8958b34e753d502c"
  "doc/product/feat-planner/section_03.md#plans-and-their-lifecycle": "sha256:aca8ea954ae51d6cba465aa69e034e97de9e66a69513985a5411190dcfd2de37"
  "doc/product/feat-planner/section_03.md#product-sections": "sha256:2c75ae06f3fa9f654344c356ba7461c2803ca62ba4bbd83087105357ac7673d3"
  "doc/product/feat-planner/section_03.md#product-to-plan-links": "sha256:8294407e0acb0cfb2237914b2e21f8b0e2ea4fecea4c781788cb78ae038874d5"
  "doc/product/feat-planner/section_03.md#run-reviews": "sha256:13e448f4c8097248d78f2b051c0482af1bb279aa6791fc7807ff8a7f243caad6"
  "doc/product/feat-planner/section_03.md#the-knowledge-base": "sha256:f3f471ae1a582f6bcd2549b0419a3d2e7296940f75574b624e2137460b5b63b7"
  "doc/product/feat-planner/section_03.md#the-model-in-one-paragraph": "sha256:56bdd68f1c59bb8ff883d3ca2e9082a25849e68af9397399c9da3c6a8be30e58"
---

# 03 Document Model and Conventions

A document's kind comes from its path and file name alone, with no front matter, marker comment, or sidecar file [The model in one paragraph](../../../../product/feat-planner/section_03.md#the-model-in-one-paragraph)

Parent links run one way, from the document that lists to the document listed, and a plan names the product sections it implements in an `Implements:` line [The model in one paragraph](../../../../product/feat-planner/section_03.md#the-model-in-one-paragraph)

The server walks `doc/` at start and on every change the watcher reports, and a feature's membership comes from a directory name shared across `doc/product/` and `doc/plans/` [Discovery](../../../../product/feat-planner/section_03.md#discovery)

The kinds table maps each path pattern to a kind and a parent, and documents under an invalid feature name are `other` with a `bad-feature-name` problem [Document kinds](../../../../product/feat-planner/section_03.md#document-kinds)

A vision or section placed at the top of `doc/product/`, or a metaplan at the top of `doc/plans/`, keeps its kind but raises `needs-feature` [Document kinds](../../../../product/feat-planner/section_03.md#document-kinds)

A product section's number identifies it within its feature, and file mode shows both titles when the section's heading differs from the vision's table [Product sections](../../../../product/feat-planner/section_03.md#product-sections)

A plan's slug is its file stem without lifecycle prefixes, and every spelling of one slug identifies the same plan [Plans and their lifecycle](../../../../product/feat-planner/section_03.md#plans-and-their-lifecycle)

When two files share a slug, the most advanced state wins, and a losing copy raises `stray-plan-copy` or `duplicate-plan-slug` depending on where it sits [Plans and their lifecycle](../../../../product/feat-planner/section_03.md#plans-and-their-lifecycle)

A metaplan entry names its plan in backticks, and an entry naming a plan from another feature raises `metaplan-entry-foreign-plan` [Metaplans](../../../../product/feat-planner/section_03.md#metaplans)

A plan's briefs are its conventional bundle plus every brief path token it names, found recursively, so the app lists the same briefs `loom run` checks [Briefs](../../../../product/feat-planner/section_03.md#briefs)

Run reviews attach to their plan by slug, appear in file mode, and are never summarized or edited by planning sessions [Run reviews](../../../../product/feat-planner/section_03.md#run-reviews)

Knowledge files are read-only in the app and change only through `loom knowledge`, and `INDEX.md` carries a regeneration notice [The knowledge base](../../../../product/feat-planner/section_03.md#the-knowledge-base)

A plan lists the product sections it implements in one `Implements:` line, where `none` declares that it implements no section [Product-to-plan links](../../../../product/feat-planner/section_03.md#product-to-plan-links)

A heading section runs to the next heading of the same or a higher level, and its anchor is the GitHub slug with a numeric suffix for repeats [Headings as anchors](../../../../product/feat-planner/section_03.md#headings-as-anchors)

Cross-references resolve as relative Markdown links, repository path tokens, or plan file names, and an unresolved one is listed as broken without blocking any step [Cross-references](../../../../product/feat-planner/section_03.md#cross-references)
