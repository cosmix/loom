# loom web dashboard

The browser view behind `loom status --web`: the same ledger `loom status --live`
draws in a terminal, re-rendered as a page that stays open on a second monitor.
The server in `loom/src/commands/status/web/` serves the built bundle and streams
snapshots over a WebSocket; this directory is the React client.

## Develop

```bash
loom status --web      # in a loom workspace: starts the server on port 7373
cd web && bun run dev  # Vite dev server; /api and /ws proxy to 127.0.0.1:7373
```

`bun run check` runs the type-check, lint, format check, and tests.

## Build

```bash
cd web && bun run build
```

`dist/` is committed and embedded into the `loom` binary by `loom/build.rs`.
Rebuild and commit `dist/` with every change under `src/`.

## Layout

```text
src/main.tsx          mounts the router inside the Jotai store and opens the socket
src/router.tsx        routes: `/` (ledger) and `/stages/:stageId` (detail)
src/routes/           shell (header, footer, legend), ledger, stage, error
src/components/       header, ledger table, panels, badges, logo
src/components/ui/    shadcn primitives
src/state/            store, atoms, snapshot application
src/lib/format.ts     display semantics shared with the TUI
src/api/              wire schema, socket, fixtures
```

Stage states and their glyphs come from `src/api/fixtures/statuses.json`, which a
Rust test pins against the `StageStatus` enum; the legend and the state badge read
that table rather than restating it.
