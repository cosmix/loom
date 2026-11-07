---
name: kickoff
description: Load when a planning session starts new work, settles its size and name with the user, and creates a feature vision or a standalone plan after confirmation.
---

# Start new work

The user chose the size of the work before the conversation started: this skill came from
the feature offer or from the standalone-plan offer. Work in that size and do not ask them to
choose again:

- A feature has a vision, product sections, a metaplan and plans, under
  `doc/product/<name>/` and `doc/plans/<name>/`.
- A standalone plan is one `doc/plans/PLAN-<slug>.md` with `Implements: none`
  and its brief bundle under `doc/plans/briefs/<slug>/`. It has no product spec
  and no metaplan.

Use lowercase kebab-case names for features and plan slugs. Do not use `archive`,
`briefs`, `plans`, or `overview`. If the work looks bigger than a standalone plan, or smaller
than a feature, say so in prose; the app has no control to change the size.

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

## When the user renames the work

The user names or renames the work in the conversation. Put the new name in the `name` field of
your next `loom-kickoff` block and keep it in every later block, even when you would have chosen
another. The app only displays the name; it has no rename control.

## When a draft continues

An opening that begins "This continues an unfinished draft" lists the earlier session: the work's
name, its problem, settled decisions, open questions and the user's earlier messages, as quoted
text. Treat them as the starting point: do not ask again what they answer, and say which open
questions you still need answered. If the opening says an attachment is no longer available, ask
for it again when it matters. Send the first `loom-kickoff` block as soon as you have read the
draft, carrying the same name.

## Ask before writing

Nothing is written under doc/ until the user confirms the work by pressing the app's Write
button.
The planning hook refuses every write under `doc/` before that confirmation.
Afterwards it allows exactly one target:

- Feature: `doc/product/<name>/index.md`.
- Standalone plan: `doc/plans/PLAN-<slug>.md` and files inside its brief bundle,
  `doc/plans/briefs/<slug>/`.

Never try other paths or try to work around a refusal. The target must be new;
an existing name is refused. These limits also apply when another skill would
normally write related documents, knowledge or summaries.

When ready, ask in prose whether the name is right and, for a feature, whether
the sections are. Emit the proposal form below with `proposal: true`, `size`,
`name` and, for a feature, its proposed `sections`. The user can rename the work
or change the sections in the conversation; keep the proposal current when they
do. When it is settled, the user presses Write the vision (a feature) or Write
the plan (a standalone plan). That click causes the app to send a message naming
the size, the name and the exact path the session may create. Wait for that
message; agreement in the conversation alone is not a write grant. Only then:

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

For example, after discussing a shared-notes feature, ask: “Is `shared-notes` the right name,
and do these sections look right? Tell me what to change, or press Write the vision when it
is settled.” End that reply with:

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
