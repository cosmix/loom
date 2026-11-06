---
kind: document
of: doc/plans/feat-planner/index.md
file: sha256:a953bfddd60f4e25b05e4cff034d8c832b3b0818d3a35ba827306dab0a084114
cites:
  "doc/plans/feat-planner/index.md#decisions": "sha256:0b59523db048898af6a01a90494417e2db648e28082e7786eec465f1ffeda068"
  "doc/plans/feat-planner/index.md#follow-ups": "sha256:5a3aaee03e77eaf0627d5df51674484a488c4ba4b39f29e0f95d8d09d2ed1aae"
  "doc/plans/feat-planner/index.md#objective-and-binding-constraints": "sha256:fc3c645554bf2f82c688ef1695107ea0cee60bb2cdcf75ddb912900d7956a007"
  "doc/plans/feat-planner/index.md#open-questions": "sha256:645167d4072e800050208b05d204f0434ba0813152d1da6a221115f311dd9e00"
  "doc/plans/feat-planner/index.md#plan-map": "sha256:edb6365c2d67ba86f380a42976159845cfcf7acc052839e8fda7d70452c9d01a"
  "doc/plans/feat-planner/index.md#state-of-the-tree-at-b0f91c00": "sha256:f2bf4ddd96e75e7c3f3e5a7e01cc68de59df57695e2a2f198799531f561e3051"
---

# Feature planner engineering metaplan

## Plan map

The metaplan cuts the loom plan specification into thirteen plans, each naming the product sections it implements and the earlier plans it depends on. [Plan map](../../../../plans/feat-planner/index.md#plan-map)
Plans 1 to 5 build a read-only app first, so a reviewer can read a project through concept mode before any session writes to its documents. [Plan map](../../../../plans/feat-planner/index.md#plan-map)
Plan 1 comes first because plans 2 to 11 all read `doc/` through its document model and checklist. [Plan map](../../../../plans/feat-planner/index.md#plan-map)
Plans 6 to 9 add authoring, and plans 10 and 11 close the loop with automatic summary rebuilds and execution from the app. [Plan map](../../../../plans/feat-planner/index.md#plan-map)
Plans 12 and 13 repair and then redesign the web app once the first eleven plans are in use. [Plan map](../../../../plans/feat-planner/index.md#plan-map)
One checkout runs one plan at a time, because `loom init` refuses while `.loom/work` holds state, so the plans run in the order of the table. [Plan map](../../../../plans/feat-planner/index.md#plan-map)
Plans 2, 3, 5, 7, 9, 10 and 11 share the planning server's route table, so plan 2 creates the table and each later plan adds its own route module. [Plan map](../../../../plans/feat-planner/index.md#plan-map)

## Constraints and tree state

Every plan inherits the rule that the files in `doc/` are the truth, so summaries are the only new committed content and they are disposable. [Objective](../../../../plans/feat-planner/index.md#objective-and-binding-constraints)
Plans 2, 4, 6 and 7 open write paths into the repository or a running Claude Code process, so each gets a security review in its final stage. [Objective](../../../../plans/feat-planner/index.md#objective-and-binding-constraints)
The planning server is built from the dashboard's parts, so plan 2 extracts the shared accept loop before adding new code, because the dashboard's modules leave little room. [State of the tree](../../../../plans/feat-planner/index.md#state-of-the-tree-at-b0f91c00)
Plan 3 splits the body of `loom plan verify` into a function over proposed text, since that file is already 504 lines and its JSON types are private. [State of the tree](../../../../plans/feat-planner/index.md#state-of-the-tree-at-b0f91c00)

## Sequencing decisions

Plan 9 keeps the review loop inside its own file, since it has four stages and sits under the split threshold. [Decisions](../../../../plans/feat-planner/index.md#decisions)
The pressure-record writer stays in plan 11 unless a pressure test runs on these plans before plan 11 starts, in which case a small plan runs the writer first so the records exist. [Open questions](../../../../plans/feat-planner/index.md#open-questions)
Plan 12 depends on plans 3, 5 and 9, and plan 13 depends on plan 12 alone. [Plan map](../../../../plans/feat-planner/index.md#plan-map)
Plan 14 is a follow-up that adds live preview to the file-mode editor, builds on plans 3, 12 and 13, all done, and nothing depends on it. [Follow-ups](../../../../plans/feat-planner/index.md#follow-ups)
