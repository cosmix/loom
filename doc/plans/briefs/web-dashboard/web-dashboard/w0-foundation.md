# W0 — Foundation: web scaffold, Rust wire model, shared fixture, zod schema

Tier: codex `gpt-5.6-luna`, effort `xhigh`. FOUNDATION: runs ALONE; W1, W2 and W3 spawn only
after this unit returns. Do not run `git` at all. Do not touch `.loom/`.

Read `doc/plans/PLAN-web-dashboard.md` § "Design decisions" first; every value below comes from it.

## Files you own (write)

- `loom/Cargo.toml`, `loom/Cargo.lock` — through `cargo add` only, never by hand
- `loom/src/commands/status.rs` — exactly ONE new line
- `loom/src/commands/status/web/mod.rs` — new, skeleton (W1 rewrites it afterwards)
- `loom/src/commands/status/web/model.rs` — new, complete
- `web/**` — new scaffold (the exact file list is in step 3)
- `.gitignore` (repo root) — one new line

Read-only: `loom/src/commands/status/data/mod.rs` (`StatusData`, `StageSummary`, `MergeSummary`,
`ProgressSummary`, `ActivityStatus`), `loom/src/commands/status/render/attention_model.rs`
(`AttentionEntry`, `attention_entries`, `failure_label`), `loom/src/orchestrator/scheduling_report.rs`
(`Alert`, `Severity`, `alerts`), `loom/src/orchestrator/tick.rs` (`read`, `Tick::age_secs`),
`loom/src/daemon/server/core.rs` (`DaemonStatus`, `DaemonServer::check_status`),
`loom/src/models/failure.rs` (`FailureInfo`, `FailureType`), `loom/src/models/session/types.rs`
(`SessionType`, `SessionBackendKind`).

## Step 1 — Cargo dependencies

From the repo root:

```bash
cargo add tungstenite@0.30.0 httparse@1.10.1 --manifest-path loom/Cargo.toml
```

`tungstenite` keeps its default feature set (`handshake`). Nothing else changes in `Cargo.toml`.

## Step 2 — Rust wire model

### `loom/src/commands/status.rs`

Insert `pub mod web;` directly after the line `pub mod ui;` (line 6). Nothing else. The file is at
398 lines; it may reach 399 and not one more.

### `loom/src/commands/status/web/mod.rs` (skeleton)

```rust
//! `loom status --web`: the embedded web dashboard. The server modules are added by the
//! server unit; this file only exposes the wire model until then.

pub mod model;
```

### `loom/src/commands/status/web/model.rs`

Every type derives `Debug, Clone, Serialize, Deserialize`; the enums also `Copy, PartialEq, Eq`.
Doc comments on every `pub` item (the doc gate runs with `-D warnings`).

