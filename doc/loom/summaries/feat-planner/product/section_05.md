---
kind: document
of: doc/product/feat-planner/section_05.md
file: sha256:01eea572800e6ac67dfc0b7b08ef2f2bc63c993fca7823fbd55c75ff54d86431
cites:
  "doc/product/feat-planner/section_05.md#document-summary-pages": "sha256:c0013de4e6355d37bfc699cd10bdd4397dd3f7d8a952e9204e05e1a7ed9ec3ea"
  "doc/product/feat-planner/section_05.md#empty-and-partial-states": "sha256:999423b5c11c617864adee32baa67f586582311f93b3b82ad8d97e5c9e6b3a5b"
  "doc/product/feat-planner/section_05.md#folding-back": "sha256:7e6416b94780187a6b180806a7850f5a22cb5cd263bdf8244a00744659f429a4"
  "doc/product/feat-planner/section_05.md#keyboard": "sha256:77ae1c7f3fbc11bf3657956152f95e2886b232431bc50a99fa79c01ebfab146e"
  "doc/product/feat-planner/section_05.md#levels": "sha256:8c978debdeb4733ee9cd01f65681efd6e2788f84aaacabed5acc27cae9c44124"
  "doc/product/feat-planner/section_05.md#opening": "sha256:d08a0fe5c58f79daafc57135081cc4b1de1e9b7aea931989fba323fb7bff5a86"
  "doc/product/feat-planner/section_05.md#product-to-plan-links": "sha256:6822d84d7d1e2eacc6dda66a024ef07cd8cc7f40a271f00d3d5da0ce955169db"
  "doc/product/feat-planner/section_05.md#rebuilds-while-reading": "sha256:f816da1f309a1f14a715a1f8fadd6b927791b7b38c73e5e9ce3b920d9a85eb3a"
  "doc/product/feat-planner/section_05.md#sentences-and-their-marks": "sha256:3546e6d8e7fc8aa88fcdfd4f6da38734732d0f015ca06d9e3ecd4a1e31fb024b"
  "doc/product/feat-planner/section_05.md#the-two-overviews": "sha256:3ba211debcb0e033c9c74216855f9ec680d1889f5f16e998f7410c757008e433"
  "doc/product/feat-planner/section_05.md#what-stays-open": "sha256:8dfbc5fb1cdc04d766d5ded820d0c164049c64c2968d75c30812169bce98a843"
---

# Concept mode

## Levels

Concept mode has three levels of text, an overview, a document summary and a source passage, each a step closer to the files. [Levels](../../../../product/feat-planner/section_05.md#levels)
The vision and the metaplan have no document summary, so an overview sentence that draws on either opens straight to its source passage. [Levels](../../../../product/feat-planner/section_05.md#levels)

## Overviews and summary pages

The product overview is built from the feature's index and the sections it names, and the engineering overview is built from its metaplans and their plans. [The two overviews](../../../../product/feat-planner/section_05.md#the-two-overviews)
Each overview has a document key with one chip per document, and a chip opens that document's summary page. [The two overviews](../../../../product/feat-planner/section_05.md#the-two-overviews)
Every document summary has its own page, with a header giving its title, path, colour and whether it is up to date. [Document summary pages](../../../../product/feat-planner/section_05.md#document-summary-pages)

## Marks and opening

Every summary sentence ends with one coloured mark per document it draws on, and a mark turns dashed when that document's summary is out of date. [Sentences and their marks](../../../../product/feat-planner/section_05.md#sentences-and-their-marks)
Clicking a summary sentence opens it in place to the sentences or passages it cites, and a fold cap at either end folds it back. [Opening](../../../../product/feat-planner/section_05.md#opening) [Folding back](../../../../product/feat-planner/section_05.md#folding-back)
The keyboard reads the column with roving focus, and Shift+Esc folds every open run on the page. [Keyboard](../../../../product/feat-planner/section_05.md#keyboard)

## Rebuilds and links

When a source file changes, its marks turn dashed at once, and changed sentences replace the old ones with a single flash when the rebuilt summary arrives. [Rebuilds while reading](../../../../product/feat-planner/section_05.md#rebuilds-while-reading)
An open run stays open across a rebuild when a new sentence rests on the same source sections, and otherwise folds back with a muted note. [What stays open](../../../../product/feat-planner/section_05.md#what-stays-open)
Links between product sections and plans come only from Implements lines and are never inferred from text. [Product to plan links](../../../../product/feat-planner/section_05.md#product-to-plan-links)

## Empty and partial states

An overview never renders half-built, and it shows its last complete version with dashed marks or the building state. [Empty and partial states](../../../../product/feat-planner/section_05.md#empty-and-partial-states)
