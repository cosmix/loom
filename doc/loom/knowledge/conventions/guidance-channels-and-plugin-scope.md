---
---
# Guidance Channels And Plugin Scope

> Guidance, plugin scope

## Vendored Agent Assets Live at Repo Root

Claude slash commands and Codex skills shipped by loom live in source at the repo root, NOT under `loom/`:

- `commands/*.md` — Claude slash commands (installed to `~/.claude/commands/`)
- `codex/skills/<name>/SKILL.md` — Codex skills (installed to `~/.codex/skills/<name>/`)

`install.sh` asserts the required source files exist before copying and fails the install if any are missing.

## Guidance Delivery Channels Convention

Agent guidance lives in the channel that delivers it closest to the decision point, cheapest:

- **Hooks** (`loom-hooks/*.sh`) — rules that must never be violated (plans path, all-files staging, commit/complete, worktree isolation, subagent verification). Deterministic; the exit-2 message re-injects the rule at the exact moment of violation.
- **Stage signals** (`orchestrator/signals/`) — stage-execution mechanics (completion checklist, adversarial review dimensions). Delivered per-stage at execution time.
- **Skills** (`skills/*/SKILL.md`) — task-scoped expertise loaded on demand (`loom-plan-writer` owns ALL plan-authoring mechanics: YAML, working_dir, acceptance design, model selection, parallelization).
- **CLAUDE.md.template** — only cross-cutting rules and the 6-item hard-stop tier (stated verbatim at top AND bottom; middle of a long file is a retrieval dead zone). Do not restate what a hook, signal, or skill already delivers — duplicated guidance drifts and dilutes.

When adding new guidance, pick the channel first; the template is the channel of last resort.

**Hook versus prose — how to choose (2026-07-28).** Prose is advice an agent may reason its way
around; a hook is a wall. Escalate a rule to a hook when _all_ of these hold:

1. The violation is **cheap to detect mechanically** — a command shape, a path, a file state.
2. The violation is **expensive or irreversible** once it happens (lost work, corrupted state,
   a security relaxation granted wrongly).
3. Prose has **already failed**, or the rule contradicts a stronger instinct the agent has.

The plans-path rule is the worked example: it was the one hard rule with no hook enforcement and
it kept being violated, because "write the plan where you were told" loses to the harness's own
suggestion. Adding `plans-path-guard.sh` ended it.

Two obligations come with choosing the hook channel:

- **The prose does not go away — it must AGREE.** An enforcement layer landed without updating
  the guidance layer produces surfaces that actively instruct the blocked behaviour, and the
  agent obeys the instruction and hits a wall it was told to walk into. After adding a hook,
  sweep every prose surface for wording the hook now retires.
- **The refusal message is the guidance.** It is read at the exact moment of the mistake, so it
  must state the rule, the allowed alternative, and the carve-out — not just "blocked".

Corollary for exceptions: an exception must live in **every block that gets copied into a
subagent prompt**, not only in the prose that explains the rule.

## Claude Code Plugin Scope in Loom Repos

Enable Claude Code plugins at **user or project scope**: `claude plugin install codex@openai-codex --scope user`.
Scope decides the file: user is `~/.claude/settings.json`, project is `.claude/settings.json`, local is
`.claude/settings.local.json`.

**Why.** Every loom-spawned session, whatever its kind, launches with `--settings <work_dir>/capsules/<session-id>.settings.json --setting-sources user,project`
(`session_capsule` and `capsule_from` in `loom/src/orchestrator/terminal/native/capsule.rs`; the flags
are emitted when the installed claude advertises them). Claude Code therefore loads the user and
project files and never the local one, so a plugin enabled at user or project scope loads in every
session with no further condition. A plugin folder `~/.claude/skills/<name>/` that holds
`.claude-plugin/plugin.json` is auto-enabled at user scope as `<name>@skills-dir` and still loads under
a capsule that carries `enabledPlugins` (checked against Claude Code 2.1.295).

**What loom writes per spawn.** `write_session_capsule` (`native/session_settings.rs`) writes the
capsule, file mode 0600 in a 0700 directory, and `cleanup_session_settings` removes it when the
session ends. `capsule_settings` (`native/session_settings/contents.rs`) builds it over
`sandbox::build_settings` (`loom/src/sandbox/settings.rs`), which only builds a document and writes
nothing. Spawning writes no `.claude/settings.local.json`: `install_required_hooks`
(`orchestrator/core/stage_executor.rs`) only refuses a spawn that has no hooks directory.

