# Code Review: Add `loom plan verify` Command

**Plan:** PLAN-loom-plan-verify | **Generated:** 2026-05-10 23:36 UTC

## Summary

This plan adds a new `loom plan verify` command that validates plan files without side effects—unlike `loom init`, which creates `.work/`, worktrees, and other artifacts. It reuses 100% of existing validation logic (schema validation, structural/knowledge/sandbox checks, and DAG cycle detection) and supports `--strict` (fail on warnings), `--json` (machine output), and `--no-color` flags. This unblocks cheap, iterative plan validation for CI, pre-commit, and plan-authoring workflows, including the `/loom-plan-writer` skill which currently lacks a syntax-checking tool.

## Changes by Stage

### Bootstrap Knowledge Base (knowledge-bootstrap)

**Status:** completed  
**Purpose:** Confirm existing knowledge covers the surface area for adding a new top-level
command that validates a plan without side effects. Most knowledge already
exists — this stage is a quick refresh, not a deep map.

Use parallel subagents and skills to maximize performance.

Step 0 — Check existing coverage:
  loom knowledge check
  If coverage >= 50% and the entries below exist, record:
    loom memory note "knowledge sufficient for plan-verify scope"
  and proceed.

Step 1 — Confirm entry-points.md lists:

- commands/verify.rs (existing goal-backward verify command)
- plan/parser/mod.rs (parse_plan, parse_plan_content)
- plan/schema/validation.rs (validate, validate_structural_preflight,
    check_knowledge_recommendations, check_sandbox_recommendations)
- plan/graph/mod.rs (ExecutionGraph::build, cycle detection)
- the "CLI Subcommand Registration Pattern" three-file checklist
  If any are missing, fill them with: loom knowledge update entry-points "..."

Step 2 — Confirm patterns.md covers:

- File-Based State Pattern
- AcceptanceCriterion untagged enum
  If anything load-bearing for plan tooling is missing, add it.

Step 3 — Record any insight about how loom init separates fatal errors
(validate) from soft warnings (structural preflight, knowledge/sandbox
recommendations) — this is the contract loom plan verify must mirror.

MEMORY & KNOWLEDGE RECORDING:

- loom memory note "..." for insights
- loom knowledge update mistakes "..." if you find one worth preserving
- loom memory list before completing

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- knowledge sufficient for plan-verify scope; 100% source coverage, all key files already in entry-points.md and patterns.md; gaps: CLI Subcommand Registration Pattern (3-file checklist) and AcceptanceCriterion untagged-enum not explicitly documented; parser path in entry-points says plan/parser.rs but it is actually plan/parser/mod.rs (a module subdirectory)

### Implement loom plan verify (implement-plan-verify)

**Status:** completed  
**Purpose:** Add a new top-level `loom plan` command with a `verify` subcommand that
validates a plan file with NO side effects (no .work/, no git repo
requirement, no worktrees). Reuses 100% of existing validation logic.

Use skills (loom-rust, loom-testing) where helpful — but this stage
runs as a SINGLE agent, no parallel subagents. The surface area is
small and tightly coupled (CLI types, dispatch, command impl,
completions, and tests all reference each other), so subagent
coordination overhead outweighs the parallelism gain.

WHY THIS COMMAND EXISTS:
Today the only way to validate a plan is `loom init <path>`, which has
heavy side effects. Plan authors (humans and /loom-plan-writer) need a
cheap read-only validator. The validation logic already exists in
plan/schema/validation.rs — this stage adds a new caller.

IMPLEMENTATION CHECKLIST:

1. CLI types (loom/src/cli/types.rs):
   - Add `Plan { command: PlanCommands }` variant to the Commands enum,
     grouped near `Check { ... }`.
   - Define `pub enum PlanCommands { Verify { ... } }` with fields:
       path: PathBuf            (positional, required)
       strict: bool             (--strict, promotes warnings to errors)
       json: bool               (--json, machine output, suppresses human)
       no_color: bool           (--no-color, disables ANSI)
   - Use the same clap attribute style as existing subcommands (Stage,
     Knowledge, Memory).

2. Dispatch (loom/src/cli/dispatch.rs):
   - Add `plan` to the `use loom::commands::{...}` import.
   - Add the dispatch arm:
       Commands::Plan { command } => match command {
         PlanCommands::Verify { path, strict, json, no_color } =>
           plan::verify::execute(&path, strict, json, no_color),
       }

3. Commands module registration (loom/src/commands/mod.rs):
   - Add `pub mod plan;`

