---
kind: document
of: doc/product/section_01.md
file: sha256:3e13b76c1f5977c0f51834b6d47011e2a5634d4bb5f7e7282e51ec45f10285f0
cites:
  "doc/product/section_01.md#metaplans": "sha256:83f4b6d2751f8d0edd8ce7be477a6bd4f5a198611c13ed79878ec3090a583a6f"
  "doc/product/section_01.md#plans": "sha256:3af55f4d6857825bd721d063fd0f0dec59b8d2fe2b34513e92ea744de3abf0cb"
  "doc/product/section_01.md#pressure-tests": "sha256:2237c4f3c1d4bea75bd99a8fc0c22b8568a85fe30cc597feec4f866642d0394b"
  "doc/product/section_01.md#product-sections": "sha256:2034bb9504eacf0af3710ed36289e1c2ce2bf37bb9db20ae5822c64012914be6"
  "doc/product/section_01.md#skipped-and-reordered-steps": "sha256:d5bfb04b6a6b4c1966fa6418dfd3e663f2ee53c9fcb740a0a40ca8e045c7a841"
  "doc/product/section_01.md#the-model-in-one-paragraph": "sha256:e21275627e37943457dece099762921a18f57d5f1245289271d7127017b795fd"
  "doc/product/section_01.md#the-seven-steps": "sha256:97955d5858dd3c4b67c87601f935f4625df3847b5cb88b30d71101df1b6223dd"
  "doc/product/section_01.md#vision": "sha256:c2d3d0981baf40d1bb14744334b19933fa37f9c622a3b18d48def332c9975a78"
  "doc/product/section_01.md#what-the-app-offers": "sha256:be144693c03a3dad978a2b22b9e6385528edc19ec1509dcb560b2bbe944410ac"
  "doc/product/section_01.md#when-the-state-is-recomputed": "sha256:647e2d67665913422b9e4295c4708cd4a8060912b4f17d34c07ee588c728d4b5"
---

# 01 Workflow Model and Project State

## The model

The app stores no workflow state and builds a project checklist of present or missing documents from the tree under `doc/`. [The model](../../../product/section_01.md#the-model-in-one-paragraph)

The current step is the earliest step with a gap, and a gap in an early step never blocks a later one. [The model](../../../product/section_01.md#the-model-in-one-paragraph)

## Steps

The seven steps run from vision and product spec through metaplan, plans, pressure test, review, and run. [The seven steps](../../../product/section_01.md#the-seven-steps)

Step 5 is optional and never holds the project back, so an untested plan is a suggestion rather than a gap. [The seven steps](../../../product/section_01.md#the-seven-steps)

Steps 6 and 7 share the ready state, and step 7 is active while any plan is `IN_PROGRESS-`. [The seven steps](../../../product/section_01.md#the-seven-steps)

## Detection

The vision lists sections in the first table under a `Specification sections` heading, and row `01` expects `section_01.md`. [Vision](../../../product/section_01.md#vision)

A listed section is present when its file exists and is not empty, and its open questions do not make it a gap. [Product sections](../../../product/section_01.md#product-sections)

An unlisted section file is shown with an offer to add it to the vision table, and it is not a gap for step 2. [Product sections](../../../product/section_01.md#product-sections)

A metaplan is `doc/plans/index.md` or any `doc/plans/*metaplan*.md`, and each heading naming a plan file in backticks is an entry. [Metaplans](../../../product/section_01.md#metaplans)

A plan is identified by its slug and is satisfied by its `PLAN-`, `IN_PROGRESS-`, `DONE-`, or archived `RETIRED-` file. [Plans](../../../product/section_01.md#plans)

A `PLAN-` file that no metaplan names is an unlisted plan, offered for pressure testing but not a gap for step 4. [Plans](../../../product/section_01.md#plans)

A plan counts as pressure-tested when the `Pressure tests` section at the end of the plan file has at least one line. [Pressure tests](../../../product/section_01.md#pressure-tests)

## Offers and state

The primary offer closes the first gap of the current step, and every other gap appears as a secondary offer. [What the app offers](../../../product/section_01.md#what-the-app-offers)

Dismissals last only for the browser session, since the app keeps no state of its own. [Skipped and reordered steps](../../../product/section_01.md#skipped-and-reordered-steps)

The checklist is recomputed at start and on file watcher changes, and the new checklist is pushed to every open tab. [When the state is recomputed](../../../product/section_01.md#when-the-state-is-recomputed)