```rust
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::commands::status::data::StatusData;
use crate::commands::status::render::attention_model::AttentionEntry;
use crate::commands::status::render::failure_label;
use crate::daemon::{DaemonServer, DaemonStatus};
use crate::models::failure::FailureType;
use crate::orchestrator::scheduling_report::{self, Alert, Severity};
use crate::orchestrator::tick;

/// One frame of the web dashboard: everything the page renders, as one JSON object.
/// Served by `/api/status` and pushed as every WebSocket text frame.
pub struct WebSnapshot {
    pub status: StatusData,
    pub attention: Vec<WebAttention>,
    pub alerts: Vec<WebAlert>,
    pub daemon: DaemonState,
    pub tick_age_secs: Option<i64>,
    pub source: SnapshotSource,
    pub generated_at: DateTime<Utc>,
}

#[serde(rename_all = "kebab-case")]
pub enum DaemonState { Running, ProcessOnly, NotRunning, Unreachable }

#[serde(rename_all = "kebab-case")]
pub enum SnapshotSource { Daemon, Files }

/// `AttentionEntry` with owned strings so it can be serialized.
pub struct WebAttention {
    pub id: String,
    pub name: String,
    pub label: String,
    pub hint: String,
    pub failure_type: Option<FailureType>,
    /// `failure_label(failure_type)` when `failure_type` is set.
    pub failure_label: Option<String>,
    pub evidence: Vec<String>,
    pub review_reason: Option<String>,
    pub cleanup_warning: Option<String>,
    pub has_human_review_choices: bool,
    pub dispute_count: Option<u32>,
    pub judge_heartbeat_secs: Option<u64>,
}

pub struct WebAlert { pub severity: WebSeverity, pub text: String }

#[serde(rename_all = "lowercase")]
pub enum WebSeverity { Info, Warning, Critical }

impl From<&AttentionEntry> for WebAttention { /* field-by-field; label.to_owned() */ }
impl From<&Alert> for WebAlert { /* Severity::Info -> Info, Warning -> Warning, Critical -> Critical */ }
impl From<DaemonStatus> for DaemonState { /* Running->Running, ProcessOnly->ProcessOnly, NotRunning->NotRunning, Unreachable->Unreachable */ }

impl DaemonState {
    /// Whether the scheduler alerts should treat the daemon as running.
    /// `Unreachable` counts as running: it means this process's sandbox cannot
    /// open the socket, not that the daemon is gone (`daemon/server/core.rs`).
    pub fn is_running(self) -> bool { matches!(self, Self::Running | Self::Unreachable) }
}

/// Wrap a `StatusData` with everything the TUI otherwise computes client-side.
/// `work_path` is the `.loom/work` directory (`WorkDir::root()`).
pub fn collect_snapshot(work_path: &Path, status: StatusData, source: SnapshotSource) -> WebSnapshot {
    let daemon = DaemonState::from(DaemonServer::check_status(work_path));
    let attention = crate::commands::status::render::attention_entries(&status.stages)
        .iter().map(WebAttention::from).collect();
    let alerts = scheduling_report::alerts(work_path, daemon.is_running())
        .iter().map(WebAlert::from).collect();
    let tick_age_secs = tick::read(work_path).ok().flatten().map(|tick| tick.age_secs(Utc::now()));
    WebSnapshot { status, attention, alerts, daemon, tick_age_secs, source, generated_at: Utc::now() }
}
```

Check the exact paths of `DaemonServer`/`DaemonStatus` re-exports in `loom/src/daemon/mod.rs:11`
(`status.rs:9` imports them as `crate::daemon::{DaemonServer, DaemonStatus}`), and that
`attention_entries` is re-exported at `render/mod.rs:13`.

### Tests in `model.rs` (`#[cfg(test)] mod tests`)

1. `fixture_matches_serde_output` — REQUIRED NAME. Build a `WebSnapshot` in code (a helper
   `fn fixture_snapshot() -> WebSnapshot`), serialize it with `serde_json::to_value`, parse
   `include_str!("../../../../../web/src/api/fixtures/snapshot.json")` (five `..`: web/ is at the
   repository root, model.rs is five directories below it) with `serde_json::from_str::<serde_json::Value>`,
   and `assert_eq!(actual, expected, "fixture out of date; expected:\n{}", serde_json::to_string_pretty(&expected).unwrap())`.
   You write the fixture file FROM this test: run it once with an empty-object fixture, copy the
   printed JSON into `web/src/api/fixtures/snapshot.json`, run it again, observe it pass.
2. `fixture_deserializes_into_web_snapshot` — `serde_json::from_str::<WebSnapshot>(FIXTURE)`
   succeeds and `snapshot.status.stages.len() == 7`.
3. `daemon_state_maps_every_variant` — the four `DaemonStatus` variants map as listed and
   `is_running()` is true only for `Running` and `Unreachable`.
4. `attention_conversion_keeps_failure_label` — a `StageSummary` with status `Blocked` and a
   `FailureInfo { failure_type: TestFailure, .. }` converts to `WebAttention { label: "BLOCKED", failure_label: Some("test"), .. }`.

`fixture_snapshot()` content — seven `StageSummary` values (fill EVERY field explicitly; read
`data/mod.rs:71-139` for the list; `generated_at` and `detected_at` are fixed
`Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0)`):

