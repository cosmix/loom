# Web Dashboard Latent Issues

>
>
>
> UI

1. **Mutex poisoning cascade.** `loom/src/commands/status/web/broadcast.rs` calls
   `.expect()` on its lock in several places; a panic inside any of those critical sections
   poisons the mutex, after which every connection thread and the producer thread panic on
   the next frame while the accept loop keeps spawning threads that panic immediately.
   Near-unreachable today — those sections only do push/`try_send`/clone.
2. **`GET /ws` without an `Upgrade` header returns the SPA page with 200, not 400** —
   `loom/src/commands/status/web/connection.rs` falls through to `Route::Spa`. Cosmetic.
3. **524 kB `index.js` chunk-size warning left in place.** `bun run build` warns that the
   bundle exceeds rollup's 500 kB default. Code-splitting conflicts with the single-bundle
   design `loom/build.rs` embeds via `include_bytes!` (and the acceptance criterion pinning
   `index.js` as one file); raising the warning limit would be suppression. Left for whoever
   owns the bundle's size budget.

## Terminal lane

- **A stalled control-mode terminal holds its resources for up to `GATE_STALL_TIMEOUT` + 2s.**
  `bridge.rs`'s inbound gate cannot distinguish a genuinely half-closed browser from a live one
  that is merely slow to drain (see mistakes/web-terminal.md) — it bounds the ambiguity
  with a 30s stall deadline rather than resolving it. Until that deadline, and for up to a
  further 2s while `PtyChild::shutdown` waits for a clean `try_wait` before `SIGKILL`, the
  terminal keeps one of `MAX_TERMINALS = 8` slots, its PTY and its tmux child alive. Deliberate
  trade-off — a platform-portable deadline over a readiness-flag detector that cannot see a
  half-closed peer while gated — not an oversight; a busy dashboard could in principle have all
  8 slots pinned by stalled peers for up to ~32s before any reclaim.

## Settings lanes

Current settings behavior and remaining concerns:

1. **Concurrent settings writes reconcile after completion.** `settings-write-coordinator.ts` tracks overlapping writes and refetches authoritative entries after they settle. Delayed cross-scope responses cannot remain the final displayed state; concurrency regressions cover this path.
2. **Phone cards mark BOTH tiers `data-effective` when a row's model and effort resolve
   at different tiers.** The effective marker is computed per row via
   `entries.some(...)`, not per lane, so a pair split across user/project tiers shows
   both cells as "in effect" instead of naming which one actually applies to which key.
3. **`createConfigClient().write`'s real `POST`/`X-Loom-Csrf` path has no contract
   test.** Every settings test uses `fakeClient`; the coverage reviewer flagged this and
   it was left unaddressed for this plan.
4. **Bundle-size warning persists and grew slightly.** `web/dist/assets/index.js` is
   852,480 bytes at the pre-plan baseline and 862,753 bytes after settings-lanes —
   consistent with item 4 above (one committed bundle, code-splitting out of scope);
   candidate work if bundle size becomes a real budget: dynamic `import()` for the
   terminal and graph routes.

## Dashboard settings: themes and notifications

Two gaps accepted rather than closed while adding the theme picker and notification switch:

1. **Two tone tokens keep hue 325 under Graphite.** `--tone-pending` and
   `--tone-dimmed` are `oklch(... 0.025 325)` for every `.dark` variant (`web/src/index.css`). At chroma 0.025 they read as near-gray in practice, but the literal hue is a leftover violet 325, not tuned per theme (Graphite is hue 270); "no new theme tokens" stayed a non-goal for this plan,
   so it was left rather than adding per-theme overrides. Every other loom token either derives
   from `var(--primary)`/`var(--border)` or is hue-neutral.
2. **No gate in the web pipeline executes the built bundle in a browser.** `bun run --cwd web
test` runs vitest+jsdom over sources; the `dist` marker greps only prove strings reached
   `web/dist/assets/index.js`; `scripts/smoke-web-dashboard.sh` only curls Rust-served routes. A
   runtime-only defect in the shipped SPA (chunk init order, a missing global, a module-script
   failure) would pass every gate this repo has today. This gap predates this plan but the plan
   made no attempt to close it -- a fix needs a headless-browser smoke that loads
   `dist/index.html` and waits for the dashboard root to render, and its own plan.