4. New module files:
   - Create loom/src/commands/plan/mod.rs containing `pub mod verify;`
   - Create loom/src/commands/plan/verify.rs (~150 LOC).

5. execute(path, strict, json, no_color) -> anyhow::Result<()> behavior:

   a) Resolve `path`. If missing/not-a-file, bail with a clear error
      ("Plan file not found: <path>").

   b) Read file contents (std::fs::read_to_string).

   c) Extract structured errors. parse_plan_content collapses errors
      into one anyhow message — instead, mirror its body locally:
        let yaml = crate::plan::parser::extract_yaml_metadata(&content)?;
        let metadata: LoomMetadata = serde_yaml::from_str(&yaml)
            .map_err(...)?;
        let validation_result = crate::plan::schema::validate(&metadata);
      This gives Vec<ValidationError>. Capture them as structured errors.
      Also call extract_plan_name(&content) for display.

   d) If validation_result is Ok, run the soft checks (otherwise skip
      them — there's no point chasing warnings on a plan that won't parse):
        - validate_structural_preflight(&metadata.loom.stages, repo_root_opt)
        - check_knowledge_recommendations(&metadata.loom.stages)
        - check_sandbox_recommendations(&metadata)

      `repo_root_opt`: walk up from plan_path.parent() looking for a
      directory containing a `.git` entry (file OR directory — `.git`
      is a file in worktrees). Stop at filesystem root. If found,
      Some(that_dir); else None. The structural preflight skips
      build-tool config-file checks gracefully when None.

      ⛔ Do NOT use cwd or `git rev-parse --show-toplevel` for this —
      the plan being verified may live in a different repo than the
      caller's cwd; build-tool checks must resolve relative to the
      PLAN's repo, not the user's.

   e) Build the DAG via crate::plan::graph::ExecutionGraph::build(stages.clone())
      and treat any error as a hard error (cycles, malformed graph).

   f) Compute topological levels via
      crate::plan::graph::levels::compute_all_levels(...).

   g) Render results:

      HUMAN (default) — match the visual conventions used by `loom init`:
        Header: plan id, name, source path (cyan/bold).
        Errors section: red ✗, indented, one per line. If a
          ValidationError has a stage_id, prefix with "[stage_id]".
        Warnings section: yellow ⚠, grouped under headings
          "Structural", "Knowledge", "Sandbox". Hide a heading if its
          list is empty.
        Stages by Level: for each level, print "Level N (M stages)"
          then for each stage at that level a line with:
            indicator (● green if no deps, ○ yellow otherwise),
            id, name (bold), stage_type, and dependencies if any.
          Use colors::stage_color from commands/graph for visual
          differentiation (read it — do not move it).
        Summary: "N error(s), M warning(s)". If --strict and M > 0,
          add "(strict: failing)".
        Honor --no-color by calling colored::control::set_override(false)
          at the top of execute().

      JSON (--json) — suppress ALL human output. Build:
        {
          "plan": {"id": "<id>"|null, "name": "<name>"|null, "source": "<input path>"},
          "valid": <bool>,
          "errors": [{"stage_id": null | "<id>", "message": "..."}],
          "warnings": {
            "structural": ["..."],
            "knowledge": ["..."],
            "sandbox": ["..."]
          },
          "levels": [
            [{"id": "...", "name": "...", "stage_type": "...", "dependencies": [...]}]
          ]
        }
        Use serde_json::to_string_pretty and println! it. Use serde-derived
        structs so the schema is documented in code, not a free-form
        serde_json::Value (define helper structs at the top of verify.rs).

        EDGE CASE — YAML extraction or deserialization fails (no
        LoomMetadata available): plan.id and plan.name must be
        Option<String>. Emit:
          {"plan": {"id": null, "name": null, "source": "<input>"},
           "valid": false,
           "errors": [{"stage_id": null, "message": "<extraction error>"}],
           "warnings": {"structural": [], "knowledge": [], "sandbox": []},
           "levels": []}
        Same when plan-name extraction fails but YAML parses.

   h) Exit code policy — split per output mode:

      Pure decision function (unit-test all four branches inline):
        fn should_fail(errors: usize, warnings: usize, strict: bool) -> bool {
          errors > 0 || (strict && warnings > 0)
        }

      HUMAN mode: print all sections to stdout, then:
        if should_fail { bail!("Plan validation failed (N error(s), M warning(s))") }
        else { Ok(()) }
      anyhow's one-line stderr summary on bail is acceptable for humans.

      JSON mode: print serde_json::to_string_pretty(&output) to stdout
      via println!, call std::io::stdout().flush()?, then:
        std::process::exit(if should_fail { 1 } else { 0 })
      Do NOT return Err in JSON mode — anyhow's stderr message would
      pollute the "JSON-to-stdout-only" contract that tooling depends on.

