# Codex Implementers and Lane Choice

Read when: choosing each stage's implementation lanes, or a stage lists codex in `implementers`.

Two lanes implement a stage's work, and neither is the default. Judge each stage against both and
license the better fit; the stage's orchestrator still picks the lane per subagent at spawn time.

- **Codex lane:** `loom-codex-forwarder`, a sonnet agent whose only tool is Bash, makes one
  foreground call to `codex-forward.sh`, which runs gpt-5.6-terra (common implementation,
  integration tests) or gpt-6-luna (boilerplate, scaffolding, simple unit tests), always at effort
  xhigh.
- **Claude lane:** `loom-software-engineer` (sonnet, or haiku by `model` override),
  `loom-senior-software-engineer` (opus), and fable by an explicit model override.

## Lane comparison

| Concern | Codex lane | Claude lane |
| --- | --- | --- |
| Claude-side cost | One forwarder boot (about 28,000 tokens) and one Bash call per unit; the implementation tokens run on codex. Knowledge records it as the cheapest implementation lane | Every spawn pays the same boot, and every implementation token bills at the tier's rate, rising from haiku through sonnet and opus to fable |
| Work per spawn | One file, or one file plus its test, at most three numbered steps, every shared interface pinned verbatim. The wrapper cancels a job still running at 540 s (exit 124); one module pair at xhigh measured 24-30 minutes | An assignment sized to finish under about 400,000 tokens, with no wall-clock cap; multi-file and cross-cutting assignments fit |
| Parallelism | Foreground spawns only, up to 6 at once over disjoint files (the doctrine cap); background fan-out is forbidden. Unreachable from an ultracode Workflow | Background spawns waited on by one `loom subagents watch`; ultracode Workflow fan-out over tens of agents |
| Context | A shell plus the navigation kit (`loom map --find-all`, `--outline`, `--impact`, `loom knowledge context`), each sub-second, so briefs name anchors. In a worktree the kit answers from the BASE layer and misses earlier units' edits. Doctrine is `~/.codex/AGENTS.md` plus the prompt, never CLAUDE.md | Read, Grep, and Glob on the live tree, CLAUDE.md, the subagent preamble, and loom skills through the Skill tool; fits exploring an unread seam |
| Checks it can run | Compile-only at most: the companion sandbox mounts the stage TMPDIR read-only, so fixture-backed tests fail at setup. The orchestrator runs the unit's tests right after it | One narrowly scoped check, run once, fixture-backed tests included |
| Guardrails | Loom's hooks never see the commands codex runs, and codex runs `workspace-write` with approval `never`: no `git` and no `.loom/` path are prose rules, backed by the orchestrator's `git status --short` after each run | Loom's hooks guard every Bash call |
| Escalation | Two tiers. A timed-out unit is re-split, never re-forwarded as is; work that cannot be cut to unit size, or needs architectural judgment, moves to Claude | Sonnet to opus to fable on evidence, and `loom-advisor` (fable) after a repeated failure |
| Heartbeat | A forward is one long Bash call: raise `subagent_timeout_secs` and read `appears hung` as advisory | SubagentStop refreshes the parent's heartbeat |
| Availability | Needs the codex CLI and plugin; on Linux also `exclude_slash_tmp` in `~/.codex/config.toml` (`loom repair --fix`). Missing at run time, the signal reroutes codex-tier work to sonnet | Always available |

Sources: BLOCK-A (subagent preamble), BLOCK-B (`SKILL.md` Section 4), and knowledge
`architecture/codex-concurrency.md`, `architecture/owned-waits.md` (unit deadline, unit verification
limit), `architecture/codex-plugin.md`, `concerns/codex-heartbeat-starvation.md`,
`mistakes/codex-navigation.md`, `mistakes/codex-worker-briefing.md`.

## Per-stage lane rule

For each standard stage with implementation work, split the work into pieces and judge every piece
against both lanes.

**Codex** when all of these hold:

- it splits into units of one file, or one file plus its test, at most three steps each;
- every interface a unit shares with another unit is pinned in the plan;
- its acceptance is mechanical: the orchestrator proves each unit right after it with a compile and
  the module's tests;
- it needs no `.loom/` path and no git.

Well-specified implementation, integration tests, boilerplate, scaffolding, and simple unit tests
usually pass, and they make up a large share of implementation work.

