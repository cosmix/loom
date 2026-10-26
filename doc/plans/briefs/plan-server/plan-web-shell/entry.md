# plan-web-shell / entry: spec for codex units W6, W7, W11, W12, W13 and the shell worker's entry files

Lane: codex (`codex-units.md`). Each unit owns the files its row names. Read
`doc/plans/briefs/plan-server/common.md` first. The shell worker (Claude) exports `PlanShell` from
`web/src/plan/shell.tsx` and `ChecklistView` from `web/src/plan/checklist-view.tsx`.

## Vite (`web/vite.config.ts`, about 40 lines today)

- `build.rolldownOptions.input = { index: "index.html", plan: "plan.html" }` (paths resolved
  from `import.meta.dirname`). Keep `output` as is (`assets/[name].js`, `assets/[name][extname]`):
  the dashboard entry must still be `assets/index.js` and `assets/index.css` (Rust tests in
  `loom/src/commands/status/web/tests/embedded.rs` read the real `web/dist`). Check after a build
  that no two chunks collide; if rolldown renames a shared chunk, that is fine, but
  `dist/index.html` must reference only `assets/index.js`/`assets/index.css` plus shared chunks.
- `build.manifest: true` (writes `dist/.vite/manifest.json`; `loom/build/assets.rs` skips
  dot-directories, so it is not embedded; confirm by reading `walk_files_with`). The manifest is
  committed with the rest of `web/dist` (it is not gitignored): later stages' bundle checks read
  the committed copy.
- `server.proxy` depends on `mode`: default (dashboard) unchanged (`127.0.0.1:7373`); `--mode plan`
  proxies `/api` and `/ws` (`ws: true`) to `127.0.0.1:7272` and sets `server.open = "/plan.html"`.
  Export the config as a function of `{ mode }`.

## `web/plan.html`, `main.tsx`, `router.tsx`, `tabs.ts`

- `plan.html`: copy `index.html`'s head (favicon, fonts), title `loom plan`,
  `<script type="module" src="/src/plan/main.tsx">`.
- `main.tsx`: imports `@/index.css` then `@/plan/plan.css`; creates the jotai store, calls
  `initClient(store)` and `connectPlanSocket(store)` (from `api-state`) and stores the returned
  handle in `planSocketAtom`, renders
  `StrictMode > Provider > ThemeProvider > RouterProvider` as `web/src/main.tsx` does.
- `router.tsx`: `export const routes: RouteObject[]` = `[{ path: "/", element: <PlanShell/>, errorElement: <RouteError/>, children: [{ index: true, element: <ChecklistView/> }, ...PLAN_TAB_ROUTES] }]`,
  `export const router = createBrowserRouter(routes)`. `PlanShell` and `ChecklistView` come from the
  `shell` worker (`web/src/plan/shell.tsx`, `web/src/plan/checklist-view.tsx`); `RouteError` is the
  dashboard's (`web/src/routes/error.tsx`).
- `tabs.ts`: `export type PlanTab = { id: string; label: string; path: string; element: () => JSX.Element }`
  and `export const PLAN_TABS: PlanTab[] = []` with a doc comment: plan 3 adds Files, plan 5 adds
  Product and Engineering; `PLAN_TAB_ROUTES` derives `{ path: tab.path + "/*", element }` from it.

## Bundle check (`web/scripts/bundle-check.mjs`)

Criteria run it as `cd web && node scripts/bundle-check.mjs`; it must work from any cwd. It
resolves its inputs from `import.meta.dirname`, never the cwd: `../dist/.vite/manifest.json`,
`../dist/<file>` and `./bundle-rules.json`. Reads the manifest and the rules:
`{ "<entry html>": { "forbidChunks": ["<glob on manifest src or file>"], "forbidText": ["<string>"] } }`.
For each entry HTML key, compute the initial closure: the entry chunk plus everything reachable
through `imports` (static), never `dynamicImports`. Fail (exit 1, one line per violation:
`<entry>: <chunk file> matches <rule>`) when a closure chunk's `src` or `file` matches a
`forbidChunks` glob, or its file text (read from `web/dist/<file>`, JS only) contains a
`forbidText` string. Exit 0 prints `bundle-check: <n> entries, <m> chunks, ok`.
Rules for plan 2: `index.html` forbids chunks `src/plan/**` and text `data-loom-plan` (the
attribute the `shell` worker puts on `PlanShell`'s root); `plan.html` forbids chunks
`node_modules/@xterm/**` and `src/components/terminal/**`.
`web/scripts/bundle-check.test.mjs` (vitest; export the closure and rule functions from `bundle-check.mjs` and run the CLI only when it is the main module): run the closure and rule logic on a hand-built manifest: a
forbidden static import fails, the same chunk reached only through `dynamicImports` passes; and
it runs the CLI as a child process (`node <abs path>/bundle-check.mjs`) once with cwd `web/` and once
with cwd the repository root, over a small fixture dist given through the env var
`LOOM_BUNDLE_CHECK_DIST` (the only override; default `../dist` from the script), and expects the
same exit and output from both.

## `loom/src/plan_server/tests/embedded.rs`

Against the real embedded table (`web_server::assets`): `page("plan.html")` exists and contains
`/assets/plan.js`; through `plan_server::start` on a temp repo, `GET /files/doc/x.md` (a SPA path,
no cookie) answers 200 with the plan shell; the dashboard's `lookup("/")` is still `index.html`.

## Done when

`bun run --cwd web typecheck`, `lint`, `format:check`, `test`, `build`, then
`node web/scripts/bundle-check.mjs` pass; `cargo test --manifest-path loom/Cargo.toml --lib plan_server::tests::embedded`
passes after the build (the Rust build embeds `web/dist`).
