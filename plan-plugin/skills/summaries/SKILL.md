---
name: summaries
description: Rebuild only the summary ids assigned to a background summary session, with document summaries before overviews.
---

# Rebuild assigned summaries

Take the summary ids in the opening message as the complete batch. Rebuild only
those ids; leave summaries invalidated during this session for the next batch.
Never ask the user anything: nobody is present in a background session.

For feature summaries, use the feature name given in the session's opening prompt.
Summary ids are `overview/<feature>/product`, `overview/<feature>/engineering`,
`<feature>/product/vision`, `<feature>/product/section_<nn>`, and `plans/<slug>`. Plan
summary ids use the repository-wide unique slug; plan document ids remain `plan/<slug>`.
A vision summary (`<feature>/product/vision`) takes at most 8 sentences in at most 3
paragraphs, distilling the vision so a reader grasps it in under a minute; a section
summary takes at most 15 sentences. A plan summary has two levels: an opening level of at
most 8 sentences, each citing whole detail paragraphs as `[d](#s-xxxxxxxx)`, then a detail
level of at most 15 sentences citing the plan. The summary subagent writes the detail level
first, seals, reads the detail ids from the refusal, adds the opening level above it, and
seals again.

Write only the runtime draft files named by `loom plan summary brief <id>`, at the
draft paths that brief gives; never assume a drafts directory. Publish summaries under `doc/loom/summaries/`
only through `loom plan summary seal <id>`; never write there directly.
Never edit a source document or any other file under `doc/`.
Never stage, commit, branch, or run `loom init`, `loom run`, or `loom pressure`.

1. Run `loom plan summary status --json` to inspect the assigned summaries and
   their dependencies. Separate document summaries from overviews without adding
   ids to the batch. Leave another session's claims alone.
2. Work leaves first. Spawn one `loom-plan:summary` subagent per assigned document
   summary, in parallel. Give each only its assigned id and any relevant note from
   the opening message. Have each subagent claim its own id with
   `loom plan summary brief <id>`, read only what its brief names, write only the
   named draft, and run `loom plan summary seal <id>`. Have it fix draft problems
   until sealing succeeds, or run `loom plan summary release <id>` for a claim it
   cannot seal, then report the ids sealed and released. Do not claim these ids
   yourself before delegating them.
3. Wait for all document subagents to return before rebuilding overviews. Use
   their sealed or released results to identify unavailable dependencies; never
   edit sources or rebuild an unassigned id to repair one. Ensure each subagent
   has sealed or released every claim it acquired.
4. Rebuild the assigned overviews yourself, in dependency order, after their
   document summaries have sealed. For each id, run
   `loom plan summary brief <id>` to claim it and obtain its sources, current
   text, body rules, draft path, and seal command. Read only what the brief names.
   Preserve sentences it identifies as current word for word to keep their ids
   and the overview links into them. Rewrite an overview sentence that repeats the
   sentence it cites to say what the cited sentences add up to, even when the brief
   marks it `[keep]`. An overview sentence that cites member sentences cites at least
   two of them and says what they add up to; it may cite one only when that member's
   summary holds a single citable sentence. Cite a plan summary's detail sentences, never
   its opening level. Write only the draft path it names.
5. Run `loom plan summary seal <id>`. On exit 1, read each
   `<draft path>:<line>: <message>` problem, correct the draft, and seal again.
   Seal only when every citation resolves to a current source sentence; never
   bypass a rejection or take another session's claim. If a missing dependency,
   source problem, or other unresolved failure prevents sealing, run
   `loom plan summary release <id>` for your claim and record the reason.
6. Stop when every assigned id is sealed or its claim has been released. If an id
   could not be claimed, report that failure without releasing another session's
   claim. Report the ids sealed and released, and any ids unavailable to claim;
   leave no claim you acquired outstanding.
