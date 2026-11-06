---
kind: document
of: doc/plans/optim-token/index.md
file: sha256:5aeec7e957b1b749959f4a552f0efe226fc2c17d43b7b4f8a35e13254f7feef7
cites:
  "doc/plans/optim-token/index.md#how-a-saving-is-shown": "sha256:83d36045b2fe2220482d554b30d9ecd97c311a15ea67793d0a629778b98edac4"
  "doc/plans/optim-token/index.md#objective-and-binding-constraints": "sha256:1a54d1bba15de0c9e005e9a0eebc9c48fe69f2d1080255adf7130fd560fdc63f"
  "doc/plans/optim-token/index.md#plan-map": "sha256:c37687dbce10fe2504d3e8e49842a970f4845073429cb4f6131c7d12487664e5"
  "doc/plans/optim-token/index.md#rules-for-every-plan": "sha256:9828b42fc35fe8c933f4e31192c30e0521822d96fab25cb86530d4bccad19929"
  "doc/plans/optim-token/index.md#runtime-facts-that-bound-every-plan": "sha256:2fb8c2a32605b9e53abe4364b62b03accd33a42e0b2e1da6b96038437651144e"
  "doc/plans/optim-token/index.md#token-optimization-metaplan": "sha256:5aeec7e957b1b749959f4a552f0efe226fc2c17d43b7b4f8a35e13254f7feef7"
---

# Token Optimization Metaplan

This metaplan orders the next round of loom efficiency work into six plan files and a set of policy notes, each authored later from its section. [Metaplan](../../../../plans/optim-token/index.md#token-optimization-metaplan)

## Plan order

Plan 3 goes first and plan 1 follows it, because plan 1 edits the post-tool-use hook and test runner that plan 3 changes first. [Plan map](../../../../plans/optim-token/index.md#plan-map)
Plan 2 is independent of plan 1, so either order works, while plan 4 waits for plan 1's worker briefs, plan 2's replay and plan 3's sections file. [Plan map](../../../../plans/optim-token/index.md#plan-map)
Plan 5 comes fifth, after plan 3's template and hook edits and plan 2's replay. [Plan map](../../../../plans/optim-token/index.md#plan-map)
Plan 7 is a separate track after plan 3 and must not run beside plan 5, because both edit the agent definitions. [Plan map](../../../../plans/optim-token/index.md#plan-map)
The policy notes for plan 6 carry no code and answer their questions from plan 2's evidence. [Plan map](../../../../plans/optim-token/index.md#plan-map)
Plans run in sequence in one checkout, since a checkout holding loom state refuses a new init, so a shared file needs only an order. [Runtime facts](../../../../plans/optim-token/index.md#runtime-facts-that-bound-every-plan)

## Evidence and constraints

The objective is fewer provider tokens per accepted run without weaker verification or review, and Claude and Codex consumption stay in separate columns. [Objective](../../../../plans/optim-token/index.md#objective-and-binding-constraints)
Each plan names which of three costs it targets: explicit rereading, repeated investigation, or resident context replay. [Objective](../../../../plans/optim-token/index.md#objective-and-binding-constraints)
A byte change that leaves agent behaviour alone is accepted on a deterministic replay that spends no provider tokens, and plan 2 builds that replay. [Evidence](../../../../plans/optim-token/index.md#how-a-saving-is-shown)
A 10% saving would need about 63 paired whole runs to resolve, so no acceptance rests on a subscription verdict. [Evidence](../../../../plans/optim-token/index.md#how-a-saving-is-shown)
Installed hooks, agents and skills are shared under the user's Claude directory, so the operator runs the dev install script after a plan that edits them merges. [Runtime facts](../../../../plans/optim-token/index.md#runtime-facts-that-bound-every-plan)
Every new command, flag, hook or state file must name the problem it solves, and a mechanism that cannot work with a small measured saving is removed. [Rules](../../../../plans/optim-token/index.md#rules-for-every-plan)
Every code stage runs the pre-push gate from the loom directory, and a stage that edits the hooks also runs the hook syntax check and both hook test runs. [Rules](../../../../plans/optim-token/index.md#rules-for-every-plan)
Until plan 3 fixes the bootstrap heuristic, a plan without a bootstrap stage records its strict-verify warning as accepted in its overview. [Metaplan](../../../../plans/optim-token/index.md#token-optimization-metaplan)
