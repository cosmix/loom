# Stage card-payload

Read `common.md` in this directory first. Working directory `loom/`; paths below are
repository-relative.

## Why

The home cards, the step rail, the feature switcher and the breadcrumbs need a feature title,
counts and a usable pitch that the wire does not carry. The planner's own docs moved from
`planner-ui` to `feat-planner` (`doc/product/feat-planner/`, `doc/plans/feat-planner/`), and
tests and docs that read the old paths fail. The web stages also need a real-browser screenshot
script, which no stage could run last time.

## P1: wire payload (codex gpt-6.1-sol/high through loom-codex-forwarder, in file-sized units the stage description lists; the orchestrator regenerates the fixtures)

**Files owned:**
- `loom/src/plan_server/wire/checklist.rs`
- `loom/src/plan_server/wire/checklist_pitch.rs`
- new `loom/src/plan_server/wire/checklist_card.rs`
- `loom/src/plan_server/wire/checklist_tests.rs`
- `loom/src/plan_server/wire/fixtures/*.json`
- `loom/src/plan_server/features.rs` (its tests only)
- `loom/src/planning/checklist.rs` and `loom/src/planning/checklist/**` (only if a count is not reachable from `Project` without them)

Read first:
- `checklist.rs:39-69` (`ChecklistView`, `StepView`)
- `checklist.rs:226-307` (the two `From` impls)
- `checklist_pitch.rs:7-64`
- `loom/src/planning/checklist.rs:133-150` (`ProjectState`: `sections_listed`, `sections_written`, `plans_by_state`, `plans_unlisted`)
- `loom/src/planning/checklist/items.rs` (the `ItemStatus::Untested` items)
- `loom/src/planning/vision.rs:12-19` (`vision_rows`)
- `loom/src/planning/project.rs:41-42` (`ProjectDoc.open_questions`)

The explorer located these ranges; re-read them. `checklist.rs` declares `pitch` with `#[path]`;
declare `checklist_card.rs` the same way and keep the file under 400 lines.

Add to `ChecklistView`, all serialised with the existing `deny_unknown_fields`:

```rust
pub title: String,
pub has_product: bool,
pub sections: Vec<SectionView>,      // one per row of the vision's section table, in order
pub plans: PlanCountsView,
pub untested: u32,                   // plans whose Pressure test step item is Untested
```

```rust
pub struct SectionView { pub number: String, pub name: String, pub written: bool, pub open_questions: u32 }
pub struct PlanCountsView { pub listed: u32, pub total: u32, pub written: u32, pub running: u32, pub done: u32 }
```

`number` is the two-digit section number ("01"). `written` means `section_NN.md` exists.
`open_questions` is that document's count, or 0. `listed` counts plans the feature's metaplans
list; `total` counts plan files in the feature (any lifecycle state). Add to `StepView`:

```rust
pub count: Option<String>,   // the counter text the rail shows; None renders "—"
pub label: String,           // the rail label under the counter
```

| Step | `count` | `label` |
| --- | --- | --- |
| 1 Vision | `"1"` when the vision exists, else None | "Vision" |
| 2 Product spec | `"<written>/<listed>"` when listed > 0, else None | "Product spec" |
| 3 Metaplan | the number of metaplans when > 0, else None | "Metaplan" |
| 4 Plans | `"<listed plans that exist>/<listed>"` when listed > 0; else the plan count when > 0; else None | "Plans" |
| 5 Pressure test | `untested` when > 0, else None | "Untested" when untested > 0, else "Pressure test" |
| 6 Review | None | "Review" |
| 7 Run | running plans when > 0 → label "Running"; else done plans when > 0 → label "Done"; else None | "Running", "Done" or "Run" |

`From<&Checklist>` cannot see the project. Fill the new fields where `pitch` is filled
(`From<&Project>`, `checklist.rs:232-234`), through one function in `checklist_card.rs`:
`pub(super) fn card_facts(project: &Project, feature: &str, view: &mut ChecklistView)`. Keep
`From<&Checklist>` defaults (empty title is not allowed: use the slug rule).

**Title** (`checklist_card.rs`), per `common.md` decision 4:
1. The vision's first H1, with a trailing `— Product Vision`, `- Product Vision`,
   `Product Vision` or `Vision` removed (case-sensitive, trimmed, along with an em dash or hyphen
   left at the end).
2. Else the first metaplan's H1, with a trailing `Engineering Metaplan` or `Metaplan` removed.
3. Else the slug, with `-` turned to spaces and each word capitalised.

Backticks are removed. A result that is empty after stripping falls through to the next rule.
On this repository that gives "Loom Plan" (`doc/product/feat-planner/index.md`) and "Token
Optimization" (`doc/plans/optim-token/index.md`).

**Pitch** (`checklist_pitch.rs`):
- `first_paragraph` skips a paragraph whose text starts with `Status:`.
- When the feature has no vision, the pitch is the first prose paragraph after the first
  metaplan's H1, by the same rules.
- Keep the 280-character cap and every existing rule (`checklist_tests.rs` pins them).

