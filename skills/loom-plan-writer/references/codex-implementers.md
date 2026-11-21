# Codex Implementers and Lane Choice

Read when: choosing each stage's implementation lanes, or a stage lists codex in `implementers`.

Two lanes implement a stage's work. Codex takes implementation that is straightforward and well
specified, when the stage licenses it, codex is available and the subscription has quota room. Codex
also takes a bounded exploratory piece inside the files it owns, using the navigation kit. Claude
takes the rest: open-ended exploration across modules, unknown-cause debugging, failing-test
convergence, verification, review and planning. No lane is first or default: judge each piece by its
nature and by quota, and license accordingly; the stage's orchestrator picks the lane per piece at
spawn time.

- **Codex lane:** one codex unit per job, running gpt-6.1-terra (implementation, integration
  tests, boilerplate, scaffolding, simple unit tests), gpt-6.2-sol (a substantial well-specified unit
  (about 4 to 6 file-sized changes) or a terra unit that failed once), or gpt-6-luna only when the
  user or the plan explicitly asks for it, always at effort xhigh. A plan names the implementer,
  `codex`, never the launch mechanism: the `codex.backend` setting picks it (see Backends) and the
  stage signal carries its commands.
- **Claude lane:** `loom-software-engineer` (sonnet), `loom-software-developer` (haiku, routine work),
  `loom-senior-software-engineer` (opus, effort medium; respawned at `effort: high` after a failed attempt), and fable by an explicit model override.

## Lane comparison

| Concern | Codex lane | Claude lane |
| --- | --- | --- |
| Claude-side cost | A few orchestrator calls per job (plus a haiku shim of about 20,000 tokens on the legacy backend); the implementation tokens run on codex. Knowledge records it as the cheapest implementation lane | Every spawn pays the same boot, and every implementation token bills at the tier's rate, rising from haiku through sonnet and opus to fable |
| Work per spawn | One concern: code, consumers and tests together in dependency order, possibly several files. Peak context at most about 250,000 tokens, well inside 1500000 ms; timeout exits 124 | About 400,000 tokens is a ceiling; measured sonnet workers peak at a median of 86,000. Bundle short tasks by the cost rule in stage-sizing.md |
| Parallelism | Up to 6 units at once over disjoint files (the doctrine cap), started only by the stage's main agent. Unreachable from an ultracode Workflow | Background spawns waited on by one `loom subagents watch`; ultracode Workflow fan-out over tens of agents |
| Context | A shell plus the navigation kit (`loom map --find-all`, `--outline`, `--impact`, `loom knowledge context`), each sub-second, so briefs name anchors. In a worktree the kit answers from the BASE layer and misses earlier units' edits. Doctrine is `~/.codex/AGENTS.md` plus the prompt, never CLAUDE.md | Read, Grep, and Glob on the live tree, CLAUDE.md, the subagent preamble, and loom skills through the Skill tool; fits exploring an unread seam |
| Checks it can run | Compile-only at most: the companion sandbox mounts the stage TMPDIR read-only, so fixture-backed tests fail at setup. The orchestrator runs the unit's tests right after it | The tests for the code it wrote and its module, fixture-backed tests included, never the whole suite |
| Guardrails | Loom's hooks never see the commands codex runs, and codex runs `workspace-write` with approval `never`: no `git` and no `.loom/` path are prose rules, backed by the orchestrator's `git status --short` after each run | Loom's hooks guard every Bash call |
| Escalation | Terra, then sol, then Claude at the tier the piece needs. A timed-out unit is re-split, never restarted as is; work that cannot fit the job limits, or needs architectural judgment, moves to Claude | Haiku, sonnet, opus at medium, opus at high, then `loom-advisor` (fable) after the opus failure at high |
| Heartbeat | Backend-specific; the stage signal says how the lead waits | SubagentStop refreshes the parent's heartbeat |
| Availability | Needs the codex CLI and the configured backend's requirements (see Backends). Missing at run time, the signal reroutes terra- and sol-tier work to sonnet, and boilerplate and simple unit tests to haiku | Always available |

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

- the change needs open-ended exploration across modules, or the design is unsettled (a bounded
  exploratory piece inside the files a unit owns, using the navigation kit, stays with codex);
- the piece converges only by running fixture-backed tests, which a codex unit cannot run;
- visual or UI design (fable, per orchestration Rule 7);
- unknown-cause debugging, or a fix that already failed on sol (a terra unit that failed once goes to sol first);
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
   as `~/.bun/bin` and `~/.local/bin`); on the legacy backend only, `claude plugin list --json`
   also lists `codex@openai-codex`.
2. Legacy backend with the plugin missing: ask permission, then run BOTH `claude plugin marketplace add openai/codex-plugin-cc`
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

