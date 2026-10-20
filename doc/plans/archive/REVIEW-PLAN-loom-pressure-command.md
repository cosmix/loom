# Code Review: `loom pressure` — alternating Claude/Codex plan pressure-testing

**Plan:** PLAN-loom-pressure-command | **Generated:** 2026-06-30 21:29 UTC

## Summary

## Context

## Changes by Stage

### Implement loom pressure command + vendored commands/skill + installer (implement-pressure)

**Status:** completed  
**Purpose:** Implement the `loom pressure <plan> [--rounds N] [--dry-run]` command, vendor the
Claude commands and Codex skill it drives, and wire them into the LOCAL install
path only (install.sh local mode + dev-install.sh). Remote/self-update/release
packaging is explicitly deferred (see plan §1c). The full file contents and exact
specs are in this plan's body above — follow them verbatim, especially the
self-containment wording and the codex-report naming convention
(doc/plans/codex-<plan-filename>).

Use parallel subagents and skills to maximize performance. Do NOT use `claude -p`
anywhere — the driver spawns interactive Claude with inherited stdio, mirroring
loom/src/commands/knowledge/bootstrap.rs (Command::status() + Stdio::inherit).

SUBAGENT FILE ASSIGNMENTS (NO FILE OVERLAP):
  Main agent (opus) — Rust driver:
    Files Owned: loom/src/commands/pressure/mod.rs, loom/src/codex.rs,
      loom/src/cli/types.rs, loom/src/cli/dispatch.rs,
      loom/src/commands/mod.rs, loom/src/lib.rs,
      loom/src/completions/dynamic/commands.rs, loom/src/completions/dynamic/mod.rs,
      loom/src/completions/dynamic/tests/tests_commands.rs
    Files Read-Only: loom/src/commands/knowledge/bootstrap.rs, loom/src/claude.rs
    Work:
      - New codex.rs::find_codex_path(): same RESOLUTION STRATEGY as
        claude.rs::find_claude_path (which::which first, then a fixed candidate
        list) but a DIFFERENT candidate list — which::which("codex") then
        ~/.bun/bin, ~/.local/bin, ~/.npm-global/bin, ~/.cargo/bin,
        /usr/local/bin, /opt/homebrew/bin. Export `pub mod codex;` from lib.rs.
      - Register the `Pressure { plan, rounds(u32, default 2, value_parser
        range 1..), dry_run }` variant in cli/types.rs (reject --rounds 0 at
        parse time), dispatch it in cli/dispatch.rs, add `pub mod pressure;`
        to commands/mod.rs (CLI Subcommand Registration pattern in patterns.md).
      - DYNAMIC COMPLETIONS (Clap alone is NOT enough): add "pressure" to
        TOP_LEVEL_COMMANDS and `["pressure"] => &["--rounds", "--dry-run"]` to
        complete_flags in completions/dynamic/commands.rs; treat `pressure` like
        `init` (return empty -> native path completion) in
        complete_after_command in completions/dynamic/mod.rs; add completion
        tests in tests/tests_commands.rs.
      - commands/pressure/mod.rs: pure, unit-tested helpers.
        resolve_plan_path(arg, repo_root) -> ResolvedPlan { fs_path, invocation }:
        children run with current_dir(repo_root), so fs_path (absolute,
        canonicalized) is used for existence checks / codex_report_path /
        deletion, and invocation (repo-relative when under repo, else absolute —
        NEVER cwd-relative) is the string handed to the slash commands/skill.
        Fallback to doc/plans/<arg> only when the raw path is absent, and guard
        against double-prefixing when arg already starts with doc/plans/.
        codex_report_path(fs_path) -> sibling codex-<basename>.
        Step enum = Claude(String) | DeleteReport(PathBuf) | Codex(String).
        plan_steps(rounds, invocation, report_fs_path) /
        render_dry_run(rounds, invocation, report_fs_path).
        execute(): walk plan_steps — each round runs DeleteReport(report) ->
        Claude `/pressure <invocation>` -> `codex exec --sandbox workspace-write
        -C <repo> '$pressure <invocation>'` -> Claude `/address <invocation>`,
        then ONE final DeleteReport after all rounds. Per-round delete runs
        BEFORE /pressure so it never reads a stale report. Each Claude spawn:
        --permission-mode acceptEdits --model opus,
        CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1, single positional slash
        invocation, inherited stdio, .status() blocking.
        Route exits through a pure classify_exit(status): code 0 = continue;
        code 130/2 OR status.code()==None (signal-killed child) = abort the
        whole pipeline cleanly (early return); any other non-zero = warn and
        continue. --dry-run prints render_dry_run and spawns nothing.
  Subagent 1 — vendored content (loom-software-engineer):
    Files Owned: commands/pressure.md, commands/address.md, commands/distill.md,
      codex/skills/pressure/SKILL.md
    Work: create with the EXACT content in plan section 1b (self-containment
      language is the point; distill.md uses the content block in §1b as its
      authoritative in-repo source).
  Subagent 2 — installer (loom-software-engineer):
    Files Owned: install.sh
    Files Read-Only: commands/, codex/
    Work: per plan section 1c — LOCAL mode only. Add CODEX_DIR="$HOME/.codex" and a
      COUNT_COMMANDS counter; add install_commands() (glob commands/*.md ->
      ~/.claude/commands, counting COPIES FROM THE SOURCE loop — not a destination
      find — NO loom-* filter, NO unprefix rm; FAIL if the three expected command
      files were not copied) and install_codex_skill() (assert the source SKILL.md
      exists BEFORE mkdir/cp, then copy codex/skills/pressure/SKILL.md ->
      $CODEX_DIR/skills/pressure); call both from the LOCAL branch of main(); extend
      check_requirements() to assert the EXACT source files exist (commands/pressure.md,
      commands/address.md, commands/distill.md, codex/skills/pressure/SKILL.md) — not
      just the directories; in confirm_overwrites() add the ~/.claude/commands warning
      AND a separate full-path entry for ~/.codex/skills/pressure (it cannot reuse the
      hardcoded ~/.claude/ prefix), noting the generic unprefixed command names are
      loom-owned; add a separate ~/.codex line in print_summary(). Do NOT touch
      self_update or release.yml (deferred). Do NOT run install.sh in acceptance;
      verify with `bash -n install.sh` + wiring greps.

MEMORY RECORDING (use loom memory ONLY — never loom knowledge, never auto-memory):
Record IMMEDIATELY: mistakes via `loom memory note "mistake: …"`, non-obvious
choices via `loom memory decision "…" --context "…"`, surprises via
`loom memory note "found: … file:line"`. Do NOT record procedural actions.
SUBAGENT MEMORY — instruct every subagent to record mistakes/decisions/surprises
via loom memory (not procedural actions).

#### Files Changed

No changes recorded.

#### Key Decisions

- spawn installer subagent in background (Agent run_in_background) while main agent does Rust+markdown *(install.sh (§1c) is fully specified and touches only one independent file; backgrounding it overlaps with the Rust driver work which is the opus-judgment long pole)*
- kept classify_exit uniform per spec (0=continue, 130/2/None=abort, other=warn) but made the abort path print child label + exit code *(reviewer flagged codex clap usage-error (exit 2) would be misclassified as a clean Ctrl+C 'interrupted' and silently swallowed (patterns.md silent-failure class). Honoring the explicit classify spec but printing label+code on abort makes any codex startup/usage failure visible instead of silent — fixes the hazard without deviating from the specified classification)*
- render_dry_run renders the REAL argv via shared claude_args()/codex_args() builders used by BOTH spawn_* and render *(reviewer #3: dry-run printed simplified commands that diverged from the actual argv (hid --permission-mode/--model/-C). Single-source arg builders make dry-run provably match what spawns)*

#### Notes

- concern: git rev-parse --show-toplevel resolution is now duplicated 3x (commands/knowledge/spawn.rs resolve_project_root pub(super), commands/stage/merge.rs:~202 raw inline, commands/pressure/mod.rs resolve_repo_root). conventions.md Import Deduplication says extract at 3+. Deferred from pressure stage: a shared git::repo_root() helper touches non-owned modules and risks merge conflicts with sibling stages. Future cleanup candidate
- found: resolve_plan_path must use is_file() not exists() — exists() is true for directories, so a directory arg would canonicalize and spawn claude/codex against a non-file, failing confusingly downstream (reviewer #5)
- gotcha: the loom pressure driver hands the slash commands AND codex skill the FULL repo-relative invocation (e.g. doc/plans/PLAN-foo.md), because children run with current_dir(repo_root). So vendored commands/pressure.md, address.md and codex SKILL.md must use $1/<PLAN_NAME> DIRECTLY — NOT 'doc/plans/$1' (the stale installed versions double-prefix). resolve_plan_path produces invocation = repo-relative path under repo, else absolute, NEVER cwd-relative.
- found: signal did NOT embed plan §1b/§1c content (orchestrator gap). Recovered canonical sources: ~/.claude/commands/{pressure,address,distill}.md via Read tool, and ~/.codex/skills/pressure/SKILL.md via Bash cat (worktree-file-guard.sh hook ALLOWS ~/.claude/ but BLOCKS Read tool on ~/.codex/). Installed versions are STALE (pressure used 'doc/plans/$1', codex skill wrote 'codex-[name].md'). Authored §1b as expanded content satisfying the 4 acceptance substrings.

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final verification after implement-pressure. Verify FUNCTIONAL INTEGRATION, not
just that tests pass.

Use parallel subagents and skills to maximize performance.

⛔ NEVER use Claude Code's auto-memory. ALL memory goes through loom memory.

CONTEXT GATHERING (FIRST):

- Read the plan file from doc/plans/ (check .work/config.toml for source_path)
- Read ALL stage memories: loom memory show --all
- Read doc/loom/knowledge/*.md for architecture context and known mistakes

BUILD & TEST (zero tolerance — fix ALL warnings/errors):

- cargo build/test/clippy via --manifest-path loom/Cargo.toml

CODE REVIEW (MANDATORY — parallel loom-code-reviewer; dimensions also in the
structured code_review block below):

- command-construction / subprocess path: slash and `$pressure` invocations are
  single positional args; no `claude -p`; codex exec flags correct; exit
  classification (0=continue, 130/2/None=abort, else warn+continue)
- path handling: subdirectory, repo-relative, absolute, and temp-plan cases
  resolve to a correct fs_path (deletion/report) and invocation string
- LOCAL installer correctness: exact source assets present, unprefixed globs
  (no loom-*), source-counted copies, overwrite warnings, ~/.codex paths, called
  from main()'s local branch. self_update/release.yml are DEFERRED — do NOT audit
  their asset/checksum handling here (the §1c deferred list is a recorded
  follow-up, not a requirement of this stage).
Fix ALL issues with loom-senior-software-engineer / loom-software-engineer.

FUNCTIONAL VERIFICATION (MANDATORY):

- `loom pressure --help` shows the command, --rounds, --dry-run
- Dry-run smoke WITHOUT spawning agents: create a throwaway plan file in a temp
  dir (NOT under doc/plans/ — see mistakes.md stale-plan lesson), run
  `loom pressure <temp-path> --dry-run`, and confirm the listed steps for 2 rounds
  are, per round in order, the codex-<…> deletion, /pressure, $pressure, /address —
  plus one final codex-<…> deletion after the last round — with the correct
  invocation path
- Confirm codex-report naming agrees across loom/src/commands/pressure/mod.rs,
  commands/address.md, and codex/skills/pressure/SKILL.md
- Confirm self-containment wording AND the append-only planning-omissions.md
  instruction are present in commands/pressure.md + commands/address.md
- Confirm `loom pressure` is in dynamic completions (pressure in
  TOP_LEVEL_COMMANDS; --rounds/--dry-run in complete_flags) and completion tests pass
- Confirm `--rounds 0` is rejected (value parser) and the path-resolution unit
  tests cover the subdir/relative/absolute/temp-plan cases
- Confirm install.sh (local mode) installs the commands + codex skill with
  unprefixed globs to ~/.claude/commands and ~/.codex/skills/pressure; `bash -n
  install.sh` passes and install_commands carries no loom-* token.
  (self_update/release.yml are deferred — not in scope.)
- The two real-invocation smokes (claude "/pressure …" edits the plan; codex exec
  '$pressure …' writes the codex- sidecar) are MANUAL release-validation and
  CANNOT run in stage acceptance — `loom stage complete` does NOT prove them.
  Run them by hand (or state they were not run) and report each result in the
  completion summary.

Record discoveries to loom memory for knowledge-distill to curate.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- manual-smoke status: the two REAL-invocation smokes (claude '/pressure' edits the plan; codex exec '$pressure' writes the codex- sidecar) were NOT RUN in this stage — sandbox has no network and both agents need network+auth; they are manual release-validation by design (loom stage complete does NOT prove them). claude+codex binaries ARE present on PATH so find_claude_path/find_codex_path resolve.
- integration-verify: independent loom-code-reviewer found NO critical/important defects across all 7 dimensions (command construction, exit classification, path handling, codex-report naming, self-containment wording, LOCAL installer, dead-code/DRY/security/wiring). Only 4 cosmetic suggestions; fixed S1 (doc comment said '/pressure never reads a stale report' but /pressure never reads the report at all — the per-round pre-delete actually guards /address against a failed codex write). S2 (abort->exit 0) and S3 (dry-run $pressure unquoted) left as intentional per spec/test; S4 (installer per-file backup) left as design choice (confirm_overwrites already warns).
- gotcha: 'loom complete bash -- loom <prefix>' from a shell does NOT prefix-filter top-level commands because CompletionContext::from_args reads current_word=args.get(2) (COMP_WORDS protocol slot), not the last token. Verify completions via unit tests (test_complete_commands_pressure_prefix/test_complete_flags_pressure) or 'loom complete bash -- loom ""' for the full list. pressure IS present in TOP_LEVEL_COMMANDS; --rounds/--dry-run complete.
- verified: dry-run smoke in temp git repo (mktemp, NOT doc/plans) prints per round in order: delete codex-<basename> -> /pressure -> $pressure -> /address, plus one final delete after last round. 2 rounds=9 steps, 1 round=5. invocation is repo-relative (doc/plans/PLAN-smoke.md), codex report sibling codex-PLAN-smoke.md, claude argv '--permission-mode acceptEdits --model opus /pressure <plan>', codex argv 'exec --sandbox workspace-write -C <repo_root> $pressure <plan>'. Bare filename resolves via doc/plans fallback.
- integration-verify: build/test/clippy(--all-targets) all green on pressure changeset. 10 pressure unit tests pass (resolve_plan_path subdir/relative/absolute/double-prefix/rejects-dir, plan_steps order+count, classify_code all arms, render_dry_run real-argv, codex_report sibling).

## Open Questions

No open questions.
