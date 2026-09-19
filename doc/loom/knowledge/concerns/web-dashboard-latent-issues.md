# Web Dashboard Latent Issues

> Latent issues found in commands/status/web/
> deliberately left unchanged — recorded so a later reader does not mistake them for
> oversights. See [architecture/web-dashboard.md](../architecture/web-dashboard.md) for
> context.

1. **Mutex poisoning cascade.** `loom/src/commands/status/web/broadcast.rs` calls
   `.expect()` on its lock in several places; a panic inside any of those critical sections
   poisons the mutex, after which every connection thread and the producer thread panic on
   the next frame while the accept loop keeps spawning threads that panic immediately.
   Near-unreachable today — those sections only do push/`try_send`/clone.
2. **`GET /ws` without an `Upgrade` header returns the SPA page with 200, not 400** —
   `loom/src/commands/status/web/connection.rs` falls through to `Route::Spa`. Cosmetic.
3. **Inherited partial-frame truncation risk.** The broadcaster's 50ms read timeout over
   `daemon/wire.rs`'s `read_exact`-based framing can truncate a large `StatusData` body
   mid-read; it self-heals via reconnect. The TUI has the identical timeout and the same
   risk, so this is inherited behaviour, not new — a real fix means owning `daemon/wire.rs`,
   out of scope for this plan.
4. **524 kB `index.js` chunk-size warning left in place.** `bun run build` warns that the
   bundle exceeds rollup's 500 kB default. Code-splitting conflicts with the single-bundle
   design `loom/build.rs` embeds via `include_bytes!` (and the acceptance criterion pinning
   `index.js` as one file); raising the warning limit would be suppression. Left for whoever
   owns the bundle's size budget.

## Terminal lane

- **A stalled control-mode terminal holds its resources for up to `GATE_STALL_TIMEOUT` + 2s.**
  `bridge.rs`'s inbound gate cannot distinguish a genuinely half-closed browser from a live one
  that is merely slow to drain (see mistakes/web-dashboard-server.md) — it bounds the ambiguity
  with a 30s stall deadline rather than resolving it. Until that deadline, and for up to a
  further 2s while `PtyChild::shutdown` waits for a clean `try_wait` before `SIGKILL`, the
  terminal keeps one of `MAX_TERMINALS = 8` slots, its PTY and its tmux child alive. Deliberate
  trade-off — a platform-portable deadline over a readiness-flag detector that cannot see a
  half-closed peer while gated — not an oversight; a busy dashboard could in principle have all
  8 slots pinned by stalled peers for up to ~32s before any reclaim.

## Settings lanes (2026-09-12)

Reviewed during the settings-lanes integration-verify and deliberately left unchanged:

1. **`useSettingsWrites` has no per-key sequence guard.** An architecture-review finding
   ("out-of-order responses clobber a newer write") is real in principle but unreachable
   from the current UI — nothing lets a user issue two writes to the same key fast
   enough to race. Left as a latent risk rather than fixed pre-emptively.
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

## Dashboard settings: themes and notifications (2026-09-18)

Two gaps accepted rather than closed while adding the theme picker and notification switch:

1. **Two tone tokens keep Aubergine's hue under Pacific and Graphite.** `--tone-pending` and
   `--tone-dimmed` are `oklch(... 0.025 325)` for every `.dark` variant (`web/src/index.css:63`
   and `:67`). At chroma 0.025 they read as near-gray in practice, but the literal hue is still
   Aubergine's 325, not tuned per theme; "no new theme tokens" stayed a non-goal for this plan,
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
