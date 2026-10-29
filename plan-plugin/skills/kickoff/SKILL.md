---
name: kickoff
description: Load when a planning session starts new work, settles its size and name with the user, and creates a feature vision or a standalone plan after confirmation.
---

# Start new work

Decide the size of the work with the user before writing:

- A feature has a vision, product sections, a metaplan and plans, under
  `doc/product/<name>/` and `doc/plans/<name>/`.
- A standalone plan is one `doc/plans/PLAN-<slug>.md` with `Implements: none`
  and its brief bundle under `doc/plans/briefs/<slug>/`. It has no product spec
  and no metaplan.

Use lowercase kebab-case names for features and plan slugs. Do not use `archive`,
`briefs`, `plans`, or `overview`. A standalone-plan opening suggests small work;
if the conversation grows into a feature, say so and let the user decide.

## Discuss the work

Read the request, supplied passages and relevant existing work before asking
for information they already contain. Find out what the user wants to build,
who it serves, what problem it solves and what success means. Ask one question
at a time in plain text. Offer choices when the answer space is small, and
compare viable approaches with their trade-offs before settling a direction.
Play your understanding back and keep proposals distinct from settled decisions.

Keep the problem, settled decisions and open questions in the conversation.
For a feature, propose sections with clear ownership of each topic. Mark each
section as settled, being worded or open as the discussion develops.

## Ask before writing

Nothing is written under doc/ until the user confirms the size and the name.
The planning hook refuses every write under `doc/` before that confirmation.
Afterwards it allows exactly one target:

- Feature: `doc/product/<name>/index.md`.
- Standalone plan: `doc/plans/PLAN-<slug>.md` and files inside its brief bundle,
  `doc/plans/briefs/<slug>/`.

Never try other paths or try to work around a refusal. The target must be new;
an existing name is refused. These limits also apply when another skill would
normally write related documents, knowledge or summaries.

When ready, ask the user to confirm the size and name. Emit the proposal form
below with `proposal: true`, `size`, `name` and, for a feature, its proposed
`sections`. Say in prose that they can pick Use this, Rename, Change sections
or Make it standalone for a feature; for a plan, offer Use this, Rename or
Make it a feature. Keep the proposal current when the user changes it.

The user's confirmation causes the app to send a message naming the size,
the name and the exact path the session may create. Wait for that message;
agreement in the conversation alone is not a write grant. Only then:

- For a feature, load `loom-plan:vision` and write the confirmed
  `doc/product/<name>/index.md` with the settled content and open questions.
  Continue as an ordinary vision session on that file, keeping the granted
  write scope in force.
- For a standalone plan, load `loom-plan:plan` with the settled scope and the
  confirmed `doc/plans/PLAN-<slug>.md` target. Use `Implements: none` and keep
  its briefs inside `doc/plans/briefs/<slug>/`.

If a write is refused, report the refusal to the user in plain text and stop.

## Keep taking shape current

After every turn that changes the taking-shape state, end the reply with one
fenced code block whose info string is exactly `loom-kickoff`. Its body is a
JSON object containing only the keys below, all optional. Send the current
state, rather than only the fields that changed; do not put prose after it.

- `size`: `"feature"` or `"plan"`.
- `name`: the lowercase kebab-case feature name or plan slug.
- `proposal`: `true` only while asking the user to confirm size and name;
  otherwise omit it or set it to `false`.
- `problem`: a string describing the problem.
- `decisions`: an array of strings containing settled decisions.
- `questions`: an array of strings containing open questions.
- `sections`: features only; an array of objects with only `title` (a string)
  and `state` (`"settled"`, `"wording"` or `"open"`). Omit it for a plan.

For example, after discussing a shared-notes feature, ask: “Use this feature
name and section list? You can pick Use this, Rename, Change sections or
Make it standalone.” End that reply with:

```loom-kickoff
{
  "size": "feature",
  "name": "shared-notes",
  "proposal": true,
  "problem": "Small teams lose decisions across separate personal notes.",
  "decisions": ["Each team has one shared notebook."],
  "questions": ["Can guests read a notebook without joining the team?"],
  "sections": [
    {"title": "Problem and users", "state": "settled"},
    {"title": "Notebook editing", "state": "wording"},
    {"title": "Sharing and access", "state": "open"}
  ]
}
```
