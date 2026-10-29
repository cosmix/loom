---
name: plan
description: Load when a planning session writes one or more missing loom plans from metaplan entries.
---

# Write a loom plan

Use the feature name given in the session's opening prompt. For a feature plan,
write `doc/plans/<feature>/PLAN-<slug>.md`, its briefs under
`doc/plans/<feature>/briefs/<slug>/`, and the owning metaplan entry when its scope
changes, plus any other editable
document under `doc/` that a settled decision or an open point binds. For several
plans, use those paths for each selected slug. Never stage, commit, or branch,
and never run `loom init`, `loom run`, or `loom pressure`.

For a standalone plan, write `doc/plans/PLAN-<slug>.md` with `Implements: none`
and briefs under `doc/plans/briefs/<slug>/`. It has no metaplan or seven-step
checklist. The top level of `doc/plans/` holds standalone plans only.

Keep plans with an `IN_PROGRESS-`, `DONE-`, or `RETIRED-` prefix, their brief
bundles, everything under `doc/plans/<feature>/archive/` or `doc/plans/archive/`,
and all `REVIEW-*.md` files read-only. If asked to change locked work, explain the limit and offer a new
metaplan entry through `loom-plan:metaplan` or a refinement of the product spec.
Never write knowledge or summaries directly; knowledge changes go through
`loom knowledge`, and summary changes go through `loom plan summary seal` using
the turn-end delegation below.

## Establish the app conventions

For a feature plan, read the metaplan entry and the product sections it names
before authoring the plan. Use the slug named in the entry's heading for
`doc/plans/<feature>/PLAN-<slug>.md`. Treat the entry's description as the plan's
scope. If the discussion changes that scope, update the entry in the same turn.
For a standalone plan, take the scope and slug from the session's opening prompt.
Plan ids stay `plan/<slug>`; slugs must be unique across the repository.
Record settled decisions in the documents they bind as they settle, with the
reason when it is not obvious; leave nothing needed later only in the chat.

For a feature plan, confirm the implemented product sections with the user,
taking the proposed list from the metaplan entry. Ask in plain text, one question per message,
never through a question tool. Put a level-1 title first, then the confirmed
`Implements:` line directly under it, outside fences and above the metadata:

```markdown
# PLAN: <Title>

Implements: section_07, section_08
```

Use comma-separated `section_<nn>` tokens for sections listed in the feature's vision.
Resolve them inside `doc/product/<feature>/`, the plan's own feature.
For a plan that implements part of a section, name the whole section and
explain the part in the prose. Write exactly one `Implements:` line. Use
`Implements: none` when the plan implements no product section; do not omit it.

Put feature briefs under `doc/plans/<feature>/briefs/<slug>/` and refer to each
by its full path, such as `doc/plans/<feature>/briefs/<slug>/stage-name.md`.
For standalone briefs, use `doc/plans/briefs/<slug>/stage-name.md`.

## Delegate plan authoring to loom's skill

After establishing these conventions, load loom's `loom-plan-writer` skill.
Let it own every part of plan authoring, including the structure, stages,
metadata, briefs, and verification design. Supply the metaplan entry for a feature
plan or the opening prompt's scope for a standalone plan, the
confirmed product-section mapping, these app conventions, and the decisions
settled in the chat. Keep the write and execution limits above in force. Skip
`loom-plan-writer`'s `/pressure` self-review step and any `loom init` or `loom run`
hand-off it suggests: the app's Pressure test and Start run offers cover both, and the
user starts them.

Write several missing plans in one chat when requested. You may give each plan
to its own subagent with its scope or metaplan entry, these conventions, and the
chat's decisions. Have each load `loom-plan-writer` and review each returned plan
against its scope and the app conventions. Do not use agent teams in authoring.

After writing a plan, run `loom plan verify <plan>`, substituting the plan's
full path, and fix every reported error before the turn ends. Do this for each
plan written in the chat.

## Finish the turn

After any turn that edited documents, run `loom plan check` and fix what it
reports. Then run `loom plan summary status --json`; spawn one
`loom-plan:summary` subagent per document summary the turn made stale, incomplete,
or missing, in parallel. Wait for them to finish, then spawn one
`loom-plan:overview` subagent for the overviews that cite those summaries. Spawn
nothing when nothing is stale. Finish both waves before ending the turn.