**What survives.** `build_settings` ends in `preserve_unowned_keys`, which copies
`PRESERVED_SETTINGS_KEYS = ["enabledPlugins", "extraKnownMarketplaces"]` (`sandbox/settings.rs`) from
the main checkout's `.claude/settings.local.json` (read by `read_checkout_settings`) into the capsule.
It runs only when `SettingsTarget::carry_plugin_keys` is set, which `capsule_settings` sets from
`implementers.includes_codex()`, and it fills only keys the generated document lacks. A local-scope
plugin therefore reaches a session only on a stage whose `implementers` include codex. Apart from
its `permissions.deny` rules (`merge_existing_permissions`), every other key in that file (`env`,
custom `hooks`, ...) never reaches the capsule; see
[Sandbox & Settings](../mistakes/sandbox-and-settings.md).

**What still touches `.claude/settings.local.json`.** `loom init` (`ensure_loom_permissions`) and
`loom repair --fix` (`fix_hooks_local`, which calls `ensure_loom_hooks_local`) merge loom hooks, the
agent-teams env var and `worktree.bgIsolation: "none"` into the main checkout's file for interactive
sessions there, leaving other keys, `enabledPlugins` included, alone. `loom repair --fix` also strips
the keys every capsule carries (`loom_written_keys_issue` in `commands/repair/settings_checks.rs`
detects them, `strip_settings_local_loom_keys` in `sandbox/config/preflight/local_settings.rs` strips them). No loom-spawned session loads the file.

Verify with a stage running; never assume:

```bash
claude plugin list --json                                # the plugin is listed as enabled, with its scope
rg -n "enabledPlugins" .loom/work/capsules/              # main checkout: the plugin keys a live session was handed
rg -n "enabledPlugins" .claude/settings.local.json       # main checkout: the source file those keys are copied from
```

## Keep one contribution guide at the project root

The root CONTRIBUTING.md is the canonical contribution guide. The former nested guide was folded into it and removed at the user’s direction. Preserve its useful release, signing, verification, security, and contributor instructions while resolving duplicates against current tooling. During consolidation, correct the stale test-filter advice: cargo test accepts one positional filter and rejects additional filters; a wrong single filter can still match zero tests.

## Cumulative series changelog coverage

The user requested all substantial user-facing 0.8.x work in the cumulative series section, including post-tag changes. Keep post-tag changes explicitly marked unreleased and preserve the last release date rather than inventing a release. Verify release membership with tag ancestry before labeling an entry unreleased: the changelog command commit d4078f31 is already in v0.8.7. Verify claims against focused source ranges: initialization recovery removes specific stale Git lock files and backs up unreadable configuration; it does not delete arbitrary .git files or directories.

## Small complete changes across executors

The engineering sections in `CLAUDE.md.template` and `AGENTS.md.template` prioritize complete behavior and affected consumers, then reuse of project code, standard/platform facilities, installed dependencies and clear new code. Required validation, data-loss handling, security and accessibility remain constraints. Codex forwarding carries a compact implementation contract even without installed doctrine; Claude worker agents apply the same choices in their existing approach sections.

`skills/loom-plan-writer/SKILL.md` grounds scope and reuse before decomposition, groups exploration, and defaults the canonical example to one implementation stage. The full example is in `references/plan-template.md`, read when authoring YAML; sandbox configuration/provisioning detail stays in `references/sandbox.md`. Pinned model-allocation text and stage verification/ownership boundaries remain intact.

`skills/loom-code-review/SKILL.md`, `agents/loom-code-reviewer.md` and `signals/cache/blocks.rs` require evidence from affected flows and expected load. Review recommendations name a concrete defect or rule and the smallest adequate repair. Loom stage findings still block regardless of severity; suggestions do not. Known limitations carry a revisit condition in existing comments or memory, without a new debt subsystem.

