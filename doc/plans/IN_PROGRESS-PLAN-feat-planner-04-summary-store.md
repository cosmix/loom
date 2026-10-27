# Plan: Summary store and CLI

Implements: section_04

## Overview

Concept mode (plan 5) shows summaries whose every sentence links to the source passage it draws on, and marks a sentence the moment its source changes. This plan builds that summary engine without any session: the store under `doc/loom/summaries/`, the summary file format with sentence ids computed from text, the evaluator that gives every sentence and every summary its state from the files alone, the engine step that rewrites paths after a same-hash rename and deletes summaries whose source is gone, and the four commands `loom plan summary seal|brief|status|release`, which work from any terminal without a server. It also adds the four `[plan]` model and effort keys for summary building.

Product spec: `doc/product/section_04.md` ("The store", "The summary file", "Hashing", "What out of date means", "Sealing a summary", the "Models" paragraph of "Who rebuilds what"), with section 12 "Paths the server reads that sessions can write" and section 02 "Runtime state". Metaplan entry 4 of `doc/plans/metaplan-planner-ui.md`. Background rebuilds, claim clearing for ended sessions and cost are plan 10 (`PLAN-feat-planner-10-summary-rebuilds.md`).

## Goals

- `loom::planning::summary` is the contract between agents and the reader: strict validation whose every message names the offending draft line, deterministic output (a rebuild that changes nothing produces no diff), and no hash ever written by an agent.
- `seal` writes through the store lock with a no-follow temp file and rename anchored at the repository root's descriptor, so a symlink planted in the store cannot redirect a terminal `seal`.
- The evaluator is a pure function of the files plus the claim list; plan 5 renders it, plan 10 schedules from it.
- Non-goals: starting any session, the scheduler, cost, the browser. Clearing claims of ended sessions is the server's job (plan 10); this plan exposes `clear_session_claims` for it.

## Decisions

1. **Module `loom/src/planning/summary/`** inside plan 1's `loom::planning`. Every public signature is in `doc/plans/briefs/feat-planner-04-summary-store/common.md`.
2. **Hashes are plan 1's.** Every span and file hash is `planning::sections::normalized_hash` (line endings and trailing whitespace normalised, then `body_hash`); anchors are `planning::sections` anchors including `#stage:<id>`. Overview `members` hash the newline-joined ordered sentence ids with the same function, so a link edit in a document summary leaves overviews current.
3. **Front matter is written by `render`, read with `serde-saphyr`** (already a dependency, `loom/Cargo.toml`). Keys are emitted in a fixed order, maps sorted, no time and no model: two seals of the same draft against the same sources are byte-identical.
4. **One sentence per line is enforced by structure**, not by sentence-boundary guessing: a sentence line is text followed by one or more trailing links; lists, tables, fences, indented code, block quotes, HTML and level-3+ headings are rejected; headings carry no links. A line holding two sentences is a reviewer's call, not a validator's.
5. **Writes are anchored.** `seal` and the engine write `doc/loom/summaries/<id>.md` by `fs::safe_fs::safe_create_new_in_workdir` (exclusive, no-follow) on a temporary name in the same directory, then `safe_rename_in_workdir`, both on `safe_open_dirfd(root)`, inside `with_store_lock` (fs2 exclusive lock on `summaries.lock`, the pattern of `quota/shared.rs` `poll_shared`). Drafts and claims are read with `fs::safe_read` because sessions write them.
6. **Claims are files created with `O_EXCL`.** That create is the refusal test for `brief`. `seal` refuses an id claimed by another session (and, from a terminal, any claimed id unless `--force`) and deletes only the caller's claim; `release` deletes the caller's claims. A claim names `$LOOM_PLAN_SESSION` when set (plan 7 sets it in every planning session, with `LOOM_PLAN_STATE` naming the runtime directory), else none; terminal claims never expire on their own and plan 10 offers their release.
7. **Renames need no agent.** When a document summary's `of` differs from its source's current path and the recorded `file` hash equals the source's current hash, the engine rewrites `of`, the `cites` keys and the body links and registers the new file hash with plan 1's `OwnWrites`. The evaluator already treats that summary as current, so `status` is right before the rewrite runs.
8. **`[plan]` keys follow `[pressure]`'s shape, with the project tier in its own table**: a user-tier `[plan]` section module (`user_config/plan.rs`); a project tier read from a `[planning]` table of `.loom/work/config.toml` (`fs/work_dir/config_sections/plan_config.rs`), because `loom init` and the daemon already own a `[plan]` table there (`source_path`, `plan_id`, `plan_name`, `base_branch`, `fs/work_dir/config_sections.rs:11-12`) and `user_config/workspace.rs::section_readable` would reject a section holding unregistered fields; `workspace.rs` maps the KeySpec section `plan` to the table `planning` through one helper; the dashboard config payload and its fixture. Plan 8 (`PLAN-feat-planner-08-guidance.md`) appends its four authoring keys to the same modules after these.
9. **One implementation stage.** Format, evaluation, engine, commands and config are one feature: the contracts exercise them together (a seal is how every fixture summary is made), and the four workers' territories are disjoint, so a second stage would buy only a merge.
10. **No knowledge-bootstrap.** The knowledge base maps the codebase; plan 1's distill added `architecture/planning-document-model`.

