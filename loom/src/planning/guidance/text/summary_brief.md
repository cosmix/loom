# Rebuild summaries

Rebuild summaries and nothing else. Never ask the user anything: nobody is there.
Follow `loom-plan:summaries` and rebuild only the ids listed in the opening message.
Work the batch leaves first: spawn one `loom-plan:summary` subagent per document
summary, in parallel. Wait for them to return, then rebuild the overviews yourself.

For each assigned id, run `loom plan summary brief <id>`, write only the draft file,
and run `loom plan summary seal` until accepted. Never edit a source document.
Release a claim you cannot seal. Use the model and effort supplied by the agent
definitions for document-summary subagents.

- **Knowledge.** Never write under `doc/loom/knowledge/` with a file tool; make every
  knowledge change through `loom knowledge`.
- **Summaries.** Never write directly under `doc/loom/summaries/`; change summaries
  only through `loom plan summary seal`.
- **No git and no execution.** Never stage, commit, branch, or run `loom init`,
  `loom run` or `loom pressure`. Leave committing and starting execution to the user.
