# Codex Implementers and Lane Choice

Read when: choosing each stage's implementation lanes, or a stage lists codex in `implementers`.

Two lanes implement a stage's work. Codex takes implementation that is straightforward and well
specified, when the stage licenses it, codex is available and the subscription has quota room. Claude
takes the rest, and always does verification, review, planning, exploration, debugging and iteration
on failing tests. No lane is first or default: judge each piece by its nature and by quota, and
license accordingly; the stage's orchestrator picks the lane per piece at spawn time.

- **Codex lane:** `loom-codex-forwarder`, a haiku forwarder of about 20,000 tokens per job whose only tool is Bash, makes one
  foreground call to `codex-forward.sh`, which runs gpt-6.1-terra (implementation, integration
  tests, boilerplate, scaffolding, simple unit tests), gpt-6.1-sol (a substantial well-specified unit
  (about 4 to 6 file-sized changes) or a terra unit that failed once), or gpt-6-luna only when the
  user or the plan explicitly asks for it, always at effort xhigh. Under `codex.backend = native` the stage's main agent starts the unit itself instead; see Native backend.
- **Claude lane:** `loom-software-engineer` (sonnet), `loom-software-developer` (haiku, routine work),
  `loom-senior-software-engineer` (opus, effort medium; respawned at `effort: high` after a failed attempt), and fable by an explicit model override.

## Lane comparison

| Concern | Codex lane | Claude lane |
| --- | --- | --- |
| Claude-side cost | One haiku forwarder (about 20,000 tokens) and one Bash call per job; the implementation tokens run on codex. Knowledge records it as the cheapest implementation lane | Every spawn pays the same boot, and every implementation token bills at the tier's rate, rising from haiku through sonnet and opus to fable |
| Work per spawn | One concern: code, consumers and tests together in dependency order, possibly several files. Peak context at most about 250,000 tokens, well inside 1500000 ms; timeout exits 124 | About 400,000 tokens is a ceiling; measured sonnet workers peak at a median of 86,000. Bundle short tasks by the cost rule in stage-sizing.md |
| Parallelism | Foreground spawns only, up to 6 at once over disjoint files (the doctrine cap); background fan-out is forbidden. Unreachable from an ultracode Workflow | Background spawns waited on by one `loom subagents watch`; ultracode Workflow fan-out over tens of agents |
| Context | A shell plus the navigation kit (`loom map --find-all`, `--outline`, `--impact`, `loom knowledge context`), each sub-second, so briefs name anchors. In a worktree the kit answers from the BASE layer and misses earlier units' edits. Doctrine is `~/.codex/AGENTS.md` plus the prompt, never CLAUDE.md | Read, Grep, and Glob on the live tree, CLAUDE.md, the subagent preamble, and loom skills through the Skill tool; fits exploring an unread seam |
| Checks it can run | Compile-only at most: the companion sandbox mounts the stage TMPDIR read-only, so fixture-backed tests fail at setup. The orchestrator runs the unit's tests right after it | The tests for the code it wrote and its module, fixture-backed tests included, never the whole suite |
| Guardrails | Loom's hooks never see the commands codex runs, and codex runs `workspace-write` with approval `never`: no `git` and no `.loom/` path are prose rules, backed by the orchestrator's `git status --short` after each run | Loom's hooks guard every Bash call |
| Escalation | Terra, then sol, then Claude at the tier the piece needs. A timed-out unit is re-split, never re-forwarded as is; work that cannot fit the job limits, or needs architectural judgment, moves to Claude | Haiku, sonnet, opus at medium, opus at high, then `loom-advisor` (fable) after the opus failure at high |
| Heartbeat | A forward is one long Bash call: raise `subagent_timeout_secs` and read `appears hung` as advisory | SubagentStop refreshes the parent's heartbeat |
| Availability | Needs the codex CLI and plugin; on Linux also `exclude_slash_tmp` in `~/.codex/config.toml` (`loom repair --fix`). Missing at run time, the signal reroutes terra- and sol-tier work to sonnet, and boilerplate and simple unit tests to haiku | Always available |

