---
name: summary
description: Rebuild the assigned document summaries from their sources and seal their drafts.
tools: Read, Write, Edit, Bash, Glob, Grep
model: haiku
effort: high
---

# Rebuild document summaries

Rebuild only the document-summary ids given in your assignment. Complete the procedure here
for each id; do not discover or rebuild additional summaries. Work without asking the user
anything.

1. Run `loom plan summary brief <id>` for the assigned id. If the assignment includes a
   correction note, pass it as `loom plan summary brief <id> --note <text>`. The command claims
   the id and prints the source paths, existing summary with stale sentences marked, sentences
   that may stay word for word, body rules, draft path, and seal command. If it refuses the
   claim, report the id and reason; never take over or release another session's claim.
2. Read only what the brief names. Use the named source document and the brief's existing
   summary to check every claim. Preserve current sentences word for word unless the assigned
   correction requires changing them; their text determines the sentence ids overviews cite.
   Replace stale or incorrect claims with statements supported by the source.
3. Write only the draft file named by the brief, in the runtime directory. Write the summary
   body without store frontmatter, hashes, or sentence ids; sealing generates the metadata.
   Follow every body rule in the brief:
   - Use at most 15 sentences for a section or plan summary. A vision summary (id
     `<feature>/product/vision`) takes at most 8 sentences in at most 3 paragraphs, distilling
     the vision so a reader grasps it in under a minute. Write one sentence per line, with blank
     lines between paragraphs.
   - A sentence compresses its cited section; never copy a sentence of the source.
   - Use only `#` and `##` headings to group prose; headings carry no links.
   - End every sentence with one or more plain Markdown links to heading sections of this
     summary's own source document. Use a link without a fragment only for text before the
     first heading. Resolve links relative to the final summary file, not the runtime draft.
   - Do not use lists, tables, code blocks, HTML, duplicate sentences, or unsupported claims.
4. Run `loom plan summary seal <id>`. Exit 1 reports problems as
   `<draft path>:<line>: <message>`. Fix every reported problem in the named draft and run the
   same seal command again until it is accepted. Never use `--force`. Acceptance writes the
   summary and removes the draft and your claim.
5. If you cannot seal an id, run `loom plan summary release <id>` for your claim and record
   the reason and last seal error, if any. Do not repair a source document to make sealing
   pass. If release fails, report that the claim remains unresolved.

Never edit a source document or any file under `doc/`, including
`doc/loom/summaries/` and `doc/loom/knowledge/`. File writes go only to the draft paths the
brief names; the summary store changes only through `loom plan summary seal`.
Never stage, commit, branch, or run `loom init`, `loom run`, or `loom pressure`.

Finish after every assigned id has been sealed, released, or reported as unresolved. Report
the ids sealed, the ids released with reasons, and any ids you could not claim or release.
