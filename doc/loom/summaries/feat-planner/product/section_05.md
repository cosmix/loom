---
kind: document
of: doc/product/feat-planner/section_05.md
file: sha256:b90049b2d9e846877c3b390af7c7dc4e98b4a27da372da869faf4680754cdc1d
cites:
  "doc/product/feat-planner/section_05.md#document-summary-pages": "sha256:c053356791b6432210713f9734193b4f940dbbefffc74aac288d78fcabe8b0bd"
  "doc/product/feat-planner/section_05.md#empty-and-partial-states": "sha256:9eb4fc8e081bb95a3ffb31ffedf8b13d431bf08f34166e8b98e3b1b75e2d4902"
  "doc/product/feat-planner/section_05.md#folding-back": "sha256:7e6416b94780187a6b180806a7850f5a22cb5cd263bdf8244a00744659f429a4"
  "doc/product/feat-planner/section_05.md#keyboard": "sha256:77ae1c7f3fbc11bf3657956152f95e2886b232431bc50a99fa79c01ebfab146e"
  "doc/product/feat-planner/section_05.md#levels": "sha256:17f117a05e66084707097ed043b2dd0e01d90bce6466bd00ee79a5f0b52ef6f0"
  "doc/product/feat-planner/section_05.md#product-to-plan-links": "sha256:dee5c87b10b70f23223a454010f81f5b38ac9fb9f05482f125d729a2464b1530"
  "doc/product/feat-planner/section_05.md#rebuilds-while-reading": "sha256:f816da1f309a1f14a715a1f8fadd6b927791b7b38c73e5e9ce3b920d9a85eb3a"
  "doc/product/feat-planner/section_05.md#sentences-and-their-marks": "sha256:3546e6d8e7fc8aa88fcdfd4f6da38734732d0f015ca06d9e3ecd4a1e31fb024b"
  "doc/product/feat-planner/section_05.md#the-two-overviews": "sha256:1d113f2a503d4506d9d1707212f3c42d8dc2fdd055cfffc7bac6681e516d047d"
  "doc/product/feat-planner/section_05.md#what-is-clickable": "sha256:acecfb116424f599eba9652dde2a28556862a8f0f83f6de4cf1c4f6706aa158d"
  "doc/product/feat-planner/section_05.md#what-stays-open": "sha256:8dfbc5fb1cdc04d766d5ded820d0c164049c64c2968d75c30812169bce98a843"
---

# Concept mode

## Levels

Concept mode reads text at three levels, from an overview through a document summary down to the source passage it cites. [Levels](../../../../product/feat-planner/section_05.md#levels)

The vision has its own document summary, and a product overview sentence that draws on the vision opens directly to the vision's source passages. [Levels](../../../../product/feat-planner/section_05.md#levels)

Each metaplan has its own document summary, and an engineering overview sentence that draws on a metaplan opens directly to its source passage. [Levels](../../../../product/feat-planner/section_05.md#levels)

## Overviews and summary pages

The product overview is assembled from the feature's index and the sections it names, and the engineering overview from the feature's metaplan and its plans. [The two overviews](../../../../product/feat-planner/section_05.md#the-two-overviews)

Each overview opens with a document key holding one chip per document, and each chip opens that document's summary page. [The two overviews](../../../../product/feat-planner/section_05.md#the-two-overviews)

Every document summary has its own page, with a header giving its title, path, colour and whether it is up to date. [Document summary pages](../../../../product/feat-planner/section_05.md#document-summary-pages)

## Marks and opening

Every summary sentence ends with one coloured mark per document it draws on, and a mark turns dashed when that document's summary is out of date. [Sentences and their marks](../../../../product/feat-planner/section_05.md#sentences-and-their-marks)

Clicking anywhere on a summary sentence opens it in place to the sentences or passages it cites, and a fold cap at either end folds the run back to the sentence it replaced. [What is clickable](../../../../product/feat-planner/section_05.md#what-is-clickable) [Folding back](../../../../product/feat-planner/section_05.md#folding-back)

The keyboard reads the column with roving focus, and Shift+Esc folds every open run on the page. [Keyboard](../../../../product/feat-planner/section_05.md#keyboard)

## Rebuilds and links

When a source file changes, its marks turn dashed at once, and changed sentences replace the old ones with a single flash when the rebuilt summary arrives. [Rebuilds while reading](../../../../product/feat-planner/section_05.md#rebuilds-while-reading)

An open run stays open across a rebuild when a new sentence rests on the same source sections, and otherwise folds back with a muted note. [What stays open](../../../../product/feat-planner/section_05.md#what-stays-open)

Links between product sections and plans come only from Implements lines and are never inferred from text. [Product to plan links](../../../../product/feat-planner/section_05.md#product-to-plan-links)

## Empty and partial states

An overview never renders half-built, and it shows its last complete version with dashed marks, or the building state when there is none. [Empty and partial states](../../../../product/feat-planner/section_05.md#empty-and-partial-states)
