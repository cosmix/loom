# Plan: Quota Meters in `loom status --live` and the Web Dashboard

Not a loom plan. Executed in this session by the orchestrator, delegating to `loom-software-engineer` (sonnet) for code and to fable subagents (`Agent` with `model: "fable"`) for the two visual surfaces, per CLAUDE.md Rule 7. Commits land on the checked-out branch `feat-web-dashboard`.

## Context

An operator watching a loom run needs to know how much Claude Code and Codex subscription budget is left, in both the rolling 5-hour window and the weekly window, to decide whether to let the orchestrator continue or `loom stop` it. Today neither `loom status --live` nor `loom status --web` shows anything account-level; the only "usage" surfaces are per-session context meters and the `loom usage` transcript report (token counts, not quota).

Decisions taken with the user (2026-09-05):

- No `settings.json` changes. Loom fetches the numbers itself, the way ccstatusline and CodexBar do.
- The orchestrator daemon polls both providers on a timer and caches the result on disk; the dashboards only read the cache.
- Static `loom status` stays unchanged; only `--live` and `--web` render the meters.
- Placement: the footer on both dashboards. The TUI footer grows by one line carrying both providers; the web footer becomes a sticky bottom status bar carrying the meters beside the existing source and timestamp. Both providers always show both window slots, with `5h —` when a provider reports no such window (the operator's current Codex plan reports only the weekly window), so nothing shifts as windows appear.
- Bundled fix: the footer's `↑↓ scroll` hint shows only when the stage table actually overflows its viewport. Today it is unconditional (`panels.rs:199-210`) while scrolling is clamped to `stages − visible rows` (`state.rs:26-33`), so on a short plan the keys correctly do nothing and the hint lies.

Data sources, verified 2026-09-05:

| Provider | Source | Auth | Shape |
| --- | --- | --- | --- |
| Claude | `GET https://api.anthropic.com/api/oauth/usage`, headers `Authorization: Bearer <token>`, `anthropic-beta: oauth-2025-04-20` (exactly what ccstatusline sends; `~/.bun/install/global/node_modules/ccstatusline/dist/ccstatusline.js:62265`) | Linux: `~/.claude/.credentials.json` → `.claudeAiOauth.accessToken`; macOS: `security find-generic-password -s "Claude Code-credentials" -w` returns the same JSON. No env overrides: the daemon runs under `DaemonEnvironment`'s allowlist (`loom/src/daemon/server/environment.rs:5-40`), which keeps `HOME` and `PATH` but scrubs everything else. | `{"five_hour":{"utilization":33.0,"resets_at":"2026-04-11T07:00:00+00:00"},"seven_day":{...}}`; newer responses may instead carry `limits:[{kind:"session"\|"weekly_all",percent,resets_at}]` (a limit with percent 0 and null resets_at is a placeholder = no active window). `utilization` is 0-100. 429 carries `retry-after`. |
| Codex | `codex app-server` (stdio, newline-delimited JSON-RPC): `{"id":0,"method":"initialize","params":{"clientInfo":{"name":"loom","title":"loom","version":"<ver>"}}}` → reply; `{"method":"initialized","params":{}}`; `{"id":1,"method":"account/rateLimits/read","params":{}}` → `{"id":1,"result":{"rateLimits":{"primary":{"usedPercent":42,"windowDurationMins":300,"resetsAt":<epoch>},"secondary":{...},"planType":"..."}}}`. Close stdin to stop the server. | codex handles its own auth (`~/.codex/auth.json`) | `primary`/`secondary` are each nullable; classify by `windowDurationMins` (300 = 5-hour, 10080 = weekly). On the operator's current plan only the weekly window is populated, so a provider may legitimately have one window. |

Existing code to reuse: `crate::commands::self_update::client::create_http_client` shape (`loom/src/commands/self_update/client.rs:22`, blocking reqwest, `https_only`, timeouts), `crate::process::run_bounded` (`loom/src/process/mod.rs:105`, process-group kill on deadline; has no stdin pipe), `crate::codex::find_codex_path` (`loom/src/codex.rs:149`), `remote_control::keychain_probe_argv` (`loom/src/remote_control.rs:216`, argv builder pattern), `fs::locking::atomic_write` (`loom/src/fs/locking.rs:76`, private; expose or copy the temp+rename+fsync sequence), `context::untrusted::inline_safe` (`loom/src/context/untrusted.rs:64`), `utils::format_elapsed` (`loom/src/utils.rs:39`), `Theme::context_style` (`loom/src/commands/status/ui/theme.rs:77`), ledger width helpers (`loom/src/commands/status/ui/tui/ledger/text.rs`), the thread-spawn pattern `spawn_status_broadcaster` (`loom/src/daemon/server/broadcast.rs:134-142`) and its join (`loom/src/daemon/server/lifecycle.rs:229,294`).

## Design

### Model (`loom/src/quota/model.rs`)

The module is named `quota`, not `usage`, because `loom/src/commands/usage/` already means transcript token accounting.

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuotaSnapshot {
    pub claude: Option<ProviderQuota>,
    pub codex: Option<ProviderQuota>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderQuota {
    /// Epoch seconds of the last successful poll (unchanged when a poll fails).
    pub observed_at: i64,
    /// Zero to two windows, five-hour first.
    pub windows: Vec<QuotaWindow>,
    /// Codex `planType`; `None` for Claude.
    pub plan: Option<String>,
    /// Last poll failure, already flattened with `inline_safe`; `None` after a success.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub kind: WindowKind,
    /// Clamped to 0..=100; NaN or infinity rejected at parse time.
    pub used_percent: f64,
    /// Epoch seconds; `None` when the provider gave no reset time.
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowKind { FiveHour, SevenDay }
```

Pure helpers next to the types, shared by both renderers' Rust side and mirrored in TypeScript: `WindowKind::label()` → `"5h"` / `"7d"`; `quota_health(used_percent) -> ContextHealth` reusing `context_health(pct.round() as u32, 100)` so the thresholds (green below 60, yellow to 90, red at 90) match the context meters; `reset_text(resets_at, now) -> Option<String>` ("resets in 2h13m" via `format_elapsed`, "resets now" when `resets_at <= now`); `age_secs(observed_at, now)`.

On disk: `.loom/work/quota/claude.json` and `.loom/work/quota/codex.json`, each one `ProviderQuota`, written with the temp+rename sequence (directory created with mode 0700; refuse to write when the target is a symlink, checked with `symlink_metadata`). Reads open the file and `take` at most 64 KiB; oversize or malformed content counts as absent and is never deleted. Workspace-local rather than `~/.loom` so the collector, the web file-poll fallback, and tests all read from the work dir they already have, and so the daemon's scrubbed environment needs no `LOOM_HOME`. A failed poll rewrites the file keeping the previous `observed_at`/`windows`/`plan` and setting `error`; a success clears `error`. A provider that cannot be polled at all (codex binary absent, no Claude credentials) gets no file, so the snapshot field is `None` and the renderers omit that row.

### Poller (`loom/src/quota/poller.rs`)

`spawn_quota_poller(server: &DaemonServer) -> JoinHandle<()>`, spawned in `lifecycle.rs` right after `spawn_status_broadcaster` (line 229) and joined with `wait_with_timeout(handle, "quota_poller")` next to the broadcaster join (line 294). That join allows 5 seconds, so the thread must observe the shutdown flag quickly: sleep in slices of at most 250ms, and pass `&AtomicBool` into the codex exchange so a child still running at shutdown is killed at once rather than at its 15s deadline. Each provider has its own `next_due: Instant`. First poll immediately on start. `POLL_INTERVAL = 180s` for both providers (ccstatusline's cache age; documented as safe for the OAuth endpoint). Claude 429 → back off for `retry-after` seconds, minimum 300s, and record `error: "rate limited"`. Any other repeated failure (for example a logged-out codex) doubles the provider's interval from 180s up to a 15-minute cap and resets on the next success, so a broken provider does not hit the network every three minutes. Every failure is recorded in the cache and logged once per state change through the daemon's existing logging; the loop never exits on error.

Because `DaemonEnvironment` scrubs the environment, reqwest sees no `HTTPS_PROXY`. Add `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` to `HOST_ENV_ALLOWLIST` (`environment.rs:5`), the daemon's first outbound HTTP caller needs them; adjust the allowlist test if one pins the list.

- `claude::fetch(client, token) -> Result<ProviderQuota>` (thin) wraps `claude::parse_response(body: &str, now) -> Result<ProviderQuota>` (pure, tested): buckets first, then the `limits` array (`session` → five-hour, `weekly_all` → seven-day), placeholder limits ignored, `resets_at` parsed with `DateTime::parse_from_rfc3339` (accept fractional seconds and offsets). Client: `https_only(true)`, connect 5s, total 10s, `user_agent("loom/<CARGO_PKG_VERSION>")`, response body capped at 64 KiB. HTTP error → `error: "HTTP 401"`-style text with the status only, never the body or token.
- `credentials::access_token() -> Result<String>`: on macOS `security find-generic-password -s "Claude Code-credentials" -w` (argv from a pure builder tested like `keychain_probe_argv`, bounded by `run_bounded`), then the file `~/.claude/.credentials.json`; the home comes from a `home: &Path` parameter so tests use a temp dir. Parses only `.claudeAiOauth.accessToken`. Token is never logged, never written to the cache, never part of an error string. Missing/unreadable → `Err("no claude.ai login")` → no claude file written.
- Number hygiene shared by both parsers: percentages arrive as `Option<f64>`, are dropped when not finite, then clamped to 0..=100; reset timestamps above `100_000_000_000` are treated as milliseconds and divided by 1000 (codex has shipped milliseconds before); a window whose timestamp fails to parse keeps `resets_at: None` instead of failing the whole provider.
- `codex::poll_once(codex_bin: &Path, deadline: Duration, shutdown: &AtomicBool) -> Result<ProviderQuota>`: spawn `codex app-server` with stdin/stdout piped, stderr null, `process_group(0)`; write the three messages; a reader thread forwards stdout lines (each capped at 64 KiB via `take`) over a `sync_channel`; loop on `recv_timeout` in 250ms slices until the frame whose `id == 1` arrives, the 15s deadline passes, or `shutdown` is set (notifications and the `id == 0` reply are skipped, not errors); on `result` → `codex::parse_snapshot(&Value) -> ProviderQuota` (pure, tested: window classification by `windowDurationMins`, missing windows, nullable `resetsAt`, `planType`); on a JSON-RPC `error` → `Err(message)`; on deadline or shutdown → `Err("codex app-server timed out")`. Teardown in every path: drop stdin, `wait_timeout(2s)`, else kill the process group (`nix::sys::signal::kill(Pid::from_raw(-pid), SIGKILL)` as `run_bounded` does) and reap. Skipped entirely when `find_codex_path()` fails. No existing helper covers a bounded request/response over stdio (`run_bounded` has no stdin pipe; `review/generate.rs` and `verify/criteria/cache_ignore.rs` only write then wait), so this stays private to `quota::codex`.

### Status wiring

`StatusData` gains `#[serde(default)] pub quota: QuotaSnapshot` (`loom/src/commands/status/data/mod.rs:62`). `collect_status_data` (`loom/src/commands/status/data/collector.rs:352`) sets it from `quota::read_snapshot(work_dir.root())`, which reads the two files, tolerates absence and malformed JSON (returns `None` for that provider), clamps percentages, and flattens `plan`/`error` through `inline_safe`. Nothing else in the daemon protocol changes: the field rides inside `Response::StatusUpdate` and inside `WebSnapshot.status`. Six `StatusData` struct literals are built without `..Default::default()` and must gain the field: `daemon/wire_tests.rs:120`, `commands/status/render/graph_tests.rs:42`, `commands/status/ui/tui/ledger/tests.rs:149`, `commands/status/ui/tui/ledger/tests_viewport.rs:63`, `commands/status/web/model_tests.rs:18`, `commands/status/data/collector.rs:389`. The collector reads only the work dir, so tests using temp work dirs never see the operator's real cache.

### TUI footer (`loom/src/commands/status/ui/tui/ledger/quota.rs` + `panels.rs` + `layout.rs`)

The footer becomes two lines when any provider has data: a new quota line above the existing legend-and-keys line. `Budget.footer` is already a field (`layout.rs:26`, always `FOOTER_HEIGHT = 1` today); it becomes `footer_height(has_quota)` and `available_table_height`/`areas` read the budget value instead of the constant. No new degrade step is needed: at `MIN_ROWS = 16`, header 4 + gap 1 + table 6 + footer 2 = 13 leaves room for the four-row alert band plus its gap. `render_footer` gains the snapshot, `now`, and a `scrollable: bool`; `footer_line` drops the `↑↓ scroll` hint when `scrollable` is false (the app passes `ordered.len() > table_viewport_rows`).

One line, both providers, e.g. at 120 columns and at 64:

```text
 claude  5h ━━━━━╌╌╌╌╌ 48% · 2h13m   7d ━━━╌╌╌╌╌╌╌ 31% · 4d2h   │  codex  5h —   7d ━━━━━━╌╌╌╌ 63% · 2d9h      · codex 4m old
 ● executing  ○ waiting  ✓ done                                                                    ? legend · ↑↓ scroll · q quit

 claude 5h 48% · 7d 31% │ codex 5h — · 7d 63%
 ● executing  ○ waiting  ✓ done            ? legend · q quit
```

Rules the fable designer must keep: bars use the ledger's `━`/`╌` idiom; fill and percent colored by `Theme::context_style(pct, 100)`; a missing window renders a dimmed `—`; `observed_at` older than 600s appends a dimmed `· <provider> Nm old`; `error` appends a dimmed, `cut_line`-truncated `· <provider>: <error>`; the countdown is `format_elapsed(resets_at − now)`, `now` passed in for determinism; three width tiers chosen by a pure `quota_layout(width) -> QuotaLayout` (full at 120: bar 10 plus countdown; medium: bar 6 plus countdown; narrow at 64: no bars, percent only), table-tested like `columns.rs`; every pad measured with `text_width`/`spans_width`, never `chars().count()`; a provider with `None` is omitted and the separator with it; when both are `None` the footer is one line exactly as today. Render tests with `ratatui::backend::TestBackend` following `ledger/tests.rs:12-33`: widths 64, 90, 120; one provider only; stale and error suffixes; the scroll hint present only when the table overflows (build a view with more stages than rows); plus a `layout.rs` test that the footer takes two rows only when quota data exists.

### Web meters

- `web/src/api/schema.ts`: `windowKindSchema = z.enum(["five-hour","seven-day"])`, `quotaWindowSchema`, `providerQuotaSchema` (`observed_at: z.number().int()`, `windows`, `plan: z.string().nullable()`, `error: z.string().nullable()`), `quotaSnapshotSchema = z.object({ claude: providerQuotaSchema.nullable(), codex: providerQuotaSchema.nullable() })`, added to `statusDataSchema` as `quota`. Export `ProviderQuota`, `QuotaWindow` types.
- `web/src/lib/quota.ts` (pure, vitest): `quotaHealth(percent): "green"|"yellow"|"red"` (same thresholds as `contextUsage`), `windowLabel(kind)`, `resetText(resetsAt, nowSecs)` mirroring the Rust text via `formatElapsed`, `ageText(observedAt, nowSecs)` (null under 600s), `providerRows(snapshot)` returning the ordered `[["claude", q], ["codex", q]]` pairs that exist.
- `web/src/components/quota-meters.tsx`, mounted inside the `Footer` in `web/src/routes/shell.tsx:45-59`, which becomes a sticky bottom status bar (`position: sticky; bottom: 0`, backdrop matching `--background`, hairline top border) present on every route: meters on the left, the existing `? legend` and `source · time` on the right, wrapping to two rows under 900px like the ledger's card fold. Renders no meter block when both providers are null, leaving the footer as it is today. Uses a `useNow(30_000)` tick for countdowns. Designed by a fable subagent under the `frontend-design` skill with these constraints: stay inside the existing token system (`--tone-*` via `toneClass`, Inter Variable for text, IBM Plex Mono for numbers, shadcn primitives from `components/ui`), both color schemes finished to the same standard, `prefers-reduced-motion` respected, every color also carried by text (percent, label), `role="img"` + `aria-label` per meter like `ContextMeter`, tooltip with exact reset time and observed age, stale and error states visibly distinct but calm, both window slots always present per provider with a dimmed `—` for a missing one. The brief should ask for one memorable idea that fits a status bar (for example a segmented gauge with hairline ticks and a thin time-to-reset track under it) rather than two plain `<progress>` bars.
- `web/src/api/fixtures/snapshot.json` gains a `quota` block (claude: two windows; codex: seven-day only with `plan: "pro"`), regenerated from the Rust test's expected output so `fixture_matches_serde_output` and the vitest schema test agree.
- Rebuild `web/dist` (`cd web && bun run build`) and commit it, as the branch convention requires.

## Implementation steps

Orchestrator does steps 0, 5, 6; everything else is delegated with the Rule 5 preamble and a file-ownership table.

0. **Pin the contract.** Paste the `model.rs` types above, the on-disk JSON, and the zod schema verbatim into every brief so waves run in parallel against one spec.

1. **Wave 1 (parallel).**
   - **S1, sonnet `loom-software-engineer` — Rust quota + wiring.** Owns `loom/src/quota/{mod,model,claude,codex,credentials,cache,poller}.rs` (+ `tests.rs` files), `loom/src/lib.rs` (`pub mod quota;`), `loom/src/daemon/server/lifecycle.rs` (spawn + join), `loom/src/daemon/server/environment.rs` (proxy vars in the allowlist), `loom/src/commands/status/data/{mod,collector,sanitize}.rs` (field, read, flatten), the six struct literals listed above, `loom/src/commands/status/web/model_tests.rs` + `web/src/api/fixtures/snapshot.json` (add the `quota` block to the Rust fixture builder and paste the regenerated JSON). No new crates: reqwest (blocking, json), chrono, serde_json, dirs, which, nix, wait-timeout are already in `loom/Cargo.toml`. Acceptance: `cargo test --manifest-path loom/Cargo.toml --lib quota::` green; `cargo test --lib commands::status::web::model_tests` green; a fake `codex` script under a temp dir exercises success, JSON-RPC error, a garbage line before the reply, a hanging server (killed at the deadline, no zombie), and shutdown mid-exchange; `parse_response` tests cover bucket shape, `limits` shape, placeholder limit, fractional-second timestamps, millisecond `resets_at`, 150% clamps to 100, NaN rejected; `credentials` tests read a temp home without touching the real one; `cache` tests cover symlink refusal, oversize file treated as absent, and error preserving the last good windows.
   - **S2, sonnet `loom-software-engineer` — web data layer.** Owns `web/src/api/schema.ts`, `web/src/api/schema.test.ts`, `web/src/lib/quota.ts`, `web/src/lib/quota.test.ts`. Acceptance: `cd web && bun run typecheck && bunx vitest run src/api src/lib` green; the fixture with the pinned `quota` block parses; a fixture without `quota` fails (the field is required on the wire since the Rust side always serializes it).

2. **Wave 2 (parallel, after wave 1 compiles).**
   - **S3, fable (`Agent`, `model: "fable"`) — web footer meters.** Load `frontend-design:frontend-design` first. Owns `web/src/components/quota-meters.tsx`, `web/src/components/quota-meters.test.tsx`, the `Footer` in `web/src/routes/shell.tsx`, and additive rules in `web/src/index.css` scoped to the footer and meters. Read-only: `schema.ts`, `lib/quota.ts`, `context-meter.tsx`, `state-badge.tsx`, `index.css` tokens. Acceptance: `bun run check` green; render test shows two providers with both window slots from the fixture, keeps today's footer when both null, marks stale and error states with text; both themes reviewed in the Vite dev server against a running `loom status --web`; the footer stays visible while the ledger scrolls and on `/stages/:id`.
   - **S4, fable (`Agent`, `model: "fable"`) — TUI footer.** Owns `loom/src/commands/status/ui/tui/ledger/quota.rs` (+ tests), `panels.rs` (footer rendering, conditional scroll hint), `layout.rs` (footer height from the budget, tests), `ledger/mod.rs` (module line; `LedgerView` gains `now_epoch: i64` and `scrollable: bool`), `app.rs` (fills the two new fields). Read-only: `header.rs`, `text.rs`, `theme.rs`, `columns.rs` for the tier-table style. Acceptance: `cargo test --lib commands::status::ui::tui::` green, including TestBackend renders at widths 64, 90, 120, one-provider and no-provider footers, the scroll hint present only when stages exceed the viewport, and a 16-row terminal with four alerts still showing a 6-row table.

3. **Codex lane note.** No stage lists codex implementers here; everything runs on Claude subagents.

4. **Mini adversarial review.** Spawn `loom-code-reviewer` on the full diff with the security focus list below; fix findings; re-run the gate.

5. **Gate (orchestrator).** `cargo build`, `cargo test` (full), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cd web && bun run check && bun run build`, `scripts/smoke-web-dashboard.sh`.

6. **Commits (orchestrator, Conventional Commits, on the current branch):** `feat(quota): poll claude and codex rate limits from the daemon`; `feat(status): carry quota in StatusData and render it in the live footer`; `fix(status): show the scroll hint only when the ledger overflows`; `feat(web): render quota meters in the dashboard footer`; `build(web): commit the built dashboard bundle`; `docs(knowledge): record the quota module and status field`.

7. **Knowledge (after the code lands, interactive rules).** `loom knowledge replace-section architecture/status-data-model.md "Payload Shapes"` to mention `StatusData.quota`; `loom knowledge update architecture/quota-poller` describing sources, cadence, cache files and the token-handling rules; fix two stale claims found during planning: `entry-points/hooks.md` § "Registration Sites for a New Hook" still says `install.sh` has two `all_hooks` arrays (it now delegates to `loom install-assets`), and `stack.md` lists tokio as the async runtime (`loom/Cargo.toml` has no tokio; the daemon is thread-based).

## Security review focus

- The OAuth token is read into memory only inside `credentials::access_token`, passed to one request, and never appears in logs, cache files, errors, or the wire.
- The daemon runs outside any stage sandbox; stage sessions keep their existing read denial of `~/.claude/.credentials.json`. Nothing here loosens that.
- Every string that came from a network response or a subprocess (`plan`, `error`, codex `limitName`) passes through `inline_safe` before it can reach a terminal or the web frame; the codex response line is length-capped before JSON parsing.
- Percentages are clamped; timestamps that fail to parse become `None` rather than an error that hides the other window.
- The codex child is bounded by a deadline, killed as a process group, and also killed on daemon shutdown; a hanging `codex app-server` cannot stall the daemon, delay its 5-second thread join, or leave descendants.
- Widening `HOST_ENV_ALLOWLIST` by the three proxy variables forwards proxy addresses only; no credential-bearing variable is added.
- `loom status --web` stays loopback-only; the frame carries percentages and epoch seconds, no identifiers.

## Verification

1. Unit and render tests above, plus the full gate.
2. Live check on the test project: `loom run` there (or any plan), then within seconds `.loom/work/quota/claude.json` and `codex.json` appear with plausible numbers matching `/usage` in Claude Code and `/status` in Codex. Stop the daemon; files persist.
3. `loom status --live` in a 120-column terminal shows the quota line above the legend line in the footer; resize to 64 columns and 16 rows and confirm the percent-only tier with the table still 6 rows tall. With 3 stages the footer shows no `↑↓ scroll`; with a plan of more stages than rows the hint appears and the arrows scroll.
4. `loom status --web` then open the dashboard: the sticky footer shows both providers in both color schemes, stays visible while scrolling and on a stage page, countdown ticks, stale marker appears when the daemon is stopped and 10 minutes pass (or by editing `observed_at` in the cache file), error state renders when `codex.json` carries `error`.
5. Remove `~/.codex` from PATH resolution (or rename the binary temporarily) and confirm the codex row disappears without any error in the daemon log; unset credentials the same way for claude.

## Out of scope, noted for later

- Polling from the `loom status --web` process itself when no daemon runs (the page shows the last cached values with their age instead).
- A user-config switch to disable polling, and `seven_day_sonnet`/`seven_day_opus` sub-buckets.
- Static `loom status` output.