**Claude** when any of these holds:

- the change is unknown until someone explores the code, or the design is unsettled;
- several files must change together and cannot be cut into pinned single-file units;
- the piece converges only by running fixture-backed tests, which a codex unit cannot run;
- visual or UI design (fable, per BLOCK-B);
- debugging, or a fix that already failed once;
- architectural judgment (opus, whatever the list says).

Then write the stage's list, preferred lane first:

| Stage's work | `implementers` |
| --- | --- |
| Mostly codex-fit pieces | `["codex", "claude"]` |
| Mostly Claude-fit pieces, with a codex-fit slice | `["claude", "codex"]` |
| No codex-fit piece, or codex unavailable | `["claude"]` |
| knowledge, knowledge-distill, integration-verify | Omit the field; preflight warns on codex there |

Keep `claude` in a codex-first list: a unit that cannot be cut to size moves to it. When codex is
available, write the list on every standard stage, `["claude"]` included, so each choice is visible;
an omitted field parses as `["claude"]`. A listed lane is AVAILABLE, never mandatory. Never write a
bare scalar (`implementers: codex`): it fails to parse, and an empty list fails validation.

## Availability and the one question

After the stage list is settled and before writing stage YAML:

1. Check the lane: `command -v codex` finds the CLI (loom also looks in the usual install dirs such
   as `~/.bun/bin` and `~/.local/bin`), and `claude plugin list --json` lists `codex@openai-codex`.
2. Plugin missing: ask permission, then run BOTH `claude plugin marketplace add openai/codex-plugin-cc`
   and `claude plugin install codex@openai-codex --scope user`. SCOPE MUST BE user OR project —
   NEVER local. Local scope writes `.claude/settings.local.json`, the one file loom rebuilds from
   scratch per worktree. `preserve_unowned_keys` now carries `enabledPlugins` and
   `extraKnownMarketplaces` through that rebuild, but it is a two-key allowlist over a
   regenerated file, not a general guarantee — user and project scope are not rewritten at all.
3. Codex unavailable (no CLI, or the user declined the install): tell the user, plan every stage on
   Claude, and never write codex into `implementers`.
4. Codex available: ask ONCE with AskUserQuestion, presenting each implementation stage's
   recommended list with a one-line reason, then apply the answer:

   ```text
   Recommended implementation lanes (confirm, or name the stages to change):
   - add-parser: ["codex", "claude"] - four single-file modules behind a pinned trait
   - fix-merge-race: ["claude"] - reproduce and debug a race whose cause is unknown
   - dashboard-panel: ["claude", "codex"] - panel design on Claude; two pinned API client files on codex
   ```

Listing codex is safe even if the executing machine might lack it: `loom run` warns at startup and
the stage signal reroutes the codex tiers' work to sonnet, so the plan needs no fallback wiring.

## Writing codex stages