1. In each codex stage's description, name the implementer and the fan-out explicitly, e.g. "Start N
   codex units, each with the tier-appropriate model — `gpt-6.1-terra` (implementation, integration
   tests, boilerplate, scaffolding, simple unit tests), `gpt-6.2-sol` (a substantial well-specified
   unit or a terra unit that failed once) or, only when the user or plan asks for luna,
   `gpt-6-luna` — always effort xhigh, and a DISJOINT file set; run `loom check` for the gate, then
   commit yourself." In a wave or ownership table the implementer column says `codex`. Never name a
   launch mechanism (an agent type, a plugin command, a wrapper script or a Bash timeout): the stage
   signal carries the launch commands for the configured backend, and a mechanism written into the
   plan can contradict them. When a stage mixes lanes, say which work goes to which lane, and put
   EVERY unit — both lanes — in ONE file-ownership table. File exclusivity is enforced across lanes:
   a codex unit and a sonnet worker writing the same file is lost work exactly as two agents in one
   lane would be.

   **Size each job as one concern.** Keep code, consumers and tests together in dependency order
   in one brief, across several files where cohesive. Two jobs never write one file. At high effort,
   each file-sized change takes about 3 minutes and 50,000 tokens of context on gpt-6.2-sol, or
   1 minute and 20,000 tokens on gpt-6.1-terra. Each job starts near 23,000 tokens. At xhigh,
   budget twice the time. Keep peak context at most about 250,000 tokens and finish well inside
   the wrapper's 1500000 ms deadline: about 4 to 6 file-sized changes on sol, about 12 on terra.
   Loom cancels a unit at the deadline. Re-split a timed-out job against its partial tree; never
   restart it as is.

   Fill a job only as far as this pays. Every turn re-reads the whole context, so cost grows
   with the square of its length: 4 jobs of 6 changes cost about 1.8 times the Codex tokens of
   24 jobs of one. Claude quota is scarcer: each job costs orchestrator turns (and about 20,000
   haiku tokens on the legacy backend), which justifies cohesive bundles. Never bundle unrelated concerns
   to reach a size.

   Order work inside a job first. A later wave exists only when a job needs another job's
   result on disk. Never slice a list of jobs into waves by the cap of 6.

   Write each job's brief to a file inside the stage worktree and hand the unit its absolute path,
   keeping the orchestrator prompt short. Without a committed brief, the orchestrator writes one
   under the worktree scratch directory first. The brief names files owned/read, entry symbols,
   the done condition and proof command, and traps. A stage's declared skills are for Claude
   workers: quote applicable rules in the Codex brief.

   **⚠️ CODEX UNITS NAME ANCHORS, NOT EXHAUSTIVE DETAIL.** A codex subagent is `gpt-6.1-terra`,
   `gpt-6.2-sol` or, on request, `gpt-6-luna` with a shell, and no Read tool — but it is not blind: loom
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
   A brief path carries no apostrophe, no `complete` substring, and not the phrase `loom stage`,
   or the launch is blocked.

## Backends

The `codex.backend` setting (`loom config -k codex.backend`, or the project's `.loom/work/config.toml` `[codex] backend`) picks how a stage runs its codex units; the plan is the same under either. `native`, the default, has the stage's main agent start each unit with `loom subagents start --provider codex` as a host-owned worker. `legacy` has it spawn loom's forwarder shim. Each backend refuses the other's launch mechanism and names its own, so a mechanism written into a plan only misleads the stage's lead. Facts that shape a plan under either backend:

- Only the stage's main agent starts codex units: a coordinator or an ultracode Workflow cannot, so no codex unit sits under a coordinator.
- A unit id matches `[A-Za-z0-9][A-Za-z0-9_-]{0,63}` (no dot), and the brief is an absolute path inside the stage worktree of at most 256 KiB.
- At most 6 units run at once; each has a 1500000 ms deadline.
- Native runs on Linux with a working bubblewrap pid namespace only. Elsewhere, or when the boundary cannot be created, the signal reports the lane UNAVAILABLE with the reason, routes codex-tier work to Claude and never falls back to the forwarder, so keep `claude` in every list that licenses codex. Legacy needs the codex plugin and, on Linux, `exclude_slash_tmp` in `~/.codex/config.toml` (`loom repair --fix`).
- Native workers run in a boundary where `.loom`, `.git` and `.claude` are read-only and no operator Codex config, rules, hooks or MCP servers load; the host refuses reset, retry, skip, commit, `loom check` and `loom stage complete` while a worker of the stage runs. The no-git and no-`.loom/` prose rules stay in every codex brief.

## Codex pre-STOP checks

```text
□ Codex availability checked; if available, ONE AskUserQuestion confirmed a per-stage lane recommendation with a one-line reason each, and every `implementers:` list matches the answer; if unavailable, the user was told and no stage lists codex
□ Lanes follow the per-stage rule: straightforward, well-specified concerns (code, consumers and tests together, pinned interfaces, mechanical acceptance) license codex; a bounded exploratory piece inside the owned files, using the navigation kit, licenses codex; open-ended exploration across modules, fixture-backed test convergence, UI design, and debugging license Claude; codex never on bookend stages; every list is a non-empty YAML sequence with no repeated lane
□ Every codex job is one concern, possibly several files, sized by measured model rates for at most about 250,000 tokens of peak context and well inside 1500000 ms; xhigh budgets twice the high-effort time. Its brief names files owned/read, entry symbols, done condition, proof command and traps; two jobs never write one file
□ Every codex unit names an absolute brief path inside the stage worktree, with no apostrophe, complete substring or loom stage phrase; declared skill rules are quoted in the brief; each unit states its tier — `gpt-6.1-terra` (implementation, integration tests, boilerplate, scaffolding, simple unit tests), `gpt-6.2-sol` (a substantial well-specified unit or a terra unit that failed once) or, only on explicit request, `gpt-6-luna` — always effort xhigh; an overrun of the 1500000 ms deadline is re-split against its partial tree
□ The plan names `codex` as the implementer and no launch mechanism: no `loom-codex-forwarder`, `codex:codex-rescue`, `codex-forward.sh` or Bash timeout in any description, wave table or ownership table
□ No wave exists only to respect the cap of 6; order work inside a job first, then use waves only for dependencies on another job's result on disk
□ Every unit id matches `[A-Za-z0-9][A-Za-z0-9_-]{0,63}` (no dot); no codex unit sits under a coordinator, because only the stage's main agent can start one; at most 6 units run at once
```