| id | name | status | stage_type | dependencies | notable fields |
| --- | --- | --- | --- | --- | --- |
| `knowledge-bootstrap` | Bootstrap Knowledge | `Completed` | `Knowledge` | [] | merged false, model "opus", elapsed 412, execution 380, activity Idle, everything else None/false/empty |
| `server` | Rust server | `Executing` | `Standard` | [knowledge-bootstrap] | activity Working, last_tool "Bash", last_activity "cargo test", staleness 3, context_tokens 312000, context_ceiling_tokens 800000, pid 4242, session_alive true, model "opus", execution_models ["sonnet", "gpt-5.6-terra"], session_type Stage, session_backend Native, elapsed 905, execution 640 |
| `client` | TypeScript client | `CompletedWithFailures` | `Standard` | [knowledge-bootstrap] | retry_count 1, max_retries Some(3), failure_info Some(TestFailure, detected_at fixed, evidence ["cargo test failed", "1 test failed: schema::parses_fixture", "see loom stage retry client"]), activity Error, elapsed 700, execution 655, model "opus" |
| `design` | Visual design | `WaitingForDeps` | `Standard` | [server, client] | activity Idle, model "opus", elapsed 0 |
| `docs` | Documentation | `MergeConflict` | `Standard` | [knowledge-bootstrap] | merged false, activity Idle, model "opus", elapsed 300, execution 290 |
| `integration-verify` | Integration Verification | `NeedsHumanReview` | `IntegrationVerify` | [design, docs] | held true, review_reason Some("acceptance criterion 3 disputed twice"), dispute_count 2, judge_heartbeat_secs Some(40), model "opus", elapsed 120 |
| `knowledge-distill` | Knowledge Distillation | `WaitingForDeps` | `KnowledgeDistill` | [integration-verify] | model "sonnet", activity Idle |

`merge`: merged [], pending ["docs"], conflicts ["docs"]. `progress`: total 7, completed 1,
executing 1, pending 3, blocked 2. `plan_name`: Some("Web Dashboard Fixture"). `attention` MUST be
`attention_entries(&status.stages)` converted, never hand-listed (it yields client, docs,
integration-verify in stage order). `alerts`: `[Info "1 stage waiting on a free slot", Warning "client failed acceptance; retrying in 30s", Critical "orchestrator loop stalled 75s"]`.
`daemon: Running`, `tick_age_secs: Some(4)`, `source: Daemon`.

## Step 3 — Web scaffold

All commands from the repo root unless a `cd` is shown. Versions are pinned; do not upgrade.

```bash
bun create vite web --template react-ts
cd web
bun install
bun add react@19.2.8 react-dom@19.2.8 react-router@8.3.1 jotai@2.20.3 zod@4.5.4 tailwindcss@4.3.3 @tailwindcss/vite@4.3.3 @fontsource-variable/inter@5.3.0 @fontsource/ibm-plex-mono@5.3.0
bun add -d vitest@5.0.0 oxfmt@0.66.0 @testing-library/react@16.3.3 jsdom@30.0.1 @types/node@26.4.1
```

Keep the scaffold's `typescript` (`~6.0.2`), `oxlint`, `@vitejs/plugin-react`, `vite`.

### `web/vite.config.ts` (replace the scaffold's)

```ts
/// <reference types="vitest/config" />
import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:7373",
      "/ws": { target: "ws://127.0.0.1:7373", ws: true },
    },
  },
  build: {
    sourcemap: false,
    rolldownOptions: {
      output: {
        entryFileNames: "assets/[name].js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: "assets/[name][extname]",
      },
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    globals: false,
  },
});
```

`build.rolldownOptions` is the Vite 8 name (`rollupOptions` is a deprecated alias:
`node_modules/vite/dist/node/index.d.ts:867-884`). `src/test/setup.ts` is written by W2; leave it
absent.

### tsconfig path alias

Add `"paths": { "@/*": ["./src/*"] }` to `compilerOptions` in BOTH `tsconfig.app.json` and
`tsconfig.node.json` (they are JSONC with comments: edit by hand, do not parse as JSON). Never add
`baseUrl`. `shadcn init` adds `compilerOptions.paths` to `tsconfig.json` itself (verified); if it
did not, add the same block there.

### shadcn/ui

```bash
bunx shadcn@4.21.0 init -b radix -p nova -y --css-variables --no-monorepo
bunx shadcn@4.21.0 add -y badge button card dialog kbd progress scroll-area separator table tooltip
bun remove @fontsource-variable/geist
```

`init` needs the `@/*` alias to exist first (it stops with "Could not find valid path aliases"
otherwise). It writes `components.json` (style `radix-nova`), `src/lib/utils.ts`, rewrites
`src/index.css`, and adds `shadcn`, `cn`, `radix-ui`, `lucide-react`, `tw-animate-css`,
`class-variance-authority`, `@fontsource-variable/geist` to dependencies. Geist is removed above
because the fonts are Inter and IBM Plex Mono.

