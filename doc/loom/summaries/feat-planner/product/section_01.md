---
kind: document
of: doc/product/feat-planner/section_01.md
file: sha256:5f6743d24a360ce5a92122689a60c297da448a3f8804fa574a05ed7a0b363da7
cites:
  "doc/product/feat-planner/section_01.md#features": "sha256:3d3ef9a7030f3f31041ff1955de5434f25150fd5485a7c5328d9656362883682"
  "doc/product/feat-planner/section_01.md#metaplans": "sha256:46d58a3e279f2d853d8a189143e959fbc8dc2a9a15134909af7551120d4ec74d"
  "doc/product/feat-planner/section_01.md#pace": "sha256:e315830310cb0ca284b5d5ed9ae67bf0066381e896862abb747a1b31f0eea632"
  "doc/product/feat-planner/section_01.md#plans": "sha256:33dd5949c97b99dcd5c3f1466bfc52bc33e5684b5d9bcf7d5e279120a0335a2b"
  "doc/product/feat-planner/section_01.md#pressure-tests": "sha256:3b964f84a7a986c94d448e2e0d9734995040720157c9d2d40bb620d845b6bb86"
  "doc/product/feat-planner/section_01.md#product-sections": "sha256:716c71097d39038f68e2fda9dba9650c29ab9412771dbd687a599fb0b2ccd299"
  "doc/product/feat-planner/section_01.md#skipped-and-reordered-steps": "sha256:0ee954e3023864f624cd07734afce25f30293c07c4294fcd45cbe09745e48ac2"
  "doc/product/feat-planner/section_01.md#standalone-plans": "sha256:18e0872ef44fbeb4a0ecbd9015f2f28b0feb98f5da1c512374a3847effbf1df7"
  "doc/product/feat-planner/section_01.md#the-model-in-one-paragraph": "sha256:5c9ec066d6c3f76acababf84a808d9de488db78dceff202880145d27175e98e4"
  "doc/product/feat-planner/section_01.md#the-seven-steps": "sha256:40289eaff09712ba4d38767ef9b7cc657159c99befc8a53a77557a8e11086209"
  "doc/product/feat-planner/section_01.md#vision": "sha256:068e9644febc0ab3d3b30acf57a930cca3dbd2e7766ded5cfa9035b3e2d50433"
  "doc/product/feat-planner/section_01.md#what-the-app-offers": "sha256:599b26451523b91d85a34e4c5dd86efa4cd49410d1cc0802d1f12f5d2a468db7"
  "doc/product/feat-planner/section_01.md#when-the-state-is-recomputed": "sha256:6beb11e9eac15e09028775db1290f5401183bac47b7d5efbf19e5f00436ba0f7"
---

# Workflow Model and Project State

## The model

The app stores no workflow state and rebuilds one checklist per feature from the files under `doc/` on every start and change. [The model in one paragraph](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)

Each feature's current step is its earliest gap, and a gap in an early step never blocks work on a later one. [The model in one paragraph](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)

The seven steps run from vision and product spec through metaplan, plans, pressure test and review to run. [The seven steps](../../../../product/feat-planner/section_01.md#the-seven-steps)

Step 5 is optional and never holds a feature back, and an untested plan is a suggestion rather than a gap. [The seven steps](../../../../product/feat-planner/section_01.md#the-seven-steps)

## Detection

A feature is the shared name of its product directory and its plans directory under `doc/`, and its name matches a lowercase pattern of at most 64 characters. [Features](../../../../product/feat-planner/section_01.md#features)

The vision lists its sections in the first numbered table under "Specification sections", and the app never guesses sections from prose. [Vision](../../../../product/feat-planner/section_01.md#vision)

A product section counts as present when its file exists and is not empty, and its open questions do not make it a gap. [Product sections](../../../../product/feat-planner/section_01.md#product-sections)

A metaplan is an `index.md` or a `*metaplan*.md` file directly in the feature's plans directory, and each entry is a level-2 heading naming a plan file in backticks. [Metaplans](../../../../product/feat-planner/section_01.md#metaplans)

A plan's lifecycle comes from its file name and location: written, running, done or retired. [Plans](../../../../product/feat-planner/section_01.md#plans)

A pressure test is recorded as one line in a `## Pressure tests` section at the end of the plan file, which the driver writes. [Pressure tests](../../../../product/feat-planner/section_01.md#pressure-tests)

Standalone plans follow their own lifecycle and have no seven-step checklist. [Standalone plans](../../../../product/feat-planner/section_01.md#standalone-plans)

## Offers and recomputation

Each feature's primary offer closes the first gap of its current step, and every other gap is listed as a secondary offer. [What the app offers](../../../../product/feat-planner/section_01.md#what-the-app-offers)

Guided pace opens one session for the first missing document, while draft-all drafts every missing document of that kind. [Pace](../../../../product/feat-planner/section_01.md#pace)

A feature with plans but no spec, or with steps out of order, reads as a normal feature with an early gap. [Skipped and reordered steps](../../../../product/feat-planner/section_01.md#skipped-and-reordered-steps)

The checklists are a pure function of the file tree, recomputed on start and on watcher changes and pushed to open tabs. [When the state is recomputed](../../../../product/feat-planner/section_01.md#when-the-state-is-recomputed)