Sources: BLOCK-A (subagent preamble), orchestration Rule 7, and knowledge
`architecture/codex-concurrency.md`, `architecture/owned-waits.md` (unit deadline, unit verification
limit), `architecture/codex-plugin.md`, `concerns/codex-heartbeat-starvation.md`,
`mistakes/codex-navigation.md`, `mistakes/codex-worker-briefing.md`.

## Per-stage lane rule

For each standard stage with implementation work, split the work into pieces and judge every piece
against both lanes.

**Codex** when all of these hold:

- each job covers one concern, including code, consumers and tests in dependency order, sized by the measured rates below;
- every interface a unit shares with another unit is pinned in the plan;
- its acceptance is mechanical: the orchestrator proves each unit right after it with a compile and
  the module's tests;
- it needs no `.loom/` path and no git.

Well-specified implementation, integration tests, boilerplate, scaffolding, and simple unit tests
usually pass, and they make up a large share of implementation work.

**Claude** when any of these holds:

- the change is unknown until someone explores the code, or the design is unsettled;
- the piece converges only by running fixture-backed tests, which a codex unit cannot run;
- visual or UI design (fable, per orchestration Rule 7);
- debugging, or a fix that already failed on sol (a terra unit that failed once goes to sol first);
- architecture, new algorithms, concurrency or security-sensitive code (opus, whatever the list says).

Then write the stage's list. The order carries no routing meaning:

| Stage's work | `implementers` |
| --- | --- |
| Mostly straightforward, well-specified pieces | `["codex", "claude"]` |
| Mostly Claude-fit pieces, with a straightforward slice | `["claude", "codex"]` |
| No straightforward, well-specified piece, or codex unavailable | `["claude"]` |
| knowledge, knowledge-distill, integration-verify | Omit the field; preflight warns on codex there |

List codex on a stage that has a straightforward, well-specified piece; write `["claude"]` when it
has none.

Keep `claude` in every list that licenses codex: a unit that cannot be cut to size moves to it. When codex is
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
4. Codex available: run `loom usage --quota` first. With Claude under pressure, recommend
   licensing codex wherever a stage has a straightforward, well-specified piece, and say so in the one-line reasons.
   Then ask ONCE with AskUserQuestion, presenting each implementation stage's recommended list
   with a one-line reason, and apply the answer:

   ```text
   Recommended implementation lanes (confirm, or name the stages to change):
   - add-parser: ["codex", "claude"] - four single-file modules behind a pinned trait
   - fix-merge-race: ["claude"] - reproduce and debug a race whose cause is unknown
   - dashboard-panel: ["claude", "codex"] - panel design on Claude; two pinned API client files on codex
   ```

Listing codex is safe even if the executing machine might lack it: `loom run` warns at startup and
the stage signal reroutes terra- and sol-tier work to sonnet, and boilerplate and simple unit tests to haiku, so the plan needs no fallback wiring.

## Writing codex stages