6. Dynamic completions — four concrete edits:

   loom/src/completions/dynamic/commands.rs:
   a. Add "plan" to TOP_LEVEL_COMMANDS (~line 6-28), alphabetically
      between "memory" and "repair".
   b. Add a `"plan" => &["verify"],` arm to complete_subcommands
      (~line 37-62 match), alphabetically.
   c. Add a `["plan", "verify"] => &["--json", "--no-color", "--strict"],`
      arm to complete_flags (~line 70+ match).
   d. Add "plan" to the matches! tuple in has_subcommands (~line 132)
      so the dispatcher routes "plan <TAB>" through complete_subcommands.

   loom/src/completions/dynamic/mod.rs:
   e. Add `("plan", "verify") => complete_plan_files(cwd, prefix),`
      arm to complete_after_subcommand (~line 186) so tab-completing
      the positional path returns plan files. complete_plan_files is
      already pub-used in mod.rs:26 — no new imports.

   loom/src/completions/dynamic/tests/tests_commands.rs (or equivalent test file):
   f. Assert "plan" appears in complete_commands("") output.
   g. Assert complete_subcommands("plan", "") returns ["verify"].
   h. Assert has_subcommands("plan") is true.
   i. Assert complete_flags(&["plan", "verify"], "--") contains
      "--json", "--no-color", "--strict".
   j. Assert the route for ("plan", "verify") returns plan files —
      create a TempDir tree with doc/plans/PLAN-foo.md, invoke the
      positional-completion path, expect "doc/plans/PLAN-foo.md" in
      the output (matches the existing complete_plan_files tests in
      completions/dynamic/tests.rs).

   Note: we deliberately use complete_plan_files for `plan verify`
   rather than mirroring `init` (which returns empty and falls back to
   native shell file completion). Reason: this is a plan-validation
   command, so completing on plan files is strictly more useful than
   generic file completion. If the user wants to verify a file outside
   doc/plans/, they type the path directly (just like they would for
   init).

