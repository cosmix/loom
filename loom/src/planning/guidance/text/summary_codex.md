# Rebuild summaries

Rebuild only the assigned summary ids below. Treat this list as the complete batch;
leave summaries invalidated during this session for the next batch. Never ask the
user anything: nobody is present in this background session. Do every id yourself;
do not use skills, subagents, or delegation.

{{IDS}}

Keep the existing `LOOM_PLAN_STATE` and `LOOM_PLAN_SESSION` environment variables
unchanged. Write only the runtime draft files named by each brief, at
`{{RUNTIME}}/drafts/<id>.md`, preserving any slashes in the id and appending `.md`.
Use the exact draft path printed by the brief; never assume another drafts directory.
Never write under `doc/` or anywhere in the repository except through
`loom plan summary seal <id>`. Never edit source documents, knowledge files,
`.claude/`, or `.codex/`. Never run git, `loom init`, `loom run`, or `loom pressure`.

Run `loom plan summary status --json` to inspect the assigned summaries and their
dependencies. Separate document summaries from overviews without adding ids to the
batch. Leave another session's claims alone. A feature summary's id names its feature:
feature ids are `overview/<feature>/product`, `overview/<feature>/engineering`,
`<feature>/product/vision`, `<feature>/product/section_<nn>`, and `<feature>/metaplan/<stem>`
(one per metaplan file of the feature, `<stem>` the file name without `.md`). Plan summary ids are
`plans/<slug>` with repository-wide unique slugs; plan document ids remain `plan/<slug>`.

Process document summaries first, one at a time in the listed order. Then process
overviews in dependency order, after their document summaries have sealed. Never
rebuild an unassigned id or edit a source to repair an unavailable dependency.

For each id, complete these steps before moving to the next id:

1. Run `loom plan summary brief <id>`. The command reads the source, claims the id,
   and prints source paths, existing summary text with stale sentences marked,
   sentences that may stay word for word, body rules, the draft path, and the seal
   command. If it refuses the claim, record the id and reason for the final report;
   never take over or release another session's claim.
2. Read only what the brief names. For a document summary, check every claim
   against its named source document and the brief's existing summary. For an
   overview, check every claim against the document summaries in scope and the
   vision passages for a product overview or metaplan passages for an engineering
   overview. Preserve current sentences word for word; their text determines the
   sentence ids overviews cite. Replace only stale or incorrect claims, with
   supported prose. Rewrite an overview sentence that repeats the sentence it cites
   to say what the cited sentences add up to, even when the brief marks it `[keep]`.
3. Write only the draft file named by the brief. Write the summary body without
   store frontmatter, hashes, or sentence ids; sealing generates the metadata.
   Follow every body rule in the brief and the format below.
4. Run `loom plan summary seal <id>`. Seal validates links and computes hashes
   itself; never write hashes. Exit 1 reports problems as
   `<draft path>:<line>: <message>`. Fix every reported problem in the named draft
   and run the same seal command again until accepted or an error cannot be fixed.
   Never use `--force` or bypass a rejection. Acceptance writes the summary and
   removes the draft and your claim.
5. If a missing dependency, source problem, or other unresolved failure prevents
   sealing, run `loom plan summary release <id>` for your claim. Record the reason
   and last seal error for the final report. Release an overview claim if a cited
   document-summary sentence is stale or missing and prevents sealing; do not
   repair its source or document summary. If release fails, report that the claim
   remains unresolved. Leave no claim you acquired outstanding without reporting it.

Use this exact draft body format for document summaries:

- A section or plan summary takes at most 15 sentences. A vision summary (id
  `<feature>/product/vision`) takes at most 8 sentences in at most 3 paragraphs, distilling the
  vision so a reader grasps it in under a minute. A metaplan summary (id `<feature>/metaplan/<stem>`)
  takes at most 15 sentences: it summarizes the metaplan's plan map and its sequencing decisions,
  citing the metaplan's own passages. Write one sentence per line, with blank lines
  between paragraphs.
- A sentence compresses its cited section; never copy a sentence of the source.
- Use only `#` and `##` headings to group prose; headings carry no links.
- End every sentence with one or more plain Markdown links to heading sections of this
  summary's own source document. Use a link without a fragment only for text before the
  first heading. Resolve links relative to the final summary file, not the runtime draft.
- Do not use lists, tables, code blocks, HTML, duplicate sentences, or unsupported claims.

Use this exact draft body format for overviews:

- Use at most 30 sentences, one sentence per line, with blank lines between paragraphs.
- Use only `#` and `##` headings to group prose; headings carry no links.
- End every sentence with one or more plain Markdown links. Cite document-summary
  sentences using their actual `#s-…` fragments, and cite heading sections of the vision
  or metaplans directly. Do not replace document-summary citations with raw section or
  plan citations. Resolve links relative to the final overview file, not the runtime draft.
- Cite every document in the overview's scope with at least one overview sentence.
- An overview sentence that cites member sentences cites at least two of them, and says what
  they add up to. Cite one only when that member's summary holds a single sentence.
- Do not use lists, tables, code blocks, HTML, duplicate sentences, or unsupported claims.

Seal only when every citation resolves to a current source sentence. Stop when every
assigned id is sealed or `seal` reports an error you cannot fix. Release your claim
before stopping on an unresolved failure. Report the ids sealed, the ids released
with reasons and the last seal error, and any ids you could not claim or release in
the final message.
