# Codex Implementers

Read when: the user may want routine implementation routed to codex, or a stage lists codex in `implementers`.

BEFORE writing any stage YAML, ask the user ONCE with AskUserQuestion: "Route routine
implementation to Codex (gpt-5.6-terra for common implementation and integration tests,
gpt-6-luna for boilerplate, scaffolding, and simple unit tests, both xhigh) instead of
Claude subagents?" with options "Codex implementers" and "Claude implementers (sonnet)".
Never assume — the default is Claude.

Listing codex in `implementers` is safe even if the executing machine might lack codex: at run
time loom detects a missing CLI/plugin, warns the user at `loom run` startup, and the stage
signal reroutes the codex tiers' work to sonnet — so the plan needs no fallback wiring of its own.

**`implementers` is a LIST of licensed lanes, not a mode switch.** It names which lanes a stage's
orchestrator may spawn subagents from, in preference order — the first is what routine
implementation reaches for. Listing a lane makes it AVAILABLE, never mandatory: the orchestrator
still chooses per subagent by what each piece of work needs. `["codex", "claude"]` is the normal
shape for an implementation stage that sends routine work to codex while keeping sonnet for tests
and opus for the hard parts. Do NOT write a bare scalar (`implementers: codex`) — it fails to
parse, and an empty list fails validation.

If the user picks Codex:

1. Check it is installed: `claude plugin list --json`.
2. If missing, ask permission, then run BOTH `claude plugin marketplace add openai/codex-plugin-cc`
   and `claude plugin install codex@openai-codex --scope user`. SCOPE MUST BE user OR project —
   NEVER local. Local scope writes `.claude/settings.local.json`, the one file loom rebuilds from
   scratch per worktree. `preserve_unowned_keys` now carries `enabledPlugins` and
   `extraKnownMarketplaces` through that rebuild, but it is a two-key allowlist over a
   regenerated file, not a general guarantee — user and project scope are not rewritten at all.
3. If the user declines the install, fall back to Claude implementers and SAY SO. Never write
   codex into `implementers` for a plugin that is not installed.
4. List codex in `implementers` on standard stages whose work includes routine implementation.
   Put it FIRST (`["codex", "claude"]`) when routine implementation is the bulk of the stage;
   put it second (`["claude", "codex"]`) when the stage is mostly architecture or debugging but
   still has a routine slice worth delegating. LEAVE IT OFF (default `["claude"]`) for stages
   that are entirely judgment work, and for knowledge / knowledge-distill / integration-verify
   stages — preflight warns if codex appears on any of those.
5. In those stages' descriptions, name the subagent and the fan-out explicitly, e.g. "Spawn N
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
   well specified at any length; an unanchored one is not. Route work to codex when its acceptance
   is mechanical and checkable; anything needing architectural judgment belongs on opus regardless
   of lane.
6. Omitting the field is always safe: a stage without it runs on the Claude lane.
7. NEVER put a `.work/` or `.loom/` path in a codex stage's `files:` list or its description. Codex
   runs with sandbox `workspace-write` and approval policy `never` — it edits anything under the
   git root without asking, and in a worktree `.loom/work` (or the legacy `.work`) is a SYMLINK to
   state shared with every parallel stage. That is the one write inside the boundary that escapes it.
8. Write the stage description so it tells its codex subagents NOT to run `git` at all, and tells
   the orchestrator to check `git status --short` after each codex run. Loom's hooks guard Claude
   Code's Bash tool, not commands codex runs inside its own session, so for the codex lane those
   rules are prose rather than enforcement and the orchestrator is the only backstop.

## Codex pre-STOP checks

```text
□ Codex opt-in asked and answered; `implementers:` lists codex only where routine implementation is delegated, only if the plugin is installed, and never on bookend stages; every list is a non-empty YAML sequence with no repeated lane
□ Every codex unit fits the 540 s wrapper deadline (one file or a file plus its test, at most three steps, shared interfaces pinned verbatim) and names its anchors — files owned/read, entry points by symbol name, done-condition and proof command, and any constraint the graph can't show. An unanchored codex block ("refactor the merge path") is underspecified regardless of length: codex has the source-graph navigation kit (`loom map`, `loom knowledge context`) but not your intent
□ Every codex subagent prompt states an explicit Bash timeout (600000 ms, the tool's maximum) alongside the tier-appropriate model — `--model gpt-5.6-terra` (common implementation, integration tests) or `--model gpt-6-luna` (boilerplate, scaffolding, simple unit tests) — always `--effort xhigh`; without it the wrapper's single Bash call hits the 120s default and the harness backgrounds the run
```
