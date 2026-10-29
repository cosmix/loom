---
kind: document
of: doc/product/planner-ui/section_03.md
file: sha256:cd9f1eae4031ac4ae3f66f685127a4d42075dc80df1f04be4c056a032bcb6876
cites:
  "doc/product/planner-ui/section_03.md#briefs": "sha256:48a2ee763acd96cd042f1fc8f969e69e9307427690213d26765288e3c2330767"
  "doc/product/planner-ui/section_03.md#cross-references": "sha256:7da932fe9c1048fa44bc11b4a197e48d6d83b4649bdfcabc65737c034f5e3a47"
  "doc/product/planner-ui/section_03.md#discovery": "sha256:5f19fd0e01d339836d6722a3e490f5d53165923bd8d312522e36b4cf8127a6a8"
  "doc/product/planner-ui/section_03.md#document-kinds": "sha256:79505546a5232bfa6bb1821624e1e8c400fde14dc0bbe84ff82a2ba6833a07bc"
  "doc/product/planner-ui/section_03.md#headings-as-anchors": "sha256:d69ef0dad30bba2c79f948e9e5b5f62700f984dab3216116833d0e6edb8503b9"
  "doc/product/planner-ui/section_03.md#metaplans": "sha256:de09aa3cfea85467298f2cf5b87ad41f3781c81381833fde91fca7c881da2bf0"
  "doc/product/planner-ui/section_03.md#plans-and-their-lifecycle": "sha256:9aa56f6db80984a03bab7ee19d8b23035e635db12ed37a8c9827cea6f123e9f1"
  "doc/product/planner-ui/section_03.md#product-sections": "sha256:b4af7fb05a9a3f67ef725c93b41eb0561f9ceda060b64dd763648e2120291fcc"
  "doc/product/planner-ui/section_03.md#product-to-plan-links": "sha256:6274fd16d0bcf13f39cc21958447b83af92353a39fffdb0eac18f69ddb568fa8"
  "doc/product/planner-ui/section_03.md#run-reviews": "sha256:13e448f4c8097248d78f2b051c0482af1bb279aa6791fc7807ff8a7f243caad6"
  "doc/product/planner-ui/section_03.md#the-model-in-one-paragraph": "sha256:56bdd68f1c59bb8ff883d3ca2e9082a25849e68af9397399c9da3c6a8be30e58"
---

# Document Model and Conventions

## The model

A document's kind comes from its path and file name alone, with no front matter, marker comment or sidecar file. [The model in one paragraph](../../../../product/planner-ui/section_03.md#the-model-in-one-paragraph)

Parent links run one way, from the document that lists to the document listed, except that a plan names product sections in an `Implements:` line. [The model in one paragraph](../../../../product/planner-ui/section_03.md#the-model-in-one-paragraph)

## Discovery and kinds

The server walks `doc/` at start and on every file watcher change, and it reads files that git ignores. [Discovery](../../../../product/planner-ui/section_03.md#discovery)

The walk skips dotfiles, the summary store `doc/loom/summaries/`, and symlinks other than regular files inside `doc/`, and it always skips symlinked directories. [Discovery](../../../../product/planner-ui/section_03.md#discovery)

The kinds table makes the vision the root of the product tree and metaplans the root of the engineering tree. [Document kinds](../../../../product/planner-ui/section_03.md#document-kinds)

A section file the vision's sections table does not list is an unlisted section, and a title that differs from the table's name is shown in file mode. [Product sections](../../../../product/planner-ui/section_03.md#product-sections)

## Plans and briefs

A plan's slug is its file stem with the lifecycle prefixes stripped, and the brief lookup gains `RETIRED-` so the app and `loom run` agree on every slug. [Plans and their lifecycle](../../../../product/planner-ui/section_03.md#plans-and-their-lifecycle)

If two files carry one slug, the most advanced state wins and file mode flags the others as stray copies. [Plans and their lifecycle](../../../../product/planner-ui/section_03.md#plans-and-their-lifecycle)

A metaplan entry is a level-2 heading that names a plan file in backticks, and a plan named by two metaplans has both as parents. [Metaplans](../../../../product/planner-ui/section_03.md#metaplans)

A plan's briefs are the files under its bundle directory, plus the brief paths the plan and the briefs already found name. [Briefs](../../../../product/planner-ui/section_03.md#briefs)

A bundle whose slug matches no plan is an orphan bundle, listed in file mode under its own heading. [Briefs](../../../../product/planner-ui/section_03.md#briefs)

`loom run` writes a run review as a `REVIEW-` file named for the plan file stem, which the app attaches by slug and never summarizes or edits. [Run reviews](../../../../product/planner-ui/section_03.md#run-reviews)

## Links and anchors

A plan names the product sections it implements in an `Implements:` line, and `none` marks a plan that implements no section. [Product-to-plan links](../../../../product/planner-ui/section_03.md#product-to-plan-links)

The app recognises relative Markdown links, repository path tokens and plan file names as cross-references, and a reference that does not resolve is listed as broken without stopping any step. [Cross-references](../../../../product/planner-ui/section_03.md#cross-references)

A heading section runs from its heading to the line before the next heading of the same or a higher level, and its anchor follows GitHub's slug rules. [Headings as anchors](../../../../product/planner-ui/section_03.md#headings-as-anchors)