## Execution Diagram

```mermaid
graph LR
    summary-store --> summary-settings-web
    summary-settings-web --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. `summary-store`

Lanes `["claude", "codex"]`: nine codex units (gpt-6.1-terra, effort high) write the store layout, front matter, rebuild brief, status report, command module, the doc sections and the three config modules, each with its unit tests; Claude keeps the summary body format (W1, sonnet), evaluation, engine, store lock, writes and claims (W2, opus: section 12's security core), the seal path and CLI wiring with the completions assertion (W3, sonnet), and the config wiring and fixture regeneration (W4, sonnet). Necessity: the only implementation stage.

Risk checklist: untrusted input (drafts and summary files are agent- and person-written: `seal-rejects-body-rules`, `seal-rejects-duplicate-sentence`, `summary-id-rejects-traversal`), filesystem paths and symlinks (`seal-refuses-symlinked-store`, `seal-refuses-symlinked-draft`), lifecycle and concurrency (`seal-serialised-by-lock`, `brief-refuses-claimed-summary`, `seal-refuses-foreign-claim`, `reseal-produces-no-diff`), configuration propagation (`plan-summary-keys-resolve`, `plan-keys-beside-run-identity`, `summary-cli-uses-session-state`), external data correctness (the anchors and hashes come from plan 1; `stale-and-incomplete-states`, `plan-summary-cites-stage-anchor`, `summary-scope-table`, `overview-members-ignore-link-edits`, `rename-rewrites-paths`, `deleted-source-deletes-summary`), reachability (`summary-cli-status-json`, `summary-cli-seal-reports-line`), terminal output of document text (`brief-prints-note-and-marks`).

### 2. `summary-settings-web`

The settings dialog caption for the `[plan]` section and the web gates over the regenerated config fixture. Necessity: one working directory per stage (`web/`), and it needs stage 1's fixture merged (Q1).

### Integration Verification

Full gate including the web gate, the security review against section 12 that metaplan "Rules for every plan" requires for this plan (store writes, draft and claim reads, id validation, terminal output of document text), and a hand-sealed summary on a fixture project from a terminal.

### Knowledge Distillation

A tier-2 topic for the summary format and engine, the `[plan]` keys in the model-and-effort convention, and `loom plan summary` in `README.md` and `skills/loom-usage/SKILL.md` (written by W3; checked here).

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
    network:
      allowed_domains: ["crates.io", "index.crates.io", "static.crates.io", "registry.npmjs.org"]
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: summary-store
      name: "Summary format, evaluator, engine and loom plan summary"
      summary: "Adds the summary store under doc/loom/summaries/: the file format and its validator, sentence ids, per-sentence and per-summary states, same-hash rename rewrites, and the seal, brief, status and release commands, plus the [plan] summary model keys."
      stage_type: standard
      skills: ["loom-rust"]
      implementers: ["claude", "codex"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: []
      description: |
        Build stage 1 of doc/plans/PLAN-feat-planner-04-summary-store.md (doc/plans/IN_PROGRESS-PLAN-feat-planner-04-summary-store.md once loom run starts). Plan 1 (PLAN-feat-planner-01-documents.md) is merged: loom::planning provides discover, sections (anchors, normalized_hash), paths, runtime (runtime_paths, RuntimePaths), watch (Batch, OwnWrites) and project (Project::load).
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/. YAML paths and the worker table are package-relative (../ for repository files); brief paths are repo-relative.
        Every worker reads doc/plans/briefs/feat-planner-04-summary-store/common.md first (authoritative API, format rules and safety rules), then its own brief.
        Territories are DISJOINT across both lanes. Workers NEVER spawn subagents. Every prompt is the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
        LANES (the user's decision: codex gpt-6.1-terra at effort high for boilerplate, unit tests and simple implementation): rows with Tier `codex terra` are codex units. Spawn each as a loom-codex-forwarder subagent in the FOREGROUND (never codex:codex-rescue), at most 6 at once over disjoint files, each prompt carrying --model gpt-6.1-terra --effort high, an explicit Bash timeout of 600000 ms, and its brief path. Codex units never run git, never touch a .loom/ path, and run compile-only checks. After each unit returns, run the proof command its brief names and git status --short; revert anything outside its row and re-brief; a timed-out unit (exit 124) is re-split, never re-forwarded as is; a unit that fails twice goes to a loom-software-engineer with the same brief. Claude rows spawn by agent type in the background and are waited on with one background loom subagents watch. The seal path, the store lock and writes, claims and evaluation stay on Claude (section 12 security core).
        SEED (orchestrator, before wave 1, under 20 lines): add `pub mod summary;` to src/planning/mod.rs; write src/planning/summary/mod.rs declaring `pub mod store; pub mod format; pub mod evaluate; pub mod engine; pub mod lock; pub mod write; pub mod claims; pub mod seal; pub mod brief; pub mod status; pub mod config;`, and create each of those files holding only its `//!` purpose line; format.rs also holds `mod body; mod front; #[cfg(test)] mod tests; #[cfg(test)] mod front_tests;` and store.rs, brief.rs and status.rs each hold `#[cfg(test)] mod tests;`, every named child file holding only a `//!` line. W1 owns summary/mod.rs afterwards. When the build breaks inside another worker's territory, wait and rebuild; never edit it.
        WAVE 1: W1 and W4 (background), then C1 store, C2 front matter, C8 user-tier plan section, C9 project-tier planning table (foreground, one message; pairs build against pinned signatures: W1 with C1 and C2, W4 with C8 and C9). WAVE 2 after all six: W2 (background) and C10 summary model resolver (foreground). WAVE 3 after both: W3 (background, seal and CLI wiring) and C3 brief, C4 status, C5 command module, C6 docs (foreground, one message).
        WEB: W4 regenerates web/src/api/fixtures/config.json (a data file the Rust test the_config_fixture_matches_a_real_payload pins, so it changes here); the settings caption and the web gates are stage summary-settings-web.
        AFTER THE WAVES: rustfmt --check --edition 2021 tests/planning_summary_contracts.rs tests/planning_summary_cli_contracts.rs tests/planning_summary_support/mod.rs (a failure is a dispute-contract, never an edit); cargo fmt --all; cargo test --test maintainability; route any failure to a fresh worker of the owning territory; one loom check.
        LEDGER: no row of maintainability-baseline.txt changes.
        TEST INTEGRITY: W3 (Claude: the file name holds a word the codex wrapper refuses) changes the existing assertion in src/completions/dynamic/tests/tests_commands.rs test_complete_subcommands_plan from vec!["verify", "check"] (plan 1) to vec!["verify", "check", "summary"], since completions derive from clap. After the final review, file one loom stage dispute-integrity summary-store --event TI-edit-loom/src/completions/dynamic/tests/tests_commands.rs (copy the exact id from loom stage review integrity <stage-id>; loom names events by repository path) --reason "the plan subcommand list gains summary (section 02 Command line)".
        MUTATION: before the final review round, for each contract apply the wrong implementation its rejects: text names, confirm it fails, restore from a saved copy (never git checkout), and record loom memory note "mutation: <contract id> red under <mutation>".

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | C1 | Store layout, ids, scope, link helpers | codex terra | src/planning/summary/store.rs; src/planning/summary/store/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c1-store.md |
        | C2 | Front matter parse and emit | codex terra | src/planning/summary/format/front.rs; src/planning/summary/format/front_tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c2-front.md |
        | W1 | Summary body format | sonnet | src/planning/summary/mod.rs; src/planning/summary/format.rs; src/planning/summary/format/body.rs; src/planning/summary/format/tests.rs; src/planning/mod.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/w1-format.md |
        | W2 | Evaluation, engine, lock, store writes, claims | opus | src/planning/summary/evaluate.rs; src/planning/summary/evaluate/tests.rs; src/planning/summary/engine.rs; src/planning/summary/engine/tests.rs; src/planning/summary/lock.rs; src/planning/summary/write.rs; src/planning/summary/claims.rs; src/planning/summary/claims/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/w2-evaluate.md |
        | W3 | Seal and CLI wiring | sonnet | src/planning/summary/seal.rs; src/planning/summary/seal/tests.rs; src/commands/plan/mod.rs; src/cli/types_ops.rs; src/cli/dispatch.rs; src/completions/dynamic/tests/tests_commands.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/w3-commands.md |
        | C3 | Rebuild brief | codex terra | src/planning/summary/brief.rs; src/planning/summary/brief/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c3-brief.md |
        | C4 | Status report | codex terra | src/planning/summary/status.rs; src/planning/summary/status/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c4-status.md |
        | C5 | loom plan summary command module | codex terra | src/commands/plan/summary.rs; src/commands/plan/summary/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c5-command.md |
        | C6 | README and loom-usage summary text | codex terra | ../README.md; ../skills/loom-usage/SKILL.md | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c6-docs.md |
        | W4 | [plan] config wiring | sonnet | src/user_config/keys.rs; src/user_config/mod.rs; src/user_config/render.rs; src/user_config/tests.rs; src/user_config/workspace.rs; src/fs/work_dir/config_sections.rs; src/commands/status/web/config_api/tests.rs; ../web/src/api/fixtures/config.json | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/w4-config.md |
        | C8 | User-tier plan section | codex terra | src/user_config/plan.rs; src/user_config/plan/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c8-user-plan.md |
        | C9 | Project-tier planning table | codex terra | src/fs/work_dir/config_sections/plan_config.rs; src/fs/work_dir/config_sections/plan_config/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c9-plan-config.md |
        | C10 | Summary model resolver | codex terra | src/planning/summary/config.rs; src/planning/summary/config/tests.rs | common.md | doc/plans/briefs/feat-planner-04-summary-store/summary-store/c10-models.md |

        CONTRACT SURFACE (the contract session writes tests/planning_summary_contracts.rs, tests/planning_summary_cli_contracts.rs and the harness helper tests/planning_summary_support/mod.rs from this text and common.md before any code; top-level #[test] fns at most 50 lines each, files at most 400 lines, rustfmt-clean; both contract files are their own test targets and include the helper with `#[path = "planning_summary_support/mod.rs"] mod support;`, the form tests/map_api_contracts.rs uses for integration/helpers.rs):
        - Support helper (harness): fn project(files: &[(&str, &str)]) -> (TempDir, PathBuf root, PathBuf loom_dir) writing a git-less tree; fn seal_draft(root, loom_dir, id, body) -> Result<(), String> that writes the draft at runtime_paths_under(loom_dir, root).drafts/<id>.md and calls loom::planning::summary::seal::seal(root, &paths, id) mapping SealError to its message lines.
        - loom::planning::summary::format::{sentence_id(&str) -> String, parse(summary_rel: &Path, text: &str) -> Result<SummaryFile, Vec<FormatError { line, message }>>}
        - loom::planning::summary::seal::{seal(root: &Path, paths: &RuntimePaths, id: &str, force: bool) -> Result<SealOutcome, SealError>, SealError::Rejected(Vec<FormatError>) and SealError::Io(anyhow::Error)}
        - loom::planning::summary::evaluate::{evaluate(project: &Project, claims: &[Claim]) -> Evaluation, SummaryState::{Current, Incomplete, Stale, Rebuilding, Missing}, SentenceState::{Current, Stale}}
        - loom::planning::summary::engine::{SummaryEngine::new(root, paths, own: OwnWrites), SummaryEngine::on_batch(&mut self, &Project, &Batch) -> anyhow::Result<EngineUpdate { evaluation, rewritten, deleted, invalidated }>}
        - loom::planning::summary::lock::with_store_lock; loom::planning::summary::claims::{list_claims, release}; loom::planning::summary::brief::brief(root, paths, ids, note) -> anyhow::Result<String>
        - loom::planning::summary::config::SummaryModels::{resolve(project: &PlanConfig, user: &UserConfig) -> SummaryModels, load(root: &Path) -> anyhow::Result<SummaryModels>} with pub fields document_model, document_effort, overview_model, overview_effort: String; loom::user_config::UserConfig::load; loom::fs::work_dir::PlanConfig::from_section_toml(body) (the body of the project tier's [planning] table, NOT the run-owned [plan] table)
        - CLI seal: `loom plan summary seal <ID> [--force]`; inside a session LOOM_PLAN_SESSION names the caller, and LOOM_PLAN_STATE (when set) is the runtime directory (plan 1's runtime_paths)
        - CLI: env!("CARGO_BIN_EXE_loom") plan summary <sub>, current_dir a fixture git repository (git init -q), LOOM_HOME set to a TempDir, LOOM_PLAN_SESSION removed.
        The contracts are red at freeze because loom::planning::summary does not exist.

        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/planning/mod.rs"
        - "src/planning/summary/**"
        - "src/commands/plan/summary.rs"
        - "src/commands/plan/mod.rs"
        - "src/cli/types_ops.rs"
        - "src/cli/dispatch.rs"
        - "src/completions/dynamic/tests/tests_commands.rs"
        - "src/user_config/**"
        - "src/fs/work_dir/config_sections.rs"
        - "src/fs/work_dir/config_sections/plan_config.rs"
        - "src/fs/work_dir/config_sections/plan_config/**"
        - "src/commands/plan/summary/**"
        - "src/commands/status/web/config_api/tests.rs"
        - "tests/planning_summary_contracts.rs"
        - "tests/planning_summary_cli_contracts.rs"
        - "tests/planning_summary_support/**"
        - "../web/src/api/fixtures/config.json"
        - "../README.md"
        - "../skills/loom-usage/SKILL.md"
      acceptance:
        - "cargo build"
        - "cargo test --lib planning::summary::"
        - "cargo test --lib user_config::"
        - "cargo test --lib completions::"
        - "cargo test --lib fs::work_dir::"
        - "cargo test --lib commands::status::web::config_api::"
        - "cargo test --test planning_summary_contracts"
        - "cargo test --test planning_summary_cli_contracts"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - "cargo run -q -- plan summary --help"
        - "rg -qF 'loom plan summary' ../README.md"
        - "rg -qF 'loom plan summary' ../skills/loom-usage/SKILL.md"
      before_stage:
        - command: "test -f src/planning/project.rs"
          description: "plan 1 (PLAN-feat-planner-01-documents.md) is merged"
      after_stage:
        - command: 'rg -qF "PlanCommands::Summary" src/cli/dispatch.rs'
          description: "loom plan summary is dispatched"
      artifacts:
        - "src/planning/summary/format.rs"
        - "src/planning/summary/evaluate.rs"
        - "src/planning/summary/engine.rs"
        - "src/planning/summary/seal.rs"
        - "src/planning/summary/brief.rs"
        - "src/planning/summary/status.rs"
        - "src/planning/summary/claims.rs"
        - "src/user_config/plan.rs"
        - "src/fs/work_dir/config_sections/plan_config.rs"
        - "src/fs/work_dir/config_sections/plan_config/**"
        - "src/commands/plan/summary/**"
      wiring:
        - source: "src/cli/dispatch.rs"
          pattern: "PlanCommands::Summary"
          literal: true
          description: "loom plan summary is dispatched"
        - source: "src/user_config/keys.rs"
          pattern: "plan.document_summary_model"
          literal: true
          description: "the [plan] summary keys are registered"
        - source: "src/user_config/workspace.rs"
          pattern: '"plan"'
          literal: true
          description: "the project tier backs the [plan] keys"
      reachable:
        - symbol: with_store_lock
          from: dispatch_plan
          description: "loom plan summary seal writes through the store lock"
      harness: ["tests/planning_summary_support/**"]
      contracts:
        - id: sentence-id-follows-text
          file: tests/planning_summary_contracts.rs
          test: sentence_id_follows_text
          scenario: "parse a document summary whose two lines are 'The app keeps  no state. [§](../../../product/section_01.md#a)' and the same words with one space and a different link label and target; compare both sentence ids with 's-' plus the first 8 hex digits of sha256(\"The app keeps no state.\")"
          rejects: "an id computed over the line with its links, or over uncollapsed whitespace, so a link edit changes the id"
        - id: seal-rejects-body-rules
          file: tests/planning_summary_contracts.rs
          test: seal_rejects_body_rules
          scenario: "seal_draft for product/section_01 with, one at a time: a '- item [§](...)' list line on line 3; a '| a | b |' table line; a fenced code block; a '<b>x</b> [§](...)' HTML line; a sentence with no trailing link; '## Heading [§](...)' with a link; a '### Deep' heading; 16 valid sentences; and for the product overview, 31 valid sentences citing the vision"
          rejects: "a validator that accepts any of the nine drafts, or rejects without naming the offending line number (each error message must contain the 1-based line of the bad line)"
        - id: seal-rejects-unresolved-anchor
          file: tests/planning_summary_contracts.rs
          test: seal_rejects_unresolved_anchor
          scenario: "a document summary draft citing ../../../product/section_01.md#no-such-heading, one citing ../../../product/section_02.md#detection-rules (another document), and one citing ../../../product/section_01.md with no fragment (the preamble)"
          rejects: "a seal that writes a summary citing an anchor the source lacks or a file other than its own source, or one that refuses a fragment-less link (the third draft seals, with a cites key for the preamble)"
        - id: seal-rejects-duplicate-sentence
          file: tests/planning_summary_contracts.rs
          test: seal_rejects_duplicate_sentence
          scenario: "a draft holding the same sentence twice with different links"
          rejects: "a seal that keeps both copies (two sentences with one id)"
        - id: seal-rejects-stale-cited-sentence
          file: tests/planning_summary_contracts.rs
          test: seal_rejects_stale_cited_sentence
          scenario: "seal product/section_01, then edit the section's cited heading section, then seal the product overview citing that summary's sentence by #s-<id> plus the vision"
          rejects: "an overview seal that links a member sentence whose cited span changed since it was sealed"
        - id: seal-rejects-uncited-member
          file: tests/planning_summary_contracts.rs
          test: seal_rejects_uncited_member
          scenario: "a vision listing 01 and 02, both sections with sealed summaries, and a product overview draft citing only section 01's sentences and the vision"
          rejects: "an overview seal that lets an in-scope document drop out silently (the error must name product/section_02)"
        - id: stale-and-incomplete-states
          file: tests/planning_summary_contracts.rs
          test: stale_and_incomplete_states
          scenario: "seal product/section_01 citing only '## A' of a section with '## A' and '## B'; evaluate after editing ## B (incomplete, every sentence current), after editing ## A (stale, the A sentence stale), after renaming heading A (stale), after writing git conflict markers into the summary file (missing), with a claim file present (rebuilding), after a hand edit adding a '- list' line to the summary (missing), and after a hand edit rewording one sentence while keeping the rules (kept, judged by its hashes)"
          rejects: "an evaluator that marks an uncited edit stale, a cited edit only incomplete, treats a renamed anchor as current, accepts a file with conflict markers or a broken hand edit, discards a valid hand edit, or ignores claims"
        - id: overview-members-ignore-link-edits
          file: tests/planning_summary_contracts.rs
          test: overview_members_ignore_link_edits
          scenario: "seal both sections and the product overview; reseal section_01 with the same sentences but different link labels and an extra cited anchor; evaluate; then reseal section_01 with one sentence reworded and evaluate again"
          rejects: "members hashed over the whole summary file (the first reseal must leave the overview Current) or not hashed at all (the second must make it Incomplete or Stale)"
        - id: summary-scope-table
          file: tests/planning_summary_contracts.rs
          test: summary_scope_table
          scenario: "a vision listing 01; section_01.md and an unlisted section_02.md; doc/plans/index.md naming PLAN-a.md and DONE-PLAN-b.md; doc/plans/PLAN-a.md, doc/plans/PLAN-c.md, doc/plans/archive/DONE-PLAN-b.md, doc/plans/archive/DONE-PLAN-d.md; store::expected_ids and store::scope"
          rejects: "a store that summarizes unnamed archived plans, skips unlisted documents, or puts them in overview scope (expected product/section_01, product/section_02, plans/a, plans/b, plans/c, product, engineering and not plans/d; product scope [product/section_01]; engineering scope [plans/a, plans/b]; outside [product/section_02, plans/c])"
        - id: plan-summary-cites-stage-anchor
          file: tests/planning_summary_contracts.rs
          test: plan_summary_cites_stage_anchor
          scenario: "a plan with stages alpha and beta; seal plans/x with a sentence citing ../../plans/PLAN-x.md#stage:alpha; edit beta's description and evaluate; then edit alpha's description and evaluate"
          rejects: "a seal that refuses the app-only stage anchor, or an evaluator that hashes the whole metadata block for it (beta's edit gives Incomplete with the sentence current; alpha's edit gives Stale)"
        - id: rename-rewrites-paths
          file: tests/planning_summary_contracts.rs
          test: rename_rewrites_paths
          scenario: "a metaplan naming PLAN-x.md and a sealed plans/x summary; rename doc/plans/PLAN-x.md to doc/plans/IN_PROGRESS-PLAN-x.md unchanged; SummaryEngine::on_batch with a Batch whose renamed holds that pair"
          rejects: "an engine that leaves of and the links on the old path, re-queues the summary (it must be in rewritten, not invalidated, and Current), or forgets to register the rewritten file's hash with OwnWrites"
        - id: deleted-source-deletes-summary
          file: tests/planning_summary_contracts.rs
          test: deleted_source_deletes_summary
          scenario: "sealed plans/x and engineering overview; delete doc/plans/PLAN-x.md and its metaplan entry; on_batch"
          rejects: "an engine that keeps the orphaned plans/x summary file (it must be deleted and listed in deleted) or leaves the overview Current"
        - id: seal-refuses-symlinked-store
          file: tests/planning_summary_cli_contracts.rs
          test: seal_refuses_symlinked_store
          scenario: "doc/loom/summaries/plans is a symlink to a directory outside the repository; seal plans/x with a valid draft; then doc/loom/summaries/plans is a real directory and plans/x.md is a symlink to an outside file; seal again"
          rejects: "a seal that writes through either link (the outside directory gains a file, or the outside file's content changes)"
        - id: seal-refuses-symlinked-draft
          file: tests/planning_summary_cli_contracts.rs
          test: seal_refuses_symlinked_draft
          scenario: "the draft path drafts/plans/x.md is a symlink to a file outside the runtime directory holding a valid body"
          rejects: "a seal that follows the draft symlink and seals the target's content"
        - id: seal-serialised-by-lock
          file: tests/planning_summary_cli_contracts.rs
          test: seal_serialised_by_lock
          scenario: "a thread takes with_store_lock and holds it 500 ms; another thread runs seal for a valid draft; record when seal returns relative to the release"
          rejects: "a seal that writes without taking the store lock (it returns before the holder releases)"
        - id: brief-refuses-claimed-summary
          file: tests/planning_summary_cli_contracts.rs
          test: brief_refuses_claimed_summary
          scenario: "brief(plans/x) succeeds and creates one claim; a second brief(plans/x) fails naming the claim; release([plans/x]) then brief again succeeds; seal of plans/x deletes its claim"
          rejects: "a brief that creates a second claim, overwrites the first, or a seal that leaves the claim behind"
        - id: brief-prints-note-and-marks
          file: tests/planning_summary_cli_contracts.rs
          test: brief_prints_note_and_marks
          scenario: "a sealed current summary product/section_01 with two sentences whose source title holds an ESC byte, one cited span then edited; brief([product/section_01], Some(note naming the first sentence's text))"
          rejects: "a brief that refuses a current summary, drops the note, marks no sentence, or prints the ESC byte (output holds the note next to the first sentence, one [stale] and one [keep] line, the draft path and the seal command, and no 0x1b)"
        - id: reseal-produces-no-diff
          file: tests/planning_summary_cli_contracts.rs
          test: reseal_produces_no_diff
          scenario: "seal the same draft for product/section_01 twice (rewriting the draft between) with unchanged sources; compare the two summary files byte for byte, and check the draft file is gone after each seal"
          rejects: "front matter carrying a time, a model, or an unordered map, or a seal that leaves the draft behind"
        - id: summary-id-rejects-traversal
          file: tests/planning_summary_cli_contracts.rs
          test: summary_id_rejects_traversal
          scenario: "seal and brief with ids '../x', 'plans/../../etc', 'plans/', 'plans/a/b', '/product' and 'product/section_1x'"
          rejects: "a command that builds a draft or store path from an unvalidated id"
        - id: plan-summary-keys-resolve
          file: tests/planning_summary_cli_contracts.rs
          test: plan_summary_keys_resolve
          scenario: "SummaryModels::resolve with PlanConfig::from_section_toml(\"overview_model = \\\"opus\\\"\") and a UserConfig parsed from \"[plan]\\ndocument_summary_model = \\\"sonnet\\\"\\noverview_model = \\\"haiku\\\"\\n\" (via LOOM_HOME and UserConfig::load in a serial test); and with both empty"
          rejects: "a resolver that ignores a tier or inverts the precedence (expected document sonnet/medium, overview opus/high; defaults haiku/medium/sonnet/high)"
        - id: seal-refuses-foreign-claim
          file: tests/planning_summary_cli_contracts.rs
          test: seal_refuses_foreign_claim
          scenario: "a claim file for plans/x naming session s1; with LOOM_PLAN_SESSION=s2 run `loom plan summary seal plans/x` on a valid draft; then with LOOM_PLAN_SESSION unset (a terminal) without and with --force; then with LOOM_PLAN_SESSION=s1"
          rejects: "a seal that writes over a summary another session holds or deletes its claim (s2 exits 1 naming s1 and leaves the claim; the terminal exits 1 without --force and seals with it; s1 seals and its claim is deleted)"
        - id: summary-cli-uses-session-state
          file: tests/planning_summary_cli_contracts.rs
          test: summary_cli_uses_session_state
          scenario: "LOOM_HOME=<tmp>/home, LOOM_PLAN_SESSION=<a uuid> and LOOM_PLAN_STATE=<tmp>/elsewhere/<project key> (a real 0700 directory holding drafts/product/section_01.md, whose parent <tmp>/elsewhere is then chmod 0555, as the sandbox makes the real planning root read-only); run `loom plan summary seal product/section_01`; then repeat with the parent writable"
          rejects: "a CLI that resolves the runtime dir from LOOM_HOME inside a session (the draft under <tmp>/elsewhere is sealed; nothing is created under <tmp>/home/plan), or one that trusts a LOOM_PLAN_STATE under a writable parent (the second run exits 2 naming LOOM_PLAN_STATE and seals nothing)"
        - id: plan-keys-beside-run-identity
          file: tests/planning_summary_cli_contracts.rs
          test: plan_keys_beside_run_identity
          scenario: "a <tmp>/.loom/work/config.toml holding the run's [plan] table (source_path, plan_id, plan_name, base_branch) and a [planning] table with overview_model = \"opus\"; SummaryModels::load(<tmp>) with an empty user config"
          rejects: "a project tier that reads the run-owned [plan] table or fails on its fields (overview_model must resolve to opus from [planning], the rest to their defaults)"
        - id: summary-cli-status-json
          file: tests/planning_summary_cli_contracts.rs
          test: summary_cli_status_json
          scenario: "runs `loom plan summary status --json` in a fixture repository with a vision, one section and one sealed summary"
          rejects: "a status command that is not dispatched, prints human text under --json, or omits a summary (schema 1, one current product/section_01, product overview missing)"
        - id: summary-cli-seal-reports-line
          file: tests/planning_summary_cli_contracts.rs
          test: summary_cli_seal_reports_line
          scenario: "writes a draft whose line 4 is a list item into <LOOM_HOME>/plan/<key>/drafts/product/section_01.md and runs `loom plan summary seal product/section_01`"
          rejects: "a CLI that exits 0, or does not print '<draft path>:4:' on stderr"

    - id: summary-settings-web
      name: "Settings dialog caption for the [plan] keys"
      summary: "Gives the dashboard settings dialog a caption for the new [plan] section and runs the web gates over the regenerated config fixture."
      stage_type: standard
      skills: ["loom-react", "loom-typescript"]
      implementers: ["claude"]
      subagent_timeout_secs: 1200
      working_dir: "web"
      dependencies: ["summary-store"]
      description: |
        Stage 2 of doc/plans/PLAN-feat-planner-04-summary-store.md (IN_PROGRESS- once running). Necessity: one working directory per stage (web/), and it needs summary-store's regenerated web/src/api/fixtures/config.json merged.
        Use parallel subagents and skills to maximize performance. (One small change kept on Claude because the caption holds an apostrophe, which codex task text cannot carry, and the caption must not change to suit the wrapper: the orchestrator delegates it to one loom-software-engineer with model haiku, using this description as the brief.)
        Change: web/src/components/settings-model.ts SECTION_CAPTIONS (the record holding the pressure and models captions) gains plan: "what loom plan's summary sessions run on"; the row pairing by _model/_effort suffix already groups the four keys. If a test in src/components/settings-model.test.ts or settings-dialog.test.tsx enumerates sections, extend it with plan. DIST ORDER: build once for the gate; after the gate and the final review are green, commit the change, then rebuild dist/ (bun run build) and commit dist/ last, staging every new file under dist/ (git add dist); never rebuild after the final review round.
        MEMORY: record mistakes, decisions and surprises via loom memory immediately; NEVER loom knowledge; NEVER Claude Code auto-memory.
      files:
        - "src/components/settings-model.ts"
        - "src/components/settings-model.test.ts"
        - "src/components/settings-dialog.test.tsx"
        - "dist/**"
      before_stage:
        - command: 'rg -qF "plan.document_summary_model" src/api/fixtures/config.json'
          description: "summary-store's regenerated config fixture is merged"
      after_stage:
        - command: 'rg -qF "plan:" src/components/settings-model.ts'
          description: "the plan caption is present"
      acceptance:
        - "bun run typecheck"
        - "bun run lint"
        - "bun run format:check"
        - "bun run test"
        - 'd=$(mktemp -d "${TMPDIR:-/tmp}/summarydist.XXXXXX") && node_modules/.bin/vite build --outDir "$d" --emptyOutDir --logLevel error && diff -r "$d" dist && test -z "$(git ls-files --others --exclude-standard -- dist)" && git diff --quiet -- dist'
      wiring:
        - source: "src/components/settings-model.ts"
          pattern: "plan:"
          literal: true
          description: "the [plan] section has a caption in the settings dialog"
      contracts:
        - id: plan-section-has-caption
          file: src/components/settings-plan-section.test.ts
          test: "the plan section rows carry a caption and pair model with effort"
          runner: vitest
          scenario: "sectionRows over the entries of src/api/fixtures/config.json; find the section named plan"
          rejects: "a settings model that shows the plan section with a null caption or as four single rows instead of two model/effort pairs"

    - id: integration-verify
      name: "Integration Verification"
      summary: "Confirms the summary engine and loom plan summary work on the merged tree, passes a security review of every store and runtime-directory write, and runs the full suite."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-security-audit"]
      working_dir: "web"
      dependencies: ["summary-settings-web"]
      description: |
        Final verification of doc/plans/PLAN-feat-planner-04-summary-store.md (IN_PROGRESS- while running). Verify FUNCTIONAL INTEGRATION, not only tests passing. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: working_dir is web/ (so the bun gates run in their package; cargo runs through --manifest-path ../loom/Cargo.toml). Read the plan, doc/plans/briefs/feat-planner-04-summary-store/common.md, doc/product/section_04.md and section_12.md ("Paths the server reads that sessions can write", "Symlinks under doc/", "Threat model"), loom memory show --all.
        BUILD AND TEST (zero tolerance): every acceptance command below through one loom check.
        FUNCTIONAL (this session's shell, LOOM_HOME pointed at a scratch directory under the worktree's target/): build a fixture project in a scratch git repository with a vision and two sections; write drafts by hand and run loom plan summary brief, seal, status, release from the terminal; confirm status --json states, that a hand edit breaking a rule turns the summary missing, and that resealing unchanged produces no git diff. Record the commands and outputs in loom memory.
        SECURITY REVIEW (required by the metaplan for this plan): a loom-code-reviewer subagent with Skill loom-security-audit, judged against section 12's threat model: every write under doc/loom/summaries/ goes through safe_open_dirfd + safe_create_new_in_workdir + safe_rename_in_workdir inside with_store_lock; drafts and claims are read no-follow; summary ids are validated before any path is built (store.rs validate_id, written by a codex unit: read it line by line against the summary_id_rejects_traversal contract); claim files are created exclusive; nothing writes outside the store, drafts/ and claims/; document text printed by brief/status passes through inline_safe or is printed only as file content the user asked for; LOOM_PLAN_SESSION is treated as untrusted text in claim files. Plus parallel reviewers for architecture (reuse of plan 1's hashes and anchors, no second scanner, determinism of render) and test coverage (section 04's state tables row by row, every seal rejection, the mutation notes). Fix or dispute every finding; never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion; resolve each implemented one with loom memory resolve <id> --outcome implemented --reason <what changed>.
        Record discoveries to loom memory, including stale knowledge: loom memory note "stale-knowledge: ...".
      acceptance:
        - "cargo build --manifest-path ../loom/Cargo.toml"
        - command: "cargo test --manifest-path ../loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - command: "cargo test --manifest-path ../loom/Cargo.toml --doc --workspace --all-features -- --test-threads=1"
          timeout_secs: 1200
        - "cargo clippy --manifest-path ../loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path ../loom/Cargo.toml --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path ../loom/Cargo.toml --workspace --all-features --no-deps"
        - "bun run check"
        - 'd=$(mktemp -d "${TMPDIR:-/tmp}/summarydist.XXXXXX") && node_modules/.bin/vite build --outDir "$d" --emptyOutDir --logLevel error && diff -r "$d" dist && test -z "$(git ls-files --others --exclude-standard -- dist)" && git diff --quiet -- dist'
        - "cargo run -q --manifest-path ../loom/Cargo.toml -- plan summary status --json | jq -e '.schema == 1'"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records the summary format, states and store rules in the knowledge base and the new [plan] keys in the model configuration convention."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; keep code spot-reads narrow.
        START with loom memory pending --group; CORRECTIONS FIRST with loom knowledge replace-section.
        Curate: a new tier-2 topic architecture/summary-store (store layout and ids, the file format and body rules, sentence ids, front matter and members hashing, sentence and summary states, seal's write path and lock, claims, rename rewrites), a 2-4 line summary and link in architecture.md, and loom knowledge annotate architecture/summary-store --blurb "Summary store: format, states, seal, claims" (blurb under 50 characters). INDEX BUDGET: plan 1 left doc/loom/knowledge/INDEX.md at most 15000 bytes; if this stage's writes push it past 15000 bytes (wc -c), shorten the longest topic blurbs with loom knowledge annotate <category>/<slug> --blurb "<shorter>" until it is back under; never delete a topic to save bytes, and never run loom knowledge check --write-baseline to absorb an INDEX size issue. Add the four [plan] keys and their defaults to conventions/model-and-effort-config.md with loom knowledge replace-section on the section that lists the [pressure] keys (rg -n '^## ' doc/loom/knowledge/conventions/model-and-effort-config.md first).
        DOCS: README.md and skills/loom-usage/SKILL.md already describe loom plan summary (stage summary-store); correct them only if the tree disagrees.
        SUGGESTIONS, RECEIPTS and the baseline ratchet as usual: every memory gets loom memory resolve; finish with loom memory pending --strict; ratchet with loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt if structural issues were removed.
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
        - "test -s doc/loom/knowledge/architecture/summary-store.md"
        - "rg -qF 'document_summary_model' doc/loom/knowledge/conventions/model-and-effort-config.md"
        - 'test "$(wc -c < doc/loom/knowledge/INDEX.md)" -le 15000'
      files: ["doc/loom/knowledge/**", "README.md", "skills/loom-usage/SKILL.md"]
```

<!-- END loom METADATA -->
