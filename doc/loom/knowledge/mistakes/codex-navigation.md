# Codex Navigation

> Reader

## We Answered a Slow Reader by Forbidding Reading (2026-08-29)

**What happened:** codex agents spent ten minutes paging `doc/loom/knowledge/` before starting
work, so the signal doctrine and `/loom-plan-writer` answered with "do NOT explore the repo" plus
"CODEX UNITS MUST BE SPECIFIED TO EXHAUSTION" - paste every signature, every snippet, every
constraint. Plans grew enormous, and the cheapest and fastest implementation lane became the most
expensive one to author. The signal itself stated the trade it was making: "you have traded a slow
agent for an ignorant one."

**Why:** the diagnosis stopped at the symptom. Codex was slow because it was reading the wrong
thing, sent there by a Claude preamble it should never have been handed - not because reading is
inherently slow for it. Meanwhile loom had already shipped a source graph and a CLI over it, and
nothing ever told codex those commands existed.

**Prevention:** when an agent is too slow at gathering context, ask what sent it to that context
and what cheaper channel already exists before taking the capability away. A prohibition that has
to be compensated by force-feeding is evidence the prohibition is the wrong fix.

**Fix:** `loom-hooks/codex-forward.sh` prepends the navigation kit and the lane's prohibitions to every
forwarded prompt; the signal doctrine and `/loom-plan-writer` now ask for anchors instead of
transcripts. See [Codex Plugin](../architecture/codex-plugin.md).

## Codex `apply_patch` Creates New Files at Mode 0664, Not Executable

A codex worker's `apply_patch` creates a brand-new file as `100664` (not `100755`), even when the
file is a script meant to be run directly (e.g. `scripts/test-pre-commit-partial-staging.sh`).
The orchestrator must `chmod +x` the file before running it or before committing, or the mode
that lands in the tree is `100644`.

## Evaluating the source graph from one checkout

**What happened:** An answer about whether the source graph mechanism beats agentic rg retrieval began with coverage counts from one loom checkout.

**Why:** Available local telemetry was mistaken for evidence about performance across repositories.

**Prevention:** Separate mechanism-level claims from project-specific measurements; compare both methods on representative tasks across project sizes and languages before claiming a general win.

**Fix:** Treat local coverage and edge counts as an implementation case study and state their scope explicitly.

## A neighbor line is not a call-site line

**What happened:** A source-graph proposal described the current neighbor output as the target declaration line; incoming caller output actually uses the neighboring symbol declaration start.

**Why:** The line field was read from the neighbor node without checking which endpoint each query direction selects.

**Prevention:** For each graph view, trace the displayed line field to its originating span and state whether it names a declaration or a reference site.

**Fix:** Describe the current field as the neighboring declaration line and propose a separate reference-site span.

## Source outline accepts one path per invocation

**What happened:** The codebase audit attempted two --outline flags in one loom map invocation and the CLI rejected the command.

**Why:** Independent queries were combined as repeated flags without checking whether the option was repeatable.

**Prevention:** Run one loom map --outline command per file; batch independent commands through the tool orchestrator.

**Fix:** Reissue each outline as its own read-only command.

## Unbounded report-heading search

**What happened:** A report review searched heading patterns across all of doc/ while also naming selected reports; the command returned over 100,000 tokens and its output was truncated.

**Why:** The broad directory operand expanded an otherwise narrow heading lookup to every document.

**Prevention:** Query headings only in the named documents. Search a directory separately for an exact filename or literal reference, using rg -l when only paths are needed.

**Fix:** Keep subsequent report reads bounded to identified sections and search rename references with the exact old basename.

## Mechanical knowledge replacement duplicated a word

**What happened:** A substring replacement introduced a duplicated article in the context-ceiling topic.

**Why:** The replacement included a sentence boundary while retaining the following article.

**Prevention:** Inspect the complete edited paragraph after a substring replacement.

**Fix:** Remove the duplicate through replace-section and check the rendered paragraph.

## Abbreviated review paths produced invalid report links

**What happened:** Four source links in an optimization-report draft used incomplete directory prefixes from review notes. The document check caught and corrected them before delivery.

**Why:** Abbreviated evidence paths were expanded from memory instead of resolved against repository paths.

**Prevention:** Resolve the exact path with rg --files before creating links from abbreviated review references, then check every local target.

**Fix:** Correct the prefixes and verify the final report links against the filesystem.

## Fence validation confused closing and opening delimiters

