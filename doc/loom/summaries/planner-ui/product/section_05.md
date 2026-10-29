---
kind: document
of: doc/product/planner-ui/section_05.md
file: sha256:ae99af1c4fabde627d67f26047cfae92266e6668182bcd48b354d305ac292dff
cites:
  "doc/product/planner-ui/section_05.md#document-summary-pages": "sha256:7f9faafa5450252c3ae3f20c2e939125207296cfbee60c54c2d576225c1da107"
  "doc/product/planner-ui/section_05.md#empty-and-partial-states": "sha256:0d37db1363d9c79d9d0601240aa42df703584338bc3eff773719cda1725c24da"
  "doc/product/planner-ui/section_05.md#folding-back": "sha256:f28c3b11e70b431c19f1cb736d0c34c59f10e7de1fc4ece612f44db30cde998c"
  "doc/product/planner-ui/section_05.md#keyboard": "sha256:a6b8a25336268688b6393028df0de9aa63f5c53cddf22907f65e73930f04e654"
  "doc/product/planner-ui/section_05.md#levels": "sha256:8c978debdeb4733ee9cd01f65681efd6e2788f84aaacabed5acc27cae9c44124"
  "doc/product/planner-ui/section_05.md#opening": "sha256:fca93486bcce6d6cdcd250827a06d31704ab7878e9fc59abd9202f7312b9e6c8"
  "doc/product/planner-ui/section_05.md#product-to-plan-links": "sha256:778b92451acbf3b7de6055b6d1373d06a2f4602e50788e3e6078ffb7c4edbdf9"
  "doc/product/planner-ui/section_05.md#rebuilds-while-reading": "sha256:f816da1f309a1f14a715a1f8fadd6b927791b7b38c73e5e9ce3b920d9a85eb3a"
  "doc/product/planner-ui/section_05.md#selecting-text-for-chat": "sha256:4da6015e20c755d524c26161377c182e334b12023331026a653151d8bae976f8"
  "doc/product/planner-ui/section_05.md#sentences-and-their-marks": "sha256:3546e6d8e7fc8aa88fcdfd4f6da38734732d0f015ca06d9e3ecd4a1e31fb024b"
  "doc/product/planner-ui/section_05.md#several-viewers": "sha256:42fc8ead1751db911b750b496bf98505103bf7286f82eb6b480e9423d9125194"
  "doc/product/planner-ui/section_05.md#the-two-overviews": "sha256:2d24da829993cf24944e2b2fdbf79ec233b058311d388bf5074ed7482cd74b03"
  "doc/product/planner-ui/section_05.md#what-concept-mode-needs-from-the-summary-data": "sha256:772292797f99d7161d6ce2ffced369d5207db4306f149940242db839eb0fde55"
  "doc/product/planner-ui/section_05.md#what-stays-open": "sha256:8dfbc5fb1cdc04d766d5ded820d0c164049c64c2968d75c30812169bce98a843"
---

# Concept Mode

## Levels

Concept mode shows three levels of text, each a step closer to the files. [Levels](../../../../product/planner-ui/section_05.md#levels)

## The two overviews

The product and engineering overviews are level 0 text with a document key above them. [The two overviews](../../../../product/planner-ui/section_05.md#the-two-overviews)

Plans that no metaplan names are listed under the engineering overview as unlisted plans. [The two overviews](../../../../product/planner-ui/section_05.md#the-two-overviews)

## Document summary pages

Each document summary is also readable on its own page, with a header showing whether it is up to date. [Document summary pages](../../../../product/planner-ui/section_05.md#document-summary-pages)

## Sentences and their marks

Every summary sentence ends with one coloured mark per document it draws on. [Sentences and their marks](../../../../product/planner-ui/section_05.md#sentences-and-their-marks)

Hovering a mark shows the document's title and path, so colour never names a document alone. [Sentences and their marks](../../../../product/planner-ui/section_05.md#sentences-and-their-marks)

## Opening and folding

Clicking a summary sentence opens it in place to the sentences or passages it cites. [Opening](../../../../product/planner-ui/section_05.md#opening) [Folding back](../../../../product/planner-ui/section_05.md#folding-back)

## Keyboard

Arrow keys and `j` or `k` move the roving focus over sentences, and `Escape` folds the innermost open run. [Keyboard](../../../../product/planner-ui/section_05.md#keyboard)

## Rebuilds while reading

When a source file changes, the marks of sentences built on it turn dashed at once. [Rebuilds while reading](../../../../product/planner-ui/section_05.md#rebuilds-while-reading)

Open runs stay open across a rebuild when a sentence at the same level rests on any source section the old one did. [What stays open](../../../../product/planner-ui/section_05.md#what-stays-open)

## Product to plan links

Links between the overviews come only from `Implements:` lines in plans. [Product to plan links](../../../../product/planner-ui/section_05.md#product-to-plan-links)

## Selecting text for chat

Selecting text at any level shows an Add to chat button, and `a` attaches the focused sentence. [Selecting text for chat](../../../../product/planner-ui/section_05.md#selecting-text-for-chat)

## Empty and partial states

An overview never renders half-built, so it shows its last complete version with dashed marks. [Empty and partial states](../../../../product/planner-ui/section_05.md#empty-and-partial-states)

## Several viewers

Read-only browsers see the same overviews, marks and rebuild flashes as the author. [Several viewers](../../../../product/planner-ui/section_05.md#several-viewers)

## What concept mode needs from the summary data

Concept mode needs a stable id, text and citations for each sentence, plus the up-to-date state and reason per summary. [What concept mode needs from the summary data](../../../../product/planner-ui/section_05.md#what-concept-mode-needs-from-the-summary-data)
