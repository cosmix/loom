---
name: plan
description: Load when a planning session writes one or more missing loom plans from metaplan entries.
---

# Write a loom plan

Before writing a feature plan, read its metaplan. If it has no `Settled:` line, or
the user says it changed since settling, stop and ask the user to settle the
metaplan first.

Use the feature name given in the session's opening prompt. For a feature plan,
write `doc/plans/<feature>/PLAN-<slug>.md`, its briefs under
`doc/plans/<feature>/briefs/<slug>/`, and the owning metaplan entry when its scope
changes, plus any other editable
document under `doc/` that a settled decision or an open point binds. For several
plans, use those paths for each selected slug. Never stage, commit, or branch,
and never run `loom init`, `loom run`, or `loom pressure`.

For a standalone plan, write `doc/plans/PLAN-<slug>.md` with `Implements: none`
and briefs under `doc/plans/briefs/<slug>/`. It has no metaplan or six-step
checklist. The top level of `doc/plans/` holds standalone plans only.

Keep plans with an `IN_PROGRESS-`, `DONE-`, or `RETIRED-` prefix, their brief
bundles, everything under `doc/plans/<feature>/archive/` or `doc/plans/archive/`,
and all `REVIEW-*.md` files read-only. If asked to change locked work, explain the limit and offer a new
metaplan entry through `loom-plan:metaplan` or a refinement of the product spec.
Never write knowledge or summaries directly; knowledge changes go through
`loom knowledge`, and summaries change only through `loom plan summary seal`.

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
For a plan that implements part of a section, name the headings it covers as
`section_<nn>#<anchor>` (the GitHub anchor of the heading) and explain the part in the
prose; name the whole section as `section_<nn>` only when the plan covers all of it:

```markdown
Implements: section_07#rebuild-budget, section_08
```

A stage that covers a narrower part than the plan gets its own line in the plan's prose, under
the stage's heading, starting at column 0 and outside fences:

```text
Implements (stage:scheduler): section_07#rebuild-budget
```

The stage id is the `id` of the stage in the plan's YAML. The metaplan entry's `Covers:` line
and the plan's `Implements:` line agree; when the scope changes, update both in the same turn.
Write exactly one `Implements:` line. Use
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
against its scope and the app conventions. Outside Team mode below, do not use
agent teams in authoring.

After writing a plan, run `loom plan verify <plan>`, substituting the plan's
full path, and fix every reported error before the turn ends. Do this for each
plan written in the chat.

## Team mode

Use team mode only when the opening message says to use it for a draft-all run.
Otherwise keep the guided behavior above. The opening names the missing plans
and the teammate count: at most five teammates, fewer when fewer plans are
missing.

The lead reads the metaplan, the product sections it names and the settled
decisions, then writes a short shared brief in the runtime drafts directory
the session may write. Include terms, decisions, which document owns each
topic and the cross-references to expect. Use the metaplan's product-section
mapping for each plan and leave unresolved mapping questions explicit.
Create one team with the stated teammate count and assign each teammate its
plans and the shared brief. Give each file exactly one owner.

Each plan teammate loads `loom-plan-writer` with its metaplan entries, the
shared brief and the app conventions above. It writes its plans plus their
brief bundles. While drafting, teammates message each other with `SendMessage`
to agree on a term, settle an overlap or ask which document owns a topic.
Keep unresolved choices explicit rather than silently deciding them.

A teammate writes only its own plans and brief bundles. The run's listed
documents and bundle paths are the complete write scope under `doc/`; the
planning hook refuses a write to any other path. This scope also governs
turn-end duties. Report needed changes to the metaplan, summaries or other
documents outside the list rather than writing them. Never try to work around
a refusal.

When every teammate has finished, the lead makes one consistency pass over
the set: terms, cross-references, duplicated or missing topics, `Implements:`
lines, and `loom plan verify <plan>` for each plan using its full path. Check
that briefs agree with their plans and make corrections only within the run's
write scope. Hand the user a review queue in the metaplan's order, the order
the app's sidebar and plan queue use, and suggest starting at its first plan.
Give each document its open-question count and a note on what the pass
changed, including when no changes were needed. Report anything that could not
be resolved.

The lead never starts a second team. If the user says “Stop the team”, end it;
documents already written stay.

## Settled documents

Never write or edit a `Settled:` line; the app's Mark settled action writes it. A plan is
settled together with its brief bundle, so editing a brief of a settled plan makes the plan
read revised: say so in the reply ("the plan was settled; it now reads revised"). The plan's
stub is its metaplan entry: when the plan's scope moves past the entry, update the entry in the
same turn or ask.

## Finish the turn

After any turn that edited documents, run `loom plan check` and fix what it reports
before you reply. Do not rebuild summaries at the end of the turn: the app rebuilds
them in the background once the documents stop changing.
