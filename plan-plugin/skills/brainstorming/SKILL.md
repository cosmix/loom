---
name: brainstorming
description: Use when a planning step loads the discussion procedure to settle a vision, product section, or metaplan.
---

# Brainstorming

## License

Adapted from the brainstorming skill of the superpowers plugin (<https://github.com/obra/superpowers>, commit 2ce8aa15).

MIT License

Copyright (c) 2025 Jesse Vincent

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Scope and writes

Use the target document named by the step skill that loaded you:
`doc/product/index.md`, `doc/product/section_<nn>.md`, `doc/plans/index.md`,
or `doc/plans/metaplan-<nn>.md`. Follow that step's document shape.
Write only that target under `doc/product/` or `doc/plans/`; write nothing else.
Never stage, commit, or branch. Never run `loom init`, `loom run`, or `loom pressure`.
Treat plans prefixed `IN_PROGRESS-`, `DONE-`, or `RETIRED-`, their brief bundles,
`doc/plans/archive/`, and `REVIEW-*.md` files as read-only. If asked to change
one, explain the limit and offer a new metaplan entry or a spec refinement.

Find out what the user wants and why before settling how it should work.
Record each decision in the target as soon as it settles, in words a reader
without the chat can understand. Include the reason when it is not obvious.
Keep open points separate from settled decisions; never defer all writing to the end.

## What do you already know?

Before replying, identify what you know about the user's intent and reasons.
If everything material is already clear, record the clear change and say what
you changed. If something material is missing, begin the discussion.

Look before you ask. Read the supplied request, spec, plan, references, and
relevant existing work until you can explain how it works today and what the
proposal would touch. Never ask for something that context already answers.

If the user supplied a detailed request, play it back in your first message:
what they want, who it serves, and why. Add relevant findings, constraints,
conflicts, or a cheaper route they may have missed. Aim for about 200 words.
Invite corrections and ask about the open point that would most change the result.
If the request is still vague, ask one open question about its largest uncertainty;
explain what the answer would change.

If the user did not ask for a discussion, offer briefly to proceed with the
available information. If they opt out, state significant choices plainly,
record the known intent and your proposals within the target, and end.

## Run the conversation

Ask questions in plain text, one question per message, never with a question tool.
Use the user's vocabulary and make the question specific to their project and
last answer or something you found. Explain what the answer would change when
that is unclear. Do not read out a generic questionnaire.

Start with open questions so the user can describe what matters. Offer choices
when the answer space is small or they are stuck. Keep the choices short and
end with one question. When asked to guess or recommend, propose something concrete
and explain why. A proposal they can correct may resolve more than another question.

Cover the territory that matters, without treating it as a script:

- What this is, who it serves, and what makes it special.
- Why now and what success looks like.
- Goals, non-goals, and anti-goals: what would make a working result a failure.
- Prior art they liked or disliked, what exists, and what they have tried.
- Constraints, resources, and details they care about.

Find out early which choices about platform, tools, medium, or resources the
user wants to make and which they leave to you. Build on existing work.
State sensible defaults plainly and record your choices as your proposals,
with reasons; never present them as decisions the user made.

When further investigation would help, offer to look into relevant code,
files, data, or external references and say what you would seek. Let the user
decide. If a quick exploration would clarify uncertainty, agree its question
and record a throwaway example or alternative in the target document. Keep
exploration within the document-only write limits and bring the finding back
to the discussion; it does not authorize building the product.

## Lay out approaches before settling a direction

Once you understand the intent, compare ways to achieve it in a message of
their own. Consider real alternatives and discard weak ones, including those
that conflict with the user's reasons or existing work. Present two or three
viable approaches, each grounded in the project, with its trade-offs.
Recommend one, explain why, and ask which direction the user wants.
If only one approach makes sense, explain why the others do not fit.
Record the chosen approach and its reason as soon as it settles.

## Play the understanding back

As soon as you understand one part, describe it back before moving to the
next. Aim for 200–300 words for a substantial part, shorter when that suffices.
Alternate playback and questions through the discussion. Ask what is wrong
or missing in each playback; cover whatever remains in the last one.

Separate what the user said from what you inferred. Label a choice you supplied
as your proposal and give its reason. Follow the loading step's convention for
marking your calls. Correct the document when the user corrects your understanding.

## Size and record the design

After the approach settles, describe the size of the work plainly and let the
user correct it. Keep a small change short; give a project enough detail for
a capable builder to work from without returning to the chat. If the scope
grows, say so and agree the larger scope. If it contains independent projects,
agree their order and discuss them one at a time.

Build the target document incrementally in the loading step's shape. Cover:

- Intent and reasons; goals, non-goals, and anti-goals.
- Constraints and the alternatives considered, their trade-offs, and the chosen approach.
- The parts of the design the user decided and what remains for the builder.

For a small change, a line or two per point may suffice. For a project, read
the document as a builder and identify gaps. Fill answers already present in
the conversation or project. Settle minor details as your calls and mark them.
Bring consequential questions only the user can answer back to the discussion,
one per message, then record the answers. Present the document with a short
list of your calls so the user can check them.

Get the user's approval of the written understanding; agreement with an idea
does not approve text they have not seen. Keep all work within the document
limits throughout. End when the discussion settles, with decisions recorded
and remaining questions under `## Open questions`, each with options and a
recommendation. Make no hand-off to a plan-writing skill; return to the step skill that loaded you.

If the session-ending message says nobody will answer further questions,
ask none. Record any settled decisions still missing and all open points with
options and a recommendation, then reply "Closing write done." and stop.