1. In each codex stage's description, name the subagent and the fan-out explicitly, e.g. "Spawn N
   `loom-codex-forwarder` subagents in the FOREGROUND, each with the tier-appropriate model —
   `--model gpt-5.6-terra` (common implementation, integration tests) or `--model gpt-6-luna`
   (boilerplate, scaffolding, simple unit tests) — always `--effort xhigh`, an explicit Bash
   timeout of 600000 ms (the Bash tool's maximum), and a DISJOINT file set; verify and commit yourself." (The forwarder is
   loom's own shim; never spawn the plugin's `codex:codex-rescue`
   directly — plugin agents' tools restriction is ignored by design, so that wrapper runs
   unrestricted. The orchestrator's signal carries the sentinel and evidence-trailer protocol;
   plans do not need to restate it.) When a stage mixes lanes, say which work goes to which lane, and put EVERY subagent —
   both lanes — in ONE file-ownership table. File exclusivity is enforced across lanes: a codex
   agent and a sonnet agent writing the same file is lost work exactly as two agents in one lane
   would be.

   **⚠️ SIZE EVERY CODEX UNIT FOR THE 540 s DEADLINE.** The forwarding wrapper cancels a job still
   running at 540000 ms and exits 124 `timed_out`; the 600000 ms Bash timeout above only keeps the
   harness from backgrounding the call. A unit is ONE file, or one file plus its test, at most three
   numbered steps, with every interface it shares with a parallel unit pinned verbatim in its brief
   so they compile together. One module pair at xhigh has measured 24-30 minutes, several times the
   deadline. A timed-out unit is re-split against the partial tree, never re-forwarded as is. Work
   that cannot be cut this small belongs to a Claude subagent.

   **⚠️ CODEX UNITS NAME ANCHORS, NOT EXHAUSTIVE DETAIL.** A codex subagent is `gpt-5.6-terra` or
   `gpt-6-luna` with a shell, and no Read tool — but it is not blind: loom's forwarding wrapper
   hands every codex prompt the source-graph navigation kit (`loom map --find-all`, `--outline`,
   `--impact`, `loom knowledge context --query`), each answering in under a second. It looks up a
   signature, a call site, or a surrounding pattern itself, so the plan does not paste them. In a
   worktree these commands cannot refresh their cache (it lives outside the worktree's sandbox), so
   they print `warning: could not refresh ...` and answer from the published BASE layer — codex
   cannot see edits made during the run. When a unit depends on a file an EARLIER unit in the same
   stage changed, name that file and say what changed rather than assume codex can look it up.

   A codex unit's YAML detail block names:

   - The files it owns (write) and may read.
   - Its entry points BY NAME — the symbols and files to start from, e.g. "start from `foo` in
     `path/to/bar.rs`", not the function's body.
   - What done means and the exact command that proves the slice works.
   - Constraints the graph cannot show it — invariants, an ordering that matters, a trap the
     knowledge files record, anything where the obvious reading is wrong.

   Calibration: a codex unit's spec is about as long as the sonnet equivalent. An anchored unit is
   well specified at any length; an unanchored one is not.
2. NEVER put a `.work/` or `.loom/` path in a codex stage's `files:` list or its description. Codex
   runs with sandbox `workspace-write` and approval policy `never` — it edits anything under the
   git root without asking, and in a worktree `.loom/work` (or the legacy `.work`) is a SYMLINK to
   state shared with every parallel stage. That is the one write inside the boundary that escapes it.
3. Write the stage description so it tells its codex subagents NOT to run `git` at all, and tells
   the orchestrator to check `git status --short` after each codex run. Loom's hooks guard Claude
   Code's Bash tool, not commands codex runs inside its own session, so for the codex lane those
   rules are prose rather than enforcement and the orchestrator is the only backstop.
4. Brief codex units against the lane's recorded defects, and have the orchestrator's gate after
   each unit or wave catch them (knowledge `mistakes/codex-worker-briefing.md`,
   `mistakes/codex-navigation.md`): codex units in `web/` do not run `oxfmt`; `cargo fmt` after a
   unit can push a file past a size limit, so brief a 10% margin; TypeScript parameter properties
   break `erasableSyntaxOnly`; literal braces inside `format!` must be doubled; a CLI placeholder in
   a rustdoc comment goes in backticks; a new file lands at mode 0664, so a script needs `chmod +x`.
   The task text carries no apostrophe, no `complete` substring, and not the phrase `loom stage`,
   or the forward is blocked.

## Codex pre-STOP checks

```text
□ Codex availability checked; if available, ONE AskUserQuestion confirmed a per-stage lane recommendation with a one-line reason each, and every `implementers:` list matches the answer; if unavailable, the user was told and no stage lists codex
□ Lanes follow the per-stage rule: codex-fit pieces (single-file units, pinned interfaces, mechanical acceptance) license codex; exploration, multi-file iteration, fixture-backed test convergence, UI design, and debugging license Claude; codex never on bookend stages; every list is a non-empty YAML sequence with no repeated lane
□ Every codex unit fits the 540 s wrapper deadline (one file or a file plus its test, at most three steps, shared interfaces pinned verbatim) and names its anchors — files owned/read, entry points by symbol name, done-condition and proof command, and any constraint the graph can't show. An unanchored codex block ("refactor the merge path") is underspecified regardless of length: codex has the source-graph navigation kit (`loom map`, `loom knowledge context`) but not your intent
□ Every codex subagent prompt states an explicit Bash timeout (600000 ms, the tool's maximum) alongside the tier-appropriate model — `--model gpt-5.6-terra` (common implementation, integration tests) or `--model gpt-6-luna` (boilerplate, scaffolding, simple unit tests) — always `--effort xhigh`; without it the wrapper's single Bash call hits the 120s default and the harness backgrounds the run
```
