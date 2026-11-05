---
kind: document
of: doc/product/feat-planner/section_01.md
file: sha256:476357d4e295d69b7486a05251d9568a2e3f14a7fddc23c8d784ac8c821a4486
cites:
  "doc/product/feat-planner/section_01.md#features": "sha256:3d3ef9a7030f3f31041ff1955de5434f25150fd5485a7c5328d9656362883682"
  "doc/product/feat-planner/section_01.md#metaplans": "sha256:46d58a3e279f2d853d8a189143e959fbc8dc2a9a15134909af7551120d4ec74d"
  "doc/product/feat-planner/section_01.md#pace": "sha256:e315830310cb0ca284b5d5ed9ae67bf0066381e896862abb747a1b31f0eea632"
  "doc/product/feat-planner/section_01.md#plans": "sha256:33dd5949c97b99dcd5c3f1466bfc52bc33e5684b5d9bcf7d5e279120a0335a2b"
  "doc/product/feat-planner/section_01.md#pressure-tests": "sha256:3b964f84a7a986c94d448e2e0d9734995040720157c9d2d40bb620d845b6bb86"
  "doc/product/feat-planner/section_01.md#product-sections": "sha256:716c71097d39038f68e2fda9dba9650c29ab9412771dbd687a599fb0b2ccd299"
  "doc/product/feat-planner/section_01.md#skipped-and-reordered-steps": "sha256:0ee954e3023864f624cd07734afce25f30293c07c4294fcd45cbe09745e48ac2"
  "doc/product/feat-planner/section_01.md#standalone-plans": "sha256:fdcf2d2b309e34994ab637ca97ae406552e353d7943dee502d4e8ce42df3da50"
  "doc/product/feat-planner/section_01.md#the-model-in-one-paragraph": "sha256:b5f031d31b2bcf01c1862be72655ca02bfee6ad2dd1db6e9479da68033c67218"
  "doc/product/feat-planner/section_01.md#the-six-steps": "sha256:795269bfdacd5bf0678bcc7b8bf3847d97c13b3a8ae5e296d89ee485e53c397e"
  "doc/product/feat-planner/section_01.md#vision": "sha256:068e9644febc0ab3d3b30acf57a930cca3dbd2e7766ded5cfa9035b3e2d50433"
  "doc/product/feat-planner/section_01.md#what-the-app-offers": "sha256:93cec403e9b1293b0a2b268214d64cd72ed813217d4e525b8ee5f2d96cd4b6b7"
  "doc/product/feat-planner/section_01.md#when-the-state-is-recomputed": "sha256:16d535fc9d03bd7f1ee9ad5d6a23d15f759a9de393f110976ca63f58b05b2993"
---

# Workflow Model and Project State

## The model

The app stores no workflow state and rebuilds one checklist per feature from the files under `doc/` on every start and change. [The model in one paragraph](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)

Each feature's current step is its earliest gap, and a gap in an early step never blocks work on a later one. [The model in one paragraph](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)

The six steps run from vision and product spec through metaplan, plans, pressure test and run. [The six steps](../../../../product/feat-planner/section_01.md#the-six-steps)

Step 5 is optional and never holds a feature back, and an untested plan is a suggestion rather than a gap. [The six steps](../../../../product/feat-planner/section_01.md#the-six-steps)

## Detection

A feature is the shared name of its product directory and its plans directory under `doc/`, and its name matches a lowercase pattern of at most 64 characters. [Features](../../../../product/feat-planner/section_01.md#features)

The vision lists its sections in the first numbered table under "Specification sections", and the app never guesses sections from prose. [Vision](../../../../product/feat-planner/section_01.md#vision)

A product section counts as present when its file exists and is not empty, and its open questions do not make it a gap. [Product sections](../../../../product/feat-planner/section_01.md#product-sections)

A metaplan is an `index.md` or a `*metaplan*.md` file directly in the feature's plans directory, and each entry is a level-2 heading naming a plan file in backticks. [Metaplans](../../../../product/feat-planner/section_01.md#metaplans)

A plan's lifecycle comes from its file name and location: written, running, done or retired. [Plans](../../../../product/feat-planner/section_01.md#plans)

A pressure test is recorded as one line in a `## Pressure tests` section at the end of the plan file, which the driver writes. [Pressure tests](../../../../product/feat-planner/section_01.md#pressure-tests)

Standalone plans follow their own lifecycle and have no six-step checklist. [Standalone plans](../../../../product/feat-planner/section_01.md#standalone-plans)

## Offers and recomputation

Each feature's primary offer closes the first gap of its current step, and every other gap is listed in the page's queue with its own offer. [What the app offers](../../../../product/feat-planner/section_01.md#what-the-app-offers)

Guided pace opens one session for the first missing document, while draft-all drafts every missing document of that kind. [Pace](../../../../product/feat-planner/section_01.md#pace)

A feature with plans but no spec, or with steps out of order, reads as a normal feature with an early gap. [Skipped and reordered steps](../../../../product/feat-planner/section_01.md#skipped-and-reordered-steps)

The checklists are a pure function of the file tree, recomputed on start and on watcher changes and pushed to open tabs. [When the state is recomputed](../../../../product/feat-planner/section_01.md#when-the-state-is-recomputed)
