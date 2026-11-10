// Screenshots of a settled document that changed since, in a real browser with no server behind
// it: the source page's head, where the settle control and See what changed sit, and the
// standfirst's note of a changed stub (See what changed inside a sentence). Each is shot with a
// settled version to compare with and with one that was never committed, wide and narrow.
//
// Usage: bun web/scripts/settle-heading-shots.mjs <out-dir>
// Vite serves `web/src`; the REST routes answer from `src/plan/api/fixtures`
// (`chat-layout-kit.mjs`). Exit 1 on a page error or an API route the mock does not answer.

import { mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { launchBrowser, settle, startPlanner } from "./chat-layout-kit.mjs";

const fixture = (name) =>
  JSON.parse(
    readFileSync(new URL(`../src/plan/api/fixtures/${name}.json`, import.meta.url), "utf8"),
  );

const DOC = "doc/product/demo/section_02.md";
/** The source page's head and the summary page's heading, each shot by its own selector. */
const PAGES = [
  {
    name: "source",
    route: "/f/demo/source/product/demo/section_02.md",
    shot: ".trace-source-head",
  },
  { name: "summary", route: "/f/demo/product/section_02", shot: ".doc-heading" },
];
const COMMIT = "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567";
const TEXT = [
  "# Beta",
  "",
  "## Three",
  "",
  "Text three. The bridge keeps one session per project and replays its transcript to every tab.",
  "",
  "Settled: body=sha256:abc",
].join("\n");

const VIEWPORTS = [
  { name: "wide", width: 1440, height: 900 },
  { name: "narrow", width: 390, height: 844 },
];
const CASES = [
  { name: "revised-committed", settle: "revised", commit: COMMIT },
  { name: "revised-never-committed", settle: "revised", commit: null },
  { name: "stub-changed-never-committed", settle: "stub_changed", commit: null },
];

function trace(settleState) {
  const view = fixture("trace");
  const doc = view.documents.find((entry) => entry.path === DOC);
  doc.settle = settleState;
  return view;
}

function summaryPage() {
  return {
    ...fixture("concept-summary"),
    key: "demo/product/section_02",
    path: DOC,
    feature: "demo",
    state: null,
    implemented_by: [],
    file_hash: "4b8d0e2f6a1c7395",
  };
}

function rest(scenario) {
  return {
    "/api/server": () => ({ ...fixture("server-info"), read_only: [] }),
    "/api/tree": () => fixture("files-tree"),
    "/api/trace/demo": () => trace(scenario.settle),
    "/api/doc": () => ({ ...fixture("files-doc"), path: DOC, text: TEXT, sections: [] }),
    "/api/settled-version": () => ({ commit: scenario.commit }),
    "/api/concept/demo/product/section_02": () => summaryPage(),
    "/api/run/state": () => fixture("run-state"),
    "/api/pressure/options": () => fixture("pressure-options"),
    "/api/summaries/panel": () => fixture("summary-panel"),
  };
}

function helloFrame() {
  const hello = fixture("hello");
  return {
    ...hello,
    server: { ...hello.server, read_only: [] },
    presence: { entries: [] },
    authoring: { holder: { ...hello.you, away_since: null, frees_at: null } },
  };
}

async function mock(page, origin, scenario, unanswered) {
  const replies = rest(scenario);
  // The page's path ends in `.md`, so it is matched by its route prefix, not by a missing dot.
  await page.route(
    (url) => url.pathname.startsWith("/f/"),
    (route) =>
      route.request().resourceType() === "document"
        ? route.continue({ url: `${origin}/plan.html` })
        : route.fallback(),
  );
  await page.route(
    (url) => url.pathname.startsWith("/api/"),
    (route) => {
      const { pathname } = new URL(route.request().url());
      const reply = replies[pathname];
      if (!reply) unanswered.add(pathname);
      return route.fulfill({
        status: reply ? 200 : 404,
        contentType: "application/json",
        body: JSON.stringify(reply ? reply() : { error: "not_found", message: "not mocked" }),
      });
    },
  );
  await page.routeWebSocket(
    (url) => url.pathname === "/ws",
    (socket) => socket.send(JSON.stringify(helloFrame())),
  );
}

async function shoot(browser, origin, outDir, { scenario, viewport, view }) {
  const context = await browser.newContext({ viewport, reducedMotion: "reduce" });
  const page = await context.newPage();
  const errors = [];
  const unanswered = new Set();
  page.on("pageerror", (error) => errors.push(error.message));
  await mock(page, origin, scenario, unanswered);
  await page.goto(origin + view.route, { waitUntil: "load" });
  await page.waitForSelector("a.trace-changes:not([aria-busy])");
  await page.evaluate(() => document.fonts.ready);
  await settle(page);
  const head = page.locator(view.shot).first();
  const file = join(outDir, `${view.name}-${scenario.name}-${viewport.name}.png`);
  await head.screenshot({ path: file });
  await context.close();
  return { file, errors, unanswered: [...unanswered] };
}

async function main() {
  const outDir = process.argv[2];
  if (!outDir) throw new Error("usage: settle-heading-shots.mjs <out-dir>");
  mkdirSync(outDir, { recursive: true });
  const planner = await startPlanner();
  const browser = await launchBrowser();
  let failed = false;
  try {
    for (const view of PAGES) {
      for (const scenario of CASES) {
        for (const viewport of VIEWPORTS) {
          const shot = await shoot(browser, planner.origin, outDir, { scenario, viewport, view });
          console.log(shot.file);
          for (const error of shot.errors) console.error(`  page error: ${error}`);
          for (const path of shot.unanswered) console.error(`  not mocked: ${path}`);
          failed ||= shot.errors.length > 0;
        }
      }
    }
  } finally {
    await browser.close();
    await planner.close();
  }
  if (failed) process.exit(1);
}

await main();