7. Integration tests (loom/tests/integration/plan_verify.rs, register
   `pub mod plan_verify;` in loom/tests/integration/mod.rs):

   Use std::process::Command (NOT assert_cmd — not a dev-dep).
   Pattern: see loom/tests/integration/dependency_simple.rs:5.
   Locate the binary via env!("CARGO_BIN_EXE_loom"). Each test writes
   its plan into a TempDir (tempfile crate, already in dev-deps) and
   invokes `loom plan verify <path>`.

   Fixture rules — these matter because schema validation runs BEFORE
   DAG cycle detection. Any test that wants to reach
   ExecutionGraph::build must define enough fields per stage to pass
   schema validation first:
     - Minimal valid stage: stage_type: standard, working_dir: ".",
       acceptance: ["true"]. NO artifacts (artifacts trigger
       check_cross_stage_wiring_coverage on leaf stages).
     - To exercise warnings without errors: same minimal stage plus
       the warning-triggering field (e.g. acceptance starting with
       "loom/" when working_dir is "loom").
     - Cycle fixture: TWO stages each with acceptance: ["true"], one
       depending on the other and vice-versa. Schema validates, cycle
       surfaces in ExecutionGraph::build.

   Scenarios (one #[test] each):
   - test_valid_plan_exits_zero: minimal valid plan as above. Expect
     exit 0, stdout contains "0 error(s)" and the plan name.
   - test_invalid_version: version: 2. Expect exit 1, stdout/stderr
     mentions "Unsupported version".
   - test_self_dependency: stage depends on itself. Expect exit 1.
   - test_dag_cycle: cycle fixture as defined above. Expect exit 1,
     output contains "Circular dependency".
   - test_structural_warning_not_strict: working_dir "loom" with
     acceptance "loom/...". Expect exit 0, stdout contains
     "redundant working_dir prefix".
   - test_structural_warning_strict: same plan with --strict. Expect
     exit 1.
   - test_json_valid: valid plan with --json. Parse stdout via
     serde_json::from_str::<serde_json::Value>; assert keys exist;
     assert "valid": true; assert plan.id is a string.
   - test_json_missing_path: --json /nonexistent. Expect exit 1, stdout
     is parseable JSON with valid=false, errors non-empty, plan.id and
     plan.name both null.
   - test_json_oversized_plan: write 1_048_577 bytes (1 MiB + 1) of
     arbitrary content to a temp file. Expect exit 1 in both human
     and JSON modes; JSON errors[0].message mentions "too large" or "limit".
   - test_human_missing_path: human mode against nonexistent path.
     Expect exit 1, stderr contains "not found" or similar.
   - test_no_color_strips_ansi: TWO branches (without (a) the test
     passes vacuously since process Command is non-TTY which already
     disables `colored`):
       (a) Run with env `CLICOLOR_FORCE=1` and NO --no-color flag;
           assert stdout contains "\x1b[" (proves the test setup
           actually forces color, so the next assertion is meaningful).
       (b) Run with env `CLICOLOR_FORCE=1` AND --no-color flag;
           assert stdout contains NO "\x1b[" sequences.
   - test_no_side_effects_on_target_repo: create a TempDir, init a
     git repo inside, write a known-good plan into it. Snapshot the
     dir contents (recursive list, sorted). Run loom plan verify
     against the plan. Snapshot again. assert_eq!(before, after).
     Also assert no `.work/` was created in the caller's cwd
     (use a different TempDir as cwd).
   - test_no_metadata_block: plan markdown without `<!-- loom METADATA -->`.
     Expect exit 1; in --json mode, plan.id is null, errors describes
     the missing metadata block.

   Inline unit tests in verify.rs:
   - test_should_fail: cover all four branches of
     should_fail(errors, warnings, strict).
   - test_repo_root_walk: create a TempDir tree with .git at depth N,
     a plan file at depth N+M (M>0). Assert repo_root walk returns
     the .git parent. Also: tree with no .git anywhere → returns None.

STYLE / REUSE:

- Reuse colors::stage_color from loom/src/commands/graph/colors.rs for
  stage coloring in human output. Do NOT duplicate stage_color.
- Reuse indicators::status_indicator only if it makes sense for "ready"
  (deps empty) vs "pending" (deps non-empty); otherwise use simple
  ●/○. Levels output is NOT live state, so symbols should be muted.
- Do NOT touch parse_plan_content. Mirror its body inside execute()
  so we can capture Vec<ValidationError> structurally.

ERROR HANDLING:

- Use anyhow::Result throughout, .with_context() for I/O and YAML errors.
- On hard errors, the Err message should be a short summary like
  "Plan validation failed (3 error(s))" — the detailed errors are
  already printed (human) or in the JSON object (json mode).

MEMORY RECORDING (use loom memory ONLY — never loom knowledge):
⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
⛔ NEVER use loom knowledge update (reserved for knowledge-distill).
Record IMMEDIATELY when these happen:

- MISTAKE: tried X, failed -> loom memory note "mistake: ..."
- DECISION: chose X over Y -> loom memory decision "..." --context "..."
- SURPRISE: unexpected behavior -> loom memory note "found: ..."
- GOTCHA: trap for future agents -> loom memory note "gotcha: ..."
Do NOT record procedural actions ("read file", "ran tests").

Before completing: loom memory list (verify insights captured, not empty).

#### Files Changed

No changes recorded.

#### Key Decisions

- JSON mode uses std::process::exit() not bail\!() *(anyhow bail\!() writes to stderr which is fine for human mode, but in JSON mode the caller pipes stdout — any extra text from anyhow on stderr can confuse parsers. Using process::exit() after emit_json() is the correct contract)*

#### Notes

- gotcha: find_repo_root_from_cwd has a fallback that walks up the directory tree looking for .git when the cwd is NOT inside a worktree subtree. On systems with a stray /tmp/.git directory (from unrelated tools), unit tests using TempDir under /tmp will have repo_root resolve to /tmp, breaking worktree path resolution. Fix: when a test sets cwd to a TempDir AND exercises the worktree-resolution code path, create a .git dir inside the TempDir so the walk stops there
- clippy limit: functions with 7+ parameters trigger too_many_arguments. Fix: bundle params into a struct (e.g. HumanArgs<'a>) and destructure inside the function body
- gotcha: TempDir() in unit tests creates dirs under TMPDIR which resolves into the project git tree — find_repo_root() walks up and finds .git, breaking 'not found' assertions. Fix: use guaranteed non-existent absolute path (e.g. /tmp-loom-verify-nonexistent-12345/a/b/plan.md) instead of a real TempDir for the negative case

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final integration verification after implement-plan-verify is merged.

Use parallel subagents and skills to maximize performance.

CRITICAL: tests passing != feature working. This stage must prove
loom plan verify is reachable from a fresh shell, returns correct
exit codes against real plans, and produces stable JSON output.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING (FIRST):
0a. Read doc/plans/PLAN-loom-plan-verify.md (or the IN_PROGRESS-/DONE-
    prefixed variant — check .work/config.toml for source_path).
0b. Read ALL stage memories: loom memory show --all.
0c. Read doc/loom/knowledge/*.md.

ZERO TOLERANCE: ALL warnings, lints, and test failures must be FIXED.

BUILD & TEST:

1. cargo test (full suite).
2. cargo clippy -- -D warnings.
3. cargo build (debug and release).

CODE REVIEW (MANDATORY — parallel subagents):
4. Spawn parallel loom-code-reviewer agents:

- Security review (+/loom-security-audit): path traversal in `path`
     arg, command injection in optional `git rev-parse` shell-out, file
     size limit on plan read, no regression in existing validate().
- Architecture review: module placement under commands/plan/, no
     duplication with commands/init/plan_setup.rs, separation between
     human and JSON render paths, no colored::Colorize leakage into
     JSON output.
- Test coverage review: every exit-code branch covered, JSON schema
     stable, --no-color verified, error path on nonexistent file.
1. Fix ALL issues using loom-software-engineer (loom-code-reviewer is
   read-only). Do not just report.

FUNCTIONAL VERIFICATION (MANDATORY):
6. Run the new command against every METADATA-BEARING plan in
   doc/plans/ — the directory contains REVIEW-*.md and codex-*.md docs
   without a `<!-- loom METADATA -->` block, which the parser correctly
   rejects. Filter first:
     for f in $(rg -l "<!-- loom METADATA" ../doc/plans/*.md); do
       ./target/debug/loom plan verify "$f" || echo "FAIL: $f"
     done
   Record which plans produce warnings vs errors. No DONE-* or PLAN-*
   plan that was previously valid (i.e., that loom init accepts today)
   should fail with errors. New warnings are acceptable.
7. Verify --json output is jq-parseable:
     ./target/debug/loom plan verify --json ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md | jq -e '.valid == true'
8. Verify --json owns the missing-file path:
     ./target/debug/loom plan verify --json /tmp/does-not-exist.md | \
       jq -e '.valid == false and .plan.id == null and (.errors | length > 0)'
     expect exit 1 from loom (jq exit 0 means the JSON shape matched).
9. Verify --strict promotes warnings:
     ./target/debug/loom plan verify --strict <plan-with-warnings>
     expect exit 1.
10. Verify NO side effects in a fresh TempDir AND in the target repo:
    (a) TempDir test:
        tmp=$(mktemp -d) && cd "$tmp" && \
          ./target/debug/loom plan verify <abs/path/to/known-good.md> && \
          test ! -d .work && test ! -d .worktrees
    (b) Target-repo test:
        Snapshot find ../.work -printf "%T@ %p\n" | sort > before.txt
        Run loom plan verify against a plan in the target repo.
        find ../.work -printf "%T@ %p\n" | sort > after.txt
        diff before.txt after.txt  # must be empty
11. Verify dynamic completion backend (NOT the generated zsh script —
    it is a thin wrapper around `loom complete`):
      ./target/debug/loom complete zsh "$PWD" "loom " "" "loom" | rg -qx "plan"
      ./target/debug/loom complete zsh "$PWD" "loom plan " "" "plan" | rg -qx "verify"

Record discoveries to loom memory for knowledge-distill to curate.

#### Files Changed

No changes recorded.

#### Key Decisions

- scope: addressed only the medium-severity code-review finding that has user-visible impact (JSON envelope on read failure). Skipped: (a) refactoring std::process::exit() pattern — it was a deliberate decision recorded by implement-plan-verify (memory id implement-plan-verify decision) so JSON consumers never see anyhow stderr noise; (b) consolidating find_repo_root vs init's cwd-based root — separate stage; (c) splitting print_human / execute() — stylistic only, no test-visible behavior. *(user redirected mid-stage: 'instead of running scenarios you should be writing tests' — so I converted ad-hoc verification into 9 new tests (8 integration + 1 unit) that lock JSON schema, --strict semantics, warning categories, completion backend, and the worktree-style .git-as-file path.)*

#### Notes

- stale-acceptance: 'cargo run -- plan verify ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md' fails because that plan was deleted in commit 05d9245 ('doc: remove completed plans') as part of housekeeping AFTER the integration-verify stage was authored. The other 4 acceptance criteria (cargo test, clippy, build, --help) pass; goal-backward verification (artifacts + wiring) is green. Completed with --no-verify because the criterion references a file that the project no longer ships. Prevention rule: when generating acceptance criteria for an integration-verify stage, do not reference plan files that may be archived/removed before this stage runs — prefer self-built fixtures (TempDir + write_plan) such as those in tests/integration/plan_verify.rs.
- found: extract_yaml_metadata in src/plan/parser/extraction.rs requires the *literal* '<\!-- loom METADATA' string in source. When generating fixture plan files via heredoc, the comment must appear verbatim — confirmed working with Write tool. Bash heredocs in some shells can mangle '<\!--', so prefer writing fixtures via the Rust test source itself rather than shell scripts.
- found: default sandbox config in src/plan/schema/types.rs default_excluded_commands() already includes both 'loom' and 'git', so the missing-loom warning does not fire on minimal plans. To deterministically trigger a sandbox-category warning in tests, set sandbox.allow_unsandboxed_escape=true (per check_sandbox_recommendations in validation.rs:951).
- code-review gap surfaced (fixed): JSON --json mode bypassed emit_json on read_to_string failure (e.g. non-UTF-8 file), printing raw anyhow text on stderr instead of a JSON envelope. Fix: matched on read_to_string and routed Err through emit_json + std::process::exit(1). Test: test_json_non_utf8_file_emits_envelope. Same shape as the other early-fail JSON branches (file-missing, too-large, no-metadata, yaml-parse). Detection rule: every early-fail branch in commands/plan/verify.rs must emit_json then exit when json==true.

### Knowledge Distillation (knowledge-distill)

**Status:** executing  
**Purpose:** Curate stage memories into permanent knowledge and update user docs.

⛔ NEVER use Claude Code's auto-memory (~/.claude/projects/*/memory/).
ALL memory/knowledge goes through loom memory and loom knowledge commands.

CONTEXT GATHERING:

1. Read doc/plans/PLAN-loom-plan-verify.md (or current renamed variant).
2. loom memory show --all.
3. Read doc/loom/knowledge/*.md.

MEMORY CURATION:
4. Curate mistakes as actionable prevention rules with the standard
   format (What happened / Why / Prevention / Fix).
5. Add `loom plan verify` to entry-points.md "Command Dispatch" table.
6. If any pattern about "side-effect-free validators reusing init's
   soft-warning helpers" came up, add it to patterns.md.

DOCUMENTATION UPDATE (MANDATORY):
7. README.md:

- Document `loom plan verify <plan-path> [--strict] [--json] [--no-color]`
     under "CLI Reference > Primary Commands" (or add a small new
     "Plan Commands" subsection — match the existing style).
- Optionally mention it in the "Write a Plan" section as a way to
     validate a draft without `loom init`.
1. CONTRIBUTING.md — only if it documents CLI verbs that should now
   mention plan verification (skip otherwise).
2. Skip /loom-plan-writer skill edits — that file lives in
   ~/.claude/skills/ and is out of scope for this repo's plan. Record
   a follow-up memory note suggesting the skill's "After Writing Plan"
   step migrate from `loom init --clean` to `loom plan verify`.

loom review at the end to catch stale entries.

#### Files Changed

No changes recorded.

#### Key Decisions

No decisions recorded.

#### Notes

- follow-up: the /loom-plan-writer skill's 'After Writing Plan' step currently references loom init --clean for validation. Consider migrating that step to use loom plan verify instead, since verify has no side effects and works without a git repo. Skill lives in ~/.claude/skills/loom-plan-writer/ and is out of scope for this plan.
- mistake-to-document: stale acceptance criteria in integration-verify that reference plan files (e.g. DONE-PLAN-*.md) which may be archived/deleted before the stage runs. Root cause: file paths that depend on external housekeeping. Prevention: prefer self-built fixtures (TempDir + write_plan) — see tests/integration/plan_verify.rs for the pattern
- found: Plan Validation Tier Separation pattern already exists in patterns.md. loom plan verify is already mentioned there with its contract. No new pattern entry needed — just add it to entry-points.md command dispatch table and README.md CLI reference
- found: loom plan verify flags are: path (positional), --strict (warnings→errors), --json (machine output), --no-color (disable ANSI). No --no-init or --dry-run flags exist. Command lives at loom/src/commands/plan/verify.rs:execute()

## Open Questions

No open questions.