### `web/src/index.css` — edit AFTER shadcn init

At the very top:

```css
@import "@fontsource-variable/inter";
@import "@fontsource/ibm-plex-mono/400.css";
@import "@fontsource/ibm-plex-mono/500.css";
@import "@fontsource/ibm-plex-mono/600.css";
```

Replace the line `@import "tailwindcss";` with these three lines, verbatim:

```css
@import "tailwindcss" source(none);
@source "./";
@source "../index.html";
```

Why: Tailwind 4 otherwise scans the whole project for class names, including the committed
`dist/` bundle, and the CSS grows on every rebuild (verified in a scratch build; the plan
records the numbers). Delete the `@import "@fontsource-variable/geist";` line. In the
`@theme inline { ... }` block set `--font-sans: 'Inter Variable', sans-serif;` and add
`--font-mono: 'IBM Plex Mono', monospace;`. Leave every other generated token in place: W3
themes the file later and owns it from then on.

### Package scripts (`web/package.json`)

```json
"scripts": {
  "dev": "vite",
  "build": "tsc -b && vite build",
  "preview": "vite preview",
  "typecheck": "tsc -b --noEmit",
  "lint": "oxlint --deny-warnings",
  "lint:fix": "oxlint --fix",
  "format": "oxfmt",
  "format:check": "oxfmt --check",
  "test": "vitest run",
  "check": "bun run typecheck && bun run lint && bun run format:check && bun run test"
}
```

### Lint and format config

`.oxlintrc.json`: keep the scaffold's plugins and rules, add
`"ignorePatterns": ["dist/**", "src/components/ui/**"]` (generated shadcn code exports variants
next to components and trips `react/only-export-components` under `--deny-warnings`).

Run `bunx oxfmt --init` to create `.oxfmtrc.json`, then create `web/.prettierignore` containing
`dist/` and `src/components/ui/` (oxfmt honours `.prettierignore`). Run `bunx oxfmt` once over
the tree so the scaffold is formatted.

### Scaffold leftovers

Delete `web/src/App.tsx`, `web/src/App.css`, `web/src/assets/`, `web/public/vite.svg`,
`web/README.md` (W3 writes the real one). Leave `web/index.html` as scaffolded (W3 rewrites it)
and `web/src/vite-env.d.ts`.

`web/.gitignore` (from the scaffold): DELETE the line `dist` — the built dist is committed.
Repo-root `.gitignore`: append `web/node_modules/` under the `# loom worktrees` block.

### `web/src/main.tsx` — write its final pinned form (W3 owns it afterwards)

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Provider } from "jotai/react";
import { RouterProvider } from "react-router";

import "@/index.css";
import { connectStatusSocket } from "@/api/ws";
import { router } from "@/router";
import { store } from "@/state/store";

connectStatusSocket(store);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>
  </StrictMode>,
);
```

`@/api/ws`, `@/router` and `@/state/store` do not exist yet (W2 and W3 write them); a
`tsc -b --noEmit` at the end of your work reports exactly those three unresolved modules and
nothing else. That is the expected state; report it.

### `web/src/api/schema.ts` — the zod mirror of `WebSnapshot`

Exports (all `export const … = z.…` plus `export type X = z.infer<typeof xSchema>`):

```ts
import { z } from "zod";

export const STAGE_STATUSES = ["waiting-for-deps", "queued", "executing", "waiting-for-input",
  "blocked", "completed", "needs-handoff", "skipped", "merge-conflict", "completed-with-failures",
  "merge-blocked", "needs-human-review", "needs-adjudication"] as const;
export const stageStatusSchema = z.enum(STAGE_STATUSES);
export const stageTypeSchema = z.enum(["standard", "knowledge", "integration-verify", "knowledge-distill"]);
export const activityStatusSchema = z.enum(["Idle", "Working", "Error", "Stale", "Orphaned"]);
export const failureTypeSchema = z.enum(["session-crash", "context-exhausted", "test-failure",
  "build-failure", "code-error", "timeout", "user-blocked", "merge-conflict", "infrastructure-error",
  "sandbox-setup-failure", "startup-refusal", "unknown"]);
