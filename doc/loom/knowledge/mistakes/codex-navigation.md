# Codex Navigation

> Forbidding reads instead of fixing a slow reader - a misdiagnosis and its correction.

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

**Fix:** `hooks/codex-forward.sh` prepends the navigation kit and the lane's prohibitions to every
forwarded prompt; the signal doctrine and `/loom-plan-writer` now ask for anchors instead of
transcripts. See [Codex Plugin](../architecture/codex-plugin.md).
