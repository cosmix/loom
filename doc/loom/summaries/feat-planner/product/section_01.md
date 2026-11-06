---
kind: document
of: doc/product/feat-planner/section_01.md
file: sha256:79df00247fe08ae5a43ced8fe569258c9566e2ad402eba0815ac4155bf05c214
cites:
  "doc/product/feat-planner/section_01.md#features": "sha256:42cd02ee3bbdd04c5479763df0a1e480271f79d937157c1867491d15a7b7f0db"
  "doc/product/feat-planner/section_01.md#metaplans": "sha256:8d463f347d815f43960717774c0bf8694a4f009f34d102f45099891125c00caf"
  "doc/product/feat-planner/section_01.md#pace": "sha256:e315830310cb0ca284b5d5ed9ae67bf0066381e896862abb747a1b31f0eea632"
  "doc/product/feat-planner/section_01.md#plans": "sha256:e8b479c0de7cacde78d6e686d133a87efce3726131e94f27f00d34b43c177727"
  "doc/product/feat-planner/section_01.md#pressure-tests": "sha256:3b964f84a7a986c94d448e2e0d9734995040720157c9d2d40bb620d845b6bb86"
  "doc/product/feat-planner/section_01.md#product-sections": "sha256:716c71097d39038f68e2fda9dba9650c29ab9412771dbd687a599fb0b2ccd299"
  "doc/product/feat-planner/section_01.md#skipped-and-reordered-steps": "sha256:0ee954e3023864f624cd07734afce25f30293c07c4294fcd45cbe09745e48ac2"
  "doc/product/feat-planner/section_01.md#standalone-plans": "sha256:fdcf2d2b309e34994ab637ca97ae406552e353d7943dee502d4e8ce42df3da50"
  "doc/product/feat-planner/section_01.md#the-model-in-one-paragraph": "sha256:b5f031d31b2bcf01c1862be72655ca02bfee6ad2dd1db6e9479da68033c67218"
  "doc/product/feat-planner/section_01.md#the-six-steps": "sha256:8522d157fb6ea4ea7e31cacd9dc704c4be160aa59f80b526f8ab810288cc5dae"
  "doc/product/feat-planner/section_01.md#vision": "sha256:068e9644febc0ab3d3b30acf57a930cca3dbd2e7766ded5cfa9035b3e2d50433"
  "doc/product/feat-planner/section_01.md#what-the-app-offers": "sha256:80f5114624bc523b7a436fdab17469eae0a2798fc617161fb6bcee6d4e0e9fe7"
  "doc/product/feat-planner/section_01.md#when-the-state-is-recomputed": "sha256:16d535fc9d03bd7f1ee9ad5d6a23d15f759a9de393f110976ca63f58b05b2993"
---

# Workflow Model and Project State

## The model

The app stores no workflow state and rebuilds one checklist per feature from the files under `doc/` on every start and change. [The model in one paragraph](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)
Each feature's current step is its earliest gap, and a gap in an early step never blocks work on a later one. [The model in one paragraph](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)
The six steps are vision, product spec, metaplan, plans, pressure test and run, and each closes on a document or a user action. [The six steps](../../../../product/feat-planner/section_01.md#the-six-steps)
Pressure testing is optional, so a plan without a test record is offered as a suggestion and never counts as a gap. [The six steps](../../../../product/feat-planner/section_01.md#the-six-steps)

## Detection

A feature is a name used by a product directory, a plans directory, or both under `doc/`, and it must be lowercase, at most 64 characters, and not one of four reserved words. [Features](../../../../product/feat-planner/section_01.md#features)
The vision lists its sections in the first numbered table under "Specification sections", and the app never guesses sections from prose. [Vision](../../../../product/feat-planner/section_01.md#vision)
A product section counts as present when its file exists and is not empty, and its open questions do not make it a gap. [Product sections](../../../../product/feat-planner/section_01.md#product-sections)
A feature's metaplan is its `index.md` directly in the plans directory, and other `*metaplan*.md` files contribute no entries. [Metaplans](../../../../product/feat-planner/section_01.md#metaplans)
A plan's state comes from its file name and location: written, running, done or retired. [Plans](../../../../product/feat-planner/section_01.md#plans)
A pressure test is recorded as one line in a `## Pressure tests` section at the end of the plan file, which the driver writes. [Pressure tests](../../../../product/feat-planner/section_01.md#pressure-tests)
Standalone plans follow their own lifecycle and have no six-step checklist. [Standalone plans](../../../../product/feat-planner/section_01.md#standalone-plans)

## Offers and recomputation

A feature's primary offer targets the first gap of its current step, and each other gap appears in the queue with an offer of its own. [What the app offers](../../../../product/feat-planner/section_01.md#what-the-app-offers)
Guided pace opens one session for the first missing document, while draft-all drafts every missing document of that kind. [Pace](../../../../product/feat-planner/section_01.md#pace)
A feature with plans but no spec, or with steps out of order, reads as a normal feature with an early gap. [Skipped and reordered steps](../../../../product/feat-planner/section_01.md#skipped-and-reordered-steps)
The checklists are a pure function of the file tree, recomputed on start and on watcher changes and pushed to open tabs. [When the state is recomputed](../../../../product/feat-planner/section_01.md#when-the-state-is-recomputed)
