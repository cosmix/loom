# Plan: Document model and project checklist

Implements: section_01, section_03

## Overview

`loom plan` reads a project's planning documents under `doc/` and tells the user which step of the workflow comes next. This plan builds the library every later planning plan reads `doc/` through (`loom::planning`): discovery, document kinds and ids, plan slugs across lifecycle spellings, heading sections with GitHub anchors and span hashes, the shared path predicate for locked plans, the runtime-directory paths, the polling watcher, the document parsers, the project checklist with its current step and offers, and the first new command, `loom plan check [--json]`. It also teaches `loom run` the `RETIRED-` prefix.

Product spec: `doc/product/section_01.md` (workflow model and project state), `doc/product/section_03.md` (document model and conventions), plus the watcher of `doc/product/section_04.md` ("The watcher"), `loom plan check` from `doc/product/section_09.md` ("Checking conventions"), the runtime directory of `doc/product/section_02.md` ("Runtime state") and the path rules of `doc/product/section_12.md` ("What a session may write", "Paths in requests", "Symlinks under `doc/`"). Metaplan entry 1 of `doc/plans/metaplan-planner-ui.md`.

## Goals

- `loom::planning` is the one place that classifies, resolves, sections, hashes and watches `doc/`; plans 2 to 11 call it and never re-derive a rule. Every public signature is published in the team ledger and repeated in `doc/plans/briefs/feat-planner-01-documents/common.md`.
- The checklist is a pure function of the tree; no app state, no front matter, no sidecar files.
- `loom plan check [--json]` prints the checklist and every convention problem, needs no server, and exits 1 on any problem.
- `loom run` and the app agree on every plan slug, `RETIRED-` included.
- Non-goals: the server, a bare `loom plan` (plan 2 makes the subcommand optional and adds `--port`, `--host`, `--no-open`), summaries (plan 4), rendering (plan 3), the `## Pressure tests` writer (plan 11; this plan only reads the record).

## Decisions