export const sessionTypeSchema = z.enum(["stage", "merge", "baseconflict", "knowledge", "adjudication"]);
export const sessionBackendSchema = z.enum(["native", "tmux"]);
export const failureInfoSchema = z.object({ failure_type: failureTypeSchema, detected_at: z.string(), evidence: z.array(z.string()) });
export const stageSummarySchema = z.object({
  id: z.string(), name: z.string(), status: stageStatusSchema, stage_type: stageTypeSchema,
  dependencies: z.array(z.string()), context_tokens: z.number().int().nullable(),
  elapsed_secs: z.number().int().nullable(), execution_secs: z.number().int().nullable(),
  base_branch: z.string().nullable(), base_merged_from: z.array(z.string()),
  failure_info: failureInfoSchema.nullable(), activity_status: activityStatusSchema,
  last_tool: z.string().nullable(), last_activity: z.string().nullable(),
  staleness_secs: z.number().int().nullable(), context_ceiling_tokens: z.number().int().nullable(),
  review_reason: z.string().nullable(), merged: z.boolean(),
  cleanup_warning: z.string().nullable().optional(),   // skip_serializing_if on the Rust side
  held: z.boolean(), retry_count: z.number().int(), max_retries: z.number().int().nullable(),
  pid: z.number().int().nullable(), session_alive: z.boolean(), model: z.string(),
  session_type: sessionTypeSchema.nullable(), incoherence: z.string().nullable(),
  execution_models: z.array(z.string()), dispute_count: z.number().int(),
  judge_heartbeat_secs: z.number().int().nullable(), session_backend: sessionBackendSchema.nullable(),
});
export const mergeSummarySchema = z.object({ merged: z.array(z.string()), pending: z.array(z.string()), conflicts: z.array(z.string()) });
export const progressSummarySchema = z.object({ total: z.number().int(), completed: z.number().int(), executing: z.number().int(), pending: z.number().int(), blocked: z.number().int() });
export const statusDataSchema = z.object({ stages: z.array(stageSummarySchema), merge: mergeSummarySchema, progress: progressSummarySchema, plan_name: z.string().nullable() });
export const attentionSchema = z.object({ id: z.string(), name: z.string(), label: z.string(), hint: z.string(),
  failure_type: failureTypeSchema.nullable(), failure_label: z.string().nullable(), evidence: z.array(z.string()),
  review_reason: z.string().nullable(), cleanup_warning: z.string().nullable(), has_human_review_choices: z.boolean(),
  dispute_count: z.number().int().nullable(), judge_heartbeat_secs: z.number().int().nullable() });
export const alertSchema = z.object({ severity: z.enum(["info", "warning", "critical"]), text: z.string() });
export const daemonStateSchema = z.enum(["running", "process-only", "not-running", "unreachable"]);
export const snapshotSchema = z.object({ status: statusDataSchema, attention: z.array(attentionSchema),
  alerts: z.array(alertSchema), daemon: daemonStateSchema, tick_age_secs: z.number().int().nullable(),
  source: z.enum(["daemon", "files"]), generated_at: z.string() });

export type StageStatus = z.infer<typeof stageStatusSchema>;
export type StageSummary = z.infer<typeof stageSummarySchema>;
export type StatusData = z.infer<typeof statusDataSchema>;
export type Attention = z.infer<typeof attentionSchema>;
export type Alert = z.infer<typeof alertSchema>;
export type DaemonState = z.infer<typeof daemonStateSchema>;
export type FailureType = z.infer<typeof failureTypeSchema>;
export type ActivityStatus = z.infer<typeof activityStatusSchema>;
export type Snapshot = z.infer<typeof snapshotSchema>;
```

Every `Option<T>` on the Rust side serializes as `null`, hence `.nullable()`; only
`cleanup_warning` can be absent. Keep the key names identical to the Rust field names (snake_case
on the wire; do not camelCase).

### `web/src/api/fixtures/snapshot.json`

Written from the Rust test's printed JSON (step 2, test 1). Pretty-printed, trailing newline.

## Done means

- `cargo test --manifest-path loom/Cargo.toml --lib commands::status::web::` passes (the four
  model tests). This is the one Rust check you run.
- `cd web && bunx tsc -b --noEmit` reports only the three expected unresolved modules from
  `main.tsx`. This is the one web check you run. No lint, no format run beyond the initial
  `bunx oxfmt` of the scaffold, no build.
- Report: the exact list of files created/modified, the three expected tsc errors, the shadcn
  version banner, and anything that deviated from this brief.