**Fixtures.** Regenerate `loom/src/plan_server/wire/fixtures/{checklist,checklist-message,hello}.json`
and any other fixture that holds a `ChecklistView`. Populate every new field, `Option` ones
included: an absent optional writes `null` and breaks web parity
(`conventions/plan-web.md`, "New PlanMessage Variants Need Both Schema Arms"). Update the Rust
tests that build `ChecklistView` or `StepView` literals (`rg -n "StepView \{|ChecklistView \{" loom/src`).

Proof: `cargo test --lib plan_server::` and
`cargo test --test plan_feature_card_contracts`, once each.

## P2: web schema and fixtures (sonnet, loom-software-engineer). Starts after P1 returns.

**Files owned:**
- `web/src/plan/api/schema.ts` and `web/src/plan/api/schema-*.ts`
- `web/src/plan/api/fixtures/**`
- `web/src/plan/api/*.test.ts`
- every web test kit or test file that builds a checklist or step object literal, found with
  `rg -ln "pitch:|mark: \"(done|gap|active|optional)\"" web/src`

Steps:
1. Mirror P1's fields in `checklistViewSchema` and the step schema (`schema.ts:~97-125`) as
   strict zod objects with required nullable fields.
2. Copy P1's regenerated Rust fixtures over the web copies. The web fixtures are copies of
   `loom/src/plan_server/wire/fixtures/*` ([why](../../../../loom/knowledge/mistakes/plan-server-delivery.md#green-jsdom-tests-hid-a-broken-browser-2026-10-29)).
3. Add the new fields to every object literal the `rg` above finds, using:
   - `title` from the slug with words capitalised;
   - `has_product: true`;
   - `sections: []`;
   - zeroed `plans`;
   - `untested: 0`;
   - step `count: null` with `label` equal to `name`.
4. Do not change any component.

Proof: `node_modules/.bin/vitest run src/plan` from `web/`, once.

## P3: stale paths and the vision pitch (haiku, loom-software-engineer, model: haiku)

**Files owned:**
- `loom/src/planning/guidance/tests.rs`
- `loom/tests/planning_guidance_contracts.rs`
- `loom/tests/plan_session_bridge_core_contracts/records.rs`
- `doc/product/feat-planner/index.md`
- `doc/product/feat-planner/section_04.md`
- `doc/product/feat-planner/section_08.md`
- `doc/plans/feat-planner/index.md`

Steps:
1. Wherever these files name the directories `doc/product/planner-ui/`, `doc/plans/planner-ui/`,
   `doc/loom/summaries/planner-ui/` or the ids `overview/planner-ui/...`, `planner-ui/product/...`,
   change `planner-ui` to `feat-planner`. Exact sites: `guidance/tests.rs:19`,
   `planning_guidance_contracts.rs:194`, `records.rs:4`, `section_04.md:35,73-98`,
   `section_08.md:328,330`, and the metaplan's lines 3-4, 12-13 and 603.
2. Leave the branch name `feat-planner-ui` and plan slugs `feat-planner-NN-*` unchanged.
3. In `doc/product/feat-planner/index.md`, insert one paragraph directly under the H1, before the
   `Status:` paragraph, with exactly this Markdown:

   ```markdown
   A browser app that takes a project from an empty `doc/` to reviewed loom plans without a terminal.
   ```

Proof: `cargo test --test planning_guidance_contracts`, once.

## P4: screenshot script (sonnet, loom-software-engineer)

**Files owned:**
- new `scripts/plan-screens.sh`
- new `web/scripts/plan-screens.mjs`
- `web/package.json`
- `web/bun.lock`

`bun add -d playwright-core@<version>` in `web/`. Pick the version whose pinned Chromium
headless shell is already in `~/.cache/ms-playwright`. Read each candidate's
`browsers.json` in `~/.bun/install/cache/playwright-core@*/`. When none matches, launch with
`executablePath` set to the cached `chrome-headless-shell` binary instead.

`scripts/plan-screens.sh <loom-binary> <out-dir> <route>...` follows the procedure that worked
last time (`mistakes/plan-server-delivery.md`, "Green jsdom Tests Hid a Broken Browser"):

1. Make a scratch dir with `mktemp -d "${TMPDIR:-/tmp}/plan-screens.XXXXXX"`.
2. In it, take a shallow clone of the worktree (`git clone --depth 1 file://$PWD`, sparse to
   `doc/`, then `git checkout --detach`). The clone holds HEAD, so the operator commits or stages
   nothing for it. When the caller passes `--worktree-doc`, copy the worktree's current `doc/`
   over the clone, so uncommitted docs show.
3. Start `<loom-binary> plan --port 0 --no-open` with `LOOM_HOME` set to the scratch dir, and
   read the token URL from the log.
4. In the same shell, run `node web/scripts/plan-screens.mjs <url> <out-dir> <routes...>`. It:
   - opens the token URL;
   - visits each route at 1440x900 and 390x844, in Ledger (light) and Aubergine (dark). Set the
     theme by the same `localStorage` key `web/src/aurora-ui/theme` reads; read it, never guess;
   - writes `<out-dir>/<route-slug>-<width>-<theme>.png` full page;
   - fails with exit 1 on any console error or a navigation that does not reach the app root.
5. Kill the server on exit (`trap`).

Usage line in a header comment; POSIX shell plus bash; `set -euo pipefail`; no network.

Proof: run it once against the current UI with routes `/` and `/f/feat-planner`, and read one
PNG.