1. **One new top-level module, `loom/src/planning/`**, `pub mod planning;` in `loom/src/lib.rs`, so integration contracts and every later plan reach it as `loom::planning::...`. Each file stays under 400 lines; tests sit in `tests.rs` files beside the modules and in `loom/tests/planning_*_contracts.rs`.
2. **Reuse the fence-aware heading scan.** `fs/knowledge/splice.rs` `atx_heading`, `fence_mask` and `section_end` (all private today, lines 133, 220, 162) become `pub(crate)`. `section_end` already stops at any heading of the same or shallower level, so level-1 sections need no change in it; only `heading_level_of`'s level 2-6 filter is specific to `replace-section` and stays where it is. Span hashes call `context::source_graph::body_hash` (`loom/src/context/source_graph/mod.rs:127`, already `sha256:<hex>`) after normalising line endings and trailing whitespace.
3. **Plan metadata anchors come from the parser's byte ranges.** `plan::parser::extraction::extract_yaml_metadata_with_ranges` (`extraction.rs:72`) gives `metadata_block_range`; `#loom-metadata` spans the lines from the start marker to the end-marker line, and `#stage:<id>` spans each `- id: <id>` line of the YAML body up to the next `- id:` at the same indent or the closing fence.
4. **Briefs come from `loom run`'s own code.** `plan::inputs::collect_inputs(root, plan)` (`plan/inputs.rs:89`, `pub(crate)`) and `plan::schema::extract_brief_paths` (`structural_checks.rs:41`) are called, never reimplemented, so the app's brief list is the list `loom run` checks. `bundle_dir` (`plan/inputs.rs:136-145`) and `normalize_plan` (`commands/usage/discovery.rs:311-325`) gain `RETIRED-`; `fs/plan_lifecycle.rs` gains `RETIRED_PREFIX` beside `IN_PROGRESS_PREFIX` and `DONE_PREFIX` (lines 23-24).
5. **Locked is decided by name, freshly, every call.** `LockedSet::scan(root)` re-walks `doc/plans/`; a plan locked while a session runs is locked at once. A path under a locked plan's bundle directory that does not exist yet is locked too, so a session cannot create a new brief for a running plan.
6. **No-follow reads everywhere.** Discovery lists with `fs::safe_read::list_dir_no_follow` and reads with `read_bounded`/`open_regular_no_follow`; a symlink is a document only when its resolved target is a regular file inside canonical `doc/` and outside `doc/loom/summaries/`, and the target is then opened from the `doc/` root with the same no-follow opens. A symlinked `doc` is reported as such, never followed.
7. **The watcher polls.** One walk per second compares size, mtime and inode and hashes only files that differ; a batch is delivered on the first walk that finds nothing new; a removed and an added path with the same hash is a rename. It walks all of `doc/`, the summary store included, because plan 4 re-evaluates outside edits to summary files; `OwnWrites` drops a summary path whose hash the engine registered. No new dependency.
8. **The runtime directory is computed here, created elsewhere.** `planning::runtime` derives `~/.loom/plan/<project-key>/` (through `user_config::config_path()`'s parent, so `LOOM_HOME` redirects it) and its file names, so plan 4's summary CLI works without a server; plan 2 creates it 0700 and holds the lock.
9. **`loom plan check` exit codes.** 0 with no problem, 1 with any problem, 2 on an error that prevents checking (not inside a git repository). `--json` prints one pretty JSON object whose shape is pinned by `loom/tests/fixtures/planning/check_schema.json`. This repository's own `doc/` has plans without `Implements:` lines, so `loom plan check` exits 1 here until they gain one; that is the spec's intent, and the smoke test asserts the JSON, not the exit code.
10. **A metaplan entry heading without a plan file is a problem only when it is numbered and does not opt out** (`## 5. Concept mode` with no backticked plan file name); an unnumbered level-2 heading stays prose, as section 01 says ("Policy notes (no plan file)"), and so does a numbered one that says `no plan file`, the form this repository's second metaplan uses (`doc/plans/optim-token-metaplan.md:387`, `## 6. Policy notes (no plan file)`). This repository has two metaplans, `doc/plans/metaplan-planner-ui.md` and `doc/plans/optim-token-metaplan.md`; integration-verify expects both and no metaplan gap.
11. **No knowledge-bootstrap.** `doc/loom/knowledge/` already maps this codebase; every seam this plan uses is cited in the briefs by symbol and line, read at `b0f91c00`. The knowledge check is red at HEAD for one reason (`INDEX.md is 16454 bytes, over the 16384-byte budget`); knowledge-distill owns that repair.

## Execution Diagram

```mermaid
graph LR
    document-model --> checklist-check
    checklist-check --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. `document-model`: discovery, kinds, slugs, sections, predicate, watcher

Builds `planning::{kinds, slug, discover, runtime, sections, paths, watch}`, widens the splice scanners, adds `RETIRED-` to the lifecycle module and both slug strippers. Lanes `["claude", "codex"]`: four codex units (gpt-6.1-terra, effort high) write the pinned single-file modules (kinds, slugs, runtime paths, plan metadata spans) with their unit tests; Claude keeps discovery and the lifecycle prefix (W1, sonnet: no-follow walk and symlink rule), heading sections and anchors (W2, opus), the path predicate (W3, opus: the security boundary plans 3 and 6 share) and the watcher (W4, opus: batching and concurrency). Waves: W2 with C1-C4, then W1, then W3 and W4. Necessity: Q3, every later plan (2, 3, 4, 6) binds to these signatures, so they get their own checkpoint and contract freeze before the checklist is built on them; Q4, the two stages together would pass 500,000 tokens.

Risk checklist: untrusted input (path syntax, slug and anchor edge cases: `plan-anchor-numbering`, `rejects-unsafe-relpaths`), filesystem paths and symlinks (`discovery-skips-escaping-symlinks`, `locked-brief-of-running-plan`), lifecycle and concurrency (`watcher-batches-and-renames`, `watcher-ignores-own-summary-write`), configuration propagation (`runtime-dir-follows-loom-home`), reachability (`retired-plan-bundle-is-an-input` drives `collect_inputs`, the code `loom run` runs), the kinds table and stable ids (`kinds-and-ids-from-path`), symlinked components (`symlink-component-found`). Process I/O volume: the walk is bounded by `doc/` and covered by the watcher contract's many-file case.

### 2. `checklist-check`: parsers, project view, checklist, `loom plan check`

Builds `planning::{vision, metaplan, implements, pressure, xref, briefs, project, checklist, check}` and the CLI. Lanes `["codex", "claude"]`: ten codex units write the six parsers, the prefixed-path refactor, the cross-reference resolver, the `loom plan check` command and the two doc sections; Claude keeps the project view and checklist (W2, opus: the state table and offers) and the CLI wiring with the completions assertion (W3, haiku: that file name holds a word the codex wrapper refuses). Necessity: Q1 on `document-model` (its frozen signatures must be merged first).

Risk checklist: untrusted input (vision tables, `Implements:` tokens, metaplan headings, cross-references: `vision-table-never-guessed`, `implements-token-forms`, `cross-reference-forms`, `brief-bundle-grouping`), every convention problem of section 09 (`plan-check-exits-nonzero-per-problem`), `empty-section-is-a-gap`, external data correctness (the metaplan heading form this repository's own metaplan uses, captured in the fixture: `metaplan-entry-any-spelling`), reachability (`plan-check-json-cli`), the checklist rows of section 01 (`checklist-state-rows`, `checklist-skipped-steps`, `pressure-step-never-a-gap`, `stray-copy-is-a-problem`).

### Integration Verification

Full gate, three review subagents (security against section 12's path and symlink rules; architecture against the ledger; test coverage against section 01's state table and section 03's rules), and a functional run of `loom plan check --json` on this repository and on a fixture project.

### Knowledge Distillation

Records the `loom::planning` architecture as a tier-2 topic, the `RETIRED-` lifecycle prefix, and the new command in `README.md` and `skills/loom-usage/SKILL.md` (already written by W3; distill checks them), and brings `INDEX.md` back under its 16384-byte budget.

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
    - id: document-model
      name: "Document model: discovery, slugs, sections, predicate, watcher"
      summary: "Adds the loom::planning library that discovers doc/, names every document, resolves plan slugs including RETIRED-, splits documents into anchored and hashed heading sections, decides which paths are locked, and watches doc/ for changes."
      stage_type: standard
      skills: ["loom-rust"]
      implementers: ["claude", "codex"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: []
      description: |
        Build stage 1 of doc/plans/PLAN-feat-planner-01-documents.md (renamed doc/plans/IN_PROGRESS-PLAN-feat-planner-01-documents.md once loom run starts).
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/ (contracts run as cargo test <name> -- --exact from it). YAML paths and the worker table are package-relative; brief paths are repo-relative.
        Every worker reads doc/plans/briefs/feat-planner-01-documents/common.md first (the authoritative public API of loom::planning, the spec rules, and the size and safety rules), then its own brief.
        Territories are DISJOINT across both lanes. Workers NEVER spawn subagents. Every prompt is the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
        LANES (the user's decision: codex gpt-6.1-terra at effort high for boilerplate, unit tests and simple implementation): rows with Tier `codex terra` are codex units. Spawn each as a loom-codex-forwarder subagent in the FOREGROUND (never codex:codex-rescue), at most 6 at once over disjoint files, each prompt carrying --model gpt-6.1-terra --effort high, an explicit Bash timeout of 600000 ms, and its brief path. Codex units never run git, never touch a .loom/ path, and run compile-only checks. After each unit returns, run the proof command its brief names and git status --short; revert anything outside its row and re-brief; a timed-out unit (exit 124) is re-split, never re-forwarded as is; a unit that fails twice goes to a loom-software-engineer with the same brief. Claude rows spawn by agent type in the background and are waited on with one background loom subagents watch.
        WAVE 1: W2 (background), then C1, C2, C3 and C4 (foreground, one message; C4 and W2 build against each other's pinned signatures, Span, normalized_hash and metadata_spans). WAVE 2 after all five return: W1 (background; uses C1 kinds and C2 slugs). WAVE 3 after W1: W3 and W4 (W3 uses slug and discover; W4 uses discover's walk and normalized_hash).
        AFTER THE WAVES: rustfmt --check --edition 2021 tests/planning_documents_contracts.rs src/plan/inputs/retired_tests.rs (a frozen file the formatter would rewrite fails completion; a failure is a dispute-contract, never an edit); then cargo fmt --all; then cargo test --test maintainability; route any failure to a fresh worker of the owning territory; then one loom check.
        LEDGER: no row of maintainability-baseline.txt changes; every new file and function is born under the limits.
        MUTATION: before the final review round, for each contract apply the wrong implementation its rejects: text names, confirm the contract fails, restore from a saved copy (never git checkout), and record loom memory note "mutation: <contract id> red under <mutation>".

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | C1 | Document kinds and ids | codex terra | src/planning/kinds.rs; src/planning/tests/kinds_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/c1-kinds.md |
        | C2 | Plan slugs and resolution | codex terra | src/planning/slug.rs; src/planning/tests/slug_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/c2-slug.md |
        | C3 | Runtime paths | codex terra | src/planning/runtime.rs; src/planning/tests/runtime_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/c3-runtime.md |
        | C4 | Plan metadata spans | codex terra | src/planning/sections/metadata.rs; src/planning/sections/metadata_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/c4-metadata.md |
        | W1 | Discovery and the lifecycle prefix | sonnet | src/lib.rs; src/planning/mod.rs; src/planning/discover.rs; src/planning/tests/mod.rs; src/planning/tests/discover_tests.rs; src/fs/plan_lifecycle.rs; src/fs/mod.rs; src/plan/inputs.rs; src/commands/usage/discovery.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/w1-discovery.md |
        | W2 | Heading sections, anchors, span hashes | opus | src/planning/sections.rs; src/planning/sections/tests.rs; src/fs/knowledge/splice.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/w2-sections.md |
        | W3 | Shared path predicate | opus | src/planning/paths.rs; src/planning/paths/tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/w3-paths.md |
        | W4 | Polling watcher | opus | src/planning/watch.rs; src/planning/watch/tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/document-model/w4-watcher.md |

        SEED (the orchestrator, before wave 1; a small change under 20 lines): add `pub mod planning;` to src/lib.rs, write src/planning/mod.rs with its module doc comment, `pub mod kinds; pub mod slug; pub mod discover; pub mod runtime; pub mod sections; pub mod paths; pub mod watch;` and `#[cfg(test)] mod tests;`; create each of those files holding only its `//!` purpose line; src/planning/tests/mod.rs holding `mod kinds_tests; mod slug_tests; mod runtime_tests; mod discover_tests;` with each of those four files holding only a `//!` line; and src/planning/sections.rs holding its `//!` line plus `mod metadata; #[cfg(test)] mod tests; #[cfg(test)] mod metadata_tests;` with those three files holding only a `//!` line. The crate compiles while workers fill their own files. Every seeded file is then owned by exactly one worker below (src/lib.rs and src/planning/mod.rs by W1, which adds nothing further to mod.rs unless it splits a module). Workers share one crate: when the build breaks inside another worker's territory, wait and rebuild; never edit it.

        CONTRACT SURFACE (the contract session writes tests/planning_documents_contracts.rs and src/plan/inputs/retired_tests.rs from this text before any code; every contract in tests/ is a top-level #[test] fn; keep each fn at most 50 lines, each file at most 400 lines, rustfmt-clean; fixtures are built in tempfile::TempDir trees created with Builder::new().prefix("loom-planning-").tempdir_in(std::env::temp_dir())):
        - loom::planning::slug::{plan_slug, plan_state, PlanState::{Written, Running, Done, Retired}, PlanEntry { slug, path, state, strays }}
        - loom::planning::discover::{discover(root: &Path) -> anyhow::Result<DocTree>, DocTree { docs: Vec<Document>, plans: BTreeMap<String, PlanEntry>, doc_dir: DocDirState }, Document { path, kind, id, size, via_symlink }}, DocTree::get(&self, rel: &Path) -> Option<&Document>, DocTree::plan(&self, slug: &str) -> Option<&PlanEntry>
        - loom::planning::kinds::{DocKind, DocId(pub String)}
        - loom::planning::sections::{sections_of(text: &str) -> DocSections, DocSections { preamble: Span, headings: Vec<HeadingSection>, metadata: Option<Span>, stages: Vec<(String, Span)>, file_hash: String }, DocSections::find(&self, anchor: &str) -> Option<&Span>, HeadingSection { level: u8, title: String, anchor: String, span: Span }, Span { start_line: usize, end_line: usize, hash: String }, github_slug(&str) -> String, normalized_hash(&str) -> String}
        - loom::planning::paths::{LockedSet::scan(root: &Path) -> anyhow::Result<LockedSet>, LockedSet::lock_reason(&self, rel: &Path) -> Option<LockReason>, LockReason::{PlanState(PlanState), Archive, RunReview, BriefOfLockedPlan { slug: String }}, check_relpath_syntax(raw: &str) -> Result<PathBuf, PathSyntaxError>}
        - loom::planning::watch::{Watcher::new(root: PathBuf, own: OwnWrites) -> Watcher, Watcher::poll(&mut self) -> anyhow::Result<Option<Batch>>, Batch { added, changed, removed, renamed: Vec<(PathBuf, PathBuf)>, initial: bool }, OwnWrites::register(&self, rel: &Path, hash: &str)}
        - loom::planning::kinds::{classify(rel: &Path) -> DocKind, doc_id(rel: &Path, kind: &DocKind) -> DocId}; loom::planning::paths::{classify_write(&LockedSet, &Path) -> DocArea::{Locked(LockReason), Knowledge, Summaries, Doc, OutsideDoc}, symlink_component(root: &Path, rel: &Path) -> anyhow::Result<Option<PathBuf>>}
        - loom::planning::runtime::{runtime_paths_under(loom_dir: &Path, root: &Path) -> anyhow::Result<RuntimePaths>, runtime_paths_from(state: Option<&Path>, session: Option<&str>, loom_dir: &Path, root: &Path) -> anyhow::Result<RuntimePaths> (state = LOOM_PLAN_STATE, session = LOOM_PLAN_SESSION), server_runtime_paths(root: &Path) -> anyhow::Result<(RuntimePaths, Option<std::ffi::OsString> /* an ignored LOOM_PLAN_STATE */)>, project_key(root: &Path) -> anyhow::Result<String>, RuntimePaths { planning_root, dir, lock, drafts, claims, summaries_lock, summary_cost, sessions }}
        - In-crate: crate::plan::inputs::collect_inputs(root, plan) -> Result<Inputs> whose files: BTreeMap<PathBuf, Origin> (test file src/plan/inputs/retired_tests.rs, declared #[cfg(test)] mod retired_tests; in src/plan/inputs.rs by the derived scaffold).
        All repo-relative paths start with doc/. The contracts are red at freeze because loom::planning does not exist and bundle_dir ignores RETIRED-.

        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/lib.rs"
        - "src/planning/**"
        - "src/fs/plan_lifecycle.rs"
        - "src/fs/mod.rs"
        - "src/fs/knowledge/splice.rs"
        - "src/plan/inputs.rs"
        - "src/plan/inputs/retired_tests.rs"
        - "src/commands/usage/discovery.rs"
        - "tests/planning_documents_contracts.rs"
      acceptance:
        - "cargo build"
        - "cargo test --lib planning::"
        - "cargo test --lib plan::inputs::"
        - "cargo test --lib fs::knowledge::splice::"
        - "cargo test --lib commands::usage::"
        - "cargo test --test planning_documents_contracts"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
      before_stage:
        - command: 'rg -qF "fn section_end" src/fs/knowledge/splice.rs'
          description: "the splice heading scanner this stage widens exists"
      after_stage:
        - command: 'rg -qF "pub mod planning;" src/lib.rs'
          description: "planning is declared in the crate root"
      artifacts:
        - "src/planning/mod.rs"
        - "src/planning/discover.rs"
        - "src/planning/slug.rs"
        - "src/planning/sections.rs"
        - "src/planning/paths.rs"
        - "src/planning/watch.rs"
        - "src/planning/runtime.rs"
      wiring:
        - source: "src/lib.rs"
          pattern: "pub mod planning;"
          literal: true
          description: "planning is a public crate module"
        - source: "src/plan/inputs.rs"
          pattern: "RETIRED"
          literal: true
          description: "loom run's brief-bundle lookup strips RETIRED-"
        - source: "src/commands/usage/discovery.rs"
          pattern: "RETIRED-"
          literal: true
          description: "usage discovery normalises RETIRED- plans"
      reachable:
        - symbol: atx_heading
          from: sections_of
          description: "heading sections are built on the splice scanner"
      contracts:
        - id: plan-slug-most-advanced-wins
          file: tests/planning_documents_contracts.rs
          test: plan_slug_most_advanced_wins
          scenario: "a tree with doc/plans/PLAN-foo.md, doc/plans/IN_PROGRESS-PLAN-foo.md and doc/plans/archive/DONE-PLAN-foo.md plus doc/plans/archive/RETIRED-PLAN-bar.md and doc/plans/PLAN-bar.md; discover(root)"
          rejects: "a resolver that keeps the first file found or the written copy (plan foo must be Done at doc/plans/archive/DONE-PLAN-foo.md with both other foo files in strays; bar must be Retired)"
        - id: plan-slug-strips-every-prefix
          file: tests/planning_documents_contracts.rs
          test: plan_slug_strips_every_prefix
          scenario: "plan_slug on PLAN-a.md, IN_PROGRESS-PLAN-a.md, DONE-PLAN-a.md, RETIRED-PLAN-a.md, REVIEW-PLAN-a.md, PLAN-.md, notes.md and DONE-PLAN-DONE-x.md"
          rejects: "a stripper that accepts REVIEW- or a non-PLAN file as a plan, returns an empty slug for PLAN-.md, or strips a second status prefix (DONE-PLAN-DONE-x.md has slug DONE-x)"
        - id: plan-anchor-numbering
          file: tests/planning_documents_contracts.rs
          test: plan_anchor_numbering
          scenario: "sections_of on a document with headings '## 4.2 Line Grammar', '## Detection rules' three times, '### Plans: and *their* lifecycle', and '# Title' first"
          rejects: "anchors that keep dots or punctuation, number repeats from -2 or leave the first repeat unsuffixed twice (expected 42-line-grammar, detection-rules, detection-rules-1, detection-rules-2, plans-and-their-lifecycle, title)"
        - id: sections-ignore-fenced-headings
          file: tests/planning_documents_contracts.rs
          test: sections_ignore_fenced_headings
          scenario: "a document with a preamble line, '# T', '## A', a fenced code block containing '## Not a heading' and '# shell comment', '### A.1', '## B'; sections_of"
          rejects: "a scan that treats a #-line inside a fence as a heading, ends ## A at ### A.1 instead of at ## B, or loses the preamble (A must contain A.1; # T must span the whole document after the preamble; find(\"\") must return the one-line preamble span)"
        - id: span-hash-normalises-whitespace
          file: tests/planning_documents_contracts.rs
          test: span_hash_normalises_whitespace
          scenario: "the same document written with \\n endings, with \\r\\n endings and trailing spaces, and with one word changed inside ## B; compare every heading span hash and file_hash"
          rejects: "hashing raw bytes (CRLF or trailing spaces change the hash) or hashing the whole file into each span (a change in ## B changes ## A's hash)"
        - id: plan-metadata-anchors
          file: tests/planning_documents_contracts.rs
          test: plan_metadata_anchors
          scenario: "a plan with prose, a <!-- loom METADATA --> block holding a yaml fence with stages alpha and beta, the END marker, then '## Pressure tests' with one line; sections_of then find(\"loom-metadata\"), find(\"stage:alpha\"), find(\"stage:beta\"), find(\"pressure-tests\")"
          rejects: "metadata anchors that are missing, a stage:alpha span that runs into beta's lines, or a Pressure tests section swallowed into loom-metadata"
        - id: discovery-skips-escaping-symlinks
          file: tests/planning_documents_contracts.rs
          test: discovery_skips_escaping_symlinks
          scenario: "doc/ holds a.md, .hidden.md, b.md~, doc/loom/summaries/product.md, link-in.md -> a.md, link-out.md -> ../.env (a real file at the repo root), link-summary.md -> loom/summaries/product.md, and dir-link -> product (a real doc/product/ with index.md); discover(root)"
          rejects: "a walk that follows link-out.md, link-summary.md or dir-link, lists the summary store, a dotfile or an editor temporary (docs must be exactly a.md, link-in.md with via_symlink true, and doc/product/index.md), or a read_doc that refuses the allowed link (read_doc(doc/link-in.md) returns a.md's text) or follows the escaping one (read_doc(doc/link-out.md) and read_doc(doc/../.env) are errors)"
        - id: locked-brief-of-running-plan
          file: tests/planning_documents_contracts.rs
          test: locked_brief_of_running_plan
          scenario: "B is the briefs directory under doc/plans (the test builds the paths with format!, so this plan names no real brief). doc/plans/IN_PROGRESS-PLAN-run.md naming `B/shared/x.md` (written out in full) in its prose, B/run/common.md, B/shared/x.md, doc/plans/PLAN-free.md, B/free/common.md, doc/plans/REVIEW-PLAN-free.md, doc/plans/archive/notes.md; LockedSet::scan then lock_reason for each path and for the not-yet-existing B/run/new.md"
          rejects: "a predicate that locks only the plan file, misses the referenced brief outside the bundle, lets a new file be created in a running plan's bundle, or locks the free plan's brief (run's three brief paths are BriefOfLockedPlan, the review RunReview, the archive file Archive, free's files None); and a classify_write that does not give Locked for those, Knowledge for doc/loom/knowledge/x.md, Summaries for doc/loom/summaries/x.md, Doc for doc/plans/PLAN-free.md and OutsideDoc for src/main.rs"
        - id: rejects-unsafe-relpaths
          file: tests/planning_documents_contracts.rs
          test: rejects_unsafe_relpaths
          scenario: "check_relpath_syntax on '../x.md', 'a/../b.md', '/etc/passwd', 'a//b.md', './a.md', 'a\\\\b.md', 'a\\u{0}b.md', 'a\\u{1b}b.md', 1025 bytes of 'a', and 'plans/PLAN-x.md'"
          rejects: "a syntax check that lets any of the first nine through (only plans/PLAN-x.md is Ok, returned as doc/plans/PLAN-x.md)"
        - id: watcher-batches-and-renames
          file: tests/planning_documents_contracts.rs
          test: watcher_batches_and_renames
          scenario: "Watcher::new on a tree of 40 files; first poll returns initial with 40 added; then write c.md, poll (no batch yet: changes found), rename a.md to archive/a.md and edit b.md, poll (still changing), poll (quiet) returns one batch; also create x.md.swp and .#y.md before the quiet poll"
          rejects: "a watcher that delivers per walk instead of on the first quiet walk, reports a rename as remove plus add, or reports editor temporaries (the batch has added [c.md], changed [b.md], renamed [(a.md, archive/a.md)], removed [])"
        - id: watcher-ignores-own-summary-write
          file: tests/planning_documents_contracts.rs
          test: watcher_ignores_own_summary_write
          scenario: "after the initial batch, write doc/loom/summaries/plans/x.md, register its normalized_hash with OwnWrites, poll until quiet; then overwrite it with different text without registering and poll until quiet"
          rejects: "a watcher that reports the engine's own write, suppresses every summary change (the second write must appear in changed or added), or keeps a stale registration (register hash H for a path, write different text, poll: the path is reported and H is dropped, so writing H's text again later is reported too)"
        - id: watcher-survives-binary-and-large-files
          file: tests/planning_documents_contracts.rs
          test: watcher_survives_binary_and_large_files
          scenario: "a tree holding doc/images/logo.png (PNG bytes, invalid UTF-8), doc/big.md of 9 MiB and doc/a.md, present before the first poll; poll; then edit a.md and poll until quiet; then append one byte to logo.png and poll until quiet"
          rejects: "a watcher whose walk fails on a file read_doc refuses (the initial batch must list all three; the a.md edit and the PNG change must each arrive in a later batch)"
        - id: runtime-dir-follows-loom-home
          file: tests/planning_documents_contracts.rs
          test: runtime_dir_follows_loom_home
          scenario: "runtime_paths_under(<tmp>/loomdir, <tmp>/my proj) for a real directory named 'my proj'; and project_key for two different directories both named proj"
          rejects: "a runtime dir built from dirs::home_dir() instead of the given loom dir, a key without the path hash (two proj checkouts collide), or a key carrying the space unsanitised (dir must be <tmp>/loomdir/plan/my_proj-<12 hex>, planning_root <tmp>/loomdir/plan)"
        - id: runtime-dir-from-session-state
          file: tests/planning_documents_contracts.rs
          test: runtime_dir_from_session_state
          scenario: "runtime_paths_from(state, session, loom_dir, root) where state = <tmp>/ro/<project_key(root)> is a real directory whose parent <tmp>/ro is chmod 0555 (not writable, as the sandbox makes the real planning root) and session = Some(a uuid); then the same with <tmp>/rw/<key> under a writable parent (a directory the session could have made itself), with session None, with a relative path, a symlink to the good directory, and a directory named for another project's key; then state None; and server_runtime_paths(root) with LOOM_PLAN_STATE set in the environment (serial test)"
          rejects: "a resolver that ignores LOOM_PLAN_STATE inside a session, trusts any well-formed one (only the read-only-parent case with a session id is accepted, dir <tmp>/ro/<key>; every other case is an error naming LOOM_PLAN_STATE; None falls back to <loom_dir>/plan/<key>), or a server resolver that honours it (server_runtime_paths ignores it and returns it as ignored for a warning)"
        - id: kinds-and-ids-from-path
          file: tests/planning_documents_contracts.rs
          test: kinds_and_ids_from_path
          scenario: "loom::planning::kinds::classify and doc_id on doc/product/index.md, doc/product/section_07.md, doc/product/notes.md, doc/plans/index.md, doc/plans/round-2-metaplan.md, doc/plans/IN_PROGRESS-PLAN-x.md, doc/plans/archive/DONE-PLAN-x.md, the common.md of bundle x (briefs dir under doc/plans, then x/), doc/plans/REVIEW-IN_PROGRESS-PLAN-x.md, doc/loom/knowledge/INDEX.md, doc/other/readme.md, doc/plans/sub/PLAN-y.md"
          rejects: "kinds or ids derived from anything but path and name, or ids that change with the lifecycle (Vision product/index, ProductSection{7} product/section_07, ProductNote, Metaplan twice, Plan running and done both plan/x, Brief{x}, RunReview{x}, Knowledge, Other, and PLAN-y under a subdirectory is Other)"
        - id: symlink-component-found
          file: tests/planning_documents_contracts.rs
          test: symlink_component_found
          scenario: "a repository where doc/plans is a symlink to a real directory elsewhere in the TempDir; symlink_component(root, doc/plans/PLAN-x.md), then with doc/plans a real directory and PLAN-x.md a symlink, then with no links and a missing tail doc/plans/new/PLAN-z.md"
          rejects: "a check that only lstat's the final component or follows links (Some(doc/plans), then Some(doc/plans/PLAN-x.md), then None)"
        - id: retired-plan-bundle-is-an-input
          file: src/plan/inputs/retired_tests.rs
          test: plan::inputs::retired_tests::retired_plan_bundle_is_an_input
          scenario: "a TempDir with doc/plans/archive/RETIRED-PLAN-foo.md and common.md in the foo bundle (the briefs directory under doc/plans, then foo/); collect_inputs(root, doc/plans/archive/RETIRED-PLAN-foo.md) must list that common.md"
          rejects: "a bundle_dir that strips only IN_PROGRESS- and DONE-, so the RETIRED plan's bundle is not an input"

    - id: checklist-check
      name: "Checklist, document parsers and loom plan check"
      summary: "Reads the vision's section table, metaplan entries, Implements: lines, pressure-test records and cross-references, computes the project checklist with its current step and offers, and ships loom plan check [--json]."
      stage_type: standard
      skills: ["loom-rust"]
      implementers: ["codex", "claude"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: ["document-model"]
      description: |
        Build stage 2 of doc/plans/PLAN-feat-planner-01-documents.md (renamed doc/plans/IN_PROGRESS-PLAN-feat-planner-01-documents.md once loom run starts).
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/. YAML paths and the worker table are package-relative (../ for repository files); brief paths are repo-relative.
        Every worker reads doc/plans/briefs/feat-planner-01-documents/common.md first, then doc/plans/briefs/feat-planner-01-documents/checklist-check/common.md (the checklist rules distilled from section 01), then its own brief.
        Territories are DISJOINT across both lanes. Workers NEVER spawn subagents. Every prompt is the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
        LANES (the user's decision: codex gpt-6.1-terra at effort high for boilerplate, unit tests and simple implementation): rows with Tier `codex terra` are codex units. Spawn each as a loom-codex-forwarder subagent in the FOREGROUND (never codex:codex-rescue), at most 6 at once over disjoint files, each prompt carrying --model gpt-6.1-terra --effort high, an explicit Bash timeout of 600000 ms, and its brief path. Codex units never run git, never touch a .loom/ path, and run compile-only checks. After each unit returns, run the proof command its brief names and git status --short; revert anything outside its row and re-brief; a timed-out unit (exit 124) is re-split, never re-forwarded as is; a unit that fails twice goes to a loom-software-engineer with the same brief. Claude rows spawn by agent type in the background and are waited on with one background loom subagents watch.
        WAVE 1 (foreground, one message): C1 vision, C2 metaplan, C3 implements, C4 pressure, C5 prefixed path tokens, C6 briefs. WAVE 2: C7 xref (uses C5's extract_prefixed_paths). WAVE 3: W2 (background; uses every parser). WAVE 4 after W2: C8 check command and C9, C10 docs (foreground) beside W3 (background, CLI wiring and the completions assertion).
        AFTER THE WAVES: rustfmt --check --edition 2021 tests/planning_checklist_contracts.rs tests/planning_check_cli_contracts.rs (a failure is a dispute-contract, never an edit); cargo fmt --all; cargo test --test maintainability; one loom check.
        TEST INTEGRITY: W3 (Claude: the file name holds a word the codex wrapper refuses) changes the existing assertion in src/completions/dynamic/tests/tests_commands.rs test_complete_subcommands_plan from vec!["verify"] to vec!["verify", "check"] (completions derive from clap, so the visible Check variant turns it red otherwise). After the final review, file one loom stage dispute-integrity checklist-check --event TI-edit-loom/src/completions/dynamic/tests/tests_commands.rs (copy the exact id from loom stage review integrity <stage-id>; loom names events by repository path) --reason "the plan subcommand list gains check (section 02 Command line)".
        LEDGER: no row of maintainability-baseline.txt changes. src/cli/dispatch.rs is 375 lines: the Check arm goes in dispatch_plan and must keep the file under 400.
        MUTATION: as in stage document-model, for every contract; record each with loom memory note "mutation: ...".

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | C1 | Vision sections table | codex terra | src/planning/vision.rs; src/planning/tests/vision_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c1-vision.md |
        | C2 | Metaplan entries | codex terra | src/planning/metaplan.rs; src/planning/tests/metaplan_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c2-metaplan.md |
        | C3 | Implements lines | codex terra | src/planning/implements.rs; src/planning/tests/implements_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c3-implements.md |
        | C4 | Pressure records and open questions | codex terra | src/planning/pressure.rs; src/planning/tests/pressure_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c4-pressure.md |
        | C5 | Prefixed path tokens | codex terra | src/plan/schema/structural_checks.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c5-prefixed-paths.md |
        | C6 | Plan briefs grouping | codex terra | src/planning/briefs.rs; src/planning/tests/briefs_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c6-briefs.md |
        | C7 | Cross-references | codex terra | src/planning/xref.rs; src/planning/tests/xref_tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c7-xref.md |
        | W2 | Project view and checklist | opus | src/planning/project.rs; src/planning/problems.rs; src/planning/checklist.rs; src/planning/checklist/offers.rs; src/planning/checklist/tests.rs; src/planning/mod.rs; src/planning/tests/mod.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/w2-checklist.md |
        | C8 | loom plan check command | codex terra | src/commands/plan/check.rs; src/commands/plan/check/tests.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c8-check-command.md |
        | C9 | README plan check section | codex terra | ../README.md | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c9-readme.md |
        | C10 | loom-usage skill rows | codex terra | ../skills/loom-usage/SKILL.md | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/c10-usage-skill.md |
        | W3 | CLI wiring and the subcommand-list assertion | haiku | src/cli/types_ops.rs; src/cli/dispatch.rs; src/commands/plan/mod.rs; src/completions/dynamic/tests/tests_commands.rs | common.md | doc/plans/briefs/feat-planner-01-documents/checklist-check/w3-command.md |

        SEED (the orchestrator, before wave 1; under 20 lines): add `pub mod vision; pub mod metaplan; pub mod implements; pub mod pressure; pub mod xref; pub mod briefs; pub mod problems; pub mod project; pub mod checklist;` to src/planning/mod.rs, `mod vision_tests; mod metaplan_tests; mod implements_tests; mod pressure_tests; mod briefs_tests; mod xref_tests;` to src/planning/tests/mod.rs, `pub mod check;` to src/commands/plan/mod.rs and `#[cfg(test)] mod tests;` to src/commands/plan/check.rs, and create each new file holding only its `//!` purpose line, so the crate compiles while workers fill their own files. W2 owns src/planning/mod.rs and src/planning/tests/mod.rs afterwards, W3 src/commands/plan/mod.rs. When the build breaks inside another worker's territory, wait and rebuild; never edit it.

        CONTRACT SURFACE (the contract session writes tests/planning_checklist_contracts.rs, tests/planning_check_cli_contracts.rs and tests/fixtures/planning/check_schema.json from this text before any code; top-level #[test] fns, each at most 50 lines, file at most 400 lines, rustfmt-clean; fixture trees in TempDirs as in stage document-model; a git repository where a test runs the binary is made with git init -q and no commit):
        - loom::planning::project::Project::load(root: &Path) -> anyhow::Result<Project>, Project::refresh(&mut self, batch: &Batch) -> anyhow::Result<()> (Project: PartialEq, Debug); pub fields checklist: Checklist, problems: Vec<Problem { code: &'static str, path: PathBuf, line: Option<usize>, message: String }>
        - loom::planning::checklist::{Checklist { current: CurrentStep, steps: Vec<(Step, StepMark)>, items, primary: Option<Offer>, secondary: Vec<Offer>, state: ProjectState }, CurrentStep::{Step(Step), Ready, ReadyAllDone}, Step::{Vision, ProductSpec, Metaplan, Plans, PressureTest, Review, Run}, StepMark::{Done, Gap, Active, Optional}, Offer (variants and id() strings in doc/plans/briefs/feat-planner-01-documents/common.md)}
        - loom::planning::briefs::{plan_briefs(root: &Path, entry: &PlanEntry, stage_ids: &[String]) -> anyhow::Result<PlanBriefs { by_stage: BTreeMap<String, Vec<PathBuf>>, common: Vec<PathBuf>, other: Vec<PathBuf>, broken: Vec<String> }>, orphan_bundles(tree: &DocTree) -> Vec<String>}; loom::planning::xref::{references(doc_rel: &Path, text: &str, tree: &DocTree, sections: &dyn Fn(&Path) -> Option<DocSections>) -> Vec<Reference { form, raw, line, target: Option<(DocId, Option<String>)> }>} (broken = target None)
        - loom::planning::implements::{parse(text: &str) -> ImplementsLine { value: Implements, line: Option<usize>, duplicate_lines: Vec<usize>, bad_tokens: Vec<String> }, Implements::{Unmapped, NoneDeclared, Sections(Vec<u32>)}}
        - CLI: env!("CARGO_BIN_EXE_loom") plan check --json, current_dir the fixture repository, LOOM_HOME set to a TempDir.
        - tests/fixtures/planning/check_schema.json: the projection given VERBATIM in doc/plans/briefs/feat-planner-01-documents/checklist-check/common.md "Schema fixture" for the fixture tree "plans with no product spec" described there; the contract builds the same projection from the real stdout and compares serde_json::Value equality.
        Contracts are red at freeze because project, checklist, implements and the check command do not exist.

        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/planning/**"
        - "src/plan/schema/structural_checks.rs"
        - "src/commands/plan/check.rs"
        - "src/commands/plan/check/**"
        - "src/commands/plan/mod.rs"
        - "src/cli/types_ops.rs"
        - "src/cli/dispatch.rs"
        - "src/completions/dynamic/tests/tests_commands.rs"
        - "tests/planning_checklist_contracts.rs"
        - "tests/planning_check_cli_contracts.rs"
        - "tests/fixtures/planning/check_schema.json"
        - "../README.md"
        - "../skills/loom-usage/SKILL.md"
      acceptance:
        - "cargo build"
        - "cargo test --lib planning::"
        - "cargo test --lib commands::plan::"
        - "cargo test --lib completions::"
        - "cargo test --lib plan::schema::"
        - "cargo test --test planning_checklist_contracts"
        - "cargo test --test planning_check_cli_contracts"
        - "cargo test --test planning_documents_contracts"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - "cargo run -q -- plan check --help"
        - "rg -qF 'loom plan check' ../README.md"
        - "rg -qF 'loom plan check' ../skills/loom-usage/SKILL.md"
      before_stage:
        - command: "test -f src/planning/sections.rs"
          description: "stage document-model is merged"
      after_stage:
        - command: 'rg -qF "PlanCommands::Check" src/cli/dispatch.rs'
          description: "loom plan check is dispatched"
      artifacts:
        - "src/planning/checklist.rs"
        - "src/planning/project.rs"
        - "src/planning/implements.rs"
        - "src/planning/vision.rs"
        - "src/planning/metaplan.rs"
        - "src/commands/plan/check.rs"
      wiring:
        - source: "src/cli/dispatch.rs"
          pattern: "PlanCommands::Check"
          literal: true
          description: "loom plan check is dispatched"
      reachable:
        - symbol: build_checklist
          from: dispatch_plan
          description: "the checklist is computed by the loom plan check command"
      harness: ["tests/fixtures/planning/check_schema.json"]
      contracts:
        - id: checklist-state-rows
          file: tests/planning_checklist_contracts.rs
          test: checklist_state_rows
          scenario: "seven fixture trees, one per row of section 01's offers table: empty doc/; vision listing 01-03 with only section_02.md written; all sections, no metaplan; metaplan doc/plans/index.md with entries for PLAN-a.md and PLAN-b.md and only PLAN-b.md written; both written, PLAN-a.md with a '## Pressure tests' line and PLAN-b.md without; both pressure-tested; both under doc/plans/archive/ as DONE-PLAN-a.md and RETIRED-PLAN-b.md. Project::load each"
          rejects: "a checklist that picks the wrong current step or primary offer for any row (expected Step(Vision)+StartVision, Step(ProductSpec)+WriteSection{1}, Step(Metaplan)+StartMetaplan, Step(Plans)+WritePlan{slug a}, Step(PressureTest) with primary PressureTest{b} and StartRun among the secondaries, Ready, ReadyAllDone)"
        - id: checklist-skipped-steps
          file: tests/planning_checklist_contracts.rs
          test: checklist_skipped_steps
          scenario: "a tree with doc/plans/PLAN-x.md and doc/plans/IN_PROGRESS-PLAN-y.md and no doc/product/ and no metaplan; and a tree with doc/product/section_01.md and no index.md"
          rejects: "a checklist that blocks or hides later work: the first must be Step(Vision) with plans_unlisted 2, a PressureTest{x} secondary offer, and step Run marked Active; the second Step(Vision) with primary StartVision, an Unlisted item for doc/product/section_01.md, and no WriteSection offer"
        - id: vision-table-never-guessed
          file: tests/planning_checklist_contracts.rs
          test: vision_table_never_guessed
          scenario: "doc/product/index.md whose sections appear only as '## 01 — Workflow' headings and a bullet list, with no 'Specification sections' table, plus section_01.md; Project::load"
          rejects: "a parser that guesses sections from prose or headings (the primary offer must be FixVisionTable, a vision-table-unreadable problem must name doc/product/index.md, and section_01.md must be an unlisted-section problem)"
        - id: implements-token-forms
          file: tests/planning_checklist_contracts.rs
          test: implements_token_forms
          scenario: "implements::parse on: 'Implements: section_07, [section_08](../product/section_08.md), section_09.md'; 'Implements: none'; no line; a line inside a fenced code block only; two Implements: lines; 'Implements: section_7x, chapter_2'"
          rejects: "a parser that drops the link or file-name forms, reads a fenced line, treats a missing line as none, or accepts bad tokens (Sections([7,8,9]); NoneDeclared; Unmapped; Unmapped; duplicate_lines non-empty with the first line counted; bad_tokens [section_7x, chapter_2])"
        - id: metaplan-entry-any-spelling
          file: tests/planning_checklist_contracts.rs
          test: metaplan_entry_any_spelling
          scenario: "doc/plans/index.md with '## 1. Delivery truth (`PLAN-02-delivery-truth.md`)', '## 2. Old (`DONE-PLAN-old.md`)', '## Policy notes (no plan file)', '## 3. Missing name', '## 4. Policy notes (no plan file)'; a second metaplan doc/plans/round-2-metaplan.md with '## 1. Next (`PLAN-next.md`)' and '## 2. Delivery again (`PLAN-02-delivery-truth.md`)'; doc/plans/archive/DONE-PLAN-old.md and doc/plans/PLAN-02-delivery-truth.md exist, PLAN-next.md does not; Project::load"
          rejects: "entry parsing that fails on a lifecycle spelling in the backticks, counts the prose heading as an entry, ignores the numbered heading without a file, or reads only one metaplan (both metaplans in Project::metaplans; the primary offer is WritePlan for next from round-2-metaplan.md; exactly one metaplan-entry-without-plan problem, on the '## 3.' line; no problem for either Policy notes heading, numbered or not)"
        - id: pressure-step-never-a-gap
          file: tests/planning_checklist_contracts.rs
          test: pressure_step_never_a_gap
          scenario: "a complete project (vision, all sections, metaplan, every plan written) where no plan has a '## Pressure tests' record; and the same with one IN_PROGRESS- plan"
          rejects: "a checklist that treats an untested plan as a gap (no item is a gap; current is Step(PressureTest) with StepMark Optional in the first, and Run is Active in the second)"
        - id: stray-copy-is-a-problem
          file: tests/planning_checklist_contracts.rs
          test: stray_copy_is_a_problem
          scenario: "doc/plans/PLAN-s.md and doc/plans/DONE-PLAN-s.md both present; Project::load"
          rejects: "a project view that reports no stray-plan-copy problem, or names the winning DONE copy instead of PLAN-s.md"
        - id: empty-section-is-a-gap
          file: tests/planning_checklist_contracts.rs
          test: empty_section_is_a_gap
          scenario: "a vision listing 01 and 02; section_01.md written, section_02.md present but empty (zero bytes), and separately holding only blank lines"
          rejects: "a checklist that counts an existing but empty section as written (current Step(ProductSpec), primary WriteSection{2})"
        - id: refresh-matches-load
          file: tests/planning_checklist_contracts.rs
          test: refresh_matches_load
          scenario: "a project with a vision, three sections and two plans; a Watcher's initial batch; then one quiet window holding 30 mixed changes: section_03.md deleted, section_02.md renamed to section_04.md, a plan renamed to IN_PROGRESS-, a metaplan entry added, 26 note files added under doc/product/; Project::refresh with that batch versus a fresh Project::load"
          rejects: "a refresh that keeps a removed or renamed document outside doc/plans/ (refresh must equal load in docs, vision_rows, metaplans, implements, checklist and problems)"
        - id: brief-bundle-grouping
          file: tests/planning_checklist_contracts.rs
          test: brief_bundle_grouping
          scenario: "PLAN-p.md with stages alpha and beta naming one brief outside its bundle and one missing brief (paths built with format! from the briefs dir under doc/plans); its bundle holds common.md, alpha/w1-a.md, gamma/notes.md and mockup.json; a second bundle orphan/ with no plan; briefs::plan_briefs and briefs::orphan_bundles"
          rejects: "brief grouping that ignores the stage ids (alpha/w1-a.md under by_stage alpha; gamma/notes.md and mockup.json and the outside brief in other; common.md in common), drops the missing reference (in broken), or misses the orphan bundle"
        - id: cross-reference-forms
          file: tests/planning_checklist_contracts.rs
          test: cross_reference_forms
          scenario: "xref::references over a section holding a relative link to section_01.md#detection-rules (existing anchor), one to section_01.md#gone, a `doc/plans/PLAN-a.md` path token, a `DONE-PLAN-a.md` plan file name while the plan is doc/plans/IN_PROGRESS-PLAN-a.md, a placeholder token with <slug>, a link to ../../loom/src/main.rs, and an https link"
          rejects: "a resolver that flags the placeholder, the code link or the URL, or resolves the missing anchor (two resolved links, the path token and plan name both resolved to plan/a, exactly one broken reference: #gone)"
        - id: plan-check-json-cli
          file: tests/planning_check_cli_contracts.rs
          test: plan_check_json_cli
          scenario: "runs the loom binary as `loom plan check --json` in the 'plans with no product spec' fixture repository (git init -q) and again in a clean fixture (vision, sections, metaplan, every plan with an Implements: line and a level-1 title); compares the projection of the first stdout (checklist-check/common.md \"Schema fixture\") with tests/fixtures/planning/check_schema.json"
          rejects: "a command that is not dispatched, prints human text under --json, drifts from the pinned schema, or exits 0 with problems (first run exit 1, clean run exit 0 with problems [])"
        - id: plan-check-outside-repo
          file: tests/planning_check_cli_contracts.rs
          test: plan_check_outside_repo
          scenario: "runs `loom plan check --json` with current_dir a TempDir that is not a git repository and GIT_CEILING_DIRECTORIES set to its parent"
          rejects: "a command that falls back to the cwd as root, or exits 0 or 1 (it must exit 2 with a stderr message naming the directory)"

        - id: plan-check-exits-nonzero-per-problem
          file: tests/planning_check_cli_contracts.rs
          test: plan_check_exits_nonzero_per_problem
          scenario: "starting from the clean fixture, eleven repositories each with one defect: a vision whose table is gone; an unlisted section_09.md; a plan without a level-1 title; a numbered metaplan heading with no plan file; a written plan without Implements:; a plan with two Implements: lines; a plan naming section_42; a section linking a missing document; a plan naming a missing brief; a stray PLAN- copy beside a DONE- copy; doc/ as a symlink. Runs `loom plan check --json` in each"
          rejects: "a checker that exits 0 on any of the eleven, or reports the wrong code (exit 1 with exactly the codes vision-table-unreadable, unlisted-section, missing-title, metaplan-entry-without-plan, implements-missing, implements-duplicate, implements-broken, broken-reference, broken-brief-reference, stray-plan-copy, doc-missing respectively)"

    - id: integration-verify
      name: "Integration Verification"
      summary: "Confirms the planning library and loom plan check work on the merged tree, on fixtures and on this repository, and that the full suite passes."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-security-audit"]
      working_dir: "."
      dependencies: ["checklist-check"]
      description: |
        Final verification of doc/plans/PLAN-feat-planner-01-documents.md (doc/plans/IN_PROGRESS-PLAN-feat-planner-01-documents.md while running). Verify FUNCTIONAL INTEGRATION, not only tests passing. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: read the plan, doc/plans/briefs/feat-planner-01-documents/common.md, loom memory show --all, and the knowledge sections the plan cites. working_dir is the repository root.
        BUILD AND TEST (zero tolerance; nothing is pre-existing): every acceptance command below, through one loom check.
        FUNCTIONAL (from this session's shell):
        - cargo run -q --manifest-path loom/Cargo.toml -- plan check from the repository root prints the checklist: record the current step and the problem counts per code in loom memory. Compare with the tree by hand: doc/product/index.md and its sections exist, so steps 1 and 2 are done unless a section is missing; this repository has TWO metaplans, doc/plans/metaplan-planner-ui.md (eleven entries, PLAN-feat-planner-*.md) and doc/plans/optim-token-metaplan.md (six entries naming PLAN-01..06, all satisfied by doc/plans/archive/DONE-PLAN-0<n>-*.md, and '## 6. Policy notes (no plan file)', which must report no problem); both must appear in checklist.state.metaplans and no metaplan gap may be reported.
        - The same with --json is valid JSON with schema 1 (jq -e '.schema == 1').
        - cargo run -q --manifest-path loom/Cargo.toml -- plan verify on a RETIRED-PLAN- copy of a fixture plan under a scratch doc/plans/archive/ still passes, and its brief bundle is found.
        CODE REVIEW: parallel loom-code-reviewer subagents - security (discover.rs and paths.rs against section 12 "Symlinks under doc/", "Paths in requests" and the locked-set row of "What a session may write": no std::fs open of a doc/ path outside safe_read, symlink targets re-opened no-follow from the doc root, a symlinked doc/ never followed, LockedSet fresh per call, nonexistent bundle paths locked); architecture (every later plan's consumption point in common.md exists with the published signature; no second heading scanner, no second slug stripper, no reimplementation of collect_inputs or extract_brief_paths); test coverage (every row of section 01's offers table and "Skipped and reordered steps", the anchor numbering, the watcher batch and rename rules, the mutation notes the stages recorded). Fix or dispute every finding; never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each implemented one with loom memory resolve <id> --outcome implemented --reason <what changed>.
        Record discoveries to loom memory for knowledge-distill, including any knowledge file the tree contradicts: loom memory note "stale-knowledge: ...".
      acceptance:
        - "cargo build --manifest-path loom/Cargo.toml"
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - command: "cargo test --manifest-path loom/Cargo.toml --doc --workspace --all-features -- --test-threads=1"
          timeout_secs: 1200
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps"
        - "cargo run -q --manifest-path loom/Cargo.toml -- plan check --json | jq -e '.schema == 1 and (.checklist.steps | length) == 7'"
        - "cargo run -q --manifest-path loom/Cargo.toml -- plan check --json | jq -e '(.checklist.state.metaplans | length) >= 2'"
        - "cargo run -q --manifest-path loom/Cargo.toml -- plan check --json | jq -e '[.problems[] | select(.code == \"metaplan-entry-without-plan\" or .code == \"doc-missing\")] | length == 0'"
      wiring:
        - source: "loom/src/cli/dispatch.rs"
          pattern: "PlanCommands::Check"
          literal: true
          description: "loom plan check is dispatched"
        - source: "loom/src/plan/inputs.rs"
          pattern: "RETIRED"
          literal: true
          description: "loom run strips RETIRED- when finding a bundle"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records the planning document model in the knowledge base and brings the knowledge index back under its size budget."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries; keep code spot-reads narrow.
        START with loom memory pending --group; read the plan and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge memory in place with loom knowledge replace-section <file> "<heading>" "<body>", never loom knowledge update.
        INDEX BUDGET: loom knowledge check is red at the plan's base because doc/loom/knowledge/INDEX.md is 16454 bytes against a 16384-byte budget. Shorten the longest topic blurbs with loom knowledge annotate <category>/<slug> --blurb "<shorter>" (each write regenerates INDEX.md) until INDEX.md is at most 15000 bytes (wc -c), leaving room for about ten later topic rows; never delete a topic to save bytes; keep every blurb you write under 50 characters. Never run loom knowledge check --write-baseline to absorb an INDEX size issue.
        Then curate: a new tier-2 topic architecture/planning-document-model (loom::planning modules and their rules: kinds and ids, slug resolution and the most-advanced-wins rule, heading sections and GitHub anchors with the app-only loom-metadata and stage:<id> anchors, normalized span hashes, the locked set by name and the brief-to-plan rule, the polling watcher and OwnWrites, the runtime directory, the checklist as a pure function and loom plan check's exit codes), with a 2-4 line summary and link in architecture.md and loom knowledge annotate architecture/planning-document-model --blurb "loom::planning: doc/ discovery, slugs, anchors, checklist". Record the RETIRED- prefix in the plan lifecycle convention wherever the knowledge base lists IN_PROGRESS- and DONE- (rg -l 'IN_PROGRESS-' doc/loom/knowledge). TIER ROUTING as usual.
        DOCS: README.md "Plan Commands" and skills/loom-usage/SKILL.md already describe loom plan check (stage checklist-check); correct them only if the tree disagrees.
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or its topic, then resolve it promoted, merged or discarded.
        RECEIPTS: every memory taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it; finish with loom memory pending --strict and resolve whatever it lists.
        LAST, if this stage removed structural issues, ratchet the baseline: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
        - "test -s doc/loom/knowledge/architecture/planning-document-model.md"
        - 'test "$(wc -c < doc/loom/knowledge/INDEX.md)" -le 15000'
        - "rg -qF 'planning-document-model' doc/loom/knowledge/architecture.md"
      files: ["doc/loom/knowledge/**", "README.md", "skills/loom-usage/SKILL.md"]
```

<!-- END loom METADATA -->