**What happened:** A document check incorrectly rejected a correctly typed code block.

**Why:** The regular expression matched the closing fence and let whitespace consume the following blank line.

**Prevention:** Track whether a fence is opening or closing before requiring a language tag.

**Fix:** Use a stateful fence check; the report itself required no fence change.

## Reported a test count without checking

**What happened:** A handoff said an extracted module had 13 tests, but it has 14.

**Why:** The count was recalled from an outline instead of checked against the extracted file.

**Prevention:** Count test attributes before reporting an exact test count.

**Fix:** Correct the handoff to 14 tests.

## Patch targeted pre-format source

**What happened:** A focused test fix did not apply because the target match arm had been reformatted.

**Why:** The patch used an earlier source shape instead of the current text.

**Prevention:** Re-read the exact current range immediately before constructing a contextual patch.

**Fix:** Apply the correction against the current formatted arm.

## Reused stale context in a multi-file patch

**What happened:** A multi-file refactor patch failed to apply because one formatted match arm differed from its assumed shape.

**Why:** The patch was composed without rereading every target after formatting.

**Prevention:** Read each exact target range after formatting before building the patch.

**Fix:** Apply each extraction against the current source.

## Bound combined tool output before reading briefs

**What happened:** Combining complete skills and briefs into one large tool response truncated the material needed for delegation.

**Why:** Per-command output budgets exceeded the combined response budget.

**Prevention:** Read long briefs by their heading ranges and keep each tool response within the aggregate output budget.

**Fix:** Workers read their complete assigned briefs separately; ownership excludes every sibling write set.

## Retired doctrine literal matched the source sweep

**What happened:** A negative doctrine test contained the retired phrase `AT MOST ONE narrowly-scoped` literally, so the plan's source sweep matched the test itself.

**Why:** The test's exact runtime needle was also present as contiguous source text.

**Prevention:** Split the retired literal with `concat!` before `narrowly`, preserving the exact compiled assertion while keeping the source sweep meaningful.

**Fix:** The doctrine test uses the split literal.

## Wait Regression Constructed the Wrong Outcome Shape

**What happened:** a new wait regression constructed `WaitOutcome::Finished` as state-only, but the variant carries `(Box<JobRecord>, Option<JobResult>)`.

**Why:** the test was drafted without checking the enum definition.

**Prevention:** read the enum definition before constructing test outcomes.

**Fix:** corrected the fixture before compiling; runtime behavior was unaffected.

## Retrieve knowledge sections instead of paging through topic files

**What happened:** during the feature planner offline-modal task, I used loom knowledge context but also paged through the planner-web-shell topic and the architecture summary directly, producing unnecessarily large reads.

**Why:** I treated a relevant topic pointer as permission to read the topic broadly instead of asking a focused follow-up query.

**Prevention:** after reading INDEX.md, retrieve documentation through loom knowledge context with a specific question and a bounded token budget. Narrow the query when the first answer is insufficient. Reserve direct source reads for the ranges identified by graph or literal searches.

**Fix:** use targeted knowledge queries for the remaining Aurora audit and report the earlier deviation accurately.

## Inspect import anchors before patching

**What happened:** an Aurora styling patch failed because its source-editor import anchor did not match the file.

**Why:** the patch assumed an import order from an earlier body-only source read.

**Prevention:** inspect the exact import range before using it as a patch anchor.

**Fix:** read each affected import block and apply the patch against the actual source.

## Check the partial-staging guard before committing shared indexes

**What happened:** a planner commit failed because INDEX.md contained both staged planner entries and unstaged entries from concurrent source-graph work.

**Why:** the repository commit hook requires each staged path to be fully staged.

**Prevention:** isolate task-owned changes to generated indexes before committing and preserve concurrent edits separately; use explicit commit pathspecs in a shared checkout.

**Fix:** isolate the planner index entries, commit the task paths, and restore the unrelated index entry.

## Validate task commits in isolation from concurrent work

**What happened:** the planner commit hook failed because concurrent, unstaged Rust changes grew src/fs/work_dir.rs and src/fs/work_dir/tests.rs beyond their maintainability baselines.

**Why:** the commit hook runs the Rust maintainability test against the entire working tree, including unstaged changes.

**Prevention:** commit a task from an isolated checkout when unrelated work is active, preserving explicit task paths and running the normal hooks against the proposed commit snapshot.

**Fix:** preserve concurrent edits, validate the planner-only patch in an isolated checkout, and advance the original branch only if its starting tip still matches.