1. In each codex stage's description, name the subagent and the fan-out explicitly, e.g. "Spawn N
   `loom-codex-forwarder` subagents in the FOREGROUND, each with the tier-appropriate model —
   `--model gpt-6.1-terra` (implementation, integration tests, boilerplate, scaffolding, simple unit tests),
   `--model gpt-6.1-sol` (a substantial well-specified unit or a terra unit that failed once) or, only when the user or plan
   asks for luna, `--model gpt-6-luna` — always `--effort xhigh`, an explicit Bash
   timeout of 1560000 ms, and a DISJOINT file set; run `loom check` for the gate, then commit yourself." (The forwarder is
   loom's own shim; never spawn the plugin's `codex:codex-rescue`
   directly — plugin agents' tools restriction is ignored by design, so that wrapper runs
   unrestricted. The orchestrator's signal carries the sentinel and evidence-trailer protocol;
   plans do not need to restate it.) When a stage mixes lanes, say which work goes to which lane, and put EVERY subagent —
   both lanes — in ONE file-ownership table. File exclusivity is enforced across lanes: a codex
   agent and a sonnet worker writing the same file is lost work exactly as two agents in one lane
   would be. Under `codex.backend = native`, describe the fan-out as "start N codex units" and leave the launch to the stage signal; see Native backend.

   **Size each job as one concern.** Keep code, consumers and tests together in dependency order
   in one brief, across several files where cohesive. Two jobs never write one file. At high effort,
   each file-sized change takes about 3 minutes and 50,000 tokens of context on gpt-6.1-sol, or
   1 minute and 20,000 tokens on gpt-6.1-terra. Each job starts near 23,000 tokens. At xhigh,
   budget twice the time. Keep peak context at most about 250,000 tokens and finish well inside
   the wrapper's 1500000 ms deadline: about 4 to 6 file-sized changes on sol, about 12 on terra.
   The wrapper cancels at the deadline and exits 124 `timed_out`; the 1560000 ms foreground Bash
   timeout leaves room for its result. Re-split a timed-out job against its partial tree; never
   re-forward it as is.

   Fill a job only as far as this pays. Every turn re-reads the whole context, so cost grows
   with the square of its length: 4 jobs of 6 changes cost about 1.8 times the Codex tokens of
   24 jobs of one. Claude quota is scarcer: a haiku forwarder costs about 20,000 tokens per job,
   plus orchestrator turns, which justifies cohesive bundles. Never bundle unrelated concerns
   to reach a size.

   Order work inside a job first. A later wave exists only when a job needs another job's
   result on disk. Never slice a list of jobs into waves by the cap of 6.

   Write each job's brief to a file. Forward ONE line naming its absolute path; the forwarder
   never retypes the brief, keeping the orchestrator prompt short. Without a committed brief,
   the orchestrator writes one under the worktree scratch directory first. The brief names files
   owned/read, entry symbols, the done condition and proof command, and traps. A stage's declared
   skills are for Claude workers: quote applicable rules in the Codex brief. The wrapper marks each
   forward with `LOOM_CODEX_FORWARD=1`; loom's context-injecting hooks stay silent while guard hooks still run.

   **⚠️ CODEX UNITS NAME ANCHORS, NOT EXHAUSTIVE DETAIL.** A codex subagent is `gpt-6.1-terra`,
   `gpt-6.1-sol` or, on request, `gpt-6-luna` with a shell, and no Read tool — but it is not blind: loom's forwarding wrapper
   hands every codex prompt the source-graph navigation kit (`loom map --find-all`, `--outline`,
   `--impact`, `loom knowledge context --query`), each answering in under a second. It looks up a
   signature, a call site, or a surrounding pattern itself, so the plan does not paste them. In a
   worktree these commands cannot refresh their cache (it lives outside the worktree's sandbox), so
   they print `warning: could not refresh ...` and answer from the published BASE layer — codex
   cannot see edits made during the run. When a unit depends on a file an EARLIER unit in the same
   stage changed, name that file and say what changed rather than assume codex can look it up.

   A codex job brief names:

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
   The one forwarded line carries no apostrophe, no `complete` substring, and not the phrase `loom stage`,
   or the forward is blocked.

## Native backend

Read when: `loom config -k codex.backend` prints `native`, or the project's `.loom/work/config.toml` sets `[codex] backend = "native"` (`loom config -k` reads only the user file). The default is `legacy`, which keeps everything above unchanged.

With the native backend the stage's main agent starts each codex unit itself, and no forwarder is spawned. The plan does not change: the same `implementers` lists, tiers, one-concern sizing, disjoint file sets, cap of 6 and brief rules apply. Only the launch differs, and the stage signal's Codex Implementers section carries the commands, so a stage description says "start N codex units" and names neither the forwarder nor a Bash timeout:

```text
loom subagents start --provider codex --unit-id <unit-id> --brief <absolute brief path> --model <model> --effort xhigh
loom request status <id> --wait 90
loom subagents watch --worker codex:<unit-id> [--worker codex:<unit-id> ...] --timeout 3600
loom subagents result --worker codex:<unit-id>
```

Each `start` runs alone in its own Bash call and `loom request status` reads its verdict in another; one background `watch` covers the whole wave; `loom subagents message` and `loom subagents cancel` steer or stop a running unit. Only the stage's main agent can run them: a coordinator or an ultracode Workflow cannot start codex units. A unit id matches `[A-Za-z0-9][A-Za-z0-9_-]{0,63}` (no dot, unlike the forwarder's), and the brief is an absolute path inside the stage worktree of at most 256 KiB. At 1500000 ms the host cancels the unit; re-split it against its partial tree, as above.