Design reference: [Ponytail core and review skills](https://github.com/DietrichGebert/ponytail/tree/6f1c2bfff0c837d268929a7c4e490918eb4fab29/skills), MIT, Copyright (c) 2026 DietrichGebert. Loom adapts the decision rules in its existing delivery channels; it does not install Ponytail hooks, modes or telemetry. Upstream benchmark percentages are not evidence of Loom or Codex savings. Compare prompt bytes separately from measured cost per accepted change.

## Loom Assets Outside ~/.claude Need a Plugin

Loom installs its Claude assets (agents, skills, commands, hooks, the global `CLAUDE.md`) into `~/.claude` and its Codex assets into `~/.codex`, nowhere else. Claude Code discovers agents, skills and the global `CLAUDE.md` only under its config dir (`CLAUDE_CONFIG_DIR`, else `~/.claude`), the project's `.claude/`, plugins and per-launch flags; Codex reads only `CODEX_HOME`, else `~/.codex`. A loom-chosen directory elsewhere reaches no session (see "A relocated Claude root never reached stage sessions" in [computed-values-and-hidden-couplings](../mistakes/computed-values-and-hidden-couplings.md)). Per-launch bridges were rejected: a symlinked `--add-dir` directory, inline `--agents` JSON per session, and pointing sessions at `CLAUDE_CONFIG_DIR`, which only helps an operator whose Claude Code already runs from that directory.

A Claude Code plugin is the native way to load assets from an arbitrary directory, for operator and stage sessions alike. Facts from the Claude Code 2.1.302 docs (<https://code.claude.com/docs/en/plugins-reference>, /sub-agents, /skills, /memory, /cli-reference):

- `--plugin-dir <dir>` loads a plugin for one session; a folder of plugins loads each child. A user-scope install needs a marketplace entry plus `enabledPlugins` in `~/.claude/settings.json`, so registration still writes into `~/.claude`.
- The manifest `.claude-plugin/plugin.json` is optional; the name defaults to the directory name. Skills come from `skills/` (or the manifest `skills` key, which can name extra skill directories), agents from `agents/`.
- Plugin skills and agents are namespaced: `/plugin:skill`, `plugin:agent`. For plugin agents the `agent_type` in `SubagentStart`/`SubagentStop` payloads is the scoped id (`loom:loom-code-reviewer`); user and project agents report the bare frontmatter `name`.
- A `CLAUDE.md` at the plugin root is NOT loaded, so the global rules need another channel (a `SessionStart` hook's `additionalContext`, or `--append-system-prompt-file` for loom-spawned sessions).
- Agent precedence on a name clash: managed settings > `--agents` > `.claude/agents/` > `~/.claude/agents/` > plugin agents.
- Claude Code also auto-loads a plugin folder at `~/.claude/skills/<name>/` as `<name>@skills-dir` (used by `loom-ui`); that location is still inside `~/.claude`.

What a plugin would cost: every exact match on a bare agent or skill name changes, including `REVIEWER_AGENT_TYPE` in `loom/src/verify/review/gate.rs` and `loom-hooks/subagent-stop.sh`, the `loom-codex-forwarder` checks in `loom-hooks/codex-forward-guard.sh`, signal doctrine (`Skill(skill="loom-orchestration")`, spawn-by-type lines in `orchestrator/signals/`), `CLAUDE.md.template`, the skills, and usage accounting's agent-type sections. The global rules need the separate channel above. A launch-level test must prove a loom-spawned session lists the namespaced agents and records a review round from one.

## Implementers Do Not Verify; the Gate Is loom check

Implementers do not verify. A subagent may run **at most one narrowly-scoped check** relevant to the files it just changed; project-wide builds, full test suites, and repo-wide lint or typecheck runs belong to the stage's gate.

Enforced by `loom-hooks/subagent-verify-guard.sh` (PreToolUse:Bash), stated in the subagent preamble `loom-hooks/_subagent-preamble.txt` (which `loom-hooks/spawn-guard.sh` prepends to every typed spawn; `CLAUDE.md.template` Rule 5 says not to paste it), and injected into stage signals by `orchestrator/signals/cache.rs` (standard and integration-verify prefixes). The `BLOCK_A` copies are pinned byte-for-byte across the two prefixes, the preamble and the guard by `tests_doctrine.rs::block_a_agrees_across_every_surface`.

**Who runs the gate.** No verifier subagent exists. In a standard stage the orchestrator runs `loom check <stage-id>` once after the implementers return; `ORCHESTRATOR_GATE` (`orchestrator/signals/cache/blocks.rs`) opens the completion list with that rule and is the main agent's only copy of the `loom check` flow: `skills/loom-orchestration/SKILL.md` no longer repeats the gate paragraph, and `tests_guidance_manifest.rs` pins the `loom check <stage-id>` and `--criterion <n>` needles in the standard stable prefix. Template Rule 14's fifth bullet logs verbose output to a file and prints the command's own exit code (`<cmd> > <log> 2>&1; echo "exit=$?"`, then `rg -n` the log); piping the command through `tail` hides its exit code, and `loom` commands are never redirected. A fix round runs `loom check <stage-id> --criterion <n>`. The loom-code-reviewer round judges the diff against each criterion's intent. The orchestrator delegates fixes, commits, and runs `loom stage complete`. An integration-verify stage follows `INTEGRATION_VERIFY_OVERRIDE` (`orchestrator/signals/cache.rs`): the orchestrator runs `loom check integration-verify` once, which carries the complete suite; reviewers run only targeted discriminating checks, which never substitute for that gate.

**One hook carve-out:** `subagent-verify-guard.sh` lets integration-verify subagents through, resolved from the stage file. It **fails safe**: more than one glob match, a non-integration-verify stage type, or a missing file all mean "no relaxation". The subagent preamble carries the matching EXCEPTION bullet outside BLOCK-A, which tells integration-verify subagents that the orchestrator runs `loom check integration-verify` itself and that reviewers run only targeted checks (`loom-hooks/_subagent-preamble.txt:18-20`).
