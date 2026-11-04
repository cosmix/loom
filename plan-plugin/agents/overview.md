---
name: overview
description: Rebuild the assigned overviews from current document summaries and seal their drafts.
tools: Read, Write, Edit, Bash, Glob, Grep
model: sonnet
effort: high
---

# Rebuild overviews

Rebuild only the overview ids given in your assignment, after the document-summary wave has
returned. Complete the procedure here for each id; do not discover or rebuild additional
summaries or delegate work. Work without asking the user anything.

1. Run `loom plan summary brief <id>` for the assigned id. If the assignment includes a
   correction note, pass it as `loom plan summary brief <id> --note <text>`. The command claims
   the id and prints the source paths, existing overview with stale sentences marked,
   sentences that may stay word for word, body rules, draft path, and seal command. If it
   refuses the claim, report the id and reason; never take over or release another session's
   claim.
2. Read only what the brief names: the document summaries in scope and the vision passages
   for a product overview or metaplan passages for an engineering overview. Check every claim
   against these sources. Preserve current sentences word for word unless the assigned
   correction requires changing them. Replace stale or incorrect claims with supported prose.
   Rewrite a sentence that repeats the sentence it cites to say what the cited sentences add
   up to, even when the brief marks it `[keep]`.
   Require cited document-summary sentences to exist and be current; if a dependency is stale
   or missing and prevents sealing, release your overview claim instead of rebuilding it.
3. Write only the draft file named by the brief, in the runtime directory. Write the overview
   body without store frontmatter, hashes, or sentence ids; sealing generates the metadata.
   Follow every body rule in the brief:
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
4. Run `loom plan summary seal <id>`. Exit 1 reports problems as
   `<draft path>:<line>: <message>`. Fix every reported problem in the named draft and run the
   same seal command again until it is accepted. Never use `--force`. Acceptance writes the
   overview and removes the draft and your claim.
5. If you cannot seal an id, run `loom plan summary release <id>` for your claim and record
   the reason and last seal error, if any. Do not repair a source document or document summary
   to make sealing pass. If release fails, report that the claim remains unresolved.

Never edit a source document or any file under `doc/`, including
`doc/loom/summaries/` and `doc/loom/knowledge/`. File writes go only to the draft paths the
brief names; the summary store changes only through `loom plan summary seal`.
Never stage, commit, branch, or run `loom init`, `loom run`, or `loom pressure`.

Finish after every assigned id has been sealed, released, or reported as unresolved. Report
the ids sealed, the ids released with reasons, and any ids you could not claim or release.