What differs in the Lane comparison table:

| Concern | Native backend |
| --- | --- |
| Claude-side cost | No forwarder spawn and none of its roughly 20,000 tokens per job; the lead pays one start call, one status call and one result call per unit, plus one watch per wave. The saving is not yet measured against matched legacy jobs |
| Parallelism | Units start through a relayed command, not a foreground spawn: up to 6 run at once, one background watch covers the wave, and no 1560000 ms Bash timeout applies |
| Checks it can run | Plan for compile-only acceptance, as in the legacy lane: the worker has no network except its model provider, so a unit whose proof needs fixtures or network is a Claude piece, or the orchestrator proves it right after the unit |
| Guardrails | The same prose rules (no git, no `.loom/` path, `git status --short` after each unit), now backed by the boundary: `.loom`, `.git` and `.claude` are read-only, and no operator Codex config, rules, hooks or MCP servers load. No Loom or Claude hook runs for the worker's commands. In addition the host refuses reset, retry (even `--force`), skip, commit, `loom check` and `loom stage complete` while a worker of the stage runs |
| Heartbeat | No long foreground Bash call; the lead waits in the watch |
| Availability | Linux with a working bubblewrap pid namespace only. Elsewhere, or when the boundary cannot be created, the signal reports the lane UNAVAILABLE with the reason, routes codex-tier work to Claude and never falls back to the forwarder, so keep `claude` in every list that licenses codex. The default stays `legacy` until an operator has qualified native launch with real Codex jobs |

## Codex pre-STOP checks

```text
□ Codex availability checked; if available, ONE AskUserQuestion confirmed a per-stage lane recommendation with a one-line reason each, and every `implementers:` list matches the answer; if unavailable, the user was told and no stage lists codex
□ Lanes follow the per-stage rule: straightforward, well-specified concerns (code, consumers and tests together, pinned interfaces, mechanical acceptance) license codex; exploration, fixture-backed test convergence, UI design, and debugging license Claude; codex never on bookend stages; every list is a non-empty YAML sequence with no repeated lane
□ Every codex job is one concern, possibly several files, sized by measured model rates for at most about 250,000 tokens of peak context and well inside 1500000 ms; xhigh budgets twice the high-effort time. Its brief names files owned/read, entry symbols, done condition, proof command and traps; two jobs never write one file
□ On the legacy backend, every forwarder prompt names an absolute brief path in ONE line, with no apostrophe, complete substring or loom stage phrase; declared skill rules are quoted in the brief; the wrapper sets LOOM_CODEX_FORWARD=1 so context-injecting hooks stay silent while guard hooks still run
□ On the legacy backend, every codex subagent prompt states a foreground Bash timeout of 1560000 ms alongside the tier-appropriate model — `--model gpt-6.1-terra` (implementation, integration tests, boilerplate, scaffolding, simple unit tests), `--model gpt-6.1-sol` (a substantial well-specified unit or a terra unit that failed once) or, only on explicit request, `--model gpt-6-luna` — always `--effort xhigh`; the wrapper cancels at 1500000 ms, exits 124, and an overrun is re-split against its partial tree
□ No wave exists only to respect the cap of 6; order work inside a job first, then use waves only for dependencies on another job's result on disk
□ If the backend is `native` (`loom config -k codex.backend` or the project's `[codex] backend`): no stage description orders a forwarder spawn or a 1560000 ms Bash timeout; every unit id matches `[A-Za-z0-9][A-Za-z0-9_-]{0,63}` (no dot) and names an absolute brief path inside the stage worktree; no codex unit sits under a coordinator, because only the stage's main agent can start one; at most 6 units run at once
```
