# Codex Navigation

> Slow reader fixed by forbidding reads

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