A related bundle-size probe, done for this plan and not acted on: `web/dist/assets/index.js`
was 869,529 bytes at `ebe48f3f` (before this plan) and 878,915 bytes after (+9 kB) -- still one
committed bundle (`bun run build`'s `(!) Some chunks are larger than 500 kB` warning left in
place, as in prior plans; see the bundle-size items above). A probe build with per-package
`codeSplitting` groups showed no single package over 500 kB on its own (react-dom 178 kB,
`@xyflow/react` 178 kB, react-router 92 kB, zod 83 kB, app code 134 kB; xterm's 331 kB is
already a lazy chunk), so three groups (react+router, xyflow+dagre, rest) would in principle
clear the warning -- but a catch-all `node_modules` group pulled the lazy xterm chunks into the
eager vendor chunk (850 kB) in the probe, and manual vendor splits can produce chunk-init-order
(TDZ) failures that only show at runtime. Since nothing in this repo executes the built bundle
in a browser (gap 2 above), the split could not be verified and was not attempted. Whoever
closes gap 2 should revisit this probe.

## Merge-Conflict Stages Never Reach the Conflicts Counter

`build_merge_report` (`git/merge/status.rs::build_merge_report`) skips every stage whose status is not `Completed`, so a stage sitting in `MergeConflict` is never counted: `merge.conflicts` (a `string[]` of stage ids in `web/src/api/schema.ts`) stays empty, and the dashboard header's ⚡ conflicts cell reads 0 while the graph shows the stage in Conflict and the attention panel lists a MERGE CONFLICT entry. The cell only counts a `Completed` stage whose `merge_conflict` flag is still set.

The conflicted card prints the word once: `ActivityLine` (the first line of the card's tray in `web/src/components/graph/stage-node.tsx::ActivityLine`) renders `mergeText` only when its text differs from `activityText`'s, and for status merge-conflict both return the fixed text "conflict" (`web/src/lib/format.ts`).

## Slow Terminal Connect Is Inside Firefox, Not in Loom's Connect Path

`handle_upgrade` (`loom/src/commands/status/web/terminal/upgrade.rs::handle_upgrade`) writes the 101 (`tungstenite::accept_with_config`) before `start_bridge` resolves the stage or runs any tmux command, and the browser shows "live" on `onopen`, so neither tmux (`history-limit` 50000), the daemon token tracker nor the status broadcaster can delay "connected". Paired Firefox messages "can't establish" plus "interrupted while the page was loading" on one URL are `WebSocket::Close()` during CONNECTING: the page closed sockets still waiting for their handshake. Firefox admits one CONNECTING WebSocket per IP across all ports and delays a failing address+path+port 200-400 ms x1.5 up to 60 s, keeping the delayed channel queued, so any failing reconnect loop to `127.0.0.1` (a stale dashboard tab, a stopped dev server) stalls every new terminal socket. Confirm with `about:logging` `nsWebSocket:5` (`CONNECTING_QUEUED`, "delaying websocket") or `network.websocket.delay-failed-reconnects=false`. The 50000-line history only costs a full-history tmux reflow per width change after "viewing" shows (the PTY spawns at 120x36, `upgrade.rs::start_bridge`); spawning it at the browser's fitted size would remove one reflow per connect but needs `web/src/api/terminal.ts`.

## Terminal Refusal Paths Drop the Socket Right After the Close Frame

`close_refusal` (4004/4008/4009) and the `start_bridge` error paths return without reading while the browser has already sent its resize frame on open, so Linux answers the close with RST. Firefox can then report 1006 instead of 4009 ("connection lost" instead of "waiting, retrying in 2s"), log "interrupted" and record a failed reconnect for that path. The fix is to read until the peer's close or about 250 ms before dropping (`upgrade.rs::close_refusal`). It is not the cause of the slow connect above.

## The Bundle Is Over 500 kB and the Gate Hides the Warning

The dashboard bundle is split: `web/vite.config.ts` `codeSplitting` groups (`react`, `zod`, `router`, `flow`, `markdown`, `cm-view`, `cm-lang`) keep `web/dist/assets/index.js` near 190 kB, and `loom/src/web_server/assets.rs` serves any `web/dist` path, so a new chunk needs no server change. `plan.js` is the chunk that sits near the 500 kB warning (about 412 kB with edit mode lazy); the rule that keeps it there is in [Plan Web](../conventions/plan-web.md#live-preview-editor-rules). The dist criterion runs vite with `--logLevel error`, so the gate never prints the warning; raising `build.chunkSizeWarningLimit` would only hide it.
