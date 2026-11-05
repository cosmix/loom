# Plan: Planner polish from the operator's review of 2026-11-05

Implements: none

## Overview

The operator reported 15 planner issues:

- The footer stacks the limit meters.
- A Pressure test spine row that only reports a status.
- A doubled Stop notice.
- An unstyled chat sidebar: composer, head, run line, menu and the Add to chat references.
- No middle summary level.
- A metaplan without a summary.
- A chat panel that collapses.
- Mixed progress glyphs.
- Small sidebar type.
- A faint Graphite accent.
- Plain Settings statistics.
- Limit meters in Settings.
- Help and docs that must follow all of this.

This plan fixes all 15 in two stages. `doc/plans/briefs/planner-polish/common.md` holds the issue list, the operator's decisions and the plan's settled design decisions (D1-D14). The per-worker briefs sit beside it. The YAML is authoritative for gates and file ownership.

Models (operator's instruction):
- Sonnet and haiku wherever they can do the work.
- Fable for the visual design, as two workers on disjoint files: V1 for the shell (footer, spine badges, roundel, spine type, Graphite accent) and V3 for the whole chat sidebar. Both load `frontend-design:frontend-design`.
- Opus at effort medium for the Settings statistics (V2), with the same skill.
- Opus or fable otherwise only after a cheaper tier failed.
- Stage main agents run at their stage-type defaults (sonnet for standard stages).

## Goals

- Integration-verify fixes each of the 15 issues and checks it in the running app, one line per issue with evidence.
- Behaviour changes ship with a test that fails on the old behaviour.
- Non-goals:
  - rebuilding stored summaries (the operator's next rebuild builds the metaplan summaries and refreshes the stale engineering overview);
  - a middle summary level on document pages (the spec keeps them summary → source);
  - changing the engineering overview's citation rules.

## Operator decisions (asked 2026-11-05)

- Issue 6: features open on the product overview again, and the stale engineering overview gets an operator rebuild.
- Issue 7: each metaplan gets a document summary, rendered like any other summary page.
- Issue 2: the Pressure page goes with its spine row. Plan rows carry the pressure record.
- Issue 15 (added mid-planning): the whole chat sidebar is redesigned by fable.
- Lanes: Claude only.

## Evidence behind the diagnoses

- **Issue 1.** `QuotaMeters` (`web/src/components/quota-meters.css:1-4`) sets `container-type: inline-size`, and its layouts switch by container query (stacked under 680px). The dashboard places it in a `minmax(0, 1fr)` grid track (`web/src/routes/shell.tsx:61-85`). The planner makes it a flex item of `.plan-footer` (`web/src/plan/ui/design.css:192-206`), so the container has no intrinsic width and selects the stacked layout.
- **Issue 3.** `Session::interrupt` (`loom/src/plan_session/bridge/commands.rs:316-338`) adds "Stopped". Claude Code's interrupt record then makes `interrupt()` (`loom/src/plan_session/transcript/content.rs:121-143`) push "Interrupted" into the same turn. Both render through `NoticeItem` (`web/src/plan/chat/items.tsx:230-257`) as separate ruled breaks.
- **Issue 4.**
  - `StopControl` renders a bare `div` as a grid row after `.chat-composer-input` (`chat/composer.tsx:354`, `stop-control.tsx:53-66`), left-aligned under Send.
  - `RunHeader` (`chat/run-header.tsx:38-58`) is `flex-wrap` with every segment a flex item, so separators orphan.
- **Issue 5.** `ChatMenu` (`chat/panel-head.tsx:40-54`) is a native `<details>` whose item is a bordered secondary `.plan-button` inside a bordered `.chat-menu-pop`, with no Escape or outside-click close.
- **Issue 6.** By design. `landingPath` sends features to `vision` (`web/src/plan/feature/landing.ts:7-11`), a document summary, which per `doc/product/feat-planner/section_05.md:28-30` has two levels (summary, source). The level-1 path is intact (`concept/run.tsx:35-51`). The stored `overview/feat-planner/engineering.md` predates the two-member rule: 12 of 28 sentences cite one member sentence.
- **Issue 7.** `summary_id_for` (`loom/src/planning/summary/store.rs:115-140`) has no metaplan arm, `MetaplanPage` renders the raw source (`web/src/plan/feature/document-page.tsx:83-101`), and `section_05.md:22,50` says "The metaplan has none".
- **Issue 8.** `panelCollapsedAtom` (`chat/prefs.ts:51,64-67`) and `CollapsedRail` (`chat/panel.tsx:165-190`).
- **Issue 9.** Busy states mix `ActivityGlyph` icon animations (`ui/activity-glyphs.tsx`), `LoadState`'s skeleton (`ui/load-state.tsx:21-28`), `BusyRoundel` in two chat spots, and about a dozen text-only messages.
- **Issue 11.** Graphite's `--primary` is `oklch(0.78 0.015 270)` (`web/src/aurora-ui/shared/styles/tokens.css:323-366`), near grey.
- **Issue 13.** `SummaryPanel` renders `QuotaMeters` (`web/src/plan/summaries/summary-panel.tsx:43`), and Settings mounts it (`settings/settings-sections.tsx:94-116`).
- **Issue 15.** The operator's screenshot: in one composer box, "Now viewing section_03 ×" sits above two chips. Each chip has a book tile, a quote cut after a lone opening quote mark, a × and a second source line. The rows are aligned three ways. Send sits under the box.

## Before `loom run`

- The operator commits `doc/plans/PLAN-planner-polish.md` and `doc/plans/briefs/planner-polish/**` on `feat-planner-ui`. `loom run` refuses an uncommitted plan or brief bundle, and a stage worktree is cut from `HEAD`.
- Optional: commit the current `doc/loom/summaries/**` (untracked at `96358689`), so the worktree's screens show summaries instead of "not built yet".

## Sibling plans

Never run this plan at the same time as `PLAN-live-test-lane`, `PLAN-autonomous-recovery`, `PLAN-loom-ui-mods`, `PLAN-model-router-hooks`, `PLAN-adopt-ecc-best-practices` or `PLAN-web-host-graft-followthrough`. Whichever runs second starts from the merged tree.

## Issue to gate map

| # | Stage | Executable proof |
| --- | --- | --- |
| 1 | chat-and-shell-visual | after_stage grid check on `.plan-footer`; frozen footer contracts; IV footer shot beside the dashboard |
| 2 | summaries-and-spine, chat-and-shell-visual | contract `plan-rows-carry-the-pressure-record`; after_stage `pressure-page.tsx` gone; badge shots |
| 3 | summaries-and-spine | contract `stop-then-interrupt-is-one-notice`; N1's unit test |
| 4 | chat-and-shell-visual | contract `stop-sits-in-the-action-row`; composer and head shots |
| 5 | chat-and-shell-visual | frozen `chat-fixes` menu contract; C1's close tests; menu shot |
| 6 | summaries-and-spine | contract `feature-opens-on-product-overview`; the engineering overview rebuild is an operator check |
| 7 | summaries-and-spine | contracts `metaplan-summary-is-expected`, `metaplan-route-is-a-summary`; wiring `DocKind::Metaplan` in the store, `Metaplan` in the page key |
| 8 | chat-and-shell-visual | contract `chat-panel-has-no-collapse`; after_stage `DESKTOP` at 1280px; shots at 1280px and 1180px |
| 9 | chat-and-shell-visual | contract `progress-uses-the-roundel`; wiring `BusyRoundel` in `activity-glyphs.tsx`; after_stage `BusyText` in B1's files |
| 10 | chat-and-shell-visual | after_stage no hard-coded 15px chat prose; shots |
| 11 | chat-and-shell-visual | contract `graphite-accent-is-bright`; Graphite shots |
| 12 | chat-and-shell-visual | Settings shots (IV issue walk) |
| 13 | chat-and-shell-visual | contract `settings-shows-no-limits`; after_stage no `QuotaMeters` in `summary-panel.tsx` |
| 14 | chat-and-shell-visual | after_stage spec and help checks; D1's report |
| 15 | chat-and-shell-visual | composer shots with two and five references (IV issue walk) |

## Paths

- Both implementation stages run with `working_dir: .` (the worktree root). The first writes `web/` and `loom/`; the second writes `web/` and `doc/product/`.
- Paths are repository-relative. Acceptance commands enter `web/` with `cd web`; cargo uses `--manifest-path loom/Cargo.toml`.
- Web contracts set `cwd: web`; Rust contracts set `cwd: loom`.
- Integration-verify keeps `working_dir: loom`.

## Gate scope

- Each implementation stage runs the planner's whole vitest tree (`vitest run src/plan`), because the changes touch shared parts (`chat/`, `ui/`, `feature/`). The visual stage also runs `src/components` and `src/aurora-ui`, because the Graphite token is shared with the dashboard.
- Rust runs are scoped to the modules and integration targets the first stage changes. Integration-verify runs the full suites.
- Each stage runs `cargo test --test maintainability` (the 400 and 50 line ratchet against `loom/maintainability-baseline.txt`, a `ratchet_files` entry). No web lint enforces the web limit, so each stage's main agent runs `wc -l` over every web file it changed before the final review round, and splits any file above 400.

## Execution Diagram

```mermaid
graph LR
    summaries-and-spine --> chat-and-shell-visual
    chat-and-shell-visual --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

There is no knowledge-bootstrap stage. `architecture/planner-web-shell.md`, `planner-feature-screens.md`, `summary-store.md` and `conventions/plan-web.md` already cover this area.

### 1. summaries-and-spine

This is the plan's behaviour stage: the metaplan summary end to end (Rust store through web route), the product-overview landing, the pressure record on plan rows with the Pressure page removed, and the single Stop notice.

Workers, in one wave:

- M1 (sonnet): the metaplan summary in Rust.
- W1 (sonnet): the web routes, landing and spine data.
- N1 (haiku): the notice.

Risk walk:

- **Behaviour.** `metaplan-summary-is-expected`, `metaplan-route-is-a-summary`, `feature-opens-on-product-overview`, `plan-rows-carry-the-pressure-record`, `stop-then-interrupt-is-one-notice`.
- **Blast radius of a new expected id.** Summary id lists and counts in lib tests and in the Rust summary contract targets. M1 updates only those, and the main agent files one `dispute-integrity`.
- **Untrusted input.** The metaplan stem becomes a path segment. `validate_id` enforces `[a-z0-9][a-z0-9-]*`, and the contract rejects `..` and an uppercase stem.
- **Planned frozen edits** (common.md table, W1's): `spine.contracts` `SPINE_ROWS` and "a feature opens its vision"; `review-fixes.contracts` "a feature with a written spec opens on its vision"; `pages.contracts` "the Pressure page lists every plan with its record state".

### 2. chat-and-shell-visual

It exists by Q1 and Q2:

- Q1: V1 styles the plan-row badge W1 adds, and D1 documents the first stage's routes and summaries.
- Q2: it writes `feature/spine-row.tsx`, `help/legend-entries.tsx` and `help/getting-around-scene.tsx` after the first stage.

Workers:

- Wave 1, in parallel:
  - V1 (fable): footer, plan badges, roundel, spine type, Graphite accent, DESIGN.md.
  - V3 (fable): the chat sidebar redesign, issues 15, 4, 5, 8's layout and the chat half of 10.
  - C1 (sonnet): no collapse, the 1280px breakpoint, menu close keys.
  - V2 (opus medium): the Settings statistics and the limits removed.
- Wave 2, after wave 1:
  - B1 (haiku): `BusyText` on text-only busy messages.
  - D1 (sonnet): help topics and the product spec.

Risk walk:

- **Behaviour.** `chat-panel-has-no-collapse`, `stop-sits-in-the-action-row`, `settings-shows-no-limits`, `progress-uses-the-roundel`, `graphite-accent-is-bright`.
- **Frozen contracts that must hold, unedited:**
  - `chat-fixes.contracts`: the `<details>` menu and the composer border.
  - `chat.contracts`: the fold label and the changed-since button.
  - `context.contracts`: the Viewing group and Dismiss context.
  - The footer contracts and `popover-stats.contracts`.
- **Planned frozen edit.** `summaries/contracts.test.tsx` loses its quota assertions (V2).
- **Accessibility.** The roundel sits in an `aria-hidden` wrapper, so the caller's status text is the only announcement. Reduced motion stops it. The 13px floor holds.
- **Help scene pins.** `help/chat-scene.test.tsx` requires one match per pin target, and Stop's selector moves with it (V3).

### Integration Verification

Opus by default (the stage type's default). It builds `web/dist` and the binary, runs a diagnostic check and the browser preflight, then the journeys and the screens, and walks the 15 issues with evidence (`loom memory note "issue-walk #N: …"`). It fixes what fails: visual misses go to fable, everything else to the cheapest capable tier. It re-runs the final full check against the rebuilt bytes, then the final review round (a) to (d), and commits the bundle last. See `doc/plans/briefs/planner-polish/integration-verify.md`.

### Knowledge Distillation

It rewrites to the new truth:

- `architecture/planner-web-shell.md`: "Header and dock" (footer grid, Settings without limits), "Chat home and the chat panel" (no collapse, 1280px, Stop placement, menu, chips, type) and the roundel.
- `architecture/planner-feature-screens.md`: landing on the product overview, the metaplan summary route, five spine groups, plan-row pressure badges, no Pressure page.
- `architecture/summary-store.md`: the metaplan id, outside both overviews.
- `conventions/plan-web.md`: the roundel and `BusyText` rule.

It records in `mistakes/`:

- the footer meters stacking because a container-query component sat in a flex item;
- the doubled Stop notice, from two producers of one event.

Then it curates the rest.

## Environment inventory

| Need | State |
| --- | --- |
| crates.io, npm registry | `allowed_domains` |
| `web/node_modules` | provisioned by `loom.provision` |
| Chromium for screens | `~/.cache/ms-playwright` on the host, as earlier planner plans used it. Each stage that shoots runs the browser preflight first. A failing preflight is filed with `loom stage block <stage-id> "<preflight output>"`. |
| Browser preflight | after the build, from the worktree root: `scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/preflight" /` |
| tmux in a stage session | not granted. `scripts/plan-journeys.sh` runs WITHOUT `--strict`, and PENDING journeys are reported. |
| Host platform | Linux. On macOS the operator sets `allow_local_binding: true` first. |
| Local server and Playwright | the server start, the URL wait and the browser run share one Bash call. A throwaway driver under `$LOOM_SCRATCH_DIR` imports `playwright-core` by absolute path (`<worktree>/web/node_modules/playwright-core/index.mjs`) and launches with `args: ["--no-sandbox"]`. A session view is fed by stubbing `WebSocket` in `addInitScript` and replaying `web/src/plan/api/fixtures/chat-snapshot.json` frames. |
| Claude login in a stage | none. A real chat, a real Stop and a real summary rebuild are operator checks after the run. |
| `web/dist` | the visual stage builds the bundle to shoot, then restores it before its final review round: `git restore web/dist`, then `git clean -fdq -- web/dist`, then `git status --short --untracked-files=all web/dist` prints nothing. Only integration-verify commits a bundle. |
| `/tmp` plan lock | J3 (edit and save) fails in a stage sandbox; an operator runs it outside a stage. |
| Throwaway drivers | every throwaway script and its output live under `$LOOM_SCRATCH_DIR`, never in the worktree. |

## After the run (operator checks)

- Rebuild summaries. Each feature metaplan gains `doc/loom/summaries/<feature>/metaplan/<stem>.md`, and `overview/feat-planner/engineering` is rebuilt with no sentence opening to a single member sentence.
- In a real chat, press Stop during a turn and see one notice.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  auto_merge: null
  sandbox:
    enabled: true
    auto_allow: true
    excluded_commands: []
    filesystem:
      deny_read:
      - ~/.ssh/**
      - ~/.aws/**
      - ~/.config/gcloud/**
      - ~/.gnupg/**
      deny_write:
      - ../../**
      allow_write: []
    network:
      allowed_domains:
      - crates.io
      - index.crates.io
      - static.crates.io
      - registry.npmjs.org
      additional_domains: []
      allow_local_binding: false
      allow_unix_sockets: []
      allow_all_unix_sockets: false
    linux:
      enable_weaker_nested: false
    command_confinement: confined
  ratchet_files:
  - loom/maintainability-baseline.txt
  - doc/loom/knowledge/check-baseline.txt
  provision:
  - working_dir: web
    command: >-
      test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile
      --ignore-scripts --backend=copyfile --config=/dev/null
  stages:
  - id: summaries-and-spine
    name: Metaplan summaries, product-overview landing, pressure badges, one Stop notice
    summary: >-
      Gives every metaplan its own summary page, opens features on the product overview again,
      moves the pressure record onto each plan in the sidebar (the Pressure page goes), and leaves
      one notice when the user presses Stop.
    description: |
      Read doc/plans/briefs/planner-polish/common.md, then the three briefs in doc/plans/briefs/planner-polish/summaries-and-spine/, before dispatch. Pass common.md and each worker's brief path to that worker.
      PATHS: working_dir is the worktree root. Every path in this stage is repository-relative; acceptance commands enter web/ with cd web, and cargo uses --manifest-path loom/Cargo.toml.
      KNOWLEDGE: architecture/summary-store.md, architecture/planner-feature-screens.md (Layout route and landing, Spine), architecture/planning-session-bridge.md, conventions/plan-web.md (Concept Mode Test Rules, UI Repair Test Rules).
      PUBLIC SURFACE for the contract session:
      - Rust loom/src/planning/summary/store/metaplan_contract_tests.rs (test path planning::summary::store::metaplan_contract_tests::a_feature_metaplan_has_a_document_summary). Build the project as loom/src/planning/summary/store/tests.rs project() does (write_files into a TempDir, Project::load), with doc/product/alpha/index.md, doc/product/alpha/section_01.md, doc/plans/alpha/index.md, doc/plans/alpha/metaplan-02.md and the misplaced doc/plans/index.md. Assert through crate::planning::summary::store: expected_ids(&project) contains "alpha/metaplan/index" and "alpha/metaplan/metaplan-02" and no id ending "metaplan/" for doc/plans/index.md; summary_id_for(&project, Path::new("doc/plans/alpha/metaplan-02.md")) is Some("alpha/metaplan/metaplan-02"); summary_path("alpha/metaplan/index") is doc/loom/summaries/alpha/metaplan/index.md; source_of(&project, "alpha/metaplan/index") is Some(doc/plans/alpha/index.md); validate_id rejects "alpha/metaplan/../x" and "alpha/metaplan/Index"; Scope::of(&project, "overview/alpha/engineering") is Some and none of its product, engineering or outside lists contains an id with "/metaplan/". At base validate_id rejects the ids, so it fails. The contract session registers it by adding `#[cfg(test)] mod metaplan_contract_tests;` after `mod vision_contract_tests;` in loom/src/planning/summary/store.rs; workers do not touch that line.
      - Rust loom/src/plan_session/transcript/stop_contract_tests.rs (test path plan_session::transcript::stop_contract_tests::stop_then_interrupt_shows_one_notice). Set up a mapper with an open turn holding a pending tool, as the test in loom/src/plan_session/transcript/tests.rs that expects notice("n-u3", NoticeKind::Interrupted, "Interrupted") does; call add_server_item with Item::Notice { id: "stopped-1", kind: NoticeKind::Stopped, text: "Stopped" }; then ingest the same "[Request interrupted by user" record that test uses. Assert the turn holds exactly one Item::Notice and its kind is Stopped. At base a second Interrupted notice follows, so it fails. Register with `#[cfg(test)] mod stop_contract_tests;` after `mod tests_server;` in loom/src/plan_session/transcript/mod.rs; workers do not touch that line.
      - Web web/src/plan/feature/polish.contracts.test.tsx (top-level it() calls). Copy finishedChecklist and its helpers, renderRoutes and the stub server from web/src/plan/feature/spine.contracts.test.tsx (do not import from a frozen file).
        - "a feature with a written spec opens on its product overview": renderRoutes("/f/atlas", finishedChecklist()); waitFor the router's pathname to be "/f/atlas/product".
        - "the metaplan route shows the metaplan summary with Source one click away": the stub answers concept key "atlas/metaplan/index" with { ...clone(web/src/plan/api/fixtures/concept-summary.json), key: "atlas/metaplan/index", title: "Metaplan", feature: "atlas", path: "doc/plans/atlas/index.md" }, with the checklist's metaplans holding doc/plans/atlas/index.md; render "/f/atlas/metaplan"; the reading column (role document, name "Reading column") holds that fixture's first sentence text, and a link named exactly "Source" has an href starting "/f/atlas/source/plans/atlas/". At base the route renders the raw source, so the sentence text is absent.
        - "plan rows carry the pressure record and the spine has no Pressure test row": a checklist with two written plans in plan_rows, one with pressure_date "2026-10-20" and one null; render "/f/atlas/engineering"; inside the navigation "Feature documents", the link for the tested plan's slug has text containing "Pressure-tested 2026-10-20", the untested one's contains "Not pressure-tested", and no link's name matches /^Pressure test\b/.
      - Do not write web contracts for this stage beyond these three.
      Frozen and binding: common.md "Frozen contracts that bind this plan"; the planned edits are W1's and M1's rows of its table.
      WORKERS (one message, all three in parallel):
      | Worker | Role | Tier | Files owned | Shared context | Brief path |
      | ------ | ---- | ---- | ----------- | -------------- | ---------- |
      | M1 | Metaplan summary id through store, rebuild and concept page (issue 7) | sonnet | loom/src/planning/summary/**; loom/src/plan_server/concept/**; loom/src/plan_server/summaries/**; loom/src/plan_server/wire/concept.rs; loom/tests/planning_summary_contracts.rs; loom/tests/planning_summary_cli_contracts.rs; loom/tests/planning_summary_support/**; loom/tests/summary_engine_contracts.rs; loom/tests/plan_concept_contracts.rs; loom/tests/plan_summary_rebuild_contracts.rs; loom/tests/planning_summary_session_cli.rs; plan-plugin/agents/summary.md; loom/src/planning/guidance/text/summary_codex.md | common.md | doc/plans/briefs/planner-polish/summaries-and-spine/m1-metaplan-summary.md |
      | W1 | Metaplan route, landing, pressure record on plan rows, Pressure page removed (issues 2, 6, 7) | sonnet | web/src/plan/router.tsx; web/src/plan/router.test.tsx; web/src/plan/feature/**; web/src/plan/api/page-key.ts; web/src/plan/api/page-key.test.ts; web/src/plan/api/schema-concept.ts; web/src/plan/concept/column-context.ts; web/src/plan/concept/concept-route.tsx; web/src/plan/concept/concept-route.test.tsx; web/src/plan/concept/document-key.tsx; web/src/plan/concept/document-key.test.tsx; web/src/plan/chat/viewed-document.ts; web/src/plan/chat/viewed-document.test.ts; web/src/plan/chat/context-kit.ts; web/src/plan/help/legend-entries.tsx; web/src/plan/help/legend.test.tsx; web/src/plan/help/getting-around-scene.tsx | common.md | doc/plans/briefs/planner-polish/summaries-and-spine/w1-web-routes.md |
      | N1 | One notice after Stop (issue 3) | haiku | loom/src/plan_session/transcript/content.rs; loom/src/plan_session/transcript/tests.rs | common.md | doc/plans/briefs/planner-polish/summaries-and-spine/n1-stop-notice.md |
      Spawn M1 and W1 as loom-software-engineer and N1 as loom-mechanical-engineer. Each prompt says: Your brief: <path>. Read it and common.md in full before anything else.
      Seams: M1's wire scope string is "metaplan"; W1's ConceptScope and page-key parsing use the same string. W1 reads the page key "<feature>/metaplan/<stem>" that M1's PageKey formats.
      A worker that reports a needed change outside its files: you make it if it passes the small-change rule, else spawn a fresh worker of the same tier after the wave. Any file in the stage's files list not in the table belongs to that follow-up worker only.
      FIX ROUNDS: a failure goes to a fresh worker one tier above the one that failed (haiku to sonnet, sonnet to opus); after a second failure on one item, loom-advisor diagnoses first.
      AFTER THE WORKERS: (cd web && node_modules/.bin/oxfmt); (cd loom && cargo fmt --all). Run wc -l over every web file the stage changed and split any above 400. The gate is one loom check summaries-and-spine beside the final loom-code-reviewer round. After the commit, file one dispute-integrity naming every planned frozen edit (common.md table: W1's three, M1's id-list rows by file and test) and every pinned test the workers reported changing.
      LEDGER: no file this stage edits may cross 400 lines, and a function over 50 lines is split.
      Use parallel subagents and skills to maximize performance.
      MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.
    dependencies: []
    acceptance:
    - cd web && node_modules/.bin/tsc -b --noEmit
    - cd web && node_modules/.bin/oxlint --deny-warnings
    - cd web && node_modules/.bin/oxfmt --check
    - cd web && node_modules/.bin/vitest run src/plan
    - cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings
    - cargo fmt --manifest-path loom/Cargo.toml --check
    - command: 'cargo test --manifest-path loom/Cargo.toml --lib -- planning::summary:: plan_server:: plan_session::transcript::'
      timeout_secs: 1800
    - command: >-
        cargo test --manifest-path loom/Cargo.toml --test planning_summary_contracts --test
        planning_summary_cli_contracts --test summary_engine_contracts --test plan_concept_contracts
        --test plan_summary_rebuild_contracts --test planning_summary_session_cli
      timeout_secs: 1800
    - cargo test --manifest-path loom/Cargo.toml --test maintainability
    - RUSTDOCFLAGS="-D warnings" cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps
    files:
    - loom/src/planning/summary/**
    - loom/src/plan_server/concept/**
    - loom/src/plan_server/summaries/**
    - loom/src/plan_server/wire/concept.rs
    - loom/tests/planning_summary_contracts.rs
    - loom/tests/planning_summary_cli_contracts.rs
    - loom/tests/planning_summary_support/**
    - loom/tests/summary_engine_contracts.rs
    - loom/tests/plan_concept_contracts.rs
    - loom/tests/plan_summary_rebuild_contracts.rs
    - loom/tests/planning_summary_session_cli.rs
    - plan-plugin/agents/summary.md
    - loom/src/planning/guidance/text/summary_codex.md
    - loom/src/plan_session/transcript/content.rs
    - loom/src/plan_session/transcript/tests.rs
    - loom/src/plan_session/transcript/mod.rs
    - loom/src/plan_session/transcript/stop_contract_tests.rs
    - web/src/plan/router.tsx
    - web/src/plan/router.test.tsx
    - web/src/plan/feature/**
    - web/src/plan/api/page-key.ts
    - web/src/plan/api/page-key.test.ts
    - web/src/plan/api/schema-concept.ts
    - web/src/plan/concept/column-context.ts
    - web/src/plan/concept/concept-route.tsx
    - web/src/plan/concept/concept-route.test.tsx
    - web/src/plan/concept/document-key.tsx
    - web/src/plan/concept/document-key.test.tsx
    - web/src/plan/chat/viewed-document.ts
    - web/src/plan/chat/viewed-document.test.ts
    - web/src/plan/chat/context-kit.ts
    - web/src/plan/help/legend-entries.tsx
    - web/src/plan/help/legend.test.tsx
    - web/src/plan/help/getting-around-scene.tsx
    working_dir: .
    stage_type: standard
    wiring:
    - source: loom/src/planning/summary/store.rs
      pattern: DocKind::Metaplan
      description: the store gives a feature metaplan a summary id (issue 7)
      literal: true
    - source: loom/src/plan_server/concept/page_key.rs
      pattern: Metaplan
      description: concept page keys address a metaplan summary page (issue 7)
      literal: true
    - source: web/src/plan/feature/spine-row.tsx
      pattern: plan-spine__badge
      description: plan rows render the pressure record (issue 2)
      literal: true
    before_stage:
    - command: test -e web/src/plan/feature/pressure-page.tsx
      description: the Pressure page exists (issue 2)
    - command: rg -qF '<SourcePage path={metaplan.path}' web/src/plan/feature/document-page.tsx
      description: the metaplan route renders raw source (issue 7)
    after_stage:
    - command: '! test -e web/src/plan/feature/pressure-page.tsx'
      description: the Pressure page is gone (D6)
    - command: '! rg -qF ''<SourcePage path={metaplan.path}'' web/src/plan/feature/document-page.tsx'
      description: the metaplan route no longer renders raw source (D3)
    - command: rg -q 'fn is_metaplan' loom/src/planning/summary
      description: the store recognises metaplan summary ids (D2)
    - command: rg -q 'fn an_interrupt_after_a_stopped_notice_adds_no_second_notice' loom/src/plan_session/transcript
      description: N1's regression test exists (D1)
    - command: '! rg -qF ''a feature opens its vision'' web/src/plan/feature/spine.contracts.test.tsx'
      description: the planned landing contract removal is in place (D4)
    subagent_timeout_secs: 900
    implementers:
    - claude
    skills:
    - loom-rust
    - loom-react
    - loom-typescript
    contracts:
    - id: metaplan-summary-is-expected
      file: loom/src/planning/summary/store/metaplan_contract_tests.rs
      cwd: loom
      test: planning::summary::store::metaplan_contract_tests::a_feature_metaplan_has_a_document_summary
      scenario: >-
        a project whose feature alpha has two metaplans and a misplaced top-level metaplan;
        expected_ids, summary_id_for, summary_path, source_of, validate_id and the engineering scope
      rejects: >-
        a store with no metaplan id, one that admits an uppercase or dotted stem, one that summarizes
        the misplaced metaplan, or one that makes the metaplan summary an overview member
    - id: stop-then-interrupt-is-one-notice
      file: loom/src/plan_session/transcript/stop_contract_tests.rs
      cwd: loom
      test: plan_session::transcript::stop_contract_tests::stop_then_interrupt_shows_one_notice
      scenario: a turn holding a Stopped server notice receives Claude Code's interrupt record
      rejects: a mapper that adds an Interrupted notice after the Stopped one
    - id: feature-opens-on-product-overview
      file: web/src/plan/feature/polish.contracts.test.tsx
      cwd: web
      test: a feature with a written spec opens on its product overview
      scenario: the feature index route for a feature with a written product spec
      rejects: the landing that sends a feature with a product spec to its vision page
    - id: metaplan-route-is-a-summary
      file: web/src/plan/feature/polish.contracts.test.tsx
      cwd: web
      test: the metaplan route shows the metaplan summary with Source one click away
      scenario: /f/atlas/metaplan with the stub serving a summary page for atlas/metaplan/index
      rejects: a metaplan route that still renders the raw source, or a Source link that leaves the feature
    - id: plan-rows-carry-the-pressure-record
      file: web/src/plan/feature/polish.contracts.test.tsx
      cwd: web
      test: plan rows carry the pressure record and the spine has no Pressure test row
      scenario: the spine over two written plans, one pressure tested and one not
      rejects: a spine that still draws a Pressure test row, or plan rows without their pressure record
  - id: chat-and-shell-visual
    name: Chat sidebar redesign, footer, roundel, type, Graphite accent, Settings statistics, help and docs
    summary: >-
      Redesigns the whole chat sidebar (references, composer, Stop, head, menu) and keeps it always
      shown on wide screens, lays the footer limits side by side, uses one activity roundel for all
      progress, enlarges sidebar type, brightens Graphite, restyles the Settings statistics without
      the limits, and updates help and the product spec.
    description: |
      Read doc/plans/briefs/planner-polish/common.md, then the six briefs in doc/plans/briefs/planner-polish/visual-and-chat/, and web/src/plan/ui/DESIGN.md, before dispatch. Pass common.md and each worker's brief path to that worker.
      PATHS: working_dir is the worktree root. Every path in this stage is repository-relative; acceptance commands enter web/ with cd web.
      KNOWLEDGE: architecture/planner-web-shell.md (Header and dock, Chat home and the chat panel), conventions/plan-web.md (Chat Panel and Review UI Rules, Visual Stages and the Screenshot Script, UI Repair Test Rules).
      PUBLIC SURFACE for the contract session (top-level it() calls):
      - Web web/src/plan/chat/polish.contracts.test.tsx:
        - "the chat panel has no collapse at a wide viewport": mount ChatPanel the way web/src/plan/chat/panel.test.tsx does (its panel-test-kit harness and wide matchMedia stub), after setting localStorage[storageKey(<the harness project_key>, "panel-collapsed")] = "true". The complementary region named "Chat" contains the textbox named "Message", and queryByRole("button", { name: /Collapse chat|Expand chat/ }) is null. At base the rail renders "Expand chat", so it fails.
        - "Stop sits in the composer's action row": render ComposerArea for a held session in state working, as web/src/plan/chat/composer.test.tsx does for its Stop cases; getByRole("button", { name: "Stop" }).closest(".chat-composer-actions") is not null. At base Stop is a row after .chat-composer-input, so it fails.
      - Web web/src/plan/settings/polish.contracts.test.tsx: "Settings shows the summary statistics and no provider limits". Render the Settings dialog open on its Summaries group, as web/src/plan/settings/settings-dialog.test.tsx does where it finds the region "Token totals", with the panel from web/src/plan/api/fixtures/summary-panel.json. Within the dialog: getByRole("region", { name: "Token totals" }) exists, and queryByRole("img", { name: /window .*% used/ }) is null.
      - Web web/src/plan/ui/polish.contracts.test.tsx:
        - "progress glyphs and loading states draw the roundel": render <ActivityGlyph kind="summaries" /> from web/src/plan/ui/activity-glyphs.tsx (with any props its type requires) and <LoadState state="loading" title="Loading the feature" /> from web/src/plan/ui/load-state.tsx; each container holds an element matching ".aurora-roundel", and LoadState's role="status" element keeps aria-label "Loading the feature".
        - "the Graphite accent is a bright colour": import "@/aurora-ui/shared/styles/tokens.css?raw"; take the text of the rule block whose selector contains '[data-dark-theme="gray"]' and declares --primary; parse --primary as oklch(L C H) and expect C >= 0.12.
      Frozen and binding: common.md "Frozen contracts that bind this plan"; V2's planned edit of summaries/contracts.test.tsx is the only frozen edit.
      BEFORE SPAWNING: cd "$(git rev-parse --show-toplevel)" && (cd web && bun run build) && (cd loom && cargo build). Run the browser preflight (plan Environment inventory); on failure spawn no worker and file loom stage block chat-and-shell-visual "<preflight output>". Shoot the base: scripts/plan-screens.sh --worktree-doc --themes=ledger,graphite loom/target/debug/loom "$LOOM_SCRATCH_DIR/before" /f/feat-planner/product, and pass the shot paths to V1 and V3.
      WORKERS, wave 1 (one message, all four in parallel):
      | Worker | Role | Tier | Files owned | Shared context | Brief path |
      | ------ | ---- | ---- | ----------- | -------------- | ---------- |
      | V1 | Footer, plan badges, roundel, spine type, Graphite accent, DESIGN.md (issues 1, 2, 9, 10, 11) | fable | web/src/plan/ui/**; web/src/plan/feature/spine.css; web/src/plan/feature/feature.css; web/src/plan/feature/spine-row.tsx; web/src/plan/shell-footer.tsx; web/src/aurora-ui/shared/styles/tokens.css; web/src/plan/help/legend-entries.tsx; web/src/plan/help/legend.test.tsx | DESIGN.md | doc/plans/briefs/planner-polish/visual-and-chat/v1-visual.md |
      | V3 | The chat sidebar redesign (issues 15, 4, 5, 8, 10) | fable | web/src/plan/chat/chat.css; web/src/plan/chat/chat-surface.css; web/src/plan/chat/chat-composer.css; web/src/plan/chat/chat-transcript.css; web/src/plan/chat/chat-decision.css; web/src/plan/chat/composer.tsx; web/src/plan/chat/composer.test.tsx; web/src/plan/chat/composer-concluded.test.tsx; web/src/plan/chat/composer-context.test.tsx; web/src/plan/chat/stop-control.tsx; web/src/plan/chat/run-header.tsx; web/src/plan/chat/end-chat.tsx; web/src/plan/chat/state-line.tsx; web/src/plan/chat/state-line.test.tsx; web/src/plan/chat/state-line-header.test.tsx; web/src/plan/chat/chat-state-glyph.tsx; web/src/plan/chat/tool-row.tsx; web/src/plan/chat/tool-row.test.tsx; web/src/plan/chat/chips.tsx; web/src/plan/chat/chips.test.tsx; web/src/plan/chat/chips-labels.test.tsx; web/src/plan/chat/file-chip.tsx; web/src/plan/chat/context-line.tsx; web/src/plan/chat/context-line.test.tsx; web/src/plan/chat/items.tsx; web/src/plan/chat/items.test.tsx; web/src/plan/chat/transcript.tsx; web/src/plan/chat/transcript.test.tsx; web/src/plan/chat/queue-strip.tsx; web/src/plan/chat/permission-card.tsx; web/src/plan/help/chat-scene.tsx; web/src/plan/help/chat-scene.test.tsx | DESIGN.md | doc/plans/briefs/planner-polish/visual-and-chat/v3-chat-sidebar.md |
      | C1 | No collapse, 1280px breakpoint, menu close keys (issues 8, 5) | sonnet | web/src/plan/chat/panel.tsx; web/src/plan/chat/panel.test.tsx; web/src/plan/chat/panel-head.tsx; web/src/plan/chat/panel-header.test.tsx; web/src/plan/chat/panel-a11y.test.tsx; web/src/plan/chat/panel-offers.test.tsx; web/src/plan/chat/prefs.ts; web/src/plan/chat/prefs.test.ts; web/src/plan/chat/offer-flow.ts; web/src/plan/chat/start-chat.tsx; web/src/plan/chat/start-chat.test.tsx; web/src/plan/home/move-into-feature.tsx; web/src/plan/home/move-into-feature.test.tsx; web/src/plan/attachments/add-to-chat.tsx; web/src/plan/attachments/add-to-chat.test.tsx; web/src/plan/header-extras.tsx | common.md | doc/plans/briefs/planner-polish/visual-and-chat/c1-chat-always-shown.md |
      | V2 | Settings statistics, limits removed (issues 12, 13) | opus medium | web/src/plan/summaries/summary-panel.tsx; web/src/plan/summaries/summary-panel.test.tsx; web/src/plan/summaries/contracts.test.tsx; web/src/plan/settings/** | DESIGN.md | doc/plans/briefs/planner-polish/visual-and-chat/v2-settings-stats.md |
      Spawn V1 and V3 as loom-senior-software-engineer with the model override fable; spawn V2 as loom-senior-software-engineer with effort medium; spawn C1 as loom-software-engineer. V1's, V3's and V2's prompts say: First invoke the frontend-design:frontend-design skill with the Skill tool. Then read your brief <path>, common.md and web/src/plan/ui/DESIGN.md in full before anything else. C1's prompt says: Your brief: <path>. Read it and common.md in full before anything else.
      Seams: V1 writes web/src/plan/ui/busy-text.tsx first; V3 and V2 import it, and report if it was not there yet (you wire it after the wave). V3 styles the head through CSS; it reports head markup needs, which you pass to C1 if running, else make (small-change rule) or give to a fresh sonnet worker. V1 is the only editor of ui/DESIGN.md: V3's and V2's exception and type lines go to V1 if running, else you add them.
      WORKERS, wave 2 (after wave 1 returns; one message, both in parallel):
      | Worker | Role | Tier | Files owned | Shared context | Brief path |
      | ------ | ---- | ---- | ----------- | -------------- | ---------- |
      | B1 | BusyText on text-only busy messages (issue 9) | haiku | web/src/plan/new/document-starter.tsx; web/src/plan/review/send-to-author.tsx; web/src/plan/review/built-marker.tsx; web/src/plan/chat/turn-footer-plans.tsx; web/src/plan/chat/restore.tsx; web/src/plan/chat/pressure/pressure-dialog.tsx; web/src/plan/chat/pressure/pressure-header.tsx | common.md | doc/plans/briefs/planner-polish/visual-and-chat/b1-busy-text.md |
      | D1 | Help topics and the product spec (issue 14) | sonnet | web/src/plan/help/topics.ts; web/src/plan/help/topics.test.tsx; web/src/plan/help/pressure-scene.tsx; web/src/plan/help/pressure-scene.test.tsx; web/src/plan/help/getting-around-scene.tsx; doc/product/feat-planner/index.md; doc/product/feat-planner/section_05.md; doc/product/feat-planner/section_06.md; doc/product/feat-planner/section_08.md; doc/product/feat-planner/section_11.md | common.md | doc/plans/briefs/planner-polish/visual-and-chat/d1-help-and-docs.md |
      Spawn B1 as loom-mechanical-engineer and D1 as loom-software-engineer; pass D1 the stage's git diff --stat and a summary of each wave-1 report.
      A worker that reports a needed change outside its files: you make it if it passes the small-change rule, else spawn a fresh worker of the same tier after the wave. Any file in the stage's files list not in the tables belongs to that follow-up worker only.
      VISUAL CHECK after wave 1: (cd web && node_modules/.bin/oxfmt), rebuild bundle and binary, then scripts/plan-screens.sh --worktree-doc --themes=ledger,graphite loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens" / /f/feat-planner/product /f/feat-planner/metaplan. Drive a throwaway Playwright script under $LOOM_SCRATCH_DIR (one Bash call with its server, WebSocket stubbed with chat-snapshot.json frames) for: the chat composer with two and with five Add to chat references and a file chip, a running turn with Stop, the open chat menu, the run line at a 20rem panel, the kickoff on /new, Settings Summaries, the footer at 1440px, and a feature page at 1280px and 1180px. Judge every item of V1's, V3's and V2's fixed targets against the shots and run common.md's refinement loop for each miss (fable for V1 or V3 areas, opus medium for V2's).
      FIX ROUNDS (non-visual): a failure goes to a fresh worker one tier above the one that failed; after a second failure on one item, loom-advisor diagnoses first.
      AFTER ALL WORKERS: (cd web && node_modules/.bin/oxfmt). Restore web/dist before the final review round (plan Environment inventory). Run wc -l over every web file the stage changed and split any above 400. The gate is one loom check chat-and-shell-visual beside the final loom-code-reviewer round. After the commit, file one dispute-integrity naming V2's planned frozen edit and every pinned test the workers reported changing, by file.
      LEDGER: no file this stage edits may cross 400 lines, and a function over 50 lines is split.
      Use parallel subagents and skills to maximize performance.
      MEMORY: record mistakes, decisions and surprises with loom memory immediately, subagents too; NEVER Claude Code auto-memory.
    dependencies:
    - summaries-and-spine
    acceptance:
    - cd web && node_modules/.bin/tsc -b --noEmit
    - cd web && node_modules/.bin/oxlint --deny-warnings
    - cd web && node_modules/.bin/oxfmt --check
    - cd web && node_modules/.bin/vitest run src/plan src/components src/aurora-ui
    files:
    - web/src/plan/ui/**
    - web/src/plan/feature/spine.css
    - web/src/plan/feature/feature.css
    - web/src/plan/feature/spine-row.tsx
    - web/src/plan/shell-footer.tsx
    - web/src/aurora-ui/shared/styles/tokens.css
    - web/src/plan/help/**
    - web/src/plan/chat/**
    - web/src/plan/home/move-into-feature.tsx
    - web/src/plan/home/move-into-feature.test.tsx
    - web/src/plan/attachments/add-to-chat.tsx
    - web/src/plan/attachments/add-to-chat.test.tsx
    - web/src/plan/header-extras.tsx
    - web/src/plan/summaries/summary-panel.tsx
    - web/src/plan/summaries/summary-panel.test.tsx
    - web/src/plan/summaries/contracts.test.tsx
    - web/src/plan/settings/**
    - web/src/plan/new/document-starter.tsx
    - web/src/plan/review/send-to-author.tsx
    - web/src/plan/review/built-marker.tsx
    - doc/product/feat-planner/index.md
    - doc/product/feat-planner/section_05.md
    - doc/product/feat-planner/section_06.md
    - doc/product/feat-planner/section_08.md
    - doc/product/feat-planner/section_11.md
    working_dir: .
    stage_type: standard
    artifacts:
    - web/src/plan/ui/busy-text.tsx
    wiring:
    - source: web/src/plan/ui/activity-glyphs.tsx
      pattern: BusyRoundel
      description: progress glyphs draw the aurora-ui activity roundel (issue 9)
      literal: true
    - source: web/src/plan/chat/pressure/pressure-header.tsx
      pattern: <BusyText
      description: text-only busy messages carry the roundel (issue 9)
      literal: true
    - source: web/src/plan/settings/settings-sections.tsx
      pattern: <SummaryPanel
      description: Settings still shows the summary statistics (issue 12)
      literal: true
    before_stage:
    - command: rg -qF 'panelCollapsedAtom' web/src/plan/chat/prefs.ts
      description: the chat panel can collapse (issue 8)
    - command: rg -qF 'QuotaMeters' web/src/plan/summaries/summary-panel.tsx
      description: Settings shows the limit meters (issue 13)
    after_stage:
    - command: '! rg -q ''panelCollapsedAtom|CollapsedRail'' web/src/plan'
      description: the collapse and the rail are gone (D7)
    - command: 'rg -qF ''(min-width: 1280px)'' web/src/plan/chat/offer-flow.ts'
      description: the panel shows from 1280px (D7)
    - command: '! rg -qF ''QuotaMeters'' web/src/plan/summaries/summary-panel.tsx'
      description: Settings shows no limit meters (D14)
    - command: '! rg -qF ''font-size: 15px'' web/src/plan/chat/chat-transcript.css'
      description: the chat prose moved up one step (D11)
    - command: 'rg -qU ''\.plan-footer \{[^}]*display: grid'' web/src/plan/ui/design.css'
      description: the footer is a grid like the dashboard's, so the meters sit side by side (D13)
    - command: '! rg -qF ''The metaplan has none'' doc/product/feat-planner/section_05.md'
      description: the product spec describes metaplan summaries (issue 14)
    - command: rg -qF 'Not pressure-tested' web/src/plan/help/topics.ts
      description: help describes the plan-row pressure badge (issue 14)
    subagent_timeout_secs: 1200
    implementers:
    - claude
    skills:
    - loom-react
    - loom-typescript
    contracts:
    - id: chat-panel-has-no-collapse
      file: web/src/plan/chat/polish.contracts.test.tsx
      cwd: web
      test: the chat panel has no collapse at a wide viewport
      scenario: the chat panel at a wide viewport with a stored panel-collapsed value of true
      rejects: a panel that renders the collapsed rail, or one that still offers Collapse chat
    - id: stop-sits-in-the-action-row
      file: web/src/plan/chat/polish.contracts.test.tsx
      cwd: web
      test: Stop sits in the composer's action row
      scenario: the composer for a held session while a turn runs
      rejects: a Stop button on its own row after the composer input
    - id: settings-shows-no-limits
      file: web/src/plan/settings/polish.contracts.test.tsx
      cwd: web
      test: Settings shows the summary statistics and no provider limits
      scenario: the Settings dialog's Summaries group over a full summary panel
      rejects: a Settings dialog that still draws the Claude or Codex limit meters, or one that drops the token totals
    - id: progress-uses-the-roundel
      file: web/src/plan/ui/polish.contracts.test.tsx
      cwd: web
      test: progress glyphs and loading states draw the roundel
      scenario: a summaries activity glyph and a loading LoadState
      rejects: a spinning icon or a skeleton as the progress indicator
    - id: graphite-accent-is-bright
      file: web/src/plan/ui/polish.contracts.test.tsx
      cwd: web
      test: the Graphite accent is a bright colour
      scenario: the Graphite theme's --primary token in tokens.css
      rejects: a near-grey Graphite accent (oklch chroma below 0.12)
  - id: integration-verify
    name: Integration Verification
    summary: >-
      Runs the full suites and the browser journeys, checks each of the 15 review issues in the
      running app with evidence, reviews the changes, and rebuilds the embedded web bundle.
    description: |
      Final verification of the planner polish plan. Verify FUNCTIONAL INTEGRATION, not just tests passing. NEVER Claude Code auto-memory.
      CONTEXT: read doc/plans/briefs/planner-polish/common.md and integration-verify.md, loom memory show --all, doc/loom/knowledge/INDEX.md and only the relevant sections.
      PATHS: the session starts in loom/ (working_dir). Commands the brief gives "from the worktree root" start with cd "$(git rev-parse --show-toplevel)"; acceptance commands run from loom/ and reach web/ with cd ../web.
      Follow integration-verify.md in order: build (web/dist and the binary), a diagnostic loom check, the browser preflight, journeys, screens, the issue walk (one line per issue with evidence), fixes each followed by a dist rebuild, the final loom check against the rebuilt bytes, the final parallel loom-code-reviewer round (a) to (d), and the commits.
      BUILD & TEST (zero tolerance): the first loom check integration-verify is diagnostic; a fix round re-checks the failed criteria with --criterion <n>. After the last fix and its dist rebuild, run the full loom check integration-verify once more: that run is the acceptance, and nothing but *.md prose may change after it and the final review round. Visual fixes go to a fresh loom-senior-software-engineer with the model override fable that invokes frontend-design:frontend-design first, with the shot paths; Settings statistics fixes to opus with effort medium and the same skill. Other fixes go to the cheapest capable engineer: haiku for a one-line value, sonnet for code, opus only after a failed sonnet attempt. Every finding is fixed or disputed, never deferred.
      JOURNEYS: run scripts/plan-journeys.sh WITHOUT --strict and list every PENDING id in the issue walk.
      ISSUE WALK: record each of the 15 lines with loom memory note "issue-walk #N: fixed|not fixed|could not check here: <evidence>", citing the gate the plan's Issue to gate map names, and the operator checks as loom memory note "issue-walk open: ...".
      SUGGESTIONS: weigh every pending reviewer suggestion; resolve each one implemented with loom memory resolve <id> --outcome implemented --reason <what changed>.
      WEB DIST: after every fix, rebuild web/dist (cd ../web && bun run build, from loom/) and the binary (cargo build). After the final round, commit the code fixes, then web/dist last as chore(web).
      BROWSER: run the browser preflight after the first build. If it fails, file loom stage block integration-verify "<preflight output>".
      Record discoveries to loom memory, including stale knowledge: loom memory note "stale-knowledge: ...".
    dependencies:
    - summaries-and-spine
    - chat-and-shell-visual
    acceptance:
    - command: cargo test --all-targets --no-fail-fast
      timeout_secs: 3600
    - cargo clippy --all-targets -- -D warnings
    - cargo fmt --check
    - cargo build
    - RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
    - >-
      cd ../web && node_modules/.bin/tsc -b --noEmit && node_modules/.bin/oxlint
      --deny-warnings && node_modules/.bin/oxfmt --check && node_modules/.bin/vitest run
    - cd ../web && node scripts/bundle-check.mjs
    - >-
      cd ../web && d=$(mktemp -d "${TMPDIR:-/tmp}/dist.XXXXXX") &&
      node_modules/.bin/vite build --outDir "$d" --emptyOutDir --logLevel error && diff -r "$d" dist
    files:
    - ../web/dist/**
    working_dir: loom
    stage_type: integration-verify
    implementers:
    - claude
    skills:
    - loom-rust
    - loom-react
    - loom-typescript
  - id: knowledge-distill
    name: Knowledge Distillation
    summary: >-
      Records metaplan summaries, the new landing, plan-row pressure badges, the always-shown chat
      sidebar and the activity roundel in the knowledge base, with the footer and Stop lessons as
      mistakes.
    description: |
      Curate all stage memories into permanent knowledge. NEVER Claude Code auto-memory. SINGLE-AGENT: do NOT spawn subagents.
      START with loom memory pending --group; read the plan and the knowledge sections it touches: architecture/planner-web-shell.md, architecture/planner-feature-screens.md, architecture/summary-store.md, conventions/plan-web.md, concerns/planner-ui-repair-followups.md.
      CORRECTIONS FIRST: apply every stale-knowledge memory with loom knowledge replace-section. Known stale claims:
      - planner-feature-screens.md "Layout route and landing" says landingPath sends every feature with a vision to vision and that a metaplan has no summary; "Spine" lists six groups including pressure and the optional pressure mark; "Pressure and Run pages" describes the Pressure page.
      - planner-web-shell.md "Header and dock" says Settings' SummaryPanel holds the quota meters; "Chat home and the chat panel" describes the collapsed rail, the 1024px breakpoint, BusyRoundel only inside a live chat, and the old sizes.
      - summary-store.md lists only section, vision and plan document ids.
      NEW KNOWLEDGE: mistakes entries for (1) the footer meters stacking because a container-query component (container-type: inline-size) sat in a flex item with no definite width (prevention: place a container-query component in a sized grid track or give it flex-basis), and (2) the doubled Stop notice from two producers of one event (prevention: when the server announces an action, the record that echoes it must not announce it again). Record the metaplan summary id grammar and that it belongs to no overview, the roundel and BusyText rule, and the 1280px chat breakpoint. Carry the issue-walk "could not check here" and "open" lines into concerns/planner-ui-repair-followups.md.
      TIER ROUTING: about 40 lines or fewer inline in the tier-1 file; larger via loom knowledge update <category>/<slug> with a 2-4 line tier-1 summary and link.
      SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or the topic it belongs to, then resolve it.
      RECEIPTS: every taken memory gets loom memory resolve right after the write; finish with loom memory pending --strict.
    dependencies:
    - integration-verify
    acceptance:
    - loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt
    - loom memory pending --strict
    files:
    - doc/loom/knowledge/**
    working_dir: .
    stage_type: knowledge-distill
    before_stage:
    - command: '! rg -qF ''/metaplan/'' doc/loom/knowledge/architecture/summary-store.md'
      description: knowledge does not yet describe metaplan summary ids
    after_stage:
    - command: rg -qF '/metaplan/' doc/loom/knowledge/architecture/summary-store.md
      description: the metaplan summary id is recorded
    - command: rg -q 'container-type' doc/loom/knowledge/mistakes
      description: the footer container-query lesson is recorded
    - command: rg -qF 'BusyText' doc/loom/knowledge
      description: the roundel rule is recorded
    implementers:
    - claude
```

<!-- END loom METADATA -->
